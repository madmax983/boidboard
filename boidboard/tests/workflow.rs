//! Workflow acceptance tests (AC-29 … AC-36).
//!
//! Two kinds of test live here, deliberately. The workflow tests run entirely
//! under `WorkflowTestEnv` — no Postgres, no Docker, mocked activities, virtual
//! clock (**AC-30**) — because that is what makes orchestration logic cheap to
//! assert. The activity tests run against the **live** test database, because an
//! idempotency or crash-resume claim asserted against a mock asserts nothing.
//!
//! The database tests isolate themselves with `TestApp::with_transactional_db`:
//! a single-connection pool already inside `begin_test_transaction`, rolled back
//! when the client drops. Nothing is committed, so the file passes twice
//! consecutively with no cleanup code (**AC-40**).

use std::collections::BTreeMap;

use autumn_harvest::event::WorkflowEvent;
use autumn_harvest::testing::{ReplayStatus, TestRunOutcome, WorkflowTestEnv};
use autumn_web::test::{TestApp, TestClient};
use boids_core::SimParams;
use boids_core::sim::SimState;
use boidboard::models::run::status as run_status;
use boidboard::models::run_signal::kind as signal_kind;
use boidboard::models::{NewRun, NewScenario, Run, RunSignal};
use boidboard::repositories::{
    PgRunRepository, PgScenarioRepository, RunRepository as _, ScenarioRepository as _,
    frames_for_run, max_tick, tick_gaps,
};
use boidboard::schema::{run_signals, runs};
use boidboard::workflow::{
    BatchCursor, BatchRequest, FinalizeRequest, SignalRecord, finalize_run_core,
    record_signal_core, simulate_batch_core, simulation_workflow_info,
};
use diesel::{ExpressionMethods as _, QueryDsl as _, SelectableHelper as _};
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl as _;
use diesel_async::pooled_connection::deadpool::Pool;
use serde_json::{Value, json};

/// The run every no-database workflow test drives. The value is arbitrary: the
/// workflow only ever passes it through to activities.
const RUN_ID: i64 = 4242;

/// Workflow input as the engine delivers it — plain JSON, exactly what a route
/// handler would post to `start_workflow`.
fn workflow_input(max_ticks: u32, batch_ticks: u32) -> Value {
    json!({
        "run_id": RUN_ID,
        "max_ticks": max_ticks,
        "batch_ticks": batch_ticks,
        "metrics_every": 10,
    })
}

/// Read a `u32` field out of an activity input.
fn read_u32(input: &Value, field: &str) -> u32 {
    input[field]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default()
}

/// A `simulate_batch` mock that really does hold the whole flock.
///
/// It seeds a genuine `agent_count`-strong `SimState` and returns that state's
/// genuine hash, so the agent array demonstrably passes through the activity on
/// every call. What comes back is a [`BatchCursor`] and nothing else — which is
/// exactly the claim AC-29 makes about the activity boundary.
fn flock_cursor_mock(
    agent_count: usize,
    max_ticks: u32,
) -> impl Fn(Value) -> Result<Value, String> + Send + Sync + 'static {
    move |input| {
        let from_tick = read_u32(&input, "from_tick");
        let batch_ticks = read_u32(&input, "batch_ticks");

        let params = SimParams {
            agent_count,
            ..SimParams::default()
        };
        let state = SimState::seeded(&params, 7);

        let next_tick = from_tick.saturating_add(batch_ticks).min(max_ticks);
        serde_json::to_value(BatchCursor {
            run_id: input["run_id"].as_i64().unwrap_or_default(),
            next_tick,
            done: next_tick >= max_ticks,
            state_hash: state.state_hash_hex(),
            frames_written: batch_ticks as usize,
        })
        .map_err(|e| e.to_string())
    }
}

/// Drive the workflow to completion against a flock of `agent_count` agents and
/// return the history it produced.
async fn cursor_history(agent_count: usize) -> Vec<WorkflowEvent> {
    let env = WorkflowTestEnv::new()
        .mock_activity("simulate_batch", flock_cursor_mock(agent_count, 2_000))
        .mock_activity("finalize_run", |_| Ok(json!({ "finalized": true })))
        .mock_activity("record_signal", |_| Ok(json!({ "recorded": true })));

    let outcome = env
        .run(
            simulation_workflow_info().handler,
            workflow_input(2_000, 100),
        )
        .await;

    assert!(
        outcome.result.is_ok(),
        "the workflow completes: {:?}",
        outcome.result
    );
    assert_replays(&outcome).await;
    outcome.events().to_vec()
}

/// Serialized size of everything the workflow itself put into history.
///
/// `WorkflowStarted` is excluded for one mechanical reason only: it carries a
/// `DateTime<Utc>` whose serialized fractional-second digits vary run to run, so
/// including it would compare clock formatting rather than payload size. Its
/// payload is still swept for agent state by `assert_no_agent_state`.
fn history_bytes(events: &[WorkflowEvent]) -> usize {
    events
        .iter()
        .filter(|event| !matches!(event, WorkflowEvent::WorkflowStarted { .. }))
        .map(|event| serde_json::to_string(event).expect("event serializes").len())
        .sum()
}

