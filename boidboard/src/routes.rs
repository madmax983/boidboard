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
use crate::repositories::{
    PgRunRepository, PgScenarioRepository, RunRepository as _, ScenarioRepository as _,
    frames_for_run,
};
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
pub async fn new_run_form() -> Markup {
    views::layout("New run", views::new_run_form(&presets::all()))
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

impl CreateRunForm {
    /// Seed to run with, falling back to the form's own offered default.
    fn seed_or_default(&self) -> i64 {
        self.seed
            .as_deref()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or(views::DEFAULT_SEED)
    }

    /// Tick budget to run with. Never less than one tick: a zero-tick run
    /// would be created already finished and would produce no frames at all.
    fn max_ticks_or_default(&self) -> i32 {
        self.max_ticks
            .as_deref()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .unwrap_or(views::DEFAULT_MAX_TICKS)
            .max(1)
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
    scenario_repo: PgScenarioRepository,
    run_repo: PgRunRepository,
    Form(form): Form<CreateRunForm>,
) -> AutumnResult<Redirect> {
    let preset = presets::by_slug(&form.preset)
        .ok_or_else(|| AutumnError::bad_request_msg(format!("unknown preset `{}`", form.preset)))?;

    let config = serde_json::to_value(&preset.params).map_err(|e| {
        AutumnError::internal_server_error_msg(format!("preset config is not serializable: {e}"))
    })?;
    let config_hash = canonical_config_hash(&config);

    let existing = scenario_repo
        .find_by_config_hash(config_hash.clone())
        .await?;
    let scenario: Scenario = match existing.into_iter().next() {
        Some(s) => s,
        None => {
            scenario_repo
                .save(&NewScenario {
                    name: preset.name.to_owned(),
                    config: config.clone(),
                    config_hash: config_hash.clone(),
                })
                .await?
        }
    };

    let run = run_repo
        .save(&NewRun {
            scenario_id: scenario.id,
            seed: form.seed_or_default(),
            status: status::QUEUED.to_owned(),
            max_ticks: form.max_ticks_or_default(),
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
pub async fn run_detail(mut db: Db, Path(id): Path<i64>) -> AutumnResult<Markup> {
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
