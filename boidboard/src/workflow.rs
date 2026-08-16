//! The durable simulation workflow.
//!
//! # The cursor-only invariant
//!
//! **The workflow carries only a cursor — never the agent array.** Every message
//! that crosses the workflow boundary (activity input, activity output, workflow
//! result) is a handful of scalars: `run_id`, a tick, a 16-hex-digit state hash.
//! The flock itself lives in exactly two places, Postgres and the inside of one
//! activity invocation, and never enters workflow history.
//!
//! That is not a micro-optimisation, it is what makes the design work at all.
//! Workflow history is replayed from the beginning on every task pickup, so any
//! payload it holds is paid for on every batch, forever. A 10 000-agent flock in
//! history would make a 100-batch run quadratic in agent count; a cursor makes it
//! O(1). **AC-29** asserts exactly this, by running the same workflow against a
//! 10-agent and a 10 000-agent flock and comparing history byte for byte.
//!
//! The corollary is the reason activities may be retried freely: since the
//! workflow tells an activity only *where to start*, not *what the state is*, a
//! duplicated delivery recomputes the identical frames and the
//! `UNIQUE (run_id, tick)` constraint absorbs them (**AC-32**).

use std::collections::BTreeMap;

use autumn_harvest::prelude::*;
use autumn_harvest_plugin::AppDbPool;
use autumn_web::AutumnResult;
use autumn_web::db::PooledConnection;
use autumn_web::error::AutumnError;
use boids_core::SimParams;
use boids_core::metrics::{FrameMetrics, frame_metrics};
use boids_core::sim::{SimState, run_batch, validate};
use chrono::SecondsFormat;
// Deliberately not `diesel::prelude::*`: it pulls in the *synchronous*
// `RunQueryDsl`, which collides with `diesel_async`'s on every `.first`/
// `.execute` call.
use diesel::result::OptionalExtension as _;
use diesel::{ExpressionMethods as _, QueryDsl as _, SelectableHelper as _};
use diesel_async::{AsyncPgConnection, RunQueryDsl as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::run::status;
use crate::models::run_signal::kind;
use crate::models::{NewFrame, NewRunSignal, Run, RunSignal};
use crate::repositories::insert_frames_idempotent;
use crate::schema::{frames, run_signals, runs};

/// Task queue every simulation activity runs on.
pub const SIMULATION_QUEUE: &str = "default";

/// Signal name that stops a run at the next batch boundary (**AC-33**).
pub const CANCEL_SIGNAL: &str = "cancel";

/// Signal name that changes simulation parameters mid-run (**AC-34**).
pub const STEER_SIGNAL: &str = "steer";

/// Everything the workflow needs to drive a run, and nothing else.
///
/// Note what is *absent*: no parameters, no seed, no state. Those live in the
/// `runs` row the activity loads, so editing this struct never widens what
/// workflow history has to carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationInput {
    /// The `runs` row this workflow drives.
    pub run_id: i64,
    /// Hard tick budget. The workflow never issues more than
    /// [`planned_batches`] batches, so a stalled cursor cannot loop forever.
    pub max_ticks: u32,
    /// Ticks simulated per `simulate_batch` call — the checkpoint interval.
    pub batch_ticks: u32,
    /// Stride at which frames get computed metrics; `0` means none.
    pub metrics_every: u32,
}

/// What one `simulate_batch` call hands back: a cursor, and nothing that scales
/// with the agent count.
///
/// Every field here is `Copy`-sized or a fixed-width string. That is the whole
/// contract — see the module docs for why it is load-bearing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchCursor {
    /// The run this cursor belongs to; echoed back so a misrouted result is
    /// detectable rather than silently applied to the wrong run.
    pub run_id: i64,
    /// The tick the next batch must start from — also the tick whose frame is
    /// the checkpoint the next batch resumes out of.
    pub next_tick: u32,
    /// Whether the run reached its own tick budget inside this batch.
    pub done: bool,
    /// Canonical hash of the flock at `next_tick`, 16 lowercase hex digits.
    pub state_hash: String,
    /// Frames this call actually inserted. `0` on a duplicate delivery, which is
    /// how idempotency shows up in the cursor (**AC-32**).
    pub frames_written: usize,
}

/// How many batches a run of `max_ticks` takes at `batch_ticks` per batch.
///
/// This is the **budget guardrail** of **AC-35**: the workflow's batch loop is
/// bounded by this number, so a `simulate_batch` that stops advancing its cursor
/// terminates the run deterministically instead of spinning forever.
///
/// `batch_ticks == 0` yields `0` rather than panicking on the division — a
/// degenerate input must fail the run, not the worker.
#[must_use]
pub const fn planned_batches(max_ticks: u32, batch_ticks: u32) -> u32 {
    if batch_ticks == 0 {
        0
    } else {
        max_ticks.div_ceil(batch_ticks)
    }
}