/// Replay the history this run produced through the same handler and assert it
/// does not diverge (**AC-31**).
///
/// Every workflow test in this file ends with this call, so replay determinism
/// is re-proved on every code path the suite exercises rather than on one
/// happy-path history. It costs nothing — the replayer reuses the history the
/// run already built.
async fn assert_replays(outcome: &TestRunOutcome) {
    let report = outcome
        .replay_check(simulation_workflow_info().handler)
        .await;
    assert!(
        matches!(report.status, ReplayStatus::ReplaySucceeded),
        "replay diverged: {report}"
    );
}

/// Field names that only ever appear in serialized agent state.
const AGENT_STATE_KEYS: [&str; 5] = ["agents", "px", "py", "vx", "vy"];

/// The longest JSON array a cursor-only history may legitimately contain.
/// A leaked flock would blow straight past this.
const MAX_HISTORY_ARRAY_LEN: usize = 8;

/// Recursively assert that `value` contains no agent state.
fn assert_no_agent_state(value: &Value, path: &str) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                assert!(
                    !AGENT_STATE_KEYS.contains(&key.as_str()),
                    "agent state leaked into workflow history at {path}.{key}"
                );
                assert_no_agent_state(child, &format!("{path}.{key}"));
            }
        }
        Value::Array(items) => {
            assert!(
                items.len() <= MAX_HISTORY_ARRAY_LEN,
                "workflow history at {path} holds a {}-element array — history must stay \
                 O(1) in agent count",
                items.len()
            );
            for (index, child) in items.iter().enumerate() {
                assert_no_agent_state(child, &format!("{path}[{index}]"));
            }
        }
        _ => {}
    }
}

// ───────────────────────────── AC-29 ─────────────────────────────

#[tokio::test]
async fn ac29_workflow_history_carries_only_a_cursor_never_the_agent_array() {
    let small = cursor_history(10).await;
    let huge = cursor_history(10_000).await;

    // 20 batches were driven, each carrying only `(run_id, next_tick)` forward.
    let batches = small
        .iter()
        .filter(|event| {
            matches!(event, WorkflowEvent::ActivityScheduled { name, .. } if name == "simulate_batch")
        })
        .count();
    assert_eq!(batches, 20, "2000 ticks in batches of 100 is 20 batches");

    // A thousandfold more agents produces the *same* history, event for event…
    assert_eq!(
        small.len(),
        huge.len(),
        "event count must not depend on agent count"
    );
    // …and byte for byte.
    assert_eq!(
        history_bytes(&small),
        history_bytes(&huge),
        "history size must not depend on agent count (10 agents vs 10 000)"
    );

    // And no event payload anywhere contains agent state.
    for (index, event) in huge.iter().enumerate() {
        let json = serde_json::to_value(event).expect("event serializes");
        assert_no_agent_state(&json, &format!("events[{index}]"));
    }
}

// ───────────────────────────── AC-30 ─────────────────────────────

/// The workflow needs no database and no wall clock.
///
/// Nothing in this test opens a connection: the `WorkflowTestEnv` is built with
/// no injected state at all, so a workflow that reached for a pool would find
/// none — and a workflow that reached for Postgres *directly* would not compile,
/// because `#[workflow]`'s determinism lint rejects IO (HVG006) in a workflow
/// body. The only clock it may read is the harness's virtual one.
#[tokio::test]
async fn ac30_the_workflow_runs_with_no_database_and_only_the_virtual_clock() {
    let env = WorkflowTestEnv::new()
        .mock_activity("simulate_batch", flock_cursor_mock(40, 500))
        .mock_activity("finalize_run", |_| Ok(json!({ "finalized": true })))
        .mock_activity("record_signal", |_| Ok(json!({ "recorded": true })));

    let outcome = env
        .run(simulation_workflow_info().handler, workflow_input(500, 100))
        .await;
    let result = outcome.result.clone().expect("the workflow completes");

    // `ctx.now()`, not `Utc::now()`: the run's finish time is the virtual clock's,
    // which is what makes it reproduce identically on replay.
    assert_eq!(
        result["finished_at"],
        json!(env.now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        "the workflow timestamps itself from the virtual clock"
    );

    // Five batches of a hundred ticks each, and not one second of wall clock.
    assert_eq!(result["batches"], json!(5));
    assert_eq!(
        outcome.elapsed(),
        chrono::Duration::zero(),
        "no real time passes: the whole run is virtual"
    );

    assert_replays(&outcome).await;
}

// ───────────────────────────── AC-31 ─────────────────────────────

#[tokio::test]
async fn ac31_the_workflow_replays_without_diverging() {
    let env = WorkflowTestEnv::new()
        .mock_activity("simulate_batch", flock_cursor_mock(60, 1_000))
        .mock_activity("finalize_run", |_| Ok(json!({ "finalized": true })))
        .mock_activity("record_signal", |_| Ok(json!({ "recorded": true })));

    let outcome = env
        .run(
            simulation_workflow_info().handler,
            workflow_input(1_000, 100),
        )
        .await;
    assert!(
        outcome.result.is_ok(),
        "the workflow completes: {:?}",
        outcome.result
    );

    let report = outcome
        .replay_check(simulation_workflow_info().handler)
        .await;
    assert!(
        matches!(report.status, ReplayStatus::ReplaySucceeded),
        "replay diverged: {report}"
    );
    assert_eq!(
        report.events_replayed,
        outcome.events().len(),
        "the whole history was replayed, not a prefix of it"
    );
}

// ───────────────────────────── AC-35 ─────────────────────────────

/// How many times an activity was scheduled in this history.
fn scheduled_count(events: &[WorkflowEvent], activity: &str) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, WorkflowEvent::ActivityScheduled { name, .. } if name == activity))
        .count()
}

