//! The stranded-run reconciler.
//!
//! # What this is a backstop for
//!
//! `simulation_workflow` finalizes its own run on every path it survives long
//! enough to react to: normal completion, the budget guardrail, a `cancel`
//! signal, and — since the failure finalizer — an activity that exhausts its
//! retries. All four are **in-band**, and in-band is where a run should be
//! closed out, because the workflow is the only thing that knows the cursor it
//! reached.
//!
//! There is a second class of ending the workflow cannot see at all:
//!
//! * the worker is `SIGKILL`ed or OOM-killed mid-batch,
//! * an operator terminates the execution through the engine,
//! * the execution is cancelled at engine level (which, deliberately, issues no
//!   further commands — see the replay comment in `simulation_workflow`),
//! * history hits a cap, or a replay fails and the execution is failed for it.
//!
//! In every one of those the *execution* reaches a terminal state and the `runs`
//! row does not. The row then claims `running` forever, `runs.error` stays NULL,
//! and every open tab polls `/runs/{id}/progress` every two seconds for a run
//! that will never move again — with no way to clear it through the UI.
//!
//! # Shape
//!
//! Three separable pieces, for the same reason the activities are split into a
//! `_core` and a shell:
//!
//! 1. [`stale_non_terminal_runs`] — *find*. One indexed query; the cutoff is a
//!    parameter, so a test picks the clock instead of waiting for one.
//! 2. [`finalize_for`] — *decide*. **Pure.** Every branch of the policy is a
//!    plain unit test with a hand-built [`WorkflowResult`] and no engine.
//! 3. [`reconcile_one`] — *apply*. Delegates to
//!    [`finalize_run_core`](crate::workflow::finalize_run_core), whose terminal
//!    guard means this can never overwrite a run that finished normally.
//!
//! That guard is also what makes the reconciler compose with the in-band path
//! with no coordination at all, and what makes it safe to run on every replica:
//! whoever gets there first wins, and everyone else's write is a no-op.

use autumn_harvest::handle::{WorkflowHandleClient, WorkflowResult, WorkflowResultState};
use autumn_harvest::types::ExecutionId;
use autumn_web::AutumnResult;
use autumn_web::error::AutumnError;
use autumn_web::prelude::AppState;
use autumn_web::reexports::tracing;
use chrono::{NaiveDateTime, Utc};
// Deliberately not `diesel::prelude::*`: it pulls in the *synchronous*
// `RunQueryDsl`, which collides with `diesel_async`'s on every `.load` call.
use diesel::{ExpressionMethods as _, QueryDsl as _, SelectableHelper as _};
use diesel_async::{AsyncPgConnection, RunQueryDsl as _};
use serde_json::Value;

use crate::models::Run;
use crate::models::run::status;
use crate::schema::runs;
use crate::workflow::{FinalizeRequest, finalize_run_core};

/// How long a non-terminal run may go without its `updated_at` moving before
/// the sweep asks the engine about it.
///
/// The clock is free: the `runs_set_updated_at` trigger stamps
/// `clock_timestamp()` on every update, and `simulate_batch_core` updates
/// `ticks_completed` at the end of every batch — so `updated_at` is "when this
/// run last made progress" without the application maintaining anything.
///
/// Thirty minutes is chosen against the **worst-case quiet period of a healthy
/// run**, not as a round number: `simulate_batch` has a 300 s `start_to_close`
/// and retries three times with exponential backoff, so a batch that is merely
/// slow and unlucky can legitimately leave the row untouched for a little over
/// twenty minutes. Sweeping sooner would not *misjudge* such a run — a live
/// execution answers `Running` and [`finalize_for`] declines — but it would
/// spend engine queries on runs that are fine, and it would shorten the grace
/// period on the one branch that judges on staleness alone: a run that names no
/// execution at all.
pub const STALE_AFTER: chrono::TimeDelta = chrono::TimeDelta::minutes(30);

/// How many stranded runs one sweep will close out.
///
/// A bound, not a target. The reconciler is a repair mechanism, so a sweep that
/// finds hundreds of stale runs is reporting an outage, not doing routine work —
/// and it should not hold a connection for minutes while it walks all of them.
/// The remainder is picked up by the next sweep.
pub const SWEEP_LIMIT: i64 = 50;