/// The simulation parameters a `steer` signal is allowed to change (**AC-34**).
///
/// Deliberately a closed list of **scalar tuning knobs**, and deliberately not
/// `agent_count`, `world` or `backend`. Those three define what a frame *is*: a
/// run whose flock changed size at tick 300 has two incompatible halves, and
/// every stored frame before the change becomes uncomparable with every frame
/// after it. Steering is meant to explore the parameter space of one experiment,
/// not to start a different experiment halfway through.
///
/// Sorted, because it is also the iteration order of the extracted overrides.
pub const STEERABLE: [&str; 9] = [
    "max_force",
    "max_speed",
    "neighbor_radius",
    "separation_radius",
    "w_alignment",
    "w_avoidance",
    "w_cohesion",
    "w_goal",
    "w_separation",
];

/// Extract the steerable parameters from a `steer` signal payload.
///
/// Unrecognised keys and non-numeric values are dropped rather than rejected: an
/// operator's typo must not fail a running experiment, and the raw payload is
/// recorded to `run_signals` regardless, so the typo is still discoverable
/// afterwards.
///
/// Iteration is over [`STEERABLE`] — a fixed array — and the result is a
/// `BTreeMap`, so the extraction is order-deterministic no matter how the JSON
/// object was built. A `HashMap` here would be a replay hazard, and inside a
/// `#[workflow]` body it is a compile error (HVG011).
#[must_use]
pub fn steer_overrides(payload: &Value) -> BTreeMap<String, f64> {
    let mut overrides = BTreeMap::new();
    for field in STEERABLE {
        if let Some(value) = payload.get(field).and_then(Value::as_f64) {
            overrides.insert(field.to_owned(), value);
        }
    }
    overrides
}

/// Apply extracted overrides onto a parameter set, in place.
///
/// The inverse of [`steer_overrides`]: every key it can produce is handled here,
/// and nothing else is touched. Keeping the two functions adjacent is the point
/// — a knob added to [`STEERABLE`] without a match arm here would be silently
/// accepted and silently ignored.
pub fn apply_overrides(params: &mut SimParams, overrides: &BTreeMap<String, f64>) {
    for (field, value) in overrides {
        match field.as_str() {
            "max_force" => params.max_force = *value,
            "max_speed" => params.max_speed = *value,
            "neighbor_radius" => params.neighbor_radius = *value,
            "separation_radius" => params.separation_radius = *value,
            "w_alignment" => params.w_alignment = *value,
            "w_avoidance" => params.w_avoidance = *value,
            "w_cohesion" => params.w_cohesion = *value,
            "w_goal" => params.w_goal = *value,
            "w_separation" => params.w_separation = *value,
            _ => {}
        }
    }
}

/// Write one operator intervention to `run_signals`, so the run can explain its
/// own behaviour afterwards (**AC-34**).
///
/// Factored out of the workflow body because cancel and steer record the same
/// shape and only the `kind` differs — and because a signal that changed a run's
/// behaviour but left no trace is the failure mode this activity exists to
/// prevent.
async fn record_signal_for(
    ctx: &WorkflowContext,
    run_id: i64,
    tick: u32,
    signal_kind: &str,
    payload: Value,
) -> HarvestResult<()> {
    ctx.execute_activity_raw(
        "record_signal",
        serde_json::json!({
            "run_id": run_id,
            "tick": tick,
            "kind": signal_kind,
            "payload": payload,
        }),
        SIMULATION_QUEUE,
    )
    .await?;
    Ok(())
}