/// The inputs the workflow sent to `activity`, oldest first.
fn scheduled_inputs(events: &[WorkflowEvent], activity: &str) -> Vec<Value> {
    events
        .iter()
        .filter_map(|event| match event {
            WorkflowEvent::ActivityScheduled { name, input, .. } if name == activity => {
                Some(input.clone())
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn ac35_max_ticks_bounds_a_run_to_exactly_ceil_max_over_batch_batches() {
    // 450 ticks in batches of 100 is four full batches and a short fifth.
    let env = WorkflowTestEnv::new()
        .mock_activity("simulate_batch", flock_cursor_mock(30, 450))
        .mock_activity("finalize_run", |_| Ok(json!({ "finalized": true })))
        .mock_activity("record_signal", |_| Ok(json!({ "recorded": true })));

    let outcome = env
        .run(simulation_workflow_info().handler, workflow_input(450, 100))
        .await;
    let result = outcome.result.clone().expect("the workflow completes");

    assert_eq!(
        scheduled_count(outcome.events(), "simulate_batch"),
        boidboard::workflow::planned_batches(450, 100) as usize,
        "exactly ceil(max_ticks / batch_ticks) batches, no more and no fewer"
    );
    assert_eq!(scheduled_count(outcome.events(), "simulate_batch"), 5);
    assert_eq!(result["ticks_completed"], json!(450));
    assert_eq!(
        result["status"],
        json!("completed"),
        "a run that spends its whole tick budget completed; it did not overrun"
    );

    // The run reached a terminal state, and `finalize_run` was told which one.
    let finalize = scheduled_inputs(outcome.events(), "finalize_run");
    assert_eq!(finalize.len(), 1, "the run is finalized exactly once");
    assert_eq!(finalize[0]["status"], json!("completed"));
    assert_eq!(finalize[0]["ticks_completed"], json!(450));

    assert_replays(&outcome).await;
}

#[tokio::test]
async fn ac35_a_cursor_that_stops_advancing_trips_the_budget_guardrail() {
    // A `simulate_batch` that reports no progress. Without a guardrail the
    // workflow would ask it to advance from tick 0 forever.
    let env = WorkflowTestEnv::new()
        .mock_activity("simulate_batch", |input| {
            Ok(json!({
                "run_id": input["run_id"],
                "next_tick": 0,
                "done": false,
                "state_hash": "0000000000000000",
                "frames_written": 0,
            }))
        })
        .mock_activity("finalize_run", |_| Ok(json!({ "finalized": true })))
        .mock_activity("record_signal", |_| Ok(json!({ "recorded": true })));

    let outcome = env
        .run(simulation_workflow_info().handler, workflow_input(450, 100))
        .await;
    let result = outcome.result.clone().expect("the workflow terminates");

    assert_eq!(
        scheduled_count(outcome.events(), "simulate_batch"),
        5,
        "the budget stops the run after ceil(450 / 100) batches rather than never"
    );
    assert_eq!(
        result["status"],
        json!("budget_exceeded"),
        "a run stopped by the guardrail says so, rather than claiming completion"
    );
    assert_eq!(result["ticks_completed"], json!(0));

    let finalize = scheduled_inputs(outcome.events(), "finalize_run");
    assert_eq!(finalize[0]["status"], json!("budget_exceeded"));

    assert_replays(&outcome).await;
}

// ───────────────────────────── AC-33 ─────────────────────────────

/// The state hash `flock_cursor_mock(agent_count, _)` reports — the real hash of
/// a real flock, recomputed here so the test can name the value it expects.
fn expected_flock_hash(agent_count: usize) -> String {
    let params = SimParams {
        agent_count,
        ..SimParams::default()
    };
    SimState::seeded(&params, 7).state_hash_hex()
}

/// Every distinct activity the workflow scheduled, sorted.
fn activity_names(events: &[WorkflowEvent]) -> Vec<String> {
    let mut names: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            WorkflowEvent::ActivityScheduled { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

#[tokio::test]
async fn ac33_a_cancel_signal_stops_the_run_at_the_next_batch_boundary() {
    let env = WorkflowTestEnv::new()
        .mock_activity("simulate_batch", flock_cursor_mock(25, 500))
        .mock_activity("finalize_run", |_| Ok(json!({ "finalized": true })))
        .mock_activity("record_signal", |_| Ok(json!({ "recorded": true })))
        .queue_signal("cancel", json!({ "reason": "operator stopped the run" }));

    let outcome = env
        .run(simulation_workflow_info().handler, workflow_input(500, 100))
        .await;
    let result = outcome.result.clone().expect("the workflow terminates");
    let events = outcome.events();

    // The batch already dispatched is allowed to finish — that is what "at the
    // next batch boundary" means — and then the run stops, four batches short of
    // the five its budget allowed.
    assert_eq!(
        scheduled_count(events, "simulate_batch"),
        1,
        "the in-flight batch completes, and no further batch is dispatched"
    );
    assert_eq!(result["status"], json!("cancelled"));

    // Partial results survive. The tick-100 checkpoint the completed batch
    // produced is exactly what the run is finalized with — nothing was rolled
    // back to tick 0.
    let hash = expected_flock_hash(25);
    assert_eq!(result["ticks_completed"], json!(100));
    assert_eq!(result["final_state_hash"], json!(hash));

    let finalize = scheduled_inputs(events, "finalize_run");
    assert_eq!(finalize.len(), 1);
    assert_eq!(finalize[0]["status"], json!("cancelled"));
    assert_eq!(
        finalize[0]["ticks_completed"],
        json!(100),
        "the cancelled run keeps the ticks it actually completed"
    );
    assert_eq!(finalize[0]["final_state_hash"], json!(hash));

    // Nothing compensating ran: the only activities in this history are the
    // batch, its provenance record, and the finalizer.
    assert_eq!(
        activity_names(events),
        vec![
            "finalize_run".to_owned(),
            "record_signal".to_owned(),
            "simulate_batch".to_owned()
        ],
        "no rollback or compensation activity was issued"
    );

    // The cancel explains itself afterwards (AC-34's provenance rule applies to
    // cancels too).
    let recorded = scheduled_inputs(events, "record_signal");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0]["kind"], json!("cancel"));
    assert_eq!(recorded[0]["tick"], json!(100));
    assert_eq!(
        recorded[0]["payload"],
        json!({ "reason": "operator stopped the run" })
    );

    assert_replays(&outcome).await;
}

#[tokio::test]
async fn ac33_engine_cancellation_kills_the_run_without_issuing_another_command() {
    // A hard cancel from the engine, as opposed to an operator's `cancel`
    // signal. Harvest records it as a `WorkflowCancelled` event that no replay
    // command may be issued past, so the *only* replay-safe response is to stop
    // issuing commands — not even a tidy-up `finalize_run`. This test pins that:
    // adding a "helpful" finalize call here would break replay determinism.
    let env = WorkflowTestEnv::new()
        .mock_activity("simulate_batch", flock_cursor_mock(25, 500))
        .mock_activity("finalize_run", |_| Ok(json!({ "finalized": true })))
        .mock_activity("record_signal", |_| Ok(json!({ "recorded": true })))
        .with_cancellation("user requested");

    let outcome = env
        .run(simulation_workflow_info().handler, workflow_input(500, 100))
        .await;

    let error = outcome
        .result
        .clone()
        .expect_err("an engine cancel is not a successful completion");
    assert!(
        error.contains("user requested"),
        "the cancellation reason survives into the failure: {error}"
    );
    assert!(
        activity_names(outcome.events()).is_empty(),
        "not one command was issued past the cancellation: {:?}",
        activity_names(outcome.events())
    );
}

#[tokio::test]
async fn a_cursor_for_the_wrong_run_fails_the_workflow_rather_than_being_applied() {
    // `BatchCursor` echoes back the run it belongs to. A cursor naming a
    // different run means a message was misrouted, and applying it would advance
    // one run using another's checkpoint — silently, and unrecoverably.
    let env = WorkflowTestEnv::new()
        .mock_activity("simulate_batch", |input| {
            Ok(json!({
                "run_id": input["run_id"].as_i64().unwrap_or_default() + 1,
                "next_tick": 100,
                "done": false,
                "state_hash": "0000000000000000",
                "frames_written": 100,
            }))
        })
        .mock_activity("finalize_run", |_| Ok(json!({ "finalized": true })))
        .mock_activity("record_signal", |_| Ok(json!({ "recorded": true })));

    let outcome = env
        .run(simulation_workflow_info().handler, workflow_input(500, 100))
        .await;

    let error = outcome
        .result
        .clone()
        .expect_err("a cursor for another run must not be applied");
    assert!(
        error.contains("4242") && error.contains("4243"),
        "the failure names both runs so the misroute is diagnosable: {error}"
    );
    assert_eq!(
        scheduled_count(outcome.events(), "finalize_run"),
        0,
        "a run whose cursor cannot be trusted is not finalized on that cursor's numbers"
    );
}

// ───────────────────────────── AC-34 ─────────────────────────────

#[tokio::test]
async fn ac34_a_steer_signal_changes_params_for_later_batches_and_records_itself() {
    let env = WorkflowTestEnv::new()
        .mock_activity("simulate_batch", flock_cursor_mock(25, 500))
        .mock_activity("finalize_run", |_| Ok(json!({ "finalized": true })))
        .mock_activity("record_signal", |_| Ok(json!({ "recorded": true })))
        .queue_signal("steer", json!({ "w_cohesion": 2.5 }))
        .queue_signal("steer", json!({ "w_separation": 0.25, "max_speed": 3.0 }));

    let outcome = env
        .run(simulation_workflow_info().handler, workflow_input(500, 100))
        .await;
    outcome.result.clone().expect("the workflow completes");
    let events = outcome.events();

    let batches = scheduled_inputs(events, "simulate_batch");
    assert_eq!(batches.len(), 5, "the run still spends its whole budget");

    // The batch that was already dispatched keeps the parameters it started
    // with; steering takes effect from the next batch on.
    assert_eq!(batches[0]["overrides"], json!({}));
    assert_eq!(batches[1]["overrides"], json!({ "w_cohesion": 2.5 }));
    assert_eq!(
        batches[2]["overrides"],
        json!({ "max_speed": 3.0, "w_cohesion": 2.5, "w_separation": 0.25 }),
        "steers accumulate rather than replacing one another"
    );
    assert_eq!(
        batches[3]["overrides"], batches[2]["overrides"],
        "with no further signal the last steer stays in force"
    );

    // The criterion, stated directly: later batches did not get the earlier
    // batches' parameters.
    assert_ne!(batches[0]["overrides"], batches[1]["overrides"]);
    assert_ne!(batches[1]["overrides"], batches[2]["overrides"]);

    // …and the run can explain why. Each steer is recorded verbatim, against
    // the tick at which it took effect.
    let recorded = scheduled_inputs(events, "record_signal");
    assert_eq!(recorded.len(), 2, "both steers are recorded, neither lost");
    assert_eq!(recorded[0]["kind"], json!("steer"));
    assert_eq!(recorded[0]["run_id"], json!(RUN_ID));
    assert_eq!(recorded[0]["tick"], json!(100));
    assert_eq!(recorded[0]["payload"], json!({ "w_cohesion": 2.5 }));
    assert_eq!(recorded[1]["tick"], json!(200));
    assert_eq!(
        recorded[1]["payload"],
        json!({ "w_separation": 0.25, "max_speed": 3.0 })
    );

    assert_replays(&outcome).await;
}

#[tokio::test]
async fn ac34_a_steer_applies_only_recognised_parameters_but_records_the_whole_payload() {
    let env = WorkflowTestEnv::new()
        .mock_activity("simulate_batch", flock_cursor_mock(25, 300))
        .mock_activity("finalize_run", |_| Ok(json!({ "finalized": true })))
        .mock_activity("record_signal", |_| Ok(json!({ "recorded": true })))
        .queue_signal(
            "steer",
            json!({ "w_cohesion": 2.0, "w_cohesian": 9.0, "note": "typo above" }),
        );

    let outcome = env
        .run(simulation_workflow_info().handler, workflow_input(300, 100))
        .await;
    outcome.result.clone().expect("the workflow completes");
    let events = outcome.events();

    // A misspelled or non-numeric key is not silently turned into a parameter…
    let batches = scheduled_inputs(events, "simulate_batch");
    assert_eq!(batches[1]["overrides"], json!({ "w_cohesion": 2.0 }));

    // …but it is not silently discarded either: provenance keeps the payload as
    // sent, which is how an operator finds their typo afterwards.
    let recorded = scheduled_inputs(events, "record_signal");
    assert_eq!(
        recorded[0]["payload"],
        json!({ "w_cohesion": 2.0, "w_cohesian": 9.0, "note": "typo above" })
    );

    assert_replays(&outcome).await;
}

#[test]
fn ac34_steer_overrides_change_the_named_parameters_and_nothing_else() {
    let mut params = SimParams::default();
    let baseline = params.clone();

    let overrides = boidboard::workflow::steer_overrides(&json!({
        "w_cohesion": 2.5,
        "max_speed": 3.0,
        "w_cohesian": 9.0,
        "agent_count": 4,
    }));
    assert_eq!(
        overrides.keys().collect::<Vec<_>>(),
        vec!["max_speed", "w_cohesion"],
        "only steerable numeric fields survive, in a deterministic order"
    );

    boidboard::workflow::apply_overrides(&mut params, &overrides);
    assert!((params.w_cohesion - 2.5).abs() < f64::EPSILON);
    assert!((params.max_speed - 3.0).abs() < f64::EPSILON);
    assert!((params.w_separation - baseline.w_separation).abs() < f64::EPSILON);
    assert_eq!(
        params.agent_count, baseline.agent_count,
        "steering may not resize the flock — that would invalidate every stored frame"
    );
}

// ══════════════════ live-database tests: the activity cores ══════════════════

const TEST_DB_URL: &str = "postgres://boid:boid@127.0.0.1:5432/boidboard_test";

/// Apply the embedded migrations to the test database once per test binary.
///
/// This commits — migrations are DDL and live outside the per-test transaction —
/// which is why it has to be idempotent. It is: Diesel tracks applied versions.
fn migrate_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        autumn_web::migrate::run_pending(TEST_DB_URL, boidboard::MIGRATIONS)
            .expect("embedded migrations apply to the test database");
    });
}

/// A transactionally-isolated client over the live test database.
fn test_client() -> TestClient {
    migrate_once();
    TestApp::new().with_transactional_db(TEST_DB_URL).build()
}

/// Seed a scenario and a `queued` run over it, exactly as the submit route will.
///
/// `config_snapshot` is a real serialized [`SimParams`], because the activity
/// deserializes it straight back into one — a hand-rolled JSON blob here would
/// test the test rather than the code.
async fn seed_run(
    pool: &Pool<AsyncPgConnection>,
    agent_count: usize,
    max_ticks: i32,
    seed: i64,
) -> Run {
    let params = SimParams {
        agent_count,
        ..SimParams::default()
    };
    let config = serde_json::to_value(&params).expect("SimParams serializes");
    let config_hash = boidboard::models::canonical_config_hash(&config);

    let scenario = PgScenarioRepository::with_pool_untracked(pool.clone())
        .save(&NewScenario {
            name: format!("workflow-test-{agent_count}-{seed}"),
            config: config.clone(),
            config_hash: config_hash.clone(),
        })
        .await
        .expect("scenario saves");

    PgRunRepository::with_pool_untracked(pool.clone())
        .save(&NewRun {
            scenario_id: scenario.id,
            seed,
            status: run_status::QUEUED.to_owned(),
            max_ticks,
            ticks_completed: 0,
            config_snapshot: config,
            config_hash,
            kernel_version: boidboard::KERNEL_VERSION.to_owned(),
            final_state_hash: None,
            error: None,
            workflow_execution_id: None,
        })
        .await
        .expect("run saves")
}

/// A batch request with no steering applied.
fn batch_request(run_id: i64, from_tick: u32, batch_ticks: u32) -> BatchRequest {
    BatchRequest {
        run_id,
        from_tick,
        batch_ticks,
        metrics_every: 10,
        overrides: BTreeMap::new(),
    }
}

// ───────────────────────────── AC-32 ─────────────────────────────

#[tokio::test]
async fn ac32_a_duplicated_simulate_batch_leaves_one_set_of_frames_and_no_gaps() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let run = seed_run(&pool, 12, 300, 99).await;
    let mut conn = pool.get().await.expect("checkout connection");

    let request = batch_request(run.id, 0, 50);

    // At-least-once delivery: the identical activity input, twice.
    let first = simulate_batch_core(&mut conn, &request)
        .await
        .expect("first delivery");
    let frames_after_first = frames_for_run(&mut conn, run.id, None)
        .await
        .expect("frames load")
        .len();

    let second = simulate_batch_core(&mut conn, &request)
        .await
        .expect("duplicate delivery");
    let frames_after_second = frames_for_run(&mut conn, run.id, None)
        .await
        .expect("frames load")
        .len();

    assert_eq!(first.next_tick, 50);
    assert_eq!(
        first.frames_written, 51,
        "ticks 0 through 50 inclusive — the seed frame plus one per simulated tick"
    );

    assert_eq!(
        second.frames_written, 0,
        "the duplicate delivery inserted nothing"
    );
    assert_eq!(
        second.state_hash, first.state_hash,
        "and recomputed exactly the same state, so the cursor is stable under retry"
    );
    assert_eq!(second.next_tick, first.next_tick);

    assert_eq!(
        frames_after_second, frames_after_first,
        "exactly one set of frames survives two deliveries"
    );
    assert_eq!(frames_after_first, 51);

    assert!(
        tick_gaps(&mut conn, run.id)
            .await
            .expect("gap scan")
            .is_empty(),
        "the checkpoint sequence is contiguous"
    );
}

