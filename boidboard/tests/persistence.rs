//! Persistence-layer acceptance tests (AC-32 persistence half, AC-37…AC-40).
//!
//! These run against the **live** Postgres test database. They are deliberately
//! *not* `#[ignore]`d: AC-40 requires them to actually execute, and the whole
//! file must pass twice consecutively, which is what proves the isolation is
//! real rather than a lucky first run.
//!
//! Isolation strategy: every test builds a `TestApp::with_transactional_db`
//! client. That opens a `max_size(1)` pool whose single connection is already
//! inside a `begin_test_transaction`; dropping the client closes the connection
//! and rolls everything back. Nothing is ever committed, so no TRUNCATE and no
//! cleanup code is needed.

use autumn_web::prelude::Patch;
use autumn_web::reexports::diesel;
use autumn_web::test::{TestApp, TestClient};
use boidboard::models::run::status as run_status;
use boidboard::models::run_signal::kind as signal_kind;
use boidboard::models::{
    NewFrame, NewRun, NewRunSignal, NewScenario, Run, Scenario, UpdateRun, UpdateScenario,
};
use boidboard::repositories::{
    PgRunRepository, PgRunSignalRepository, PgScenarioRepository, RunRepository as _,
    RunSignalRepository as _, ScenarioRepository as _, frames_for_run, insert_frames_idempotent,
    max_tick, tick_gaps,
};
use diesel_async::RunQueryDsl as _;

const TEST_DB_URL: &str = "postgres://boid:boid@127.0.0.1:5432/boidboard_test";

/// Apply the embedded migrations to the test database exactly once per test
/// binary. This commits (migrations are DDL outside the per-test transaction),
/// which is why it must be idempotent — it is, Diesel tracks applied versions.
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

/// One `TEXT` column, for `information_schema` probes.
#[derive(diesel::QueryableByName)]
struct TextRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    value: String,
}

async fn text_rows(conn: &mut diesel_async::AsyncPgConnection, sql: &str) -> Vec<String> {
    diesel::sql_query(sql)
        .load::<TextRow>(conn)
        .await
        .expect("information_schema probe")
        .into_iter()
        .map(|r| r.value)
        .collect()
}

// ───────────────────────────── AC-37 ─────────────────────────────

#[tokio::test]
async fn ac37_migrations_create_the_four_tables() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let mut conn = pool.get().await.expect("checkout connection");

    let tables = text_rows(
        &mut conn,
        "SELECT table_name::text AS value
           FROM information_schema.tables
          WHERE table_schema = 'public'
            AND table_name IN ('scenarios', 'runs', 'frames', 'run_signals')
          ORDER BY table_name",
    )
    .await;

    assert_eq!(
        tables,
        vec!["frames", "run_signals", "runs", "scenarios"],
        "all four Boidboard tables must exist via embedded migrations"
    );
}

#[tokio::test]
async fn ac37_frames_has_unique_run_id_tick() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let mut conn = pool.get().await.expect("checkout connection");

    let cols = text_rows(
        &mut conn,
        "SELECT kcu.column_name::text AS value
           FROM information_schema.table_constraints tc
           JOIN information_schema.key_column_usage kcu
             ON tc.constraint_name = kcu.constraint_name
            AND tc.constraint_schema = kcu.constraint_schema
          WHERE tc.table_schema = 'public'
            AND tc.table_name = 'frames'
            AND tc.constraint_type = 'UNIQUE'
          ORDER BY kcu.ordinal_position",
    )
    .await;

    assert_eq!(
        cols,
        vec!["run_id", "tick"],
        "frames must carry UNIQUE (run_id, tick) — the AC-32 idempotency guarantee"
    );
}

// ─────────────────── canonical_config_hash ───────────────────

#[test]
fn canonical_config_hash_is_insensitive_to_json_key_order() {
    let a = serde_json::json!({
        "agents": 40,
        "cohesion": 0.5,
        "world": { "width": 200.0, "height": 100.0 }
    });
    let b = serde_json::json!({
        "world": { "height": 100.0, "width": 200.0 },
        "cohesion": 0.5,
        "agents": 40
    });

    assert_eq!(
        boidboard::models::canonical_config_hash(&a),
        boidboard::models::canonical_config_hash(&b),
        "reordering keys at any nesting depth must not change the hash"
    );
}