/// Non-terminal runs that have not moved since `cutoff`, oldest first.
///
/// The cutoff is a **parameter** rather than being computed here, which is what
/// makes the query testable: a test picks a cutoff in the future to make its own
/// rows stale, and one in the past to prove a fresh run is left alone, instead
/// of sleeping for half an hour or fighting the `updated_at` trigger.
///
/// Oldest first because that is the order in which runs became stranded, and a
/// truncated sweep should clear the longest-standing damage.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn stale_non_terminal_runs(
    conn: &mut AsyncPgConnection,
    cutoff: NaiveDateTime,
    limit: i64,
) -> AutumnResult<Vec<Run>> {
    Ok(runs::table
        .filter(runs::status.eq_any(vec![status::QUEUED, status::RUNNING]))
        .filter(runs::updated_at.lt(cutoff))
        .order(runs::updated_at.asc())
        .limit(limit)
        .select(Run::as_select())
        .load(conn)
        .await?)
}

/// What the durable engine has to say about one run's execution.
///
/// The three cases are genuinely different verdicts and collapsing any two of
/// them would be a bug:
///
/// * [`Snapshot`](Self::Snapshot) — the engine answered. Its answer decides.
/// * [`NoExecution`](Self::NoExecution) — there is nothing to ask about. The run
///   names no execution, or names one the engine has never heard of, so nothing
///   will ever drive it.
/// * [`Unavailable`](Self::Unavailable) — the engine could not be asked. **Not**
///   an answer: failing a healthy run because Harvest's storage blipped is far
///   worse than waiting for the next sweep.
#[derive(Debug, Clone, Copy)]
pub enum EngineView<'a> {
    /// The engine's compact result for this run's execution.
    Snapshot(&'a WorkflowResult),
    /// The run names no execution the engine knows about.
    NoExecution,
    /// The engine could not be consulted this sweep.
    Unavailable,
}