/// Drive one run to a terminal state, one checkpointed batch at a time.
///
/// The loop is deliberately dull: ask `simulate_batch` to advance from the
/// current cursor, take the new cursor, repeat. All the state — parameters,
/// flock, frames — is the activity's business.
///
/// # Errors
///
/// Returns an error if `batch_ticks` is zero (a run that could never advance) or
/// if an activity exhausts its retries.
#[workflow]
pub async fn simulation_workflow(
    ctx: &WorkflowContext,
    input: SimulationInput,
) -> HarvestResult<Value> {
    let planned = planned_batches(input.max_ticks, input.batch_ticks);
    if planned == 0 {
        return Err(HarvestError::Config(format!(
            "run {} cannot advance: batch_ticks={} max_ticks={}",
            input.run_id, input.batch_ticks, input.max_ticks
        )));
    }

    // Accumulated steer overrides. A `BTreeMap` and not a `HashMap`: iterating
    // a hash container inside a workflow body is a compile error (HVG011), and
    // the ordering is what makes the activity input byte-identical on replay.
    let mut overrides: BTreeMap<String, f64> = BTreeMap::new();
    let mut next_tick: u32 = 0;
    let mut batches: u32 = 0;
    let mut state_hash = String::new();
    // Only a loop that exhausts its batch budget without the run ever reaching
    // `max_ticks` leaves this untouched — and that is exactly the guardrail case.
    let mut terminal = status::BUDGET_EXCEEDED;

    for _ in 0..planned {
        // An engine-level cancel is a *kill*, not a graceful stop, and it is
        // deliberately handled differently from the `cancel` signal below.
        //
        // Harvest records cancellation as a `WorkflowCancelled` event that has
        // no workflow-command counterpart and is never consumed by the replay
        // matcher. Any command issued past it — including a tidy-up
        // `finalize_run` — therefore lands on that event and is reported as
        // non-determinism on the next replay. So the only correct thing to do
        // here is stop issuing commands and surface the cancellation; Harvest
        // marks the execution cancelled from its own history.
        //
        // The graceful path, and the one the UI's Cancel button uses, is the
        // `cancel` **signal** at the batch boundary below: it finishes the batch
        // in flight, records its own provenance, and finalizes the run row.
        if ctx.is_cancelled() {
            return Err(HarvestError::Cancelled(
                ctx.cancellation_reason()
                    .unwrap_or("cancelled by the engine")
                    .to_owned(),
            ));
        }

        // A batch that exhausts its retries must close the run out before the
        // workflow gives up. Without this the run row is left at `running`
        // forever: nothing else in the application writes `status::FAILED`, so
        // every open tab polls `/runs/{id}/progress` every two seconds for a run
        // that will never move again, and the operator has no way to clear it.
        //
        // # Why this is replay-safe where the engine-cancel path above is not
        //
        // The two look symmetrical and are not. An **`ActivityFailed` event is
        // consumed by the replay matcher** — it is the recorded outcome of a
        // command the workflow itself issued — so the next command after it,
        // this `finalize_run`, lands on a fresh position in history and replays
        // identically forever. A `WorkflowCancelled` event has no command
        // counterpart and is *not* consumed, so any command issued past it lands
        // on that event and is reported as non-determinism (see the comment at
        // the top of this loop). **Do not unify the two paths.**
        //
        // `finalize_run_core` is terminal-safe, so this races nothing: a run
        // already finalized as `cancelled` or `completed` is returned unchanged.
        let raw = match ctx
            .execute_activity_raw(
                "simulate_batch",
                serde_json::json!({
                    "run_id": input.run_id,
                    "from_tick": next_tick,
                    "batch_ticks": input.batch_ticks,
                    "metrics_every": input.metrics_every,
                    "overrides": overrides,
                }),
                SIMULATION_QUEUE,
            )
            .await
        {
            Ok(value) => value,
            Err(e) => {
                // `next_tick` and `state_hash` still hold the last checkpoint the
                // run genuinely reached, so the frames its completed batches
                // wrote stay owned by the row rather than being disclaimed.
                ctx.execute_activity_raw(
                    "finalize_run",
                    serde_json::json!({
                        "run_id": input.run_id,
                        "status": status::FAILED,
                        "ticks_completed": next_tick,
                        "final_state_hash": state_hash,
                        "error": e.to_string(),
                    }),
                    SIMULATION_QUEUE,
                )
                .await?;
                return Err(e);
            }
        };

        let cursor: BatchCursor = serde_json::from_value(raw)?;

        // A cursor naming another run means a message was misrouted. Applying it
        // would advance this run using a different run's checkpoint — silently,
        // and with no way to tell afterwards which frames came from where. Fail
        // the run instead; a stopped run is recoverable, a corrupted one is not.
        if cursor.run_id != input.run_id {
            return Err(HarvestError::Config(format!(
                "simulate_batch returned a cursor for run {} while driving run {}",
                cursor.run_id, input.run_id
            )));
        }

        batches += 1;
        next_tick = cursor.next_tick;
        state_hash = cursor.state_hash;

        if cursor.done || next_tick >= input.max_ticks {
            terminal = status::COMPLETED;
            break;
        }

        // ── The batch boundary ────────────────────────────────────────────
        //
        // Operator signals are read *here*, between batches, and nowhere else.
        // That is the whole of AC-33's "at the next batch boundary": a batch
        // already dispatched runs to completion and its frames are kept, so a
        // cancel costs at most one batch of work and never loses one.
        //
        // `try_wait_for_signal` is the non-blocking claim — it consumes a
        // buffered signal if one is there and returns immediately if not, so
        // the loop never parks waiting for an operator who is not coming.
        if let Some(payload) = ctx.try_wait_for_signal(CANCEL_SIGNAL)? {
            record_signal_for(ctx, input.run_id, next_tick, kind::CANCEL, payload).await?;
            terminal = status::CANCELLED;
            break;
        }

        // One steer per boundary, oldest first, rather than draining the whole
        // backlog at once. A steer is an operator *intervention*, and each one
        // gets its own `run_signals` row tied to the exact tick from which it
        // took effect — collapsing a burst into a single parameter change would
        // throw away precisely the correspondence that lets a run explain its
        // own trajectory afterwards (AC-34).
        if let Some(payload) = ctx.try_wait_for_signal(STEER_SIGNAL)? {
            overrides.append(&mut steer_overrides(&payload));
            record_signal_for(ctx, input.run_id, next_tick, kind::STEER, payload).await?;
        }
    }

    ctx.execute_activity_raw(
        "finalize_run",
        serde_json::json!({
            "run_id": input.run_id,
            "status": terminal,
            "ticks_completed": next_tick,
            "final_state_hash": state_hash,
        }),
        SIMULATION_QUEUE,
    )
    .await?;

    Ok(serde_json::json!({
        "run_id": input.run_id,
        "status": terminal,
        "ticks_completed": next_tick,
        "batches": batches,
        "final_state_hash": state_hash,
        // `ctx.now()`, never `Utc::now()`: under `WorkflowTestEnv` this is the
        // virtual clock, and in production it is the recorded one — either way
        // replaying the history reproduces the same instant (AC-30, AC-31).
        // Truncated to whole seconds so the field has a fixed width, which is
        // what lets AC-29 compare two histories byte for byte.
        "finished_at": ctx.now().to_rfc3339_opts(SecondsFormat::Secs, true),
    }))
}