#[test]
fn canonical_config_hash_is_sensitive_to_every_value_change() {
    let base = serde_json::json!({
        "agents": 40,
        "cohesion": 0.5,
        "world": { "width": 200.0, "height": 100.0 }
    });
    let base_hash = boidboard::models::canonical_config_hash(&base);

    let mutations = [
        serde_json::json!({ "agents": 41, "cohesion": 0.5, "world": { "width": 200.0, "height": 100.0 } }),
        serde_json::json!({ "agents": 40, "cohesion": 0.6, "world": { "width": 200.0, "height": 100.0 } }),
        serde_json::json!({ "agents": 40, "cohesion": 0.5, "world": { "width": 200.0, "height": 100.5 } }),
        serde_json::json!({ "agents": 40, "cohesion": 0.5, "world": { "width": 200.0 } }),
        serde_json::json!({ "agents": 40, "cohesion": 0.5, "world": { "width": 200.0, "height": 100.0 }, "extra": null }),
    ];
    for m in &mutations {
        assert_ne!(
            boidboard::models::canonical_config_hash(m),
            base_hash,
            "changing any value must change the hash: {m}"
        );
    }
}

#[test]
fn canonical_config_hash_is_a_pinned_sha256_of_the_canonical_form() {
    // The canonical form of an object is its keys in sorted order, so both of
    // these serialise to `{"a":2,"b":1}`, whose SHA-256 is pinned here. Pinning
    // the digest (not just equality) makes the hash stable across processes and
    // across releases — provenance records stay comparable forever.
    let reordered = serde_json::json!({ "b": 1, "a": 2 });
    assert_eq!(
        boidboard::models::canonical_config_hash(&reordered),
        "d3626ac30a87e6f7a6428233b3c68299976865fa5508e4267c5415c76af7a772"
    );

    let config = serde_json::json!({
        "world": { "width": 200.0, "height": 100.0 },
        "cohesion": 0.5,
        "agents": 40
    });
    assert_eq!(
        boidboard::models::canonical_config_hash(&config),
        "0f0b1c185c4d2628a7a3d25102d5c04bfd8467f05c40a5cf70be70ea748f43f8"
    );
}

// Array ordering is data, not incidental: two configs that differ only in the
// order of an obstacle list are genuinely different scenarios.
#[test]
fn canonical_config_hash_respects_array_order() {
    let a = serde_json::json!({ "obstacles": [1, 2] });
    let b = serde_json::json!({ "obstacles": [2, 1] });
    assert_ne!(
        boidboard::models::canonical_config_hash(&a),
        boidboard::models::canonical_config_hash(&b)
    );
}

// ───────────────────────── fixtures ─────────────────────────

/// A representative scenario configuration.
fn sample_config() -> serde_json::Value {
    serde_json::json!({
        "agent_count": 120,
        "world": { "width": 400.0, "height": 300.0 },
        "weights": { "separation": 1.5, "alignment": 1.0, "cohesion": 0.9 },
        "max_speed": 4.0
    })
}

/// Insert a scenario through the generated repository and return it.
async fn seed_scenario(
    pool: &diesel_async::pooled_connection::deadpool::Pool<diesel_async::AsyncPgConnection>,
    name: &str,
    config: serde_json::Value,
) -> Scenario {
    let repo = PgScenarioRepository::with_pool_untracked(pool.clone());
    repo.save(&NewScenario {
        name: name.to_owned(),
        config_hash: boidboard::models::canonical_config_hash(&config),
        config,
    })
    .await
    .expect("scenario saves")
}

/// Insert a run whose `config_snapshot` is a copy of the scenario's config
/// taken *now* — the copy is the whole point of AC-39.
async fn seed_run(
    pool: &diesel_async::pooled_connection::deadpool::Pool<diesel_async::AsyncPgConnection>,
    scenario: &Scenario,
    seed: i64,
) -> Run {
    let repo = PgRunRepository::with_pool_untracked(pool.clone());
    repo.save(&NewRun {
        scenario_id: scenario.id,
        seed,
        status: run_status::QUEUED.to_owned(),
        max_ticks: 1_000,
        ticks_completed: 0,
        config_snapshot: scenario.config.clone(),
        config_hash: scenario.config_hash.clone(),
        kernel_version: boidboard::KERNEL_VERSION.to_owned(),
        final_state_hash: None,
        error: None,
        workflow_execution_id: None,
    })
    .await
    .expect("run saves")
}

