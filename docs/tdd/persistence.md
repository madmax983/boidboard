# TDD log — application shell and persistence layer

Every behaviour below was built **red → green → refactor**. The `RED` blocks
contain **real, unedited (only trimmed) output** captured from
`cargo test -p boidboard --test persistence` before the implementation existed.

Test file: `boidboard/tests/persistence.rs`
Live database: `postgres://boid:boid@127.0.0.1:5432/boidboard_test`

Structural precondition (not an AC, so no red/green cycle of its own): the
walking-skeleton `src/main.rs` was split into `src/lib.rs` (all handlers,
workflow, activity, models, repositories, migrations) plus a three-line
`src/main.rs` shim calling `boidboard::run()`. A binary crate cannot be imported
by its own `tests/`, so without this split none of the tests below could exist.

### AC-37 — all four tables exist via embedded migrations, with `UNIQUE (run_id, tick)` on frames

**RED** — `ac37_migrations_create_the_four_tables`, `ac37_frames_has_unique_run_id_tick`

```
test ac37_migrations_create_the_four_tables ... FAILED
test ac37_frames_has_unique_run_id_tick ... FAILED

thread 'ac37_migrations_create_the_four_tables' (9690) panicked at boidboard/tests/persistence.rs:75:5:
assertion `left == right` failed: all four Boidboard tables must exist via embedded migrations
  left: []
 right: ["frames", "run_signals", "runs", "scenarios"]

thread 'ac37_frames_has_unique_run_id_tick' (9689) panicked at boidboard/tests/persistence.rs:102:5:
assertion `left == right` failed: frames must carry UNIQUE (run_id, tick) — the AC-32 idempotency guarantee
  left: []
 right: ["run_id", "tick"]

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
```

**GREEN** — added the four Diesel migrations (`20260816000001_create_scenarios` … `20260816000004_create_run_signals`), embedded them via `embed_migrations!()` in `src/lib.rs`, and registered `.migrations(MIGRATIONS)` on the app builder; the tests' `migrate_once()` then applies them to `boidboard_test`.

```
Running migration 20260816000001_create_scenarios
Running migration 20260816000002_create_runs
Running migration 20260816000003_create_frames
Running migration 20260816000004_create_run_signals
test ac37_migrations_create_the_four_tables ... ok
test ac37_frames_has_unique_run_id_tick ... ok
```

**REFACTOR** — dropped the separately-planned `CREATE INDEX ... ON frames (run_id, tick)`: the `UNIQUE (run_id, tick)` constraint already materialises exactly that btree, so a second one would be dead weight and would make the `information_schema` constraint probe ambiguous.

### canonical_config_hash — stable under key reordering, sensitive to every value

**RED** — `canonical_config_hash_is_insensitive_to_json_key_order`, `canonical_config_hash_is_sensitive_to_every_value_change`, `canonical_config_hash_is_a_pinned_sha256_of_the_canonical_form`, `canonical_config_hash_respects_array_order`

```
error[E0425]: cannot find function `canonical_config_hash` in module `boidboard::models`
   --> boidboard/tests/persistence.rs:125:28
    |
125 |         boidboard::models::canonical_config_hash(&a),
    |                            ^^^^^^^^^^^^^^^^^^^^^ not found in `boidboard::models`
...
error: could not compile `boidboard` (test "persistence") due to 8 previous errors
```

**GREEN** — `src/models/config_hash.rs`: a canonical JSON writer (object keys sorted at every depth, arrays left in order, no whitespace) feeding a hand-rolled SHA-256, pinned in unit tests against the FIPS 180-4 vectors. **No new dependency was added** — a crypto crate for one 32-byte digest was not a trade worth making.

**REFACTOR** — none needed; the digest and the canonicaliser were split into separate private functions from the start, each with its own unit test (`sha256_matches_the_fips_180_4_vectors`, `canonical_form_sorts_object_keys_at_every_depth`, `canonical_form_escapes_strings_like_serde_json`).