// ═══════════════════════ Activity cores ═══════════════════════
//
// Each activity is a thin shell around a `_core` function that takes an
// `&mut AsyncPgConnection` explicitly. That split is deliberate: the shell's
// only job is to find a connection in the activity context, which cannot be
// unit tested without a worker, while the core is an ordinary async function
// that a test can call directly against the live database — twice in a row, if
// that is what proving idempotency takes.

/// What one `simulate_batch` invocation is asked to do.
///
/// The whole request, note, is five scalars and a small map of tuning knobs.
/// Nothing here scales with the flock; see the module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchRequest {
    /// The run to advance.
    pub run_id: i64,
    /// The tick to resume from. **This, not the database, decides where the
    /// batch starts** — which is what makes a duplicate delivery recompute the
    /// identical frames instead of running the next batch twice (**AC-32**).
    pub from_tick: u32,
    /// How many ticks to simulate, clamped to the run's remaining budget.
    pub batch_ticks: u32,
    /// Stride at which frames carry computed metrics; `0` means none.
    pub metrics_every: u32,
    /// Accumulated steer overrides (**AC-34**).
    #[serde(default)]
    pub overrides: BTreeMap<String, f64>,
}

/// Advance one run by one batch, checkpointing every tick.
///
/// # Why a frame per tick
///
/// `metrics_every` subsamples *metrics*, not *frames*. Every simulated tick gets
/// a row, so the stored tick sequence is contiguous and
/// [`tick_gaps`](crate::repositories::tick_gaps) means what its name says: a gap
/// is a lost batch, not a sampling stride. A contiguous sequence is also what
/// lets a resumed run restart from any tick and what lets the run detail view
/// scrub — with [`frames_for_run`](crate::repositories::frames_for_run) doing the
/// subsampling at read time, where the rendering budget actually applies.
///
/// Metrics for an unsampled tick are stored as JSON `null` rather than omitted:
/// they are a pure function of the frame, so they can always be recomputed, and
/// a run with 10 000 agents should not pay for an O(N²) nearest-neighbour scan on
/// every tick.
///
/// # Why the ticks are simulated one at a time
///
/// Through `run_batch(state, params, 1, metrics_every)` rather than one
/// `run_batch(.., batch_ticks, ..)` call, because a single call returns only the
/// *final* state and there would be no intermediate flock to checkpoint. Going
/// through `run_batch` even for one tick keeps the "sample on the absolute tick
/// number" rule in the kernel, where AC-20 tests it, instead of re-deriving it
/// here where it would silently drift.
///
/// # Errors
///
/// Returns an error if the run does not exist, if its `config_snapshot` is not a
/// valid [`SimParams`], if the checkpoint frame `from_tick` names is missing, or
/// if a database operation fails.
pub async fn simulate_batch_core(
    conn: &mut AsyncPgConnection,
    request: &BatchRequest,
) -> AutumnResult<BatchCursor> {
    let run: Run = runs::table
        .filter(runs::id.eq(request.run_id))
        .select(Run::as_select())
        .first(conn)
        .await?;

    // The run's own configuration, untouched. Seeding uses *these* parameters
    // even when a steer is in force: the initial flock is part of the run's
    // identity, so a resumed batch must re-seed the same one a fresh run would.
    let base_params: SimParams = serde_json::from_value(run.config_snapshot.clone())?;
    let mut params = base_params.clone();
    apply_overrides(&mut params, &request.overrides);
    if let Err(problems) = validate(&params) {
        return Err(AutumnError::unprocessable_msg(format!(
            "run {} has invalid parameters: {}",
            run.id,
            problems.join("; ")
        )));
    }

    let mut state = load_checkpoint(conn, &run, &base_params, request.from_tick).await?;
    let mut frames = Vec::new();

    // Tick 0 has no producing batch, so the seed state would otherwise never be
    // checkpointed and the sequence would start at 1.
    if request.from_tick == 0 {
        let metrics = (request.metrics_every > 0).then(|| frame_metrics(&state.agents, &params));
        frames.push(checkpoint_frame(run.id, &state, metrics)?);
    }

    let budget = u32::try_from(run.max_ticks).unwrap_or(0);
    let ticks = request
        .batch_ticks
        .min(budget.saturating_sub(request.from_tick));
    for _ in 0..ticks {
        let (next, sampled) = run_batch(&state, &params, 1, request.metrics_every);
        state = next;
        // Indexing rather than `Vec::first()`: with `diesel_async::RunQueryDsl`
        // in scope the inherent slice method loses to the trait's resolution.
        let metrics = if sampled.is_empty() {
            None
        } else {
            Some(sampled[0].1)
        };
        frames.push(checkpoint_frame(run.id, &state, metrics)?);
    }

    let frames_written = insert_frames_idempotent(conn, run.id, &frames).await?;
    let next_tick = state.tick;
    let completed = i32::try_from(next_tick).unwrap_or(i32::MAX);

    // `ticks_completed` only ever moves forward, and only while the run is still
    // live: a retried older batch must not rewind the counter, and a batch that
    // lands after a cancel must not resurrect the run.
    diesel::update(
        runs::table
            .filter(runs::id.eq(run.id))
            .filter(runs::ticks_completed.le(completed))
            .filter(runs::status.eq_any(vec![status::QUEUED, status::RUNNING])),
    )
    .set((
        runs::ticks_completed.eq(completed),
        runs::status.eq(status::RUNNING),
    ))
    .execute(conn)
    .await?;

    Ok(BatchCursor {
        run_id: run.id,
        next_tick,
        done: next_tick >= budget,
        state_hash: state.state_hash_hex(),
        frames_written,
    })
}