// ───────────────────────────── AC-38 ─────────────────────────────

#[tokio::test]
async fn ac38_run_round_trips_its_full_provenance_record() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let scenario = seed_scenario(&pool, "Classic Flock", sample_config()).await;
    let run = seed_run(&pool, &scenario, 0x5EED_0BE1).await;

    let runs = PgRunRepository::with_pool_untracked(pool.clone());
    let completed = runs
        .update(
            run.id,
            &UpdateRun {
                status: Patch::Set(run_status::COMPLETED.to_owned()),
                ticks_completed: Patch::Set(1_000),
                final_state_hash: Patch::Set(Some("f00dcafe".to_owned())),
                workflow_execution_id: Patch::Set(Some("wf-exec-7".to_owned())),
                ..Default::default()
            },
        )
        .await
        .expect("run updates to completed");

    // Re-read from the database rather than trusting the returned row.
    let stored = runs
        .find_by_id(completed.id)
        .await
        .expect("find_by_id query")
        .expect("run exists");

    assert_eq!(stored.kernel_version, boidboard::KERNEL_VERSION);
    assert_eq!(
        stored.config_hash,
        boidboard::models::canonical_config_hash(&sample_config())
    );
    assert_eq!(stored.seed, 0x5EED_0BE1);
    assert_eq!(stored.final_state_hash.as_deref(), Some("f00dcafe"));
    assert_eq!(stored.status, run_status::COMPLETED);
    assert_eq!(stored.ticks_completed, 1_000);
    assert_eq!(stored.max_ticks, 1_000);
    assert_eq!(stored.scenario_id, scenario.id);
    assert_eq!(stored.config_snapshot, sample_config());
    assert_eq!(stored.workflow_execution_id.as_deref(), Some("wf-exec-7"));
    assert_eq!(stored.error, None);
}

// ───────────────────────────── AC-39 ─────────────────────────────

/// The regression test for the mutation bug: a run must own a *copy* of the
/// configuration it executed, never a live view of its scenario.
#[tokio::test]
async fn ac39_editing_a_scenario_cannot_mutate_an_existing_runs_config() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let original_config = sample_config();
    let scenario = seed_scenario(&pool, "Classic Flock", original_config.clone()).await;
    let run = seed_run(&pool, &scenario, 42).await;

    let runs = PgRunRepository::with_pool_untracked(pool.clone());
    runs.update(
        run.id,
        &UpdateRun {
            status: Patch::Set(run_status::COMPLETED.to_owned()),
            ticks_completed: Patch::Set(1_000),
            final_state_hash: Patch::Set(Some("deadbeef".to_owned())),
            ..Default::default()
        },
    )
    .await
    .expect("run completes");

    // Now edit the scenario out from under the completed run — a different
    // agent count, different weights, a different everything.
    let edited_config = serde_json::json!({
        "agent_count": 5,
        "world": { "width": 10.0, "height": 10.0 },
        "weights": { "separation": 0.0, "alignment": 0.0, "cohesion": 0.0 },
        "max_speed": 0.1
    });
    let scenarios = PgScenarioRepository::with_pool_untracked(pool.clone());
    let edited = scenarios
        .update(
            scenario.id,
            &UpdateScenario {
                name: Patch::Set("Classic Flock (edited)".to_owned()),
                config_hash: Patch::Set(boidboard::models::canonical_config_hash(&edited_config)),
                config: Patch::Set(edited_config.clone()),
            },
        )
        .await
        .expect("scenario updates");

    // The edit really happened…
    assert_eq!(edited.config, edited_config);
    assert_ne!(edited.config_hash, scenario.config_hash);

    // …and the completed run is completely untouched by it.
    let stored = runs
        .find_by_id(run.id)
        .await
        .expect("find_by_id query")
        .expect("run still exists");

    assert_eq!(
        stored.config_snapshot, original_config,
        "the run's config_snapshot must still be the config it actually ran"
    );
    assert_eq!(
        stored.config_hash,
        boidboard::models::canonical_config_hash(&original_config),
        "the run's config_hash must still describe the config it actually ran"
    );
    assert_eq!(stored.final_state_hash.as_deref(), Some("deadbeef"));
    assert_eq!(stored.seed, 42);
    assert_eq!(stored.kernel_version, boidboard::KERNEL_VERSION);
}

