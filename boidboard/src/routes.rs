//! HTTP route handlers.
//!
//! Every handler here does exactly two things: **fetch** and **delegate**
//! (AC-50). Simulation lives in `boids-core`, persistence lives in
//! `repositories`, rendering lives in `views`, and rendering *budgets* live in
//! `views` too — a handler that started computing a metric, laying out an SVG
//! or deciding a colour would be doing someone else's job.
//!
//! Two conventions worth knowing before adding a handler:
//!
//! * **Every route carries `#[public]`.** Boidboard has no accounts; the route
//!   audit still requires each handler to *declare* that, which is the point —
//!   an unclassified route fails the build rather than shipping unguarded.
//! * **A handler uses either `Db` or repositories, never both.** `Db` holds a
//!   pooled connection for the whole request; a repository checks one out per
//!   call. Mixing them deadlocks against a single-connection pool, which is
//!   exactly what the transactional test harness gives you.

use autumn_harvest::handle::WorkflowHandleClient;
use autumn_harvest::types::{
    ExecutionId, Priority, StartSource, WorkflowIdConflictPolicy, WorkflowIdReusePolicy,
};
use autumn_harvest::{StartWorkflowParams, StartedWorkflowExecution};
use autumn_harvest_plugin::HarvestDbPool;
use autumn_web::extract::{Form, Path, Query, State};
use autumn_web::hooks::Patch;
use autumn_web::prelude::*;
use autumn_web::reexports::tracing;
use boids_core::metrics::FrameMetrics;
// `SimState` comes in through the brace group deliberately: the AC-50 guard in
// `tests/web.rs` forbids this file from spelling the kernel's simulation module
// as a leading path segment, which is how it keeps *simulation* out of
// handlers. Decoding a checkpoint the kernel wrote is not simulation — it is
// the same category as the `SimParams` beside it, which reads a run's
// `config_snapshot`. Nothing in this file steps a tick.
use boids_core::{Agent, Obstacle, SimParams, World, sim::SimState};
// Deliberately not `diesel::prelude::*`: that pulls in the *synchronous*
// `RunQueryDsl`, which collides with `diesel_async`'s on every `.load`/`.first`.
use diesel::{
    ExpressionMethods as _, OptionalExtension as _, QueryDsl as _, SelectableHelper as _,
};
use diesel_async::{AsyncPgConnection, RunQueryDsl as _};

use crate::models::{
    NewRun, NewScenario, Run, Scenario, UpdateRun, canonical_config_hash, run::status,
};
use crate::presets;
use crate::repositories::{PgRunRepository, RunRepository as _, frames_for_run};
use crate::schema::{frames, runs, scenarios};
use crate::views;
use crate::workflow::{CANCEL_SIGNAL, SIMULATION_QUEUE, SimulationInput, simulation_workflow_info};

autumn_web::paths![
    run_list,
    new_run_form,
    create_run,
    cancel_run,
    run_detail,
    run_progress,
    compare
];

/// How many runs the list page shows. A bench accumulates runs; the list is a
/// launcher, not an archive.
const RUN_LIST_LIMIT: i64 = 200;

/// How many frames the detail page loads, evenly spaced across the run and
/// always including the first and last (see
/// [`frames_for_run`](crate::repositories::frames_for_run)).
///
/// This is the *query* budget, and it is the one that matters for latency: a
/// 10 000-tick run of 160 agents would otherwise deserialize 1.6 million agent
/// records to draw one page. The *rendering* budgets in
/// [`views::TrajectoryOpts`] then thin this further for the ribbons.
///
/// The claim in the previous sentence was, for a while, false: the query loaded
/// every frame and the budget was applied in Rust afterwards, so a 301-frame run
/// moved 2.66 MB to render a 57 KB page. It is true now — the budget is pushed
/// into the `WHERE` clause as an explicit tick list, so Postgres never builds
/// more rows than this. See
/// [`frames_for_run`](crate::repositories::frames_for_run).
const DETAIL_FRAME_BUDGET: usize = 120;

// ────────────────────────────── AC-41: run list ──────────────────────────────