/// Load the flock a batch starts from: the stored checkpoint, or the seed.
///
/// `from_tick` is the authority, not "the latest frame in the table". Resuming
/// from the highest stored tick would make a retried delivery run the *next*
/// batch rather than repeat its own, which is precisely the bug the
/// `UNIQUE (run_id, tick)` constraint could not save us from.
async fn load_checkpoint(
    conn: &mut AsyncPgConnection,
    run: &Run,
    base_params: &SimParams,
    from_tick: u32,
) -> AutumnResult<SimState> {
    if from_tick == 0 {
        return Ok(SimState::seeded(base_params, run.seed.cast_unsigned()));
    }

    let tick = i32::try_from(from_tick)
        .map_err(|_| AutumnError::unprocessable_msg(format!("tick {from_tick} is out of range")))?;
    let stored: Value = frames::table
        .filter(frames::run_id.eq(run.id))
        .filter(frames::tick.eq(tick))
        .select(frames::agents)
        .first(conn)
        .await
        .optional()?
        .ok_or_else(|| {
            AutumnError::unprocessable_msg(format!(
                "run {} has no checkpoint at tick {from_tick} to resume from",
                run.id
            ))
        })?;

    // Rebuilt through `SimState`'s own wire form, never through `Vec<Agent>`:
    // see `checkpoint_frame` for why that distinction is load-bearing.
    Ok(serde_json::from_value(
        serde_json::json!({ "tick": from_tick, "agents": stored }),
    )?)
}