// ───────────────────────── AC-32 (persistence half) ─────────────────────────

/// `n` contiguous frames starting at `from`, deterministic content so the
/// second insert really is byte-identical to the first.
fn frame_batch(run_id: i64, from: i32, n: i32) -> Vec<NewFrame> {
    (from..from + n)
        .map(|tick| NewFrame {
            run_id,
            tick,
            agents: serde_json::json!([{ "x": tick, "y": -tick }]),
            state_hash: format!("hash-{tick}"),
            metrics: serde_json::json!({ "polarization": 1.0 }),
        })
        .collect()
}

#[tokio::test]
async fn ac32_reinserting_the_same_frames_is_a_no_op() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let scenario = seed_scenario(&pool, "Classic Flock", sample_config()).await;
    let run = seed_run(&pool, &scenario, 7).await;
    let mut conn = pool.get().await.expect("checkout connection");

    let batch = frame_batch(run.id, 0, 10);

    let first = insert_frames_idempotent(&mut conn, run.id, &batch)
        .await
        .expect("first insert");
    assert_eq!(first, 10, "the first insert writes every frame");

    // At-least-once activity delivery: the very same batch arrives again.
    let second = insert_frames_idempotent(&mut conn, run.id, &batch)
        .await
        .expect("second insert");
    assert_eq!(
        second, 0,
        "re-inserting the same (run_id, tick) inserts nothing"
    );

    let stored = frames_for_run(&mut conn, run.id, None)
        .await
        .expect("read frames back");
    assert_eq!(stored.len(), 10, "exactly one row survives per tick");
    assert_eq!(
        stored.iter().map(|f| f.tick).collect::<Vec<_>>(),
        (0..10).collect::<Vec<_>>(),
        "frames come back in ascending tick order"
    );
    // The rows are the originals, not silently overwritten.
    assert_eq!(stored[3].state_hash, "hash-3");
}

#[tokio::test]
async fn ac32_partially_overlapping_batches_insert_only_the_new_ticks() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let scenario = seed_scenario(&pool, "Classic Flock", sample_config()).await;
    let run = seed_run(&pool, &scenario, 8).await;
    let mut conn = pool.get().await.expect("checkout connection");

    insert_frames_idempotent(&mut conn, run.id, &frame_batch(run.id, 0, 10))
        .await
        .expect("ticks 0..10");

    // A retried batch that overlaps ticks 5..10 and extends to 15.
    let inserted = insert_frames_idempotent(&mut conn, run.id, &frame_batch(run.id, 5, 10))
        .await
        .expect("ticks 5..15");
    assert_eq!(inserted, 5, "only the five genuinely new ticks are written");

    assert_eq!(
        max_tick(&mut conn, run.id).await.expect("max_tick"),
        Some(14),
        "max_tick is the resume cursor"
    );
    assert_eq!(
        tick_gaps(&mut conn, run.id).await.expect("tick_gaps"),
        Vec::<i32>::new(),
        "a contiguous run has no gaps"
    );
}

#[tokio::test]
async fn ac32_max_tick_is_none_for_a_run_with_no_frames() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let scenario = seed_scenario(&pool, "Classic Flock", sample_config()).await;
    let run = seed_run(&pool, &scenario, 9).await;
    let mut conn = pool.get().await.expect("checkout connection");

    assert_eq!(max_tick(&mut conn, run.id).await.expect("max_tick"), None);
    assert_eq!(
        tick_gaps(&mut conn, run.id).await.expect("tick_gaps"),
        Vec::<i32>::new()
    );
    assert!(
        frames_for_run(&mut conn, run.id, None)
            .await
            .expect("frames")
            .is_empty()
    );
}

#[tokio::test]
async fn ac32_tick_gaps_reports_every_missing_tick() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let scenario = seed_scenario(&pool, "Classic Flock", sample_config()).await;
    let run = seed_run(&pool, &scenario, 10).await;
    let mut conn = pool.get().await.expect("checkout connection");

    // Ticks 0,1,2 … 5,6 … 9 — holes at 3,4 and 7,8.
    let gappy: Vec<NewFrame> = frame_batch(run.id, 0, 10)
        .into_iter()
        .filter(|f| !matches!(f.tick, 3 | 4 | 7 | 8))
        .collect();
    insert_frames_idempotent(&mut conn, run.id, &gappy)
        .await
        .expect("insert gappy frames");

    assert_eq!(
        tick_gaps(&mut conn, run.id).await.expect("tick_gaps"),
        vec![3, 4, 7, 8],
        "every missing tick between the first and last stored tick is reported"
    );
}