/// Read a run back through Diesel on the connection the test already holds.
async fn load_run(conn: &mut AsyncPgConnection, run_id: i64) -> Run {
    runs::table
        .filter(runs::id.eq(run_id))
        .select(Run::as_select())
        .first(conn)
        .await
        .expect("run loads")
}

/// Every recorded intervention for a run, oldest first.
async fn load_signals(conn: &mut AsyncPgConnection, run_id: i64) -> Vec<RunSignal> {
    run_signals::table
        .filter(run_signals::run_id.eq(run_id))
        .order(run_signals::id.asc())
        .select(RunSignal::as_select())
        .load(conn)
        .await
        .expect("signals load")
}

// ─────────────────── AC-33, the persistence half ───────────────────

#[tokio::test]
async fn ac33_a_cancelled_run_keeps_the_frames_its_completed_batches_wrote() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let run = seed_run(&pool, 10, 400, 5).await;
    let mut conn = pool.get().await.expect("checkout connection");

    // Two batches complete…
    simulate_batch_core(&mut conn, &batch_request(run.id, 0, 50))
        .await
        .expect("batch 1");
    let second = simulate_batch_core(&mut conn, &batch_request(run.id, 50, 50))
        .await
        .expect("batch 2");
    assert_eq!(second.next_tick, 100);

    // …and then the operator cancels at that boundary, exactly as the workflow
    // does: record the intervention, then finalize.
    record_signal_core(
        &mut conn,
        &SignalRecord {
            run_id: run.id,
            tick: 100,
            kind: signal_kind::CANCEL.to_owned(),
            payload: json!({ "reason": "seen enough" }),
        },
    )
    .await
    .expect("cancel is recorded");

    let finalized = finalize_run_core(
        &mut conn,
        &FinalizeRequest {
            run_id: run.id,
            status: run_status::CANCELLED.to_owned(),
            ticks_completed: 100,
            final_state_hash: second.state_hash.clone(),
            error: None,
        },
    )
    .await
    .expect("run finalizes");

    assert_eq!(finalized.status, run_status::CANCELLED);
    assert_eq!(finalized.ticks_completed, 100);
    assert_eq!(
        finalized.final_state_hash.as_deref(),
        Some(second.state_hash.as_str()),
        "the cancelled run keeps the checkpoint it actually reached"
    );

    // Nothing was rolled back: both completed batches are still on disk, whole.
    let frames = frames_for_run(&mut conn, run.id, None)
        .await
        .expect("frames load");
    assert_eq!(
        frames.len(),
        101,
        "ticks 0 through 100 survive the cancellation"
    );
    assert_eq!(
        max_tick(&mut conn, run.id).await.expect("max tick"),
        Some(100)
    );
    assert!(
        tick_gaps(&mut conn, run.id)
            .await
            .expect("gap scan")
            .is_empty()
    );

    // And a late duplicate delivery of an already-run batch — the classic
    // at-least-once tail — cannot resurrect a cancelled run.
    simulate_batch_core(&mut conn, &batch_request(run.id, 0, 50))
        .await
        .expect("late duplicate delivery");
    assert_eq!(
        load_run(&mut conn, run.id).await.status,
        run_status::CANCELLED,
        "a straggler batch must not put a terminal run back into `running`"
    );
}