/// Render one flock as a persistable frame.
///
/// # The agents are serialized through `SimState`, deliberately
///
/// `Agent`'s own `Serialize` writes plain JSON numbers, and `serde_json` parses
/// floats with a fast best-effort algorithm that can land one ULP away from what
/// was written. One ULP changes the canonical state hash, which would make every
/// resumed run look like a divergence. `SimState` exists precisely to avoid that
/// — it serializes each coordinate as an exact decimal string — so the frame's
/// `agents` column stores *its* array, not a directly-serialized `Vec<Agent>`.
/// This is what makes **AC-36** true rather than usually-true.
fn checkpoint_frame(
    run_id: i64,
    state: &SimState,
    metrics: Option<FrameMetrics>,
) -> AutumnResult<NewFrame> {
    let mut wire = serde_json::to_value(state)?;
    let agents = wire
        .get_mut("agents")
        .map(Value::take)
        .ok_or_else(|| AutumnError::unprocessable_msg("serialized SimState has no agents"))?;

    Ok(NewFrame {
        run_id,
        tick: i32::try_from(state.tick)
            .map_err(|_| AutumnError::unprocessable_msg("tick is out of range"))?,
        agents,
        state_hash: state.state_hash_hex(),
        metrics: match metrics {
            Some(metrics) => serde_json::to_value(metrics)?,
            None => Value::Null,
        },
    })
}

/// What one `finalize_run` invocation is asked to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalizeRequest {
    /// The run to close out.
    pub run_id: i64,
    /// The terminal status to record; must be one
    /// [`is_terminal`](crate::models::run::status::is_terminal) accepts.
    pub status: String,
    /// Ticks the run actually completed. Never moves backwards.
    pub ticks_completed: u32,
    /// Hash of the last checkpoint the run reached; empty for a run that never
    /// completed a batch.
    pub final_state_hash: String,
    /// Why the run failed, when it did.
    #[serde(default)]
    pub error: Option<String>,
}

/// One operator intervention, as `record_signal` receives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalRecord {
    /// The run the intervention applies to.
    pub run_id: i64,
    /// The tick from which it took effect.
    pub tick: u32,
    /// [`STEER`](crate::models::run_signal::kind::STEER) or
    /// [`CANCEL`](crate::models::run_signal::kind::CANCEL).
    pub kind: String,
    /// The payload exactly as the operator sent it.
    pub payload: Value,
}

/// Close a run out in its terminal state, recording the provenance a completed
/// run is required to carry (**AC-38**).
///
/// **Idempotent, and terminal-safe.** A run that is already terminal is returned
/// unchanged rather than re-finalized: at-least-once delivery means this can
/// arrive twice, and the second delivery must not overwrite a `cancelled` run
/// with `completed` — or vice versa — depending on which message won the race.
/// `ticks_completed` only ever moves forward for the same reason.
///
/// # Errors
///
/// Returns an error if `status` is not terminal, if the run does not exist, or
/// if the update fails.
pub async fn finalize_run_core(
    conn: &mut AsyncPgConnection,
    request: &FinalizeRequest,
) -> AutumnResult<Run> {
    if !status::is_terminal(&request.status) {
        return Err(AutumnError::unprocessable_msg(format!(
            "'{}' is not a terminal status; a run cannot be finalized into it",
            request.status
        )));
    }

    let run: Run = runs::table
        .filter(runs::id.eq(request.run_id))
        .select(Run::as_select())
        .first(conn)
        .await?;
    if status::is_terminal(&run.status) {
        return Ok(run);
    }

    let completed = i32::try_from(request.ticks_completed)
        .unwrap_or(i32::MAX)
        .max(run.ticks_completed);
    // An empty hash means "no batch ever completed", which is an absent
    // provenance record, not a hash of the empty string.
    let final_state_hash =
        (!request.final_state_hash.is_empty()).then(|| request.final_state_hash.clone());

    Ok(
        diesel::update(runs::table.filter(runs::id.eq(request.run_id)))
            .set((
                runs::status.eq(&request.status),
                runs::ticks_completed.eq(completed),
                runs::final_state_hash.eq(final_state_hash),
                runs::error.eq(request.error.clone()),
            ))
            .returning(Run::as_returning())
            .get_result(conn)
            .await?,
    )
}