#[tokio::test]
async fn ac32_gaps_are_scoped_to_one_run() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let scenario = seed_scenario(&pool, "Classic Flock", sample_config()).await;
    let run_a = seed_run(&pool, &scenario, 11).await;
    let run_b = seed_run(&pool, &scenario, 12).await;
    let mut conn = pool.get().await.expect("checkout connection");

    // Two runs may hold the same tick numbers — uniqueness is per (run_id, tick).
    insert_frames_idempotent(&mut conn, run_a.id, &frame_batch(run_a.id, 0, 5))
        .await
        .expect("run a frames");
    let inserted_b = insert_frames_idempotent(&mut conn, run_b.id, &frame_batch(run_b.id, 0, 5))
        .await
        .expect("run b frames");
    assert_eq!(
        inserted_b, 5,
        "the same tick numbers in another run are new rows"
    );

    assert_eq!(max_tick(&mut conn, run_a.id).await.expect("max a"), Some(4));
    assert_eq!(max_tick(&mut conn, run_b.id).await.expect("max b"), Some(4));
    assert_eq!(
        frames_for_run(&mut conn, run_a.id, None)
            .await
            .expect("a frames")
            .len(),
        5
    );
}

#[tokio::test]
async fn ac32_frames_for_run_subsamples_evenly_to_a_rendering_budget() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let scenario = seed_scenario(&pool, "Classic Flock", sample_config()).await;
    let run = seed_run(&pool, &scenario, 13).await;
    let mut conn = pool.get().await.expect("checkout connection");

    insert_frames_idempotent(&mut conn, run.id, &frame_batch(run.id, 0, 101))
        .await
        .expect("insert 101 frames");

    let sampled = frames_for_run(&mut conn, run.id, Some(5))
        .await
        .expect("subsampled frames");
    assert_eq!(
        sampled.iter().map(|f| f.tick).collect::<Vec<_>>(),
        vec![0, 25, 50, 75, 100],
        "evenly spaced, first and last always included"
    );

    // A budget bigger than the run returns everything, not a padded list.
    let all = frames_for_run(&mut conn, run.id, Some(1_000))
        .await
        .expect("oversized budget");
    assert_eq!(all.len(), 101);

    // Degenerate budgets are handled rather than panicking.
    assert_eq!(
        frames_for_run(&mut conn, run.id, Some(1))
            .await
            .expect("budget of one")
            .len(),
        1
    );
    assert!(
        frames_for_run(&mut conn, run.id, Some(0))
            .await
            .expect("budget of zero")
            .is_empty()
    );
}

// ───────────────────────────── AC-40 ─────────────────────────────