### AC-38 — a run stores kernel_version + config_hash + seed + final_state_hash and round-trips them unchanged

**RED** — `ac38_run_round_trips_its_full_provenance_record`

```
error[E0432]: unresolved import `boidboard::models::run`
  --> boidboard/tests/persistence.rs:17:24
   |
17 | use boidboard::models::run::status as run_status;
   |                        ^^^ could not find `run` in `models`

error[E0432]: unresolved imports `boidboard::models::NewRun`, `boidboard::models::NewScenario`, `boidboard::models::Run`, `boidboard::models::Scenario`, `boidboard::models::UpdateRun`
  --> boidboard/tests/persistence.rs:18:25
   |
18 | use boidboard::models::{NewRun, NewScenario, Run, Scenario, UpdateRun};
   |                         ^^^^^^  ^^^^^^^^^^^  ^^^  ^^^^^^^^  ^^^^^^^^^ no `UpdateRun` in `models`

error[E0432]: unresolved imports `boidboard::repositories::PgRunRepository`, `boidboard::repositories::PgScenarioRepository`
  --> boidboard/tests/persistence.rs:19:31
   |
19 | use boidboard::repositories::{PgRunRepository, PgScenarioRepository};
   |                               ^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^ no `PgScenarioRepository` in `repositories`

error: could not compile `boidboard` (test "persistence") due to 7 previous errors
```

Then, after the models landed, a second genuine red — the model attribute was wrong, not just missing:

```
error[E0560]: struct `UpdateRun` has no field named `ticks_completed`
   --> boidboard/tests/persistence.rs:265:17
    |
265 |                 ticks_completed: Patch::Set(1_000),
    |                 ^^^^^^^^^^^^^^^ `UpdateRun` does not have this field
```

**GREEN** — `src/schema.rs` (all four Diesel tables) plus `src/models/{scenario,run,frame,run_signal}.rs` and `src/repositories/{scenario,run,frame,run_signal}_repository.rs`. The four models are one schema, so they landed together; their per-AC behaviour is driven by the cycles above and below.

**REFACTOR** — the `E0560` red above forced a real model decision: `#[default]` excludes a field from **both** `New{Model}` and `Update{Model}`, so `#[default] ticks_completed` would have been permanently unwritable by the workflow. Dropped the attribute — the column keeps its SQL `DEFAULT 0`, and model callers pass `0` explicitly at creation. Repository **traits** are now re-exported alongside the `Pg*Repository` structs from `crate::repositories`, since the generated CRUD lives on the trait and is unusable without it in scope.

### AC-39 — editing a scenario cannot mutate an existing run's stored configuration

**RED** — `ac39_editing_a_scenario_cannot_mutate_an_existing_runs_config`