/// Record one operator intervention against a run (**AC-34**).
///
/// **Idempotent.** An identical `(run_id, tick, kind, payload)` is *one*
/// intervention however many times an at-least-once activity delivers it, so a
/// matching row is reused rather than duplicated. There is no unique constraint
/// to lean on here — unlike frames, two genuinely distinct steers can share a
/// tick — so the check is a lookup, made exact by Postgres's semantic `jsonb`
/// equality rather than by string comparison of the serialized payload.
///
/// Returns the id of the row that now represents the intervention.
///
/// # Errors
///
/// Returns an error if `tick` is out of range or if a database operation fails.
pub async fn record_signal_core(
    conn: &mut AsyncPgConnection,
    record: &SignalRecord,
) -> AutumnResult<i64> {
    let tick = i32::try_from(record.tick).map_err(|_| {
        AutumnError::unprocessable_msg(format!("tick {} is out of range", record.tick))
    })?;

    let existing: Option<i64> = run_signals::table
        .filter(run_signals::run_id.eq(record.run_id))
        .filter(run_signals::tick.eq(tick))
        .filter(run_signals::kind.eq(&record.kind))
        .filter(run_signals::payload.eq(&record.payload))
        .select(run_signals::id)
        .first(conn)
        .await
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }

    let inserted: RunSignal = diesel::insert_into(run_signals::table)
        .values(&NewRunSignal {
            run_id: record.run_id,
            tick,
            kind: record.kind.clone(),
            payload: record.payload.clone(),
        })
        .returning(RunSignal::as_returning())
        .get_result(conn)
        .await?;
    Ok(inserted.id)
}

// ═══════════════════════ Failure classification ═══════════════════════

/// Stable, low-cardinality names for the ways an activity here can fail.
///
/// These are **classes, not messages**. Harvest stamps `error_type` onto the
/// `ActivityFailed` event, uses it as the `error.type` dimension on
/// `harvest.activity.duration` / `harvest.activity.failed`, and matches
/// [`RetryPolicy::non_retryable_errors`] against it. A hand-formatted string
/// would drift every time a message is reworded, silently changing both a
/// dashboard and a retry policy — which is exactly what happened when every
/// failure here was reported as `Database`.
pub mod error_type {
    /// The request can never succeed as sent: parameters that fail validation,
    /// a checkpoint that does not exist, a tick out of range, a non-terminal
    /// status handed to the finalizer. Permanent, so retries are skipped.
    pub const INVALID_CONFIG: &str = "InvalidConfig";
    /// Everything else these activities can hit: a pool timeout, a dropped
    /// connection, a transient Postgres error. Transient, so the activity's own
    /// retry policy applies.
    pub const DATABASE: &str = "Database";
    /// The worker was built with no application database pool. Permanent within
    /// the process: a worker that started without a pool will not grow one.
    pub const NO_DATABASE_POOL: &str = "NoDatabasePool";
    /// The activity input did not deserialize, or its output did not serialize.
    /// Permanent: the same bytes fail the same way on every attempt.
    pub const INVALID_INPUT: &str = "InvalidInput";
}

/// Turn a `_core` failure into Harvest's typed failure surface.
///
/// **The `_core`/shell split is the classification point.** Every core returns
/// [`AutumnError`], which already carries an HTTP status, and the cores use that
/// status deliberately: `unprocessable_msg` (422) is how they say "this request
/// is wrong", and everything else is a database fault. So the mapping is one
/// line of policy — **422 is permanent, anything else is transient** — rather
/// than a growing list of string matches.
///
/// Permanent failures come back `non_retryable`, which makes the worker skip the
/// remaining attempts on the spot. That matters: `simulate_batch` is retried
/// three times with exponential backoff, so an invalid config used to take four
/// worker slots and several seconds of backoff to report a verdict that was
/// already final on the first attempt.
#[must_use]
pub fn classify_activity_error(error: &AutumnError) -> ActivityFailure {
    let message = error.to_string();
    if error.status() == autumn_web::reexports::http::StatusCode::UNPROCESSABLE_ENTITY {
        ActivityFailure::non_retryable(error_type::INVALID_CONFIG, message)
    } else {
        ActivityFailure::retryable(error_type::DATABASE, message)
    }
}

/// A malformed activity payload: permanent, because the same bytes fail
/// identically on every attempt.
fn invalid_payload(what: &str, error: &serde_json::Error) -> ActivityFailure {
    ActivityFailure::non_retryable(error_type::INVALID_INPUT, format!("{what}: {error}"))
}