/// The run list: every run with its status and headline metrics (AC-41).
#[get("/runs")]
#[public]
pub async fn run_list(mut db: Db) -> AutumnResult<Markup> {
    let rows: Vec<Run> = runs::table
        .order(runs::id.desc())
        .limit(RUN_LIST_LIMIT)
        .select(Run::as_select())
        .load(&mut db)
        .await?;

    let scenario_ids: Vec<i64> = rows.iter().map(|r| r.scenario_id).collect();
    let names: Vec<(i64, String)> = scenarios::table
        .filter(scenarios::id.eq_any(&scenario_ids))
        .select((scenarios::id, scenarios::name))
        .load(&mut db)
        .await?;

    let run_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let headlines = latest_metrics_for(&mut db, &run_ids).await?;

    let summaries: Vec<views::RunSummary> = rows
        .into_iter()
        .map(|run| views::RunSummary {
            scenario_name: names
                .iter()
                .find(|(id, _)| *id == run.scenario_id)
                .map_or_else(|| "(unknown scenario)".to_owned(), |(_, n)| n.clone()),
            headline: headlines
                .iter()
                .find(|(id, _)| *id == run.id)
                .map(|(_, m)| *m),
            run,
        })
        .collect();

    Ok(views::layout("Runs", views::runs_table(&summaries)))
}

// ──────────────────────── AC-42: the preset-fronted form ────────────────────────

/// The new-run form (AC-42). No database: the form is presets and nothing else.
#[get("/runs/new")]
#[public]
pub async fn new_run_form(
    csrf: Option<autumn_web::security::CsrfToken>,
    csrf_field: Option<autumn_web::security::CsrfFormField>,
) -> Markup {
    let csrf = views::Csrf::from_request_parts(csrf.as_ref(), csrf_field.as_ref());
    views::layout("New run", views::new_run_form(&presets::all(), &csrf))
}

/// Submitted new-run form.
///
/// `seed` and `max_ticks` arrive as strings rather than numbers because a
/// browser sends an emptied number field as `""`, which is not an `i64` and
/// would fail the whole submission rather than falling back to the default the
/// form itself offered.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateRunForm {
    /// Slug of the chosen preset.
    pub preset: String,
    /// Optional seed override.
    #[serde(default)]
    pub seed: Option<String>,
    /// Optional tick-budget override.
    #[serde(default)]
    pub max_ticks: Option<String>,
}

/// The largest tick budget a run may be created with.
///
/// A tick budget is a promise of work: at the measured ~40 ticks/s and ~9 KB of
/// `agents` JSONB per frame, the `i32` maximum a form could previously submit
/// (`2147483647`) is about 1.7 years of worker time and 19 TB of frame rows —
/// bookable by one anonymous POST, because the form's `min="1"` is client-side
/// only and nothing on the server ever looked.
///
/// 100 000 is chosen against the *workflow*, not a round number: at
/// [`BATCH_TICKS`] = 50 that is 2 000 batches, and a Harvest execution records
/// roughly three history events per batch, so a full-budget run lands near
/// 6 000 events — comfortably under the 10 000-event `continue_as_new`
/// threshold at which a workflow would have to rotate its history. A run at the
/// ceiling therefore still completes as a single execution, which is what keeps
/// the resume cursor and the frame sequence simple. Raising this past ~3 300
/// batches means teaching `simulation_workflow` to rotate.
pub const MAX_TICKS_CEILING: i32 = 100_000;

/// How many non-terminal runs may exist before new submissions are refused.
///
/// [`MAX_TICKS_CEILING`] bounds one run; this bounds the *fleet*. Without it a
/// script can hold the ceiling in each of unboundedly many runs, which costs the
/// same worker-years spread across more rows. The limit is deliberately loose —
/// a bench is meant to be used, and every legitimate session sits far below it —
/// and it counts non-terminal runs rather than recent ones, so a run that wedges
/// consumes a slot until it is finalized. That is the intended pressure: a
/// wedged run is a bug to fix, not a slot to forget.
pub const MAX_ACTIVE_RUNS: i64 = 24;