// ─────────────────── AC-34, the provenance half ───────────────────

#[tokio::test]
async fn ac34_a_recorded_signal_survives_redelivery_and_keeps_its_payload_verbatim() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let run = seed_run(&pool, 8, 200, 11).await;
    let mut conn = pool.get().await.expect("checkout connection");

    let payload = json!({ "w_cohesion": 2.5, "note": "tighten the flock" });
    let record = SignalRecord {
        run_id: run.id,
        tick: 100,
        kind: signal_kind::STEER.to_owned(),
        payload: payload.clone(),
    };

    let first = record_signal_core(&mut conn, &record)
        .await
        .expect("first delivery");
    let second = record_signal_core(&mut conn, &record)
        .await
        .expect("duplicate delivery");
    assert_eq!(
        first, second,
        "a redelivered signal resolves to the same row, not a new one"
    );

    let signals = load_signals(&mut conn, run.id).await;
    assert_eq!(
        signals.len(),
        1,
        "at-least-once delivery must not duplicate provenance"
    );
    assert_eq!(signals[0].kind, signal_kind::STEER);
    assert_eq!(signals[0].tick, 100);
    assert_eq!(
        signals[0].payload, payload,
        "the payload is stored as the operator sent it, typos and notes included"
    );

    // A genuinely different intervention is a genuinely different row.
    record_signal_core(
        &mut conn,
        &SignalRecord {
            run_id: run.id,
            tick: 150,
            kind: signal_kind::STEER.to_owned(),
            payload: json!({ "w_cohesion": 0.5 }),
        },
    )
    .await
    .expect("second steer");
    assert_eq!(load_signals(&mut conn, run.id).await.len(), 2);
}