// ═══════════════════════ Activities ═══════════════════════
//
// Each of these is a shell: find a connection, delegate to the matching `_core`
// function, serialize the answer. Nothing here has behaviour of its own, which
// is the point — behaviour that a worker has to be running to reach is
// behaviour that cannot be tested.
//
// Each returns `Result<Value, ActivityFailure>` rather than `HarvestResult<_>`:
// the `#[activity]` macro recognises that return type syntactically and routes
// the error through the typed encoding, so `error_type` and `non_retryable`
// survive into workflow history and into metrics.

/// Check out a connection to the **application** database inside an activity.
///
/// `AppDbPool` is injected into activity state by `HarvestPlugin` from the
/// Autumn app's own pool — there is nothing to register, and no
/// `.state::<Pool>()` call to remember. In split deployments it stays bound to
/// the business database rather than Harvest's system storage, which is exactly
/// what these activities want: they touch `runs`, `frames` and `run_signals`,
/// never the queue tables.
async fn app_conn(ctx: &ActivityContext) -> Result<PooledConnection, ActivityFailure> {
    let pool = ctx.state::<AppDbPool>().ok_or_else(|| {
        // Permanent *within this process*: a worker that booted without a pool
        // will not acquire one between retries, so backing off five times only
        // delays the same answer.
        ActivityFailure::non_retryable(
            error_type::NO_DATABASE_POOL,
            "no AppDbPool in activity state — the Harvest plugin was built without an \
             application database pool",
        )
    })?;
    pool.get().await.map_err(|e| {
        ActivityFailure::retryable(
            error_type::DATABASE,
            format!("checking out a connection: {e}"),
        )
    })
}

/// Advance one run by one checkpointed batch (**AC-29**, **AC-32**).
///
/// `start_to_close` is generous because a batch is CPU-bound simulation whose
/// cost scales with `batch_ticks * agent_count²`, and the retry policy is
/// deliberately ordinary: this activity is idempotent by construction, so
/// retrying it is free.
///
/// # Errors
///
/// Returns an error if the input is not a [`BatchRequest`], if no application
/// database pool is available, or if the batch itself fails.
#[activity(
    start_to_close = "300s",
    retry = RetryPolicy::exponential(3, std::time::Duration::from_secs(1))
)]
pub async fn simulate_batch(ctx: &ActivityContext, input: Value) -> Result<Value, ActivityFailure> {
    let request: BatchRequest =
        serde_json::from_value(input).map_err(|e| invalid_payload("simulate_batch input", &e))?;
    let mut conn = app_conn(ctx).await?;
    let cursor = simulate_batch_core(&mut conn, &request)
        .await
        .map_err(|e| classify_activity_error(&e))?;
    serde_json::to_value(cursor).map_err(|e| invalid_payload("simulate_batch cursor", &e))
}

/// Close a run out in its terminal state (**AC-33**, **AC-35**, **AC-38**).
///
/// # Errors
///
/// Returns an error if the input is not a [`FinalizeRequest`], if no application
/// database pool is available, or if the update fails.
#[activity(
    start_to_close = "60s",
    retry = RetryPolicy::exponential(5, std::time::Duration::from_secs(1))
)]
pub async fn finalize_run(ctx: &ActivityContext, input: Value) -> Result<Value, ActivityFailure> {
    let request: FinalizeRequest =
        serde_json::from_value(input).map_err(|e| invalid_payload("finalize_run input", &e))?;
    let mut conn = app_conn(ctx).await?;
    let run = finalize_run_core(&mut conn, &request)
        .await
        .map_err(|e| classify_activity_error(&e))?;
    Ok(serde_json::json!({
        "run_id": run.id,
        "status": run.status,
        "ticks_completed": run.ticks_completed,
        "final_state_hash": run.final_state_hash,
    }))
}

/// Record one operator intervention for provenance (**AC-34**).
///
/// # Errors
///
/// Returns an error if the input is not a [`SignalRecord`], if no application
/// database pool is available, or if the write fails.
#[activity(
    start_to_close = "30s",
    retry = RetryPolicy::exponential(5, std::time::Duration::from_secs(1))
)]
pub async fn record_signal(ctx: &ActivityContext, input: Value) -> Result<Value, ActivityFailure> {
    let record: SignalRecord =
        serde_json::from_value(input).map_err(|e| invalid_payload("record_signal input", &e))?;
    let mut conn = app_conn(ctx).await?;
    let signal_id = record_signal_core(&mut conn, &record)
        .await
        .map_err(|e| classify_activity_error(&e))?;
    Ok(serde_json::json!({ "run_signal_id": signal_id }))
}