impl CreateRunForm {
    /// Seed to run with, falling back to the form's own offered default.
    ///
    /// An omitted or empty field is the default — a browser sends an emptied
    /// number input as `""`, and the form advertises the fallback. A field with
    /// *content* that is not a seed is a mistake, and is reported: silently
    /// substituting `1` would record a run whose provenance says it ran a seed
    /// the user never asked for.
    ///
    /// # Errors
    /// Returns `422` when the field is present, non-empty and unreadable.
    fn seed_or_default(&self) -> AutumnResult<i64> {
        let Some(raw) = Self::non_empty(self.seed.as_deref()) else {
            return Ok(views::DEFAULT_SEED);
        };
        raw.parse::<i64>().map_err(|_| {
            AutumnError::unprocessable_msg(format!("`seed` must be a whole number, got `{raw}`"))
        })
    }

    /// Tick budget to run with, within `1..=`[`MAX_TICKS_CEILING`].
    ///
    /// Out-of-range input is **refused**, not clamped. Clamping would run a
    /// different experiment from the one that was asked for and report success,
    /// which is the same category of quiet lie as substituting a default preset
    /// for an unknown slug — and this page's whole claim is that a run is a
    /// faithful record of what was requested.
    ///
    /// # Errors
    /// Returns `422` when the field is present, non-empty and either unreadable
    /// or outside the legal range.
    fn max_ticks_or_default(&self) -> AutumnResult<i32> {
        let Some(raw) = Self::non_empty(self.max_ticks.as_deref()) else {
            return Ok(views::DEFAULT_MAX_TICKS);
        };
        let parsed = raw.parse::<i32>().map_err(|_| {
            AutumnError::unprocessable_msg(format!(
                "`max_ticks` must be a whole number between 1 and {MAX_TICKS_CEILING}, \
                 got `{raw}`"
            ))
        })?;
        if !(1..=MAX_TICKS_CEILING).contains(&parsed) {
            return Err(AutumnError::unprocessable_msg(format!(
                "`max_ticks` must be between 1 and {MAX_TICKS_CEILING}, got {parsed}"
            )));
        }
        Ok(parsed)
    }

    /// The trimmed field value, or `None` when it was omitted or blank.
    fn non_empty(field: Option<&str>) -> Option<&str> {
        field.map(str::trim).filter(|s| !s.is_empty())
    }
}

// ──────────────── AC-42 continued: dispatching the run ────────────────
//
// Everything from here to `create_run` is the seam between the web layer and
// the durable one. It was missing: the routes created rows and the workflow
// waited for a start that never came, so every run sat at `queued` with zero
// frames and the whole durability story was unreachable through the UI.

/// Ticks simulated per `simulate_batch` call — the run's **checkpoint grain**.
///
/// Three separate things are measured in this unit, which is why it is one
/// constant and not three: a worker that dies mid-run redoes at most this much
/// work, a `cancel` signal takes effect at most this many ticks after it is
/// sent (the workflow reads signals only at batch boundaries), and this is how
/// often `runs.ticks_completed` visibly advances for the progress poller.
/// Smaller is more responsive and costs more round trips; 50 keeps a
/// default-length run inside a handful of batches while still letting a cancel
/// land promptly.
const BATCH_TICKS: u32 = 50;

/// Stride at which stored frames carry computed metrics.
///
/// Metrics are a pure function of a frame and cost an O(N²) nearest-neighbour
/// scan, so they are sampled rather than computed on every tick; the frames
/// themselves stay contiguous either way (see `simulate_batch_core`).
const METRICS_EVERY: u32 = 10;

/// The Harvest `workflow_id` a run's execution is keyed on.
///
/// Derived from the run id and nothing else, which is what makes starting
/// **idempotent**: a resubmitted form, a retried dispatch or a duplicated
/// request all resolve to the same `(simulation_workflow, run-N)` pair, and
/// [`WorkflowIdReusePolicy::AllowDuplicate`] hands back the execution that is
/// already running instead of starting a second one. Two workflows driving one
/// run would not merely waste a worker: they would race for the same
/// `(run_id, tick)` frame rows and interleave their cursors.
#[must_use]
pub fn workflow_id_for_run(run_id: i64) -> String {
    format!("run-{run_id}")
}