/// The whole persistence layer end to end: create scenario → create run →
/// insert frames → read back → update status. Runs against the live Postgres.
///
/// The second half of AC-40 — that the suite passes twice consecutively — is a
/// property of *how* these tests are isolated rather than of any one test: each
/// one runs inside a transaction that is rolled back when its client drops, so
/// nothing is left behind for the next run to trip over.
#[tokio::test]
async fn ac40_full_repository_round_trip_against_live_postgres() {
    let client = test_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    // ── create scenario ──
    let config = sample_config();
    let scenarios = PgScenarioRepository::with_pool_untracked(pool.clone());
    let scenario = scenarios
        .save(&NewScenario {
            name: "Nervous Swarm".to_owned(),
            config: config.clone(),
            config_hash: boidboard::models::canonical_config_hash(&config),
        })
        .await
        .expect("scenario saves");
    assert!(scenario.id > 0);
    assert_eq!(
        scenarios
            .find_by_config_hash(scenario.config_hash.clone())
            .await
            .expect("derived query")
            .len(),
        1,
        "the derived find_by_config_hash query works"
    );

    // ── create run ──
    let runs = PgRunRepository::with_pool_untracked(pool.clone());
    let run = runs
        .save(&NewRun {
            scenario_id: scenario.id,
            seed: 1_234_567_890,
            status: run_status::RUNNING.to_owned(),
            max_ticks: 200,
            ticks_completed: 0,
            config_snapshot: scenario.config.clone(),
            config_hash: scenario.config_hash.clone(),
            kernel_version: boidboard::KERNEL_VERSION.to_owned(),
            final_state_hash: None,
            error: None,
            workflow_execution_id: Some("wf-42".to_owned()),
        })
        .await
        .expect("run saves");
    assert_eq!(
        runs.find_by_scenario_id(scenario.id)
            .await
            .expect("derived query")
            .len(),
        1
    );

    // ── insert frames, in two batches, with a replayed overlap ──
    {
        let mut conn = pool.get().await.expect("checkout connection");
        assert_eq!(
            insert_frames_idempotent(&mut conn, run.id, &frame_batch(run.id, 0, 100))
                .await
                .expect("batch 1"),
            100
        );
        assert_eq!(
            insert_frames_idempotent(&mut conn, run.id, &frame_batch(run.id, 100, 100))
                .await
                .expect("batch 2"),
            100
        );
        // Harvest redelivers batch 2 — must change nothing.
        assert_eq!(
            insert_frames_idempotent(&mut conn, run.id, &frame_batch(run.id, 100, 100))
                .await
                .expect("batch 2 redelivered"),
            0
        );

        // ── read back ──
        assert_eq!(
            max_tick(&mut conn, run.id).await.expect("max_tick"),
            Some(199)
        );
        assert_eq!(
            tick_gaps(&mut conn, run.id).await.expect("tick_gaps"),
            Vec::<i32>::new()
        );
        let all = frames_for_run(&mut conn, run.id, None)
            .await
            .expect("all frames");
        assert_eq!(all.len(), 200);
        assert_eq!(all[0].tick, 0);
        assert_eq!(all[199].tick, 199);
        assert_eq!(all[199].state_hash, "hash-199");
        assert_eq!(all[7].agents, serde_json::json!([{ "x": 7, "y": -7 }]));

        let budgeted = frames_for_run(&mut conn, run.id, Some(20))
            .await
            .expect("budgeted frames");
        assert_eq!(budgeted.len(), 20);
        // Indexed rather than `.first()`/`.last()`: `diesel_async::RunQueryDsl`
        // is in scope and its `first` wins method resolution on a `Vec`.
        assert_eq!(budgeted[0].tick, 0);
        assert_eq!(budgeted[19].tick, 199);
    }

    // ── record a steer signal (AC-34 provenance) ──
    let signals = PgRunSignalRepository::with_pool_untracked(pool.clone());
    signals
        .save(&NewRunSignal {
            run_id: run.id,
            tick: 100,
            kind: signal_kind::STEER.to_owned(),
            payload: serde_json::json!({ "weights": { "cohesion": 2.0 } }),
        })
        .await
        .expect("signal saves");
    let recorded = signals.find_by_run_id(run.id).await.expect("signal lookup");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].kind, signal_kind::STEER);
    assert_eq!(recorded[0].tick, 100);

    // ── update status ──
    let completed = runs
        .update(
            run.id,
            &UpdateRun {
                status: Patch::Set(run_status::COMPLETED.to_owned()),
                ticks_completed: Patch::Set(200),
                final_state_hash: Patch::Set(Some("final-hash".to_owned())),
                ..Default::default()
            },
        )
        .await
        .expect("status update");
    assert_eq!(completed.status, run_status::COMPLETED);
    assert!(run_status::is_terminal(&completed.status));
    assert!(
        completed.updated_at > completed.created_at,
        "updated_at must advance when a run changes: created_at={} updated_at={}",
        completed.created_at,
        completed.updated_at
    );

    // ── and the whole thing survives a fresh read ──
    let reread = runs
        .find_by_id(run.id)
        .await
        .expect("find_by_id")
        .expect("run exists");
    assert_eq!(reread.status, run_status::COMPLETED);
    assert_eq!(reread.ticks_completed, 200);
    assert_eq!(reread.final_state_hash.as_deref(), Some("final-hash"));
    assert_eq!(reread.config_snapshot, config);
    assert_eq!(reread.workflow_execution_id.as_deref(), Some("wf-42"));
    assert_eq!(
        runs.find_by_status(run_status::COMPLETED.to_owned())
            .await
            .expect("derived status query")
            .len(),
        1
    );
}
