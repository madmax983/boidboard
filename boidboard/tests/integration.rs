//! End-to-end integration tests across the web ↔ workflow seam.
//!
//! Every other test file in this crate proves one *layer*: `web.rs` proves the
//! routes render, `workflow.rs` proves the orchestration replays and the
//! activity cores are idempotent, `persistence.rs` proves the repositories
//! round-trip. Each of them passes with the two halves of the application
//! entirely disconnected — which is exactly the state they were in. This file
//! exists to prove the *seam*: that creating a run through the HTTP form
//! actually starts the durable workflow, that a real Harvest worker drives it
//! to a terminal state, and that the frames it produced are visible on the page.
//!
//! # How a real worker gets into a test
//!
//! `TestApp::plugin(..)` runs a plugin's startup hooks, and `HarvestPlugin`'s
//! startup hook is the one that boots the runtime — so a `TestApp` carrying
//! [`boidboard::harvest_plugin`] runs a **genuine** Harvest worker polling a
//! **genuine** Postgres task queue. Nothing here is mocked: the workflow is
//! dispatched, the activities check out real connections, and the assertions
//! read the rows they wrote.
//!
//! # Why the dev database, and why serially
//!
//! The plugin's startup hook resolves Harvest's storage from `AutumnConfig`,
//! which reads `autumn.toml` — so the worker's queue tables and the app pool
//! must name the *same* database, and `autumn.toml` names the dev one. These
//! tests therefore commit (a workflow that never commits is a workflow no
//! worker can see), which is the opposite of the transactional isolation the
//! other files use. Nothing asserts on global counts as a result: every
//! assertion is scoped to the run the test itself created, so the file passes
//! repeatedly against an accumulating database.
//!
//! They also run one at a time, behind [`SERIAL`]. Two concurrent tests would
//! each boot a worker, and a test finishing drops its runtime — killing its
//! worker wherever it happened to be, including in the middle of *another*
//! test's batch. That is a real Harvest scenario (the lease expires and the
//! task is redelivered), just not one worth paying for on every run.

use std::time::{Duration, Instant};

use autumn_web::test::{TestApp, TestClient, TestResponse};
use boidboard::models::run::status;
use boidboard::models::run_signal::kind as signal_kind;
use boidboard::models::{Run, RunSignal};
use boidboard::repositories::{frames_for_run, max_tick, tick_gaps};
use boidboard::schema::{run_signals, runs};
use diesel::{ExpressionMethods as _, QueryDsl as _, SelectableHelper as _};
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::{AsyncPgConnection, RunQueryDsl as _};

/// The database `autumn.toml` names, and therefore the one the Harvest worker
/// the plugin boots will poll. Pointing the app pool anywhere else would give
/// the worker one database and the routes another.
const DEV_DB_URL: &str = "postgres://boid:boid@127.0.0.1:5432/boidboard_dev";

/// How long a test waits for a run to reach a terminal state before declaring
/// the worker dead. Generous: the point of a timeout here is to fail with a
/// readable message instead of hanging CI, not to assert on latency.
const TERMINAL_TIMEOUT: Duration = Duration::from_secs(120);

/// Poll interval while waiting on the worker.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Serializes the tests in this file; see the module docs.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// ────────────────────────────── harness ──────────────────────────────

/// Apply the app's embedded migrations to the dev database once per binary.
///
/// Harvest's own migrations are applied by the plugin's startup hook, which
/// runs them automatically on the `dev` profile — so a completely fresh
/// database is enough to run this file.
fn migrate_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        autumn_web::migrate::run_pending(DEV_DB_URL, boidboard::MIGRATIONS)
            .expect("embedded migrations apply to the dev database");
    });
}

/// A pool big enough for a worker, its activities and the test's own queries to
/// hold connections at the same time. A single-connection pool — what the other
/// test files use — would deadlock the moment the worker checked one out.
fn app_pool() -> Pool<AsyncPgConnection> {
    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(DEV_DB_URL);
    Pool::builder(manager)
        .max_size(16)
        .build()
        .expect("the dev pool builds")
}

/// The whole application — every route it mounts and the plugin it mounts them
/// beside — with a live worker behind it.
fn integration_client() -> TestClient {
    migrate_once();
    TestApp::new()
        // `dev` is what makes the plugin apply Harvest's migrations itself.
        .profile("dev")
        .with_db(app_pool())
        .routes(boidboard::all_routes())
        .plugin(boidboard::harvest_plugin())
        .build()
}

/// The run id a `303` from `POST /runs` redirects to.
fn redirected_run_id(response: &TestResponse) -> i64 {
    let location = response
        .header("location")
        .expect("a created run redirects to its own page");
    location
        .trim_start_matches("/runs/")
        .parse()
        .unwrap_or_else(|e| panic!("the redirect target `{location}` names a run id: {e}"))
}