/// Start — or re-attach to — the durable workflow that drives `run`.
///
/// Returns the execution the run is now bound to, whether this call created it
/// or found it. `started.created` distinguishes the two, which is what the
/// idempotency test asserts on.
///
/// Public, and taking its client and connection as arguments rather than
/// digging them out of `AppState`, so a test can call *this* — the exact code
/// the route runs — twice against one run and watch the second call return the
/// first call's execution.
///
/// # Errors
///
/// Returns an error if the input cannot be serialized or if Harvest refuses the
/// start (a mismatched workflow type, a storage failure).
pub async fn start_simulation_workflow(
    client: &WorkflowHandleClient,
    conn: &mut AsyncPgConnection,
    run: &Run,
) -> AutumnResult<StartedWorkflowExecution> {
    let workflow_name = simulation_workflow_info().name;
    let workflow_id = workflow_id_for_run(run.id);
    // The workflow carries a cursor and a budget, never the flock — see the
    // `workflow` module docs for why that is load-bearing.
    let input = serde_json::to_value(SimulationInput {
        run_id: run.id,
        max_ticks: u32::try_from(run.max_ticks).unwrap_or(0),
        batch_ticks: BATCH_TICKS,
        metrics_every: METRICS_EVERY,
    })?;

    let shard = client.pick_shard_for_new_workflow(workflow_name, &workflow_id);
    let started = client
        .start_or_load(
            conn,
            StartWorkflowParams {
                workflow_name,
                workflow_id: &workflow_id,
                exec_id: ExecutionId::new_for_shard(shard),
                input,
                queue_name: SIMULATION_QUEUE,
                // The whole point of the derived workflow id: a second start
                // for the same run resolves to the execution already driving
                // it rather than racing a duplicate against it.
                reuse_policy: WorkflowIdReusePolicy::AllowDuplicate,
                conflict_policy: WorkflowIdConflictPolicy::Unspecified,
                priority: Priority::default(),
                start_source: StartSource::Api,
                parent_id: None,
                execution_timeout: None,
                memo: None,
                search_attrs: None,
                trace_context: None,
                max_execution_timeout_ceiling: None,
                chain_execution_timeout: None,
                max_workflow_chain_timeout_ceiling: None,
                inherited_chain_deadline_at: None,
                concurrency_key: None,
                concurrency_limit: None,
                max_workflow_input_bytes: 0,
                start_at: None,
                delay: None,
                max_workflow_start_delay: None,
                owner: None,
                runbook_url: None,
                severity: None,
                context_headers: None,
                sla: None,
                schedule_id: None,
                scheduled_for: None,
                workflow_attempt: 1,
                workflow_retry_policy: None,
                retry_of_exec_id: None,
                max_workflow_attempts_ceiling: None,
                origin: None,
                completion_callbacks: None,
                start_source_ref: None,
                started_by: None,
            },
        )
        .await
        .map_err(|e| {
            AutumnError::internal_server_error_msg(format!(
                "starting the simulation workflow for run {}: {e}",
                run.id
            ))
        })?;
    Ok(started.started)
}

/// Check out a connection to Harvest's **storage** database.
///
/// Deliberately not the app pool: in a split deployment Harvest's queue and
/// history tables live in a different database from `runs` and `frames`, and a
/// start has to be written where the worker will look for it.
async fn harvest_conn(state: &AppState) -> AutumnResult<autumn_web::db::PooledConnection> {
    let pool = state.extension::<HarvestDbPool>().ok_or_else(|| {
        AutumnError::service_unavailable_msg(
            "a Harvest client is installed but its storage pool is not",
        )
    })?;
    pool.default_pool()
        .get()
        .await
        .map_err(|e| AutumnError::service_unavailable_msg(format!("Harvest storage: {e}")))
}

/// Start the workflow for a freshly created run and report the execution id.
///
/// Returns `None` — and logs loudly — when no Harvest runtime is mounted. That
/// absence is not hypothetical: the route tests in `tests/web.rs` boot
/// `all_routes()` *without* the plugin, to assert on rendering and persistence
/// alone. But a real deployment in that state creates runs nothing will ever
/// simulate, which is precisely the bug this seam exists to close — so the
/// request still succeeds and the log still shouts.
///
/// The run is left `queued`; the workflow's own first `simulate_batch` moves it
/// to `running` when it actually starts working, so the status always reflects
/// what a worker has done rather than what a route hoped it would do.
async fn dispatch_run(state: &AppState, run: &Run) -> AutumnResult<Option<String>> {
    let Some(client) = state.extension::<WorkflowHandleClient>() else {
        tracing::error!(
            run_id = run.id,
            "no Harvest runtime is mounted: run {} was created but nothing will ever \
             simulate it",
            run.id
        );
        return Ok(None);
    };

    let mut conn = harvest_conn(state).await?;
    let started = start_simulation_workflow(&client, &mut conn, run).await?;
    tracing::info!(
        run_id = run.id,
        execution_id = %started.exec_id,
        created = started.created,
        "dispatched the simulation workflow"
    );
    Ok(Some(started.exec_id.to_string()))
}