/// Decide how — or whether — to close a stale run out. **Pure.**
///
/// Returns `None` for every run the reconciler has no business touching: one
/// that is already terminal, one whose execution is still running, one whose
/// execution chained into a successor that is still driving the same row, and
/// one the engine could not be asked about.
///
/// # Why a completed execution is read from its own output
///
/// If the engine says `COMPLETED` while the row is still non-terminal, the
/// workflow ran to the end and its closing `finalize_run` did not land. The
/// workflow's **result payload** is then the authority on what it decided — it
/// carries the terminal status it chose, the tick it reached and the hash it
/// ended on. That distinction is load-bearing: a run stopped by the `max_ticks`
/// guardrail is a *successfully completed execution* whose run status is
/// `budget_exceeded`, and reading `COMPLETED` off the execution state alone
/// would quietly relabel it.
#[must_use]
pub fn finalize_for(run: &Run, view: EngineView<'_>) -> Option<FinalizeRequest> {
    if status::is_terminal(&run.status) {
        return None;
    }

    let reached = u32::try_from(run.ticks_completed).unwrap_or(0);
    let hash = run.final_state_hash.clone().unwrap_or_default();

    match view {
        EngineView::Unavailable => None,
        EngineView::NoExecution => Some(FinalizeRequest {
            run_id: run.id,
            status: status::FAILED.to_owned(),
            ticks_completed: reached,
            final_state_hash: hash,
            error: Some(format!(
                "run {} has no workflow execution the engine knows about, and has not \
                 moved since {}: nothing will ever simulate it",
                run.id, run.updated_at
            )),
        }),
        EngineView::Snapshot(result) => match result.state {
            // Still being worked on, or handed to a successor that is.
            WorkflowResultState::Running | WorkflowResultState::ContinuedAsNew => None,
            WorkflowResultState::Completed => {
                let output = result.output.as_ref();
                Some(FinalizeRequest {
                    run_id: run.id,
                    status: output
                        .and_then(|o| o.get("status"))
                        .and_then(Value::as_str)
                        .filter(|s| status::is_terminal(s))
                        .unwrap_or(status::COMPLETED)
                        .to_owned(),
                    ticks_completed: output
                        .and_then(|o| o.get("ticks_completed"))
                        .and_then(Value::as_u64)
                        .and_then(|t| u32::try_from(t).ok())
                        .unwrap_or(reached),
                    final_state_hash: output
                        .and_then(|o| o.get("final_state_hash"))
                        .and_then(Value::as_str)
                        .map_or(hash, ToOwned::to_owned),
                    // A completed execution is not an error, even when the run
                    // it closed out was cancelled or hit its budget.
                    error: Some(format!(
                        "reconciled: the workflow execution completed but run {} was \
                         never finalized",
                        run.id
                    )),
                })
            }
            WorkflowResultState::Cancelled => Some(FinalizeRequest {
                run_id: run.id,
                status: status::CANCELLED.to_owned(),
                ticks_completed: reached,
                final_state_hash: hash,
                error: Some(engine_reason(
                    run.id,
                    "was cancelled at engine level",
                    result.error.as_deref(),
                )),
            }),
            // Failed, timed out and terminated are all "this run stopped and did
            // not finish". `runs` deliberately has one status for that, with the
            // distinction carried in `error` where a human can read it — a status
            // enum that mirrored the engine's would make every UI filter a
            // translation table.
            WorkflowResultState::Failed => Some(FinalizeRequest {
                run_id: run.id,
                status: status::FAILED.to_owned(),
                ticks_completed: reached,
                final_state_hash: hash,
                error: Some(engine_reason(run.id, "failed", result.error.as_deref())),
            }),
            WorkflowResultState::TimedOut => Some(FinalizeRequest {
                run_id: run.id,
                status: status::FAILED.to_owned(),
                ticks_completed: reached,
                final_state_hash: hash,
                error: Some(engine_reason(
                    run.id,
                    "exceeded its workflow timeout",
                    result.error.as_deref(),
                )),
            }),
            WorkflowResultState::Terminated => Some(FinalizeRequest {
                run_id: run.id,
                status: status::FAILED.to_owned(),
                ticks_completed: reached,
                final_state_hash: hash,
                error: Some(engine_reason(
                    run.id,
                    "was terminated",
                    result.error.as_deref(),
                )),
            }),
        },
    }
}

/// One sentence a person can act on, keeping the engine's own words when it
/// supplied any.
///
/// `runs.error` is the only thing the detail page has to show, so "reconciled"
/// alone would be a worse answer than the NULL it replaces.
fn engine_reason(run_id: i64, what: &str, engine_error: Option<&str>) -> String {
    match engine_error {
        Some(reason) if !reason.trim().is_empty() => {
            format!("reconciled: the workflow execution driving run {run_id} {what}: {reason}")
        }
        _ => format!("reconciled: the workflow execution driving run {run_id} {what}"),
    }
}

/// Close one stale run out, if [`finalize_for`] says it should be.
///
/// Returns the updated row, or `None` when the run was left alone. **Idempotent
/// and safe to race**: the write goes through
/// [`finalize_run_core`](crate::workflow::finalize_run_core), whose terminal
/// guard returns an already-terminal run unchanged. Two replicas sweeping at the
/// same instant, or a sweep racing the workflow's own finalizer, therefore
/// converge on whichever verdict landed first rather than fighting.
///
/// # Errors
///
/// Returns an error if the update fails.
pub async fn reconcile_one(
    conn: &mut AsyncPgConnection,
    run: &Run,
    view: EngineView<'_>,
) -> AutumnResult<Option<Run>> {
    let Some(request) = finalize_for(run, view) else {
        return Ok(None);
    };
    let updated = finalize_run_core(conn, &request).await?;
    // The guard fired between the decision and the write — someone else got
    // there first, and their verdict stands.
    if updated.status != request.status {
        return Ok(None);
    }
    Ok(Some(updated))
}