// ───────────────────────────── AC-36 ─────────────────────────────

/// Agents, ticks, batch size and seed shared by both halves of the crash-resume
/// experiment. The two runs differ in exactly one respect: one of them was
/// interrupted.
const RESUME_AGENTS: usize = 15;
const RESUME_MAX_TICKS: i32 = 300;
const RESUME_BATCH: u32 = 50;
const RESUME_SEED: i64 = 0x0B01_D5EE;

/// Drive `run_id` from `from_tick` to the end of its budget, returning the final
/// state hash. This is the workflow's loop, minus the workflow.
async fn drive_to_completion(
    conn: &mut AsyncPgConnection,
    run_id: i64,
    from_tick: u32,
) -> (u32, String) {
    let mut cursor = from_tick;
    let mut hash = String::new();
    let budget = u32::try_from(RESUME_MAX_TICKS).expect("budget fits in u32");
    while cursor < budget {
        let batch = simulate_batch_core(conn, &batch_request(run_id, cursor, RESUME_BATCH))
            .await
            .expect("batch runs");
        cursor = batch.next_tick;
        hash = batch.state_hash;
    }
    (cursor, hash)
}

/// The state hash a run of `RESUME_MAX_TICKS` ticks reaches **with no database
/// involved at all** — pure kernel, one call, nothing checkpointed.
///
/// This, and not another batched run, is the reference AC-36 compares against.
/// Comparing two *batched* runs would compare the checkpoint machinery with
/// itself: a systematic bug in it — an off-by-one in which frame a batch resumes
/// from, say — corrupts both sides identically and the assertion still passes.
/// A mutation check proved exactly that, so the reference was moved out of the
/// database entirely.
fn in_memory_reference_hash() -> String {
    let params = SimParams {
        agent_count: RESUME_AGENTS,
        ..SimParams::default()
    };
    let seeded = SimState::seeded(&params, RESUME_SEED.cast_unsigned());
    let ticks = u32::try_from(RESUME_MAX_TICKS).expect("budget fits in u32");
    let (final_state, _) = boids_core::sim::run_batch(&seeded, &params, ticks, 0);
    final_state.state_hash_hex()
}