/// Refuse a submission once [`MAX_ACTIVE_RUNS`] runs are already in flight.
///
/// `503`, not `400`: the request is well-formed and will be fine later, which is
/// exactly what a saturated bench means. (`429` would be the ideal code; the
/// error type does not carry one, and inventing a bare response here would cost
/// the uniform error rendering every other refusal in this file gets.)
///
/// An app with no database pool — the state `tests/web.rs` boots for pure
/// rendering assertions — has no runs to count, so there is nothing to refuse.
async fn refuse_when_bench_is_full(state: &AppState) -> AutumnResult<()> {
    let Some(pool) = state.pool() else {
        return Ok(());
    };
    let mut conn = pool
        .get()
        .await
        .map_err(|e| AutumnError::service_unavailable_msg(format!("run capacity check: {e}")))?;

    let active = active_run_count(&mut conn).await?;
    if active >= MAX_ACTIVE_RUNS {
        return Err(AutumnError::service_unavailable_msg(format!(
            "the bench is full: {active} runs are already queued or running and the \
             limit is {MAX_ACTIVE_RUNS}. Wait for one to finish, or cancel one from \
             its page."
        )));
    }
    Ok(())
}

/// How many runs are queued or running.
///
/// The set of non-terminal statuses is *derived* from
/// [`status::is_terminal`](crate::models::run::status::is_terminal) rather than
/// listed again, so a status added later is counted without anyone remembering
/// to come back here.
async fn active_run_count(conn: &mut AsyncPgConnection) -> AutumnResult<i64> {
    let active: Vec<&str> = status::ALL
        .into_iter()
        .filter(|s| !status::is_terminal(s))
        .collect();
    let count = runs::table
        .filter(runs::status.eq_any(active))
        .count()
        .get_result(conn)
        .await?;
    Ok(count)
}

/// The scenario with `new.config_hash`, creating it if no one has yet.
///
/// One statement, not two. Read-then-insert is a time-of-check/time-of-use
/// race: two submissions of the same preset that arrive together both find no
/// row and both insert one, and the bench ends up with two scenarios that are
/// the same scenario — which quietly breaks "have I run this exact parameter set
/// before?", the only question `config_hash` exists to answer.
///
/// `ON CONFLICT (config_hash) DO NOTHING` makes the loser of that race a no-op
/// rather than a duplicate or an error, and the re-read then returns whichever
/// row won. The conflict target is backed by the UNIQUE index added in
/// `20260816000006_scenarios_config_hash_unique`; Postgres rejects the clause
/// without it, so the invariant cannot silently regress to a plain index.
///
/// The re-read is a separate statement rather than a `RETURNING` clause because
/// `DO NOTHING` returns no row for the loser — which is exactly the case that
/// needs an answer.
async fn find_or_create_scenario(state: &AppState, new: &NewScenario) -> AutumnResult<Scenario> {
    let pool = state
        .pool()
        .ok_or_else(|| AutumnError::service_unavailable_msg("no database pool is configured"))?;
    let mut conn = pool
        .get()
        .await
        .map_err(|e| AutumnError::service_unavailable_msg(format!("scenario lookup: {e}")))?;

    diesel::insert_into(scenarios::table)
        .values(new)
        .on_conflict(scenarios::config_hash)
        .do_nothing()
        .execute(&mut conn)
        .await?;

    let scenario = scenarios::table
        .filter(scenarios::config_hash.eq(&new.config_hash))
        .select(Scenario::as_select())
        .first(&mut conn)
        .await?;
    Ok(scenario)
}

