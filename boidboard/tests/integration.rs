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

/// The same application with the Harvest management API **opted in** behind
/// `token`, which is the only configuration that mounts it at all.
fn admin_api_client(token: &str) -> TestClient {
    migrate_once();
    TestApp::new()
        .profile("dev")
        .with_db(app_pool())
        .routes(boidboard::all_routes())
        .plugin(boidboard::harvest_plugin_with_admin_api(token))
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

// ══════════════════ C1: the Harvest management API is not public ══════════════════
//
// A security review drove these three requests at a running instance with no
// headers and no cookies and got `201`, `303` and `200` back. The management
// API mounts *engine-level* control — start, cancel, pause, resume, signal —
// over every execution in the database, so an anonymous caller could:
//
// * steer a live run with a `steer` signal, making it simulate parameters its
//   `config_snapshot` does not record — the reproducibility fingerprint the
//   detail page prints then asserts something false;
// * engine-cancel an execution, which kills it *without* running
//   `finalize_run`, leaving `runs.status = 'running'` forever. The app's own
//   `POST /runs/{id}/cancel` then 422s and every open tab polls the progress
//   fragment every two seconds for good.
//
// One anonymous POST per run is therefore a permanent, unrecoverable
// self-DoS, and a second one is provenance forgery.

/// The exact requests the reviewer ran, against the plugin the application
/// actually mounts.
const ANONYMOUS_MANAGEMENT_POCS: &[(&str, &str)] = &[
    ("POST", "/api/harvest/workflows/simulation_workflow/start"),
    ("GET", "/api/harvest/workflows"),
    ("POST", "/api/harvest/ui/workflows/1/cancel"),
];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_harvest_management_api_is_not_reachable_anonymously() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();

    for (method, path) in ANONYMOUS_MANAGEMENT_POCS {
        let request = if *method == "GET" {
            client.get(path)
        } else {
            client.post(path).json(&serde_json::json!({}))
        };
        let response = request.send().await;
        assert_eq!(
            response.status.as_u16(),
            404,
            "anonymous {method} {path} must not reach Harvest's management API: \
             engine-level start/cancel/signal over every execution is not something \
             an app with no login flow can expose. Got {}",
            response.status
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_opted_in_management_api_refuses_callers_without_the_token() {
    let _serial = SERIAL.lock().await;
    let client = admin_api_client("s3cret-operator-token");

    let anonymous = client.get("/api/harvest/workflows").send().await;
    assert_eq!(
        anonymous.status.as_u16(),
        401,
        "an opted-in management API must still refuse an anonymous caller, got {}",
        anonymous.status
    );

    let wrong = client
        .get("/api/harvest/workflows")
        .header("authorization", "Bearer not-the-token")
        .send()
        .await;
    assert_eq!(
        wrong.status.as_u16(),
        401,
        "a wrong bearer must be refused, got {}",
        wrong.status
    );

    // A near-miss must not pass either: a prefix comparison would let
    // `s3cret-operator-token-and-more` through, and a length-only one would
    // let any 20-character string through.
    let near_miss = client
        .get("/api/harvest/workflows")
        .header("authorization", "Bearer s3cret-operator-token-and-more")
        .send()
        .await;
    assert_eq!(
        near_miss.status.as_u16(),
        401,
        "a token with the right prefix must be refused, got {}",
        near_miss.status
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_opted_in_management_api_admits_the_configured_token() {
    let _serial = SERIAL.lock().await;
    let client = admin_api_client("s3cret-operator-token");

    let response = client
        .get("/api/harvest/workflows")
        .header("authorization", "Bearer s3cret-operator-token")
        .send()
        .await;
    assert_ne!(
        response.status.as_u16(),
        401,
        "the configured token must be admitted, or the opt-in is useless"
    );
    assert_ne!(
        response.status.as_u16(),
        404,
        "opting in must actually mount the management API"
    );
}

// ═══════════════ H1: the tick budget has a ceiling, and says so ═══════════════
//
// `max_ticks` had a floor (`.max(1)`) and no ceiling. The reviewer submitted
// `max_ticks=2147483647` through the ordinary form and the run was accepted and
// started: at the measured ~40 ticks/s and ~9 KB of `agents` JSONB per frame,
// that is roughly 1.7 years of worker time and ~19 TB of frame rows from one
// form POST. The form's `min="1"` is client-side only, so nothing on the server
// ever looked.
//
// The same handler had a quieter version of the same bug: `max_ticks=4294967295`
// fails `parse::<i32>()` and silently became the default 300, so a user asking
// for a long run got a short one and was told nothing.

/// The reviewer's own PoC value, plus the neighbours that share its bug class.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_out_of_range_tick_budget_is_refused_with_an_explanation() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();

    for budget in ["2147483647", "100001", "0", "-5"] {
        let response = client
            .post("/runs")
            .form(&format!("preset=classic-flock&seed=1&max_ticks={budget}"))
            .send()
            .await;
        assert_eq!(
            response.status.as_u16(),
            422,
            "max_ticks={budget} must be refused, not accepted: one form POST must not \
             be able to book years of worker time. Got {}",
            response.status
        );
        assert!(
            response
                .text()
                .contains(&boidboard::routes::MAX_TICKS_CEILING.to_string()),
            "the refusal must name the ceiling so the user can pick a legal value; got: {}",
            response.text()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unreadable_tick_budget_is_refused_rather_than_silently_defaulted() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();

    // `4294967295` overflows i32; `sixty` is not a number at all. Both used to
    // fall through `parse().ok()` into the default 300, running a different
    // experiment from the one that was asked for without saying so.
    for budget in ["4294967295", "sixty", "1e6"] {
        let response = client
            .post("/runs")
            .form(&format!("preset=classic-flock&seed=1&max_ticks={budget}"))
            .send()
            .await;
        assert_eq!(
            response.status.as_u16(),
            422,
            "max_ticks={budget} is unreadable and must be reported, not silently \
             replaced by the default. Got {}",
            response.status
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_ceiling_itself_and_an_omitted_budget_are_both_accepted() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();

    // The boundary is inclusive — a rejected ceiling would be an off-by-one
    // that quietly moves the real limit.
    let at_ceiling = client
        .post("/runs")
        .form(&format!(
            "preset=classic-flock&seed=1&max_ticks={}",
            boidboard::routes::MAX_TICKS_CEILING
        ))
        .send()
        .await;
    at_ceiling.assert_status(303);
    let id = redirected_run_id(&at_ceiling);
    assert_eq!(
        load_run(&pool, id).await.max_ticks,
        boidboard::routes::MAX_TICKS_CEILING
    );
    // Do not leave a 100 000-tick run running for the rest of the suite.
    client
        .post(&format!("/runs/{id}/cancel"))
        .send()
        .await
        .assert_status(303);

    // A browser sends an emptied number field as `""`; that is still the
    // documented "use the offered default" path and must not become a 422.
    let empty = client
        .post("/runs")
        .form("preset=classic-flock&seed=1&max_ticks=")
        .send()
        .await;
    empty.assert_status(303);
    let id = redirected_run_id(&empty);
    assert_eq!(
        load_run(&pool, id).await.max_ticks,
        boidboard::views::DEFAULT_MAX_TICKS
    );
    client
        .post(&format!("/runs/{id}/cancel"))
        .send()
        .await
        .assert_status(303);
}

// ═════════════ H1 (fleet half): the bench refuses to be flooded ═════════════

/// Marks the rows this file's capacity test creates so it can take them away
/// again; the dev database is shared and committed to.
const CAPACITY_FIXTURE: &str = "capacity-guard-fixture";

/// Create cheap, undispatched `queued` runs until the bench is at its limit.
async fn fill_the_bench(conn: &mut AsyncPgConnection) {
    diesel::sql_query(
        "INSERT INTO scenarios (name, config, config_hash)
         SELECT $1, '{}'::jsonb, $1
          WHERE NOT EXISTS (SELECT 1 FROM scenarios WHERE config_hash = $1)",
    )
    .bind::<diesel::sql_types::Text, _>(CAPACITY_FIXTURE)
    .execute(conn)
    .await
    .expect("fixture scenario");

    diesel::sql_query(
        "INSERT INTO runs (scenario_id, seed, status, max_ticks, ticks_completed,
                           config_snapshot, config_hash, kernel_version)
         SELECT s.id, 0, 'queued', 1, 0, '{}'::jsonb, $1, $1
           FROM (SELECT id FROM scenarios WHERE config_hash = $1 LIMIT 1) s,
                generate_series(1, GREATEST($2 - (SELECT count(*) FROM runs
                                                   WHERE status IN ('queued','running')), 0))",
    )
    .bind::<diesel::sql_types::Text, _>(CAPACITY_FIXTURE)
    .bind::<diesel::sql_types::BigInt, _>(boidboard::routes::MAX_ACTIVE_RUNS)
    .execute(conn)
    .await
    .expect("fill the bench");
}

/// Remove every row [`fill_the_bench`] created, and nothing else.
async fn empty_the_bench(conn: &mut AsyncPgConnection) {
    diesel::sql_query("DELETE FROM runs WHERE config_hash = $1")
        .bind::<diesel::sql_types::Text, _>(CAPACITY_FIXTURE)
        .execute(conn)
        .await
        .expect("drain the bench");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn submissions_are_refused_once_the_bench_is_full_and_accepted_again_after() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();
    let mut conn = pool.get().await.expect("checkout");

    // A previous crashed run of this test would otherwise poison every test in
    // the file, so start from a known bench.
    empty_the_bench(&mut conn).await;
    fill_the_bench(&mut conn).await;

    let refused = client
        .post("/runs")
        .form("preset=classic-flock&seed=1&max_ticks=1")
        .send()
        .await;
    let body = refused.text();
    let status = refused.status.as_u16();

    // Put the bench back before asserting, so a failure here does not leave the
    // database saturated for every test that follows.
    empty_the_bench(&mut conn).await;

    assert_eq!(
        status,
        503,
        "with {} runs already in flight a new submission must be refused: an \
         unbounded number of concurrent runs is the same worker-years as an \
         unbounded single run, just spread out. Body: {body}",
        boidboard::routes::MAX_ACTIVE_RUNS
    );
    assert!(
        body.contains("bench is full"),
        "the refusal must explain itself and say what to do; got: {body}"
    );

    // The guard is a capacity limit, not a kill switch: with the bench drained
    // the very same submission succeeds.
    let accepted = client
        .post("/runs")
        .form("preset=classic-flock&seed=1&max_ticks=1")
        .send()
        .await;
    accepted.assert_status(303);
    await_terminal(&pool, redirected_run_id(&accepted)).await;
}
// ══════════ H2: the detail page's frame budget is a query budget ══════════
//
// `DETAIL_FRAME_BUDGET` claimed to be "the *query* budget, and it is the one
// that matters for latency", while `frames_for_run` loaded **every** frame of
// the run and thinned the result in Rust afterwards. Measured on a 301-frame
// run: 2.66 MB transferred to render a 57 KB page, linear in run length and
// unbounded. Combined with an uncapped `max_ticks`, one `GET /runs/{id}` became
// a multi-gigabyte allocation.
//
// Proving the budget moved into SQL needs an observation that separates
// "sampled by tick, in the WHERE clause" from "sampled by index, in Rust". A
// contiguous run cannot tell them apart — tick == index there, which is exactly
// why the bug survived. A run with a **hole** can: index sampling lands on
// ticks the query never named.

/// Marks the fixture rows this section creates so it can take them away again.
const FRAME_BUDGET_FIXTURE: &str = "frame-budget-fixture";

/// A completed run carrying `ticks` frames at the ticks `keep` selects,
/// inserted directly rather than simulated — this section is about the *read*
/// path, and 3 000 real ticks would cost a minute of worker time to prove
/// nothing extra.
async fn seed_frame_fixture(conn: &mut AsyncPgConnection, highest: i32, keep: &str) -> i64 {
    #[derive(diesel::QueryableByName)]
    struct Id {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        id: i64,
    }

    diesel::sql_query(
        "INSERT INTO scenarios (name, config, config_hash)
         SELECT $1, '{}'::jsonb, $1
          WHERE NOT EXISTS (SELECT 1 FROM scenarios WHERE config_hash = $1)",
    )
    .bind::<diesel::sql_types::Text, _>(FRAME_BUDGET_FIXTURE)
    .execute(conn)
    .await
    .expect("fixture scenario");

    let rows: Vec<Id> = diesel::sql_query(
        "INSERT INTO runs (scenario_id, seed, status, max_ticks, ticks_completed,
                           config_snapshot, config_hash, kernel_version)
         SELECT s.id, 0, 'completed', $2, $2, '{}'::jsonb, $1, $1
           FROM (SELECT id FROM scenarios WHERE config_hash = $1 LIMIT 1) s
         RETURNING id",
    )
    .bind::<diesel::sql_types::Text, _>(FRAME_BUDGET_FIXTURE)
    .bind::<diesel::sql_types::Integer, _>(highest)
    .load(conn)
    .await
    .expect("fixture run");
    let id = rows[0].id;

    diesel::sql_query(format!(
        "INSERT INTO frames (run_id, tick, agents, state_hash, metrics)
         SELECT $1, g, '[]'::jsonb, 'fixture-' || g, '{{}}'::jsonb
           FROM generate_series(0, $2) g
          WHERE {keep}"
    ))
    .bind::<diesel::sql_types::BigInt, _>(id)
    .bind::<diesel::sql_types::Integer, _>(highest)
    .execute(conn)
    .await
    .expect("fixture frames");

    id
}

/// Remove every row [`seed_frame_fixture`] created, and nothing else.
async fn drop_frame_fixtures(conn: &mut AsyncPgConnection) {
    diesel::sql_query(
        "DELETE FROM frames WHERE run_id IN (SELECT id FROM runs WHERE config_hash = $1)",
    )
    .bind::<diesel::sql_types::Text, _>(FRAME_BUDGET_FIXTURE)
    .execute(conn)
    .await
    .expect("drop fixture frames");
    diesel::sql_query("DELETE FROM runs WHERE config_hash = $1")
        .bind::<diesel::sql_types::Text, _>(FRAME_BUDGET_FIXTURE)
        .execute(conn)
        .await
        .expect("drop fixture runs");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_detail_query_asks_for_at_most_the_budget_and_only_the_ticks_it_named() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();
    let mut conn = pool.get().await.expect("checkout");
    drop_frame_fixtures(&mut conn).await;

    // ── a long, healthy run: the budget bounds the result ──
    let long = seed_frame_fixture(&mut conn, 3000, "true").await;
    assert_eq!(
        frames_for_run(&mut conn, long, None)
            .await
            .expect("all frames")
            .len(),
        3001,
        "the fixture really does hold 3 001 frames — otherwise the bound below \
         is bounding nothing"
    );

    let named = boidboard::repositories::frame_queries::sampled_ticks(0, 3000, 120);
    assert!(
        named.len() <= 120,
        "the tick list is what goes into `tick = ANY(...)`, so it is the ceiling \
         on the rows Postgres can build; it must itself be bounded, got {}",
        named.len()
    );

    let budgeted = frames_for_run(&mut conn, long, Some(120))
        .await
        .expect("budgeted frames");
    assert_eq!(
        budgeted.iter().map(|f| f.tick).collect::<Vec<_>>(),
        named,
        "a 3 001-frame run must come back as exactly the ≤120 frames the query \
         named, endpoints included"
    );

    // ── a run with a hole: index sampling and tick sampling disagree ──
    //
    // Ticks 0 and 51..=100 exist; 1..=50 are missing. Sampling by *tick* can
    // only ever return ticks from `sampled_ticks(0, 100, 5)`; sampling by
    // *index* over the 51 loaded rows lands on ticks 62 and 88, which the query
    // never asked for — which is the whole difference between a budget the
    // database enforces and a budget applied to rows already transferred.
    let gappy = seed_frame_fixture(&mut conn, 100, "g = 0 OR g > 50").await;
    let named_gappy = boidboard::repositories::frame_queries::sampled_ticks(0, 100, 5);
    assert_eq!(named_gappy, vec![0, 25, 50, 75, 100]);

    let sampled = frames_for_run(&mut conn, gappy, Some(5))
        .await
        .expect("gappy frames");
    let ticks: Vec<i32> = sampled.iter().map(|f| f.tick).collect();

    drop_frame_fixtures(&mut conn).await;

    assert!(
        ticks.iter().all(|t| named_gappy.contains(t)),
        "every returned frame must be one the query named ({named_gappy:?}); got \
         {ticks:?}. A tick outside that list can only have arrived by loading \
         every frame and thinning afterwards."
    );
    assert_eq!(
        ticks,
        vec![0, 75, 100],
        "the frames that exist among the named ticks, and nothing else"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_detail_page_of_a_long_run_still_renders_from_the_budgeted_query() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();

    let id = create_run(&client, "preset=classic-flock&seed=11&max_ticks=400").await;
    let run = await_terminal(&pool, id).await;
    assert_eq!(run.status, status::COMPLETED, "error={:?}", run.error);

    // 401 stored frames, at most 120 drawn: the page is the same page it was
    // before the query learned to do the sampling.
    let mut conn = pool.get().await.expect("checkout");
    assert_eq!(
        frames_for_run(&mut conn, id, None)
            .await
            .expect("all")
            .len(),
        401
    );
    assert!(
        frames_for_run(&mut conn, id, Some(120))
            .await
            .expect("budgeted")
            .len()
            <= 120
    );

    client
        .get(&format!("/runs/{id}"))
        .send()
        .await
        .assert_ok()
        .assert_selector("svg.flock")
        .assert_selector("svg.trajectories polyline.trail")
        .assert_selector_count("svg.flock polygon.agent", 120);
}

// ═══════════ M1: the forms work under the profile that turns CSRF on ═══════════
//
// No test at any layer ran under a production profile, which is why three
// separate "would this work in production?" findings went unnoticed at once.
// The review demonstrated both halves of this one: on `dev` (CSRF off) a
// cross-origin `POST /runs` from `Origin: https://evil.example` succeeded with a
// `303`, and with CSRF on the app's own form got a `403`, because the
// hand-written markup emitted no `_csrf` field. Either the app was forgeable or
// it was broken, decided by a single config toggle that `autumn.toml` did not
// state.
//
// `autumn.toml` now sets `[security.csrf] enabled = true` explicitly rather than
// inheriting it from the profile name — a custom profile such as `staging` gets
// no smart defaults at all — so this test builds the configuration that file
// declares and drives the real form through it.

/// A client under the `prod` profile with CSRF on, matching `autumn.toml`.
///
/// No Harvest plugin: a `prod` boot deliberately refuses to auto-migrate, and
/// this test is about the request pipeline rather than the worker. A run created
/// without a runtime stays `queued` and is deleted at the end of the test.
fn prod_profile_client() -> TestClient {
    migrate_once();
    let mut config = autumn_web::config::AutumnConfig {
        profile: Some("prod".to_owned()),
        ..autumn_web::config::AutumnConfig::default()
    };
    config.security.csrf.enabled = true;
    config.security.signing_secret.secret =
        Some("test-signing-secret-for-the-prod-profile-guard".to_owned());
    config.database.url = Some(DEV_DB_URL.to_owned());
    // The `prod` profile stops trusting `localhost` implicitly — a real deploy
    // must name its hostnames under `[security.trusted_hosts]` or every request
    // is a `400 Invalid Host header`. Found by writing this test, which is the
    // point of having one.
    config.security.trusted_hosts.hosts = vec!["localhost".to_owned()];

    TestApp::new()
        .config(config)
        .with_db(app_pool())
        .routes(boidboard::all_routes())
        .build()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn under_a_csrf_enabled_profile_a_forged_cross_origin_post_is_refused() {
    let _serial = SERIAL.lock().await;
    let client = prod_profile_client();

    let forged = client
        .post("/runs")
        .header("host", "localhost")
        .header("origin", "https://evil.example")
        .form("preset=classic-flock&seed=1&max_ticks=1")
        .send()
        .await;

    assert_eq!(
        forged.status.as_u16(),
        403,
        "a POST carrying no CSRF token must be refused; on the dev profile this \
         same request returned 303 and created a run. Got {}",
        forged.status
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn under_a_csrf_enabled_profile_the_apps_own_form_still_submits() {
    let _serial = SERIAL.lock().await;
    let client = prod_profile_client();
    let pool = client.state().pool().expect("app pool").clone();

    // Fetch the form the way a browser does: the response sets the CSRF cookie
    // (the client's jar replays it) and embeds the matching token.
    let form = client
        .get("/runs/new")
        .header("host", "localhost")
        .send()
        .await;
    form.assert_ok();
    let tokens = form.selector_attr(r#"form.new-run input[name="_csrf"]"#, "value");
    let token = tokens.into_iter().flatten().next().expect(
        "with CSRF enabled the new-run form must embed a `_csrf` field, or \
             every submission is a 403 and the app is simply broken in production",
    );
    assert!(!token.is_empty(), "an empty token is worse than none");

    let submitted = client
        .post("/runs")
        .header("host", "localhost")
        .form(&format!(
            "_csrf={token}&preset=classic-flock&seed=1&max_ticks=1"
        ))
        .send()
        .await;

    assert_ne!(
        submitted.status.as_u16(),
        403,
        "the app's own form must not be refused by its own CSRF layer: {}",
        submitted.text()
    );
    submitted.assert_status(303);

    // No worker is mounted here, so nothing will ever finish this run; take it
    // back out rather than leaving it holding a slot against MAX_ACTIVE_RUNS.
    let id = redirected_run_id(&submitted);
    let mut conn = pool.get().await.expect("checkout");
    diesel::delete(runs::table.find(id))
        .execute(&mut conn)
        .await
        .expect("remove the undispatched run");
}

// ═════════ find-or-create by config hash is no longer a race ═════════
//
// `create_run` read `scenarios` by `config_hash` and inserted when it found
// nothing — a time-of-check/time-of-use gap. Two submissions of the same preset
// arriving together both saw no row and both inserted one, leaving two
// scenarios that are the same scenario and quietly breaking the only question
// `config_hash` exists to answer. The insert is now
// `ON CONFLICT (config_hash) DO NOTHING` plus a re-read, which Postgres only
// accepts against a UNIQUE index — so the invariant lives in the schema, where
// a future read-then-write cannot get it wrong either.

const TOCTOU_FIXTURE: &str = "config-hash-uniqueness-fixture";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_schema_refuses_two_scenarios_with_the_same_config_hash() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();
    let mut conn = pool.get().await.expect("checkout");

    let insert = || {
        diesel::sql_query(
            "INSERT INTO scenarios (name, config, config_hash) VALUES ($1, '{}'::jsonb, $1)",
        )
        .bind::<diesel::sql_types::Text, _>(TOCTOU_FIXTURE)
    };

    // Clear any residue from an earlier failed run of this test.
    diesel::sql_query("DELETE FROM scenarios WHERE config_hash = $1")
        .bind::<diesel::sql_types::Text, _>(TOCTOU_FIXTURE)
        .execute(&mut conn)
        .await
        .expect("clear fixture");

    insert()
        .execute(&mut conn)
        .await
        .expect("the first insert wins");
    let second = insert().execute(&mut conn).await;

    diesel::sql_query("DELETE FROM scenarios WHERE config_hash = $1")
        .bind::<diesel::sql_types::Text, _>(TOCTOU_FIXTURE)
        .execute(&mut conn)
        .await
        .expect("clear fixture");

    assert!(
        second.is_err(),
        "a second scenario with the same config_hash must be impossible: with only \
         a plain index it succeeds, and then `ON CONFLICT (config_hash)` has no \
         constraint to resolve against"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_submissions_of_one_preset_all_land_on_one_scenario() {
    let _serial = SERIAL.lock().await;
    let client = integration_client();
    let pool = client.state().pool().expect("app pool").clone();

    // Four submissions of the same preset at once — the shape that used to
    // produce duplicate scenarios.
    let submissions = submit_the_same_preset_repeatedly(&client, 4).await;
    for id in &submissions {
        await_terminal(&pool, *id).await;
    }

    let mut conn = pool.get().await.expect("checkout");
    let mut scenario_ids = Vec::new();
    for id in &submissions {
        scenario_ids.push(load_run(&pool, *id).await.scenario_id);
    }
    scenario_ids.dedup();
    assert_eq!(
        scenario_ids.len(),
        1,
        "every run of one preset must point at the same scenario row, got \
         {scenario_ids:?}"
    );

    #[derive(diesel::QueryableByName)]
    struct Count {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }
    let rows: Vec<Count> = diesel::sql_query(
        "SELECT count(*) AS count FROM scenarios
          WHERE config_hash = (SELECT config_hash FROM scenarios WHERE id = $1)",
    )
    .bind::<diesel::sql_types::BigInt, _>(scenario_ids[0])
    .load(&mut conn)
    .await
    .expect("scenario count");
    assert_eq!(
        rows[0].count, 1,
        "and exactly one scenario carries that config hash"
    );
}

/// Submit the same preset `n` times back to back and return the run ids.
///
/// Sequential rather than genuinely parallel: `TestClient` drives the router
/// directly, and what this asserts — one scenario per config hash — is a
/// property of the *statement*, which the schema now enforces regardless of
/// interleaving. The unique-index test above is what proves the interleaved case.
async fn submit_the_same_preset_repeatedly(client: &TestClient, n: usize) -> Vec<i64> {
    let mut ids = Vec::with_capacity(n);
    for _ in 0..n {
        ids.push(create_run(client, "preset=highway&seed=3&max_ticks=1").await);
    }
    ids
}