/// Ask the engine what became of one run's execution.
///
/// Every failure mode is mapped to the *verdict it justifies*, never to a
/// default: an unparseable or unknown execution id means there is nothing to
/// drive the run ([`EngineView::NoExecution`]), while a storage error means the
/// question could not be put ([`EngineView::Unavailable`]) and the run must be
/// left alone until the next sweep.
async fn engine_view_of(client: &WorkflowHandleClient, run: &Run) -> Option<WorkflowResult> {
    let exec_id = run.workflow_execution_id.as_deref()?.parse().ok()?;
    let exec_id: ExecutionId = exec_id;
    match client.handle(exec_id).result_snapshot().await {
        Ok(result) => Some(result),
        Err(e) => {
            tracing::warn!(
                run_id = run.id,
                error = %e,
                "could not read the execution state for a stale run; leaving it for the \
                 next sweep"
            );
            None
        }
    }
}

/// Close out runs the workflow could not close out itself.
///
/// Every five minutes — six times per [`STALE_AFTER`] window, so a stranded run
/// is cleared within a few minutes of becoming eligible, while the steady-state
/// cost is one indexed query that returns nothing. (The interval is a literal in
/// the attribute because the macro needs one; a test asserts it stays shorter
/// than [`STALE_AFTER`], so the two cannot drift apart silently.)
///
/// Runs on the **fleet** rather than per replica — Autumn's scheduler elects one
/// runner per tick — because a sweep is repair work and doing it once is enough.
/// The idempotency of [`reconcile_one`] means a double election would be
/// harmless anyway; the coordination just saves the queries.
///
/// # Errors
///
/// Returns an error if the sweep query or a finalizing update fails. A single
/// run that cannot be read from the engine is logged and skipped, not fatal:
/// one unreachable execution must not stop the rest of the sweep.
#[autumn_web::scheduled(every = "5m", name = "reconcile_stranded_runs")]
pub async fn reconcile_stranded_runs(state: AppState) -> AutumnResult<()> {
    let Some(pool) = state.pool() else {
        // No database configured: nothing to reconcile, and nothing to shout
        // about — this is the shape of the route-only test harness.
        return Ok(());
    };
    let mut conn = pool.get().await.map_err(|e| {
        AutumnError::service_unavailable_msg(format!("reconciler could not get a connection: {e}"))
    })?;

    let cutoff = Utc::now().naive_utc() - STALE_AFTER;
    let stale = stale_non_terminal_runs(&mut conn, cutoff, SWEEP_LIMIT).await?;
    if stale.is_empty() {
        return Ok(());
    }

    // Without a Harvest client there is no engine to consult, so every run is
    // `Unavailable` and nothing is decided. That is the honest answer: a
    // deployment with no runtime mounted has runs nothing will simulate, but
    // this sweep cannot tell those apart from runs a worker elsewhere is driving.
    let Some(client) = state.extension::<WorkflowHandleClient>() else {
        tracing::warn!(
            stale = stale.len(),
            "found stale runs but no Harvest runtime is mounted to ask about them"
        );
        return Ok(());
    };

    let mut reconciled = 0_usize;
    for run in &stale {
        // `snapshot` is bound before the view so it outlives the borrow
        // `EngineView::Snapshot` takes of it.
        let snapshot = match run.workflow_execution_id.as_deref() {
            None => None,
            Some(_) => engine_view_of(&client, run).await,
        };
        let view = match (&snapshot, run.workflow_execution_id.as_deref()) {
            (Some(result), _) => EngineView::Snapshot(result),
            (None, None) => EngineView::NoExecution,
            // Distinguishing "unknown execution" from "storage error" is
            // `engine_view_of`'s job; both surface here as "leave it alone".
            (None, Some(_)) => EngineView::Unavailable,
        };
        match reconcile_one(&mut conn, run, view).await {
            Ok(Some(updated)) => {
                reconciled += 1;
                tracing::info!(
                    run_id = updated.id,
                    status = %updated.status,
                    "reconciled a stranded run"
                );
            }
            Ok(None) => {}
            Err(e) => tracing::error!(run_id = run.id, error = %e, "reconciling a stale run"),
        }
    }

    if reconciled > 0 {
        tracing::info!(reconciled, stale = stale.len(), "reconciler sweep finished");
    }
    Ok(())
}