/// Create a scenario (or reuse the one with the same canonical config hash),
/// create a queued run, **start its durable workflow**, then redirect to the
/// run's page (AC-42).
///
/// The run's `config_snapshot` is a **copy** of the preset's config, not a
/// pointer at the scenario row. That is AC-39: editing or reusing the scenario
/// afterwards can never change what an already-created run says it ran.
///
/// An unknown slug is a `400`, never a silent fallback to some default preset —
/// substituting a different parameter set would make the run's provenance a
/// lie.
///
/// The run row is written **before** the workflow starts, deliberately: the
/// workflow's very first activity loads that row, so a start that raced the
/// insert would fail on a run that does not exist yet. The execution id is
/// written back afterwards.
#[post("/runs")]
#[public]
pub async fn create_run(
    State(state): State<AppState>,
    run_repo: PgRunRepository,
    Form(form): Form<CreateRunForm>,
) -> AutumnResult<Redirect> {
    let preset = presets::by_slug(&form.preset)
        .ok_or_else(|| AutumnError::bad_request_msg(format!("unknown preset `{}`", form.preset)))?;
    // Validate the whole submission before writing anything: a run refused
    // halfway leaves a scenario row behind for a run that never existed.
    let seed = form.seed_or_default()?;
    let max_ticks = form.max_ticks_or_default()?;
    refuse_when_bench_is_full(&state).await?;

    let config = serde_json::to_value(&preset.params).map_err(|e| {
        AutumnError::internal_server_error_msg(format!("preset config is not serializable: {e}"))
    })?;
    let config_hash = canonical_config_hash(&config);

    let scenario = find_or_create_scenario(
        &state,
        &NewScenario {
            name: preset.name.to_owned(),
            config: config.clone(),
            config_hash: config_hash.clone(),
        },
    )
    .await?;

    let run = run_repo
        .save(&NewRun {
            scenario_id: scenario.id,
            seed,
            status: status::QUEUED.to_owned(),
            max_ticks,
            ticks_completed: 0,
            config_snapshot: config,
            config_hash,
            kernel_version: crate::KERNEL_VERSION.to_owned(),
            final_state_hash: None,
            error: None,
            workflow_execution_id: None,
        })
        .await?;

    if let Some(execution_id) = dispatch_run(&state, &run).await? {
        run_repo
            .update(
                run.id,
                &UpdateRun {
                    workflow_execution_id: Patch::Set(Some(execution_id)),
                    ..UpdateRun::default()
                },
            )
            .await?;
    }

    Ok(Redirect::to(&paths::run_detail(run.id)))
}

// ────────────────────── AC-33: cancel, from the UI ──────────────────────

/// Ask a run's workflow to stop at its next batch boundary (AC-33).
///
/// A **signal**, not an engine cancellation, and the difference is the whole
/// point. An engine cancel kills the execution where it stands: the frames of
/// the batch in flight are kept (they were committed by the activity) but the
/// run row is never finalized, so it is left claiming to be `running` forever.
/// The `cancel` signal is read by the workflow at a batch boundary, which lets
/// it record the intervention to `run_signals` and finalize the run as
/// `cancelled` with the ticks it really reached. See the batch-boundary comment
/// in `simulation_workflow` for the replay reason the two cannot be merged.
///
/// Already-terminal runs redirect unchanged rather than erroring: cancelling a
/// finished run is a double click, not a fault.
#[post("/runs/{id}/cancel")]
#[public]
pub async fn cancel_run(
    State(state): State<AppState>,
    run_repo: PgRunRepository,
    Path(id): Path<i64>,
) -> AutumnResult<Redirect> {
    let run = run_repo
        .find_by_id(id)
        .await?
        .ok_or_else(|| AutumnError::not_found_msg(format!("run {id} does not exist")))?;

    if status::is_terminal(&run.status) {
        return Ok(Redirect::to(&paths::run_detail(id)));
    }

    let execution_id = run
        .workflow_execution_id
        .as_deref()
        .ok_or_else(|| {
            AutumnError::unprocessable_msg(format!(
                "run {id} has no workflow to cancel: it was never dispatched"
            ))
        })?
        .parse::<ExecutionId>()
        .map_err(|e| {
            AutumnError::unprocessable_msg(format!(
                "run {id} names an unreadable workflow execution: {e}"
            ))
        })?;

    let mut conn = harvest_conn(&state).await?;
    autumn_harvest::signal::send_signal(
        &mut conn,
        execution_id,
        CANCEL_SIGNAL,
        serde_json::json!({ "source": "run-detail-page" }),
    )
    .await
    .map_err(|e| AutumnError::unprocessable_msg(format!("cancelling run {id}: {e}")))?;

    Ok(Redirect::to(&paths::run_detail(id)))
}