Honest note: the schema support for this AC (`runs.config_snapshot`) shipped in the
AC-38 green step, because the four models are one schema — so this test passed the
first time it ran, which is *no evidence at all*. To get a real red, the exact bug
the AC forbids was injected into the test database for one run: an `AFTER UPDATE`
trigger on `scenarios` that propagates the new config down onto its runs ("keeping
runs in sync"), which is precisely how this bug shows up in the wild. The test
caught it:

```
running 1 test
test ac39_editing_a_scenario_cannot_mutate_an_existing_runs_config ... FAILED

thread 'ac39_editing_a_scenario_cannot_mutate_an_existing_runs_config' (5425) panicked at boidboard/tests/persistence.rs:379:5:
assertion `left == right` failed: the run's config_snapshot must still be the config it actually ran
  left: Object {"agent_count": Number(5), "max_speed": Number(0.1), "weights": Object {"alignment": Number(0.0), "cohesion": Number(0.0), "separation": Number(0.0)}, "world": Object {"height": Number(10.0), "width": Number(10.0)}}
 right: Object {"agent_count": Number(120), "max_speed": Number(4.0), "weights": Object {"alignment": Number(1.0), "cohesion": Number(0.9), "separation": Number(1.5)}, "world": Object {"height": Number(300.0), "width": Number(400.0)}}

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.10s
```

**GREEN** — removed the injected trigger. The real design already holds: `runs.config_snapshot JSONB NOT NULL` is an independent copy taken at run creation, and `runs.config_hash` describes that copy, so nothing on the `scenarios` side can reach a run that already exists.

**REFACTOR** — none needed. (The bug-injection block was temporary scaffolding and is not in the committed test.)

### AC-32 (persistence half) — `insert_frames_idempotent` is a no-op on replay; `tick_gaps` finds holes

**RED** — `ac32_reinserting_the_same_frames_is_a_no_op`, `ac32_partially_overlapping_batches_insert_only_the_new_ticks`, `ac32_max_tick_is_none_for_a_run_with_no_frames`, `ac32_tick_gaps_reports_every_missing_tick`, `ac32_gaps_are_scoped_to_one_run`, `ac32_frames_for_run_subsamples_evenly_to_a_rendering_budget`

```
error[E0432]: unresolved imports `boidboard::repositories::frames_for_run`, `boidboard::repositories::insert_frames_idempotent`, `boidboard::repositories::max_tick`, `boidboard::repositories::tick_gaps`
  --> boidboard/tests/persistence.rs:21:5
   |
21 |     frames_for_run, insert_frames_idempotent, max_tick, tick_gaps,
   |     ^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^  ^^^^^^^^  ^^^^^^^^^ no `tick_gaps` in `repositories`
   |     |               |                         |
   |     |               |                         no `max_tick` in `repositories`
   |     |               no `insert_frames_idempotent` in `repositories`
   |     no `frames_for_run` in `repositories`

error: could not compile `boidboard` (test "persistence") due to 23 previous errors
```

**GREEN** — `src/repositories/frame_queries.rs`: `insert_frames_idempotent` (`ON CONFLICT (run_id, tick) DO NOTHING`, returning rows actually inserted), `max_tick` (resume cursor), `frames_for_run` (ascending, optional even subsampling) and `tick_gaps`.

**REFACTOR** — the subsample index maths moved out into a private `subsample<T>` with its own unit tests (`subsample_keeps_endpoints_and_spreads_the_rest`, `subsample_handles_degenerate_budgets`), so the endpoint/rounding behaviour is pinned without a database. Also had to *narrow* the imports: `use diesel::prelude::*` drags in the **synchronous** `RunQueryDsl` and every `.load`/`.execute` became an `E0034 multiple applicable items in scope`; the module now imports only `ExpressionMethods`, `QueryDsl` and `SelectableHelper` from diesel plus `diesel_async::RunQueryDsl`.

### AC-40 — repository round-trips against live Postgres, and the suite passes twice consecutively

**RED** — `ac40_full_repository_round_trip_against_live_postgres`

First red, before the test would even compile:

```
error[E0277]: the trait bound `Vec<Frame>: Table` is not satisfied
   --> boidboard/tests/persistence.rs:682:29
    |
682 |         assert_eq!(budgeted.first().map(|f| f.tick), Some(0));
    |                             ^^^^^ the trait `Table` is not implemented for `Vec<Frame>`
```

Then the red that mattered — a runtime failure exposing a column that could never change:

```
running 15 tests
test ac40_full_repository_round_trip_against_live_postgres ... FAILED

thread 'ac40_full_repository_round_trip_against_live_postgres' (32744) panicked at boidboard/tests/persistence.rs:722:5:
updated_at must advance when a run changes: created_at=2026-08-16 05:05:37.113373 updated_at=2026-08-16 05:05:37.113373

test result: FAILED. 14 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.55s
```

**GREEN** — migration `20260816000005_runs_updated_at_trigger`: a `BEFORE UPDATE` trigger on `runs` setting `updated_at := clock_timestamp()`.

**REFACTOR** — none needed, but two decisions came out of the red:
* `updated_at` is maintained by the **database**, not the application. `#[default]` excludes a field from `UpdateRun`, so Rust cannot write it at all; a trigger is the only place the invariant can live, and it also means no caller can forget to bump it or backdate it.
* The trigger uses `clock_timestamp()`, not `now()`. `now()` is the *transaction* start time, so a create-then-update inside one transaction — exactly what every test here does — would stamp both timestamps identically and the guard would pass vacuously.

The `.first()` red is also worth keeping in mind for anyone writing further tests: with `diesel_async::RunQueryDsl` in scope, its `first` wins method resolution on a plain `Vec`, so index instead.

---

## Verification

`cargo test -p boidboard`, run twice consecutively with no cleanup in between:

```
########## RUN 1 ##########
running 6 tests    (lib unit tests)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
running 15 tests   (tests/persistence.rs — live Postgres)
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s
running 24 tests   (doc-tests, generated repository docs)
test result: ok. 0 passed; 0 failed; 24 ignored; 0 measured; 0 filtered out; finished in 0.00s

########## RUN 2 ##########
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
running 15 tests
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.41s
running 24 tests
test result: ok. 0 passed; 0 failed; 24 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo clippy -p boidboard --all-targets -- -D warnings` → exit 0, no diagnostics.

The app boots on the `dev` profile, applies all five migrations automatically, and answers on both routes:

```
GET /                     → 200  "Boidboard"
GET /health               → 200  {"status":"ok","version":"0.6.0","profile":"dev","pool":{"size":10,…}}
GET /api/harvest/health   → 200  {"runtime_ready":true,"queues":["default"],"scheduler":{"running":true,…}}
```

## Public API other slices build on

```rust
// Models — each generates New{Model} / Update{Model} / {Model}::factory()
boidboard::models::{Scenario, NewScenario, UpdateScenario}
boidboard::models::{Run,      NewRun,      UpdateRun}
boidboard::models::{Frame,    NewFrame,    UpdateFrame}
boidboard::models::{RunSignal, NewRunSignal, UpdateRunSignal}
boidboard::models::canonical_config_hash(&serde_json::Value) -> String
boidboard::models::run::status::{QUEUED, RUNNING, COMPLETED, CANCELLED, FAILED, BUDGET_EXCEEDED, ALL, is_terminal}
boidboard::models::run_signal::kind::{STEER, CANCEL, ALL}

// Pool-based repositories (route handlers). The trait must be in scope for the
// generated CRUD; both the trait and the struct are re-exported.
boidboard::repositories::{PgScenarioRepository, ScenarioRepository}  // + find_by_config_hash
boidboard::repositories::{PgRunRepository,      RunRepository}       // + find_by_scenario_id, find_by_status
boidboard::repositories::{PgFrameRepository,    FrameRepository}     // + find_by_run_id
boidboard::repositories::{PgRunSignalRepository, RunSignalRepository} // + find_by_run_id

// Connection-based queries (workflow activities)
boidboard::repositories::insert_frames_idempotent(&mut AsyncPgConnection, run_id: i64, &[NewFrame]) -> AutumnResult<usize>
boidboard::repositories::max_tick(&mut AsyncPgConnection, run_id: i64)                              -> AutumnResult<Option<i32>>
boidboard::repositories::frames_for_run(&mut AsyncPgConnection, run_id: i64, limit: Option<usize>)  -> AutumnResult<Vec<Frame>>
boidboard::repositories::tick_gaps(&mut AsyncPgConnection, run_id: i64)                             -> AutumnResult<Vec<i32>>

// Shell
boidboard::run().await          // build + serve the app
boidboard::MIGRATIONS           // EmbeddedMigrations, for test harnesses
boidboard::KERNEL_VERSION       // provenance stamp written to runs.kernel_version
```