/// Submit the new-run form.
async fn create_run(client: &TestClient, form: &str) -> i64 {
    let response = client.post("/runs").form(form).send().await;
    response.assert_status(303);
    redirected_run_id(&response)
}

/// Read one run row.
async fn load_run(pool: &Pool<AsyncPgConnection>, id: i64) -> Run {
    let mut conn = pool.get().await.expect("checkout");
    runs::table
        .find(id)
        .select(Run::as_select())
        .first(&mut conn)
        .await
        .expect("the run exists")
}

/// How many Harvest executions are driving a run.
///
/// Read straight out of Harvest's own storage rather than inferred from the
/// `runs` row: the run row can only ever name *one* execution, so asking it
/// would answer the question by construction. This counts what actually exists.
async fn executions_for_run(conn: &mut AsyncPgConnection, run_id: i64) -> i64 {
    #[derive(diesel::QueryableByName)]
    struct Count {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }

    let rows: Vec<Count> = diesel::sql_query(
        "SELECT count(*) AS count FROM harvest_workflow_executions WHERE workflow_id = $1",
    )
    .bind::<diesel::sql_types::Text, _>(boidboard::routes::workflow_id_for_run(run_id))
    .load(conn)
    .await
    .expect("execution count");
    // Indexing rather than `Vec::first()`: with `diesel_async::RunQueryDsl` in
    // scope the inherent slice method loses to the trait's resolution.
    if rows.is_empty() { 0 } else { rows[0].count }
}

/// Wait until a run reaches a terminal status, or fail with what it was doing
/// when the patience ran out.
async fn await_terminal(pool: &Pool<AsyncPgConnection>, id: i64) -> Run {
    let deadline = Instant::now() + TERMINAL_TIMEOUT;
    loop {
        let run = load_run(pool, id).await;
        if status::is_terminal(&run.status) {
            return run;
        }
        assert!(
            Instant::now() < deadline,
            "run {id} never reached a terminal state: status={} ticks_completed={}/{} \
             workflow_execution_id={:?} error={:?} — is a Harvest worker running?",
            run.status,
            run.ticks_completed,
            run.max_ticks,
            run.workflow_execution_id,
            run.error
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ───────────────────── the run is actually dispatched ─────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creating_a_run_records_the_workflow_execution_it_started() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();

    let id = create_run(&client, "preset=classic-flock&seed=42&max_ticks=200").await;

    let run = load_run(&pool, id).await;
    assert!(
        run.workflow_execution_id.is_some(),
        "creating run {id} must start its durable workflow and record the execution \
         it started; the run row still says workflow_execution_id=NULL, which is the \
         whole bug: the simulation will never run"
    );
}

// ───────────────────── the headline: a run really runs ─────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_created_run_is_driven_to_completion_with_its_frames_in_postgres() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();

    let id = create_run(&client, "preset=classic-flock&seed=42&max_ticks=200").await;
    let run = await_terminal(&pool, id).await;

    assert_eq!(
        run.status,
        status::COMPLETED,
        "run {id} must finish, not merely stop: error={:?}",
        run.error
    );
    assert_eq!(
        run.ticks_completed, 200,
        "a completed run has spent its whole tick budget"
    );
    let hash = run
        .final_state_hash
        .as_deref()
        .expect("a completed run records the hash of the flock it ended on (AC-38)");
    assert_eq!(
        hash.len(),
        16,
        "the reproducibility hash is 16 hex digits, got `{hash}`"
    );

    // The frames are the actual product of the run: the workflow itself never
    // touches them, so their presence is proof the activities ran for real.
    let mut conn = pool.get().await.expect("checkout");
    assert_eq!(
        max_tick(&mut conn, id).await.expect("max tick"),
        Some(200),
        "the last checkpointed tick must be the budget"
    );
    assert_eq!(
        tick_gaps(&mut conn, id).await.expect("tick gaps"),
        Vec::<i32>::new(),
        "a contiguous tick sequence is what makes a resumed run resumable; a gap \
         is a lost batch"
    );
    let frames = frames_for_run(&mut conn, id, None).await.expect("frames");
    assert_eq!(
        frames.len(),
        201,
        "ticks 0..=200 inclusive: the seed state is checkpointed too"
    );
    assert_eq!(
        frames.last().expect("a last frame").state_hash,
        hash,
        "the run's final hash must be the hash of the frame it actually ended on"
    );
}

// ───────────────────────── one run, one execution ─────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_is_dispatched_once_however_many_times_the_start_is_delivered() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();

    let id = create_run(&client, "preset=classic-flock&seed=42&max_ticks=200").await;
    let run = load_run(&pool, id).await;
    let first = run
        .workflow_execution_id
        .clone()
        .expect("the run was dispatched");

    // Re-deliver the start through the *same* function the route calls — a
    // double-submitted form, a retried request, a redelivered message. Two
    // executions driving one run would not merely waste a worker: they would
    // race for the same `(run_id, tick)` rows with independent cursors.
    let state = client.state();
    let harvest = state
        .extension::<autumn_harvest::handle::WorkflowHandleClient>()
        .expect("the plugin installs a workflow handle client");
    let storage = state
        .extension::<autumn_harvest_plugin::HarvestDbPool>()
        .expect("the plugin installs its storage pool");
    let mut conn = storage
        .default_pool()
        .get()
        .await
        .expect("harvest checkout");

    let again = boidboard::routes::start_simulation_workflow(&harvest, &mut conn, &run)
        .await
        .expect("a repeated start is not an error — it resolves to the live execution");

    assert_eq!(
        again.exec_id.to_string(),
        first,
        "the second start must resolve to the execution already driving run {id}"
    );
    assert!(
        !again.created,
        "the second start must attach to the existing execution, not create one"
    );
    assert_eq!(
        executions_for_run(&mut conn, id).await,
        1,
        "run {id} must be driven by exactly one workflow execution"
    );

    // And the one execution really is the one that finishes the run.
    let done = await_terminal(&pool, id).await;
    assert_eq!(done.status, status::COMPLETED);
    assert_eq!(done.workflow_execution_id.as_deref(), Some(first.as_str()));
    assert_eq!(
        executions_for_run(&mut conn, id).await,
        1,
        "and still exactly one afterwards"
    );
}