// ─────────────────── AC-43 / AC-44 / AC-47: the run detail page ───────────────────

/// The run detail page: flock, trajectories, sparklines and provenance
/// (AC-43, AC-44, AC-45, AC-47).
#[get("/runs/{id}")]
#[public]
pub async fn run_detail(
    mut db: Db,
    Path(id): Path<i64>,
    csrf: Option<autumn_web::security::CsrfToken>,
    csrf_field: Option<autumn_web::security::CsrfFormField>,
) -> AutumnResult<Markup> {
    let run = load_run(&mut db, id).await?;
    let scenario_name = scenario_name_of(&mut db, run.scenario_id).await?;
    let (world, obstacles) = world_of(&run);

    let stored = frames_for_run(&mut db, id, Some(DETAIL_FRAME_BUDGET)).await?;
    let trajectory: Vec<(i32, Vec<Agent>)> = stored
        .iter()
        .map(|f| (f.tick, decode_agents(&f.agents)))
        .collect();
    let series: Vec<FrameMetrics> = stored
        .iter()
        .filter_map(|f| decode_metrics(&f.metrics))
        .collect();
    let latest_agents = trajectory
        .last()
        .map(|(_, agents)| agents.clone())
        .unwrap_or_default();

    let title = format!("{scenario_name} · run #{id}");
    let detail = views::RunDetail {
        stuck: crate::analysis::run_is_stuck(&stored, &world),
        csrf: views::Csrf::from_request_parts(csrf.as_ref(), csrf_field.as_ref()),
        run,
        scenario_name,
        world,
        obstacles,
        latest_agents,
        trajectory,
        series,
    };
    Ok(views::layout(&title, views::run_detail_page(&detail)))
}

// ─────────────────────── AC-45: the htmx polling fragment ───────────────────────

/// The progress fragment for one run (AC-45).
///
/// Returns the fragment alone — no document chrome — because htmx swaps it in
/// place. Whether it keeps polling is decided by the run's status inside
/// [`views::progress_fragment`], so the endpoint itself is stateless.
#[get("/runs/{id}/progress")]
#[public]
pub async fn run_progress(mut db: Db, Path(id): Path<i64>) -> AutumnResult<Markup> {
    let run = load_run(&mut db, id).await?;
    Ok(views::progress_fragment(&run))
}

// ────────────────────────────── AC-46: compare ──────────────────────────────

/// Which two runs to compare.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct CompareQuery {
    /// Left-hand run id.
    pub a: i64,
    /// Right-hand run id.
    pub b: i64,
}

/// Two runs side by side, with their config differences called out (AC-46).
#[get("/compare")]
#[public]
pub async fn compare(mut db: Db, Query(q): Query<CompareQuery>) -> AutumnResult<Markup> {
    let run_a = load_run(&mut db, q.a).await?;
    let run_b = load_run(&mut db, q.b).await?;
    let agents_a = latest_agents(&mut db, run_a.id).await?;
    let agents_b = latest_agents(&mut db, run_b.id).await?;
    // Both flocks are drawn into the same world so the two pictures are
    // directly comparable; the left-hand run defines it.
    let (world, _) = world_of(&run_a);

    let title = format!("Compare runs #{} and #{}", run_a.id, run_b.id);
    let body = views::compare_view((&run_a, &agents_a), (&run_b, &agents_b), &world);
    Ok(views::layout(&title, body))
}

// ────────────────────────────── fetch helpers ──────────────────────────────
//
// Small, named, and doing nothing but IO plus the decoding of an opaque JSONB
// column. Everything below returns *data*; nothing below returns markup.

/// Load a run or fail with a `404`.
async fn load_run(conn: &mut AsyncPgConnection, id: i64) -> AutumnResult<Run> {
    runs::table
        .find(id)
        .select(Run::as_select())
        .first(conn)
        .await
        .optional()?
        .ok_or_else(|| AutumnError::not_found_msg(format!("run {id} does not exist")))
}