/// **The durability claim.**
///
/// A run interrupted mid-flight, whose in-memory state is then thrown away
/// entirely, resumes from its Postgres checkpoint and finishes at *exactly* the
/// state an uninterrupted run reaches — the same 64-bit hash, not a similar
/// trajectory. If this test fails, checkpointing is decoration and the whole
/// durable-workflow architecture is unjustified.
#[tokio::test]
async fn ac36_a_run_resumed_from_its_postgres_checkpoint_reaches_the_same_final_state_hash() {
    // The answer, computed once, in memory, by the kernel alone.
    let reference_hash = in_memory_reference_hash();

    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let uninterrupted = seed_run(&pool, RESUME_AGENTS, RESUME_MAX_TICKS, RESUME_SEED).await;
    let interrupted = seed_run(&pool, RESUME_AGENTS, RESUME_MAX_TICKS, RESUME_SEED).await;
    let mut conn = pool.get().await.expect("checkout connection");

    // ── A checkpointed run that was never interrupted. ──
    let (whole_tick, whole_hash) = drive_to_completion(&mut conn, uninterrupted.id, 0).await;
    assert_eq!(whole_tick, 300);
    assert_eq!(
        whole_hash, reference_hash,
        "cutting a run into batches must not change its answer"
    );

    // ── The crash. Everything this scope knows dies with it. ──
    let hash_at_the_crash = {
        let mut cursor = 0;
        let mut hash = String::new();
        for _ in 0..2 {
            let batch =
                simulate_batch_core(&mut conn, &batch_request(interrupted.id, cursor, RESUME_BATCH))
                    .await
                    .expect("pre-crash batch");
            cursor = batch.next_tick;
            hash = batch.state_hash;
        }
        assert_eq!(cursor, 100, "two batches completed before the interruption");
        hash
    };
    // Everything the crashed process held is now out of scope. From here on the
    // only thing that knows where the run got to is Postgres.

    // ── The resume. The cursor is *rediscovered*, not remembered. ──
    let recovered = max_tick(&mut conn, interrupted.id)
        .await
        .expect("checkpoint scan")
        .expect("a checkpoint survived the interruption");
    assert_eq!(
        recovered, 100,
        "the run's position is recoverable from the frame table alone"
    );
    let (resumed_tick, resumed_hash) = drive_to_completion(
        &mut conn,
        interrupted.id,
        u32::try_from(recovered).expect("tick fits in u32"),
    )
    .await;
    assert_eq!(resumed_tick, 300);

    // ── The claim. ──
    assert_eq!(
        resumed_hash, reference_hash,
        "a run resumed from its Postgres checkpoint must reach the IDENTICAL \
         final state as an uninterrupted one, not a similar one"
    );

    // Guards against the test being vacuously true: the flock really does move,
    // so "identical" is a statement about the simulation and not about a
    // constant.
    assert_ne!(
        hash_at_the_crash, reference_hash,
        "the state at tick 100 differs from the state at tick 300, so equality \
         at tick 300 means something"
    );

    // Stronger than the endpoints: every checkpoint along the way agrees too,
    // including the ones written on either side of the seam.
    let reference_frames = frames_for_run(&mut conn, uninterrupted.id, None)
        .await
        .expect("reference frames");
    let resumed_frames = frames_for_run(&mut conn, interrupted.id, None)
        .await
        .expect("resumed frames");
    assert_eq!(reference_frames.len(), 301, "ticks 0 through 300 inclusive");
    assert_eq!(resumed_frames.len(), reference_frames.len());
    for (reference, resumed) in reference_frames.iter().zip(resumed_frames.iter()) {
        assert_eq!(reference.tick, resumed.tick);
        assert_eq!(
            reference.state_hash, resumed.state_hash,
            "the two runs diverged at tick {}",
            reference.tick
        );
    }

    assert!(
        tick_gaps(&mut conn, interrupted.id)
            .await
            .expect("gap scan")
            .is_empty(),
        "the resumed run's checkpoint sequence has no hole at the seam"
    );
}

// ─────────────────── activity registration ───────────────────

/// The names the workflow schedules must be the names the worker registers.
///
/// A mismatch between `execute_activity_raw("simulate_batch", …)` and whatever
/// `activities![…]` ends up containing is invisible until a run hangs in
/// production waiting for a handler that was never registered. Asserting the
/// generated `{fn}_info().name` here catches a rename at compile-and-test time.
#[test]
fn the_registered_activity_names_match_the_names_the_workflow_schedules() {
    assert_eq!(
        boidboard::workflow::simulate_batch_info().name,
        "simulate_batch"
    );
    assert_eq!(boidboard::workflow::finalize_run_info().name, "finalize_run");
    assert_eq!(
        boidboard::workflow::record_signal_info().name,
        "record_signal"
    );
    assert_eq!(
        boidboard::workflow::simulation_workflow_info().name,
        "simulation_workflow"
    );

    // The workflow schedules everything onto one queue, and the activities
    // default to it.
    assert_eq!(boidboard::workflow::SIMULATION_QUEUE, "default");
}