// ──────────────────────── cancel, through the UI ────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_a_run_from_the_ui_stops_it_and_keeps_the_frames_it_earned() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();

    // A budget far beyond what this test will wait for, so the run is certainly
    // still mid-flight when the cancel lands.
    let id = create_run(&client, "preset=classic-flock&seed=7&max_ticks=5000").await;

    client
        .post(&format!("/runs/{id}/cancel"))
        .send()
        .await
        .assert_status(303)
        .assert_header("location", &format!("/runs/{id}"));

    let run = await_terminal(&pool, id).await;
    assert_eq!(
        run.status,
        status::CANCELLED,
        "AC-33: the run must end cancelled, not merely stop being touched; \
         error={:?}",
        run.error
    );
    assert!(
        run.ticks_completed > 0,
        "a graceful cancel finishes the batch in flight rather than throwing it away"
    );
    assert!(
        run.ticks_completed < run.max_ticks,
        "a cancel that only landed after the budget was spent proves nothing: \
         ticks_completed={} max_ticks={}",
        run.ticks_completed,
        run.max_ticks
    );

    // The frames from the batches that did complete are still there, and still
    // contiguous — a cancel costs at most one batch of work and loses none.
    let mut conn = pool.get().await.expect("checkout");
    assert_eq!(
        max_tick(&mut conn, id).await.expect("max tick"),
        Some(run.ticks_completed),
        "the last stored frame must be the tick the run says it reached"
    );
    assert_eq!(
        tick_gaps(&mut conn, id).await.expect("tick gaps"),
        Vec::<i32>::new(),
        "cancelling must not punch a hole in the tick sequence"
    );

    // AC-34's provenance half: the intervention is on the record, at the tick
    // it took effect from, so the run can explain its own ending afterwards.
    let signals: Vec<RunSignal> = run_signals::table
        .filter(run_signals::run_id.eq(id))
        .select(RunSignal::as_select())
        .load(&mut conn)
        .await
        .expect("signals load");
    assert_eq!(
        signals.len(),
        1,
        "exactly one cancel intervention should be recorded, got {signals:?}"
    );
    assert_eq!(signals[0].kind, signal_kind::CANCEL);
    assert_eq!(
        signals[0].tick, run.ticks_completed,
        "the recorded tick is the boundary the cancel took effect from"
    );
}

// ─────────────────── what the workflow produced is visible ───────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_detail_page_of_a_completed_run_draws_the_flock_and_its_hash() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();

    let id = create_run(&client, "preset=classic-flock&seed=42&max_ticks=200").await;
    let run = await_terminal(&pool, id).await;
    let hash = run
        .final_state_hash
        .as_deref()
        .expect("a completed run carries its hash")
        .to_owned();

    // Nothing here is seeded: every mark on this page was drawn from a row a
    // Harvest activity wrote during this test.
    client
        .get(&format!("/runs/{id}"))
        .send()
        .await
        .assert_ok()
        .assert_selector(r#"span.status-badge[data-status="completed"]"#)
        .assert_selector("svg.flock")
        .assert_selector_count("svg.flock polygon.agent", 120)
        .assert_selector("svg.trajectories polyline.trail")
        .assert_no_selector("dd.prov-final-state-hash.prov-pending")
        .assert_text("dd.prov-final-state-hash", &hash);
}