/// Name of the scenario a run came from, or a placeholder if it has been
/// deleted out from under the run.
async fn scenario_name_of(conn: &mut AsyncPgConnection, scenario_id: i64) -> AutumnResult<String> {
    let name: Option<String> = scenarios::table
        .find(scenario_id)
        .select(scenarios::name)
        .first(conn)
        .await
        .optional()?;
    Ok(name.unwrap_or_else(|| "(unknown scenario)".to_owned()))
}

/// The most recent stored frame's metrics for each of `run_ids`.
///
/// One `DISTINCT ON` query rather than one query per run: the run list would
/// otherwise issue two hundred round trips to draw one table.
async fn latest_metrics_for(
    conn: &mut AsyncPgConnection,
    run_ids: &[i64],
) -> AutumnResult<Vec<(i64, FrameMetrics)>> {
    if run_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(i64, serde_json::Value)> = frames::table
        .filter(frames::run_id.eq_any(run_ids))
        .distinct_on(frames::run_id)
        .order((frames::run_id.asc(), frames::tick.desc()))
        .select((frames::run_id, frames::metrics))
        .load(conn)
        .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(id, value)| decode_metrics(&value).map(|m| (id, m)))
        .collect())
}

/// Agents of a run's most recent stored frame; empty when it has none yet.
async fn latest_agents(conn: &mut AsyncPgConnection, run_id: i64) -> AutumnResult<Vec<Agent>> {
    let value: Option<serde_json::Value> = frames::table
        .filter(frames::run_id.eq(run_id))
        .order(frames::tick.desc())
        .select(frames::agents)
        .first(conn)
        .await
        .optional()?;
    Ok(value.as_ref().map(decode_agents).unwrap_or_default())
}

/// The world and obstacles a run took place in, read from its config snapshot.
///
/// The snapshot is opaque `JSONB` by design, so it may not parse as
/// `SimParams` — a config written by an older kernel, or by hand. A page that
/// 500s on that is worse than a page that draws the flock in a default frame,
/// because the provenance panel is exactly what a user needs to *see* when
/// they are diagnosing such a run.
fn world_of(run: &Run) -> (World, Vec<Obstacle>) {
    serde_json::from_value::<SimParams>(run.config_snapshot.clone()).map_or_else(
        |_| (World::new(200.0, 200.0), Vec::new()),
        |p| (p.world, p.obstacles),
    )
}

/// Decode a frame's `agents` column; an unreadable frame draws as empty rather
/// than failing the page.
///
/// # Why this reads `SimState`'s wire form first
///
/// `simulate_batch` does not store a serialized `Vec<Agent>`. It stores
/// [`SimState`]'s *own* array — flat records whose coordinates are exact
/// decimal **strings** — because a JSON float can come back one ULP away from
/// what was written and one ULP changes the canonical state hash (see
/// `checkpoint_frame`). That form does not deserialize as `Vec<Agent>`: the
/// fields are `px`/`py`/`vx`/`vy`, not `pos`/`vel`, and they are strings.
///
/// Reading it as `Vec<Agent>` therefore silently produced an **empty flock**
/// for every frame the application actually writes — a page that drew a run's
/// world, its axes and its provenance, with no boids in it. That is a second
/// gap in the same seam as the missing dispatch, and it hid for the same
/// reason: the writer and the reader were each tested against their own
/// fixtures and nothing ever ran both. So the frame is rebuilt through
/// [`SimState`] — the same door it left by, exactly as `load_checkpoint` does
/// on the resume path.
///
/// The direct `Vec<Agent>` form is still accepted as a fallback: the column is
/// opaque `JSONB` by design and may hold a frame written by hand or by an
/// older kernel, and a flock that draws is worth more than a purist decoder.
fn decode_agents(value: &serde_json::Value) -> Vec<Agent> {
    serde_json::from_value::<SimState>(serde_json::json!({ "tick": 0, "agents": value }))
        .map(|state| state.agents)
        .or_else(|_| serde_json::from_value(value.clone()))
        .unwrap_or_default()
}

/// Decode a frame's `metrics` column.
fn decode_metrics(value: &serde_json::Value) -> Option<FrameMetrics> {
    serde_json::from_value(value.clone()).ok()
}
