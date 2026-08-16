# Product-completeness fixes — `boidboard`

Everything below was written test-first: the test went in, it was run, the
failure was captured, and only then was the code changed. Where the test was
written *after* the fix (the reconciler's registration guard), the fix was
deliberately removed again, the failure captured, and the fix restored — and
that is said so explicitly.

**All terminal output in this document is real and pasted unedited**, except
where a line says `[elided]`, which only ever removes a full page of rendered
HTML from an assertion message.

**Scope.** `src/views/**` (was `src/views.rs`), `src/workflow.rs`,
`src/analysis.rs` (new), `src/reconcile.rs` (new), `tests/web.rs`,
`tests/workflow.rs`. Three one-line-class insertions were also needed in
`src/lib.rs` and `src/routes.rs` to make a new module reachable; they are listed
verbatim in [What `lib.rs` and `routes.rs` needed](#what-librs-and-routesrs-needed)
because that file is owned elsewhere. No new dependencies. No `unwrap` /
`expect` / `panic!` outside tests.

## Result

| | before | after |
|---|---|---|
| `cargo test --workspace` — `tests/web.rs` | 50 | **65** |
| `cargo test --workspace` — `tests/workflow.rs` | 16 | **32** |
| `cargo test --workspace` — everything | 343 | **392** |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean | **clean** |

Two consecutive full-suite runs, both green:

```
$ cargo test --workspace          # run 1
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 40.81s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
test result: ok. 65 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.49s
test result: ok. 238 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.40s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.94s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 24 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo test --workspace          # run 2
     Running unittests src/lib.rs (target/debug/deps/boidboard-baee932c5587b378)
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
     Running unittests src/main.rs (target/debug/deps/boidboard-15a64585ee31d4ed)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/integration.rs (target/debug/deps/integration-0b260d528c085c99)
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 36.58s
     Running tests/persistence.rs (target/debug/deps/persistence-1c5597b6c6af2195)
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.37s
     Running tests/web.rs (target/debug/deps/web-74b239dd49c57ecd)
test result: ok. 65 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.62s
     Running tests/workflow.rs (target/debug/deps/workflow-6ba5514827e65d7e)
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.49s
     Running unittests src/lib.rs (target/debug/deps/boids_core-afd92f88d94ede64)
test result: ok. 238 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.06s
     Running tests/cross_process.rs (target/debug/deps/cross_process-43961bbead32ab3c)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.98s
     Running tests/purity.rs (target/debug/deps/purity-8ce6cbfd3de100ec)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests boidboard
test result: ok. 0 passed; 0 failed; 24 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests boids_core
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.26s
```

## Summary

| # | Finding | Verdict | Where |
|---|---|---|---|
| 1a | a poisoned run is stranded at `running`; nothing ever writes `status::FAILED` | **FIXED** | `workflow.rs` |
| 1b | terminations the workflow cannot see leave the row stranded anyway | **FIXED** | `reconcile.rs` (new) |
| 2a | `POST /runs/{id}/cancel` has no button | **FIXED** | `views/pages.rs` |
| 2b | AC-27 stuck detection has no caller | **FIXED** | `analysis.rs` (new), `views/pages.rs` |
| 3 | forms emit no `_csrf` field | **FIXED** (POST forms) / **WON'T-FIX** (the GET compare form) | `views/pages.rs` |
| 4a | every activity failure mislabelled `Database` and retried | **FIXED** | `workflow.rs` |
| 4b | `views.rs` is ~1050 lines | **FIXED** | `views/{mod,svg,pages,style}.rs` |
| 4c | `metrics_panel` destructures an anonymous 4-tuple | **FIXED** | `views/pages.rs` |

---

## 1a — a batch failure left the run at `running` forever — FIXED

### The gap

`grep` found `status::FAILED` only in the constant list, in view CSS and in view
test fixtures. The single `finalize_run` call site omitted `error`, so
`FinalizeRequest.error` was dead code, `runs.error` was always NULL, and the
`run.error` branch of `progress_fragment` was unreachable in production. A run
whose activity exhausted its retries sat at `running` forever while every open
tab polled `/runs/{id}/progress` every two seconds.

### Red

Three new tests in `tests/workflow.rs`:

* `a_batch_failure_finalizes_the_run_as_failed_before_the_workflow_gives_up`
* `a_failed_run_keeps_the_ticks_its_completed_batches_reached`
* `the_failure_finalizer_is_a_command_and_not_a_second_engine_cancel_path`

```
$ cargo test -p boidboard --test workflow -- a_batch_failure a_failed_run the_failure_finalizer
test a_batch_failure_finalizes_the_run_as_failed_before_the_workflow_gives_up ... FAILED
test the_failure_finalizer_is_a_command_and_not_a_second_engine_cancel_path ... FAILED
test a_failed_run_keeps_the_ticks_its_completed_batches_reached ... FAILED
thread 'a_batch_failure_finalizes_the_run_as_failed_before_the_workflow_gives_up' (12279) panicked at boidboard/tests/workflow.rs:585:5:
assertion `left == right` failed: the run must be closed out exactly once, not left at `running` forever
  left: 0
thread 'the_failure_finalizer_is_a_command_and_not_a_second_engine_cancel_path' (12281) panicked at boidboard/tests/workflow.rs:669:5:
assertion `left == right` failed: the failure path issues exactly one extra command: the finalizer
  left: ["simulate_batch"]
thread 'a_failed_run_keeps_the_ticks_its_completed_batches_reached' (12280) panicked at boidboard/tests/workflow.rs:636:5:
assertion `left == right` failed
  left: 0
test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.23s
```

### Green

The batch dispatch in `simulation_workflow` is now wrapped: an activity failure
issues `finalize_run` with `status::FAILED`, the cursor the run genuinely
reached, and `e.to_string()` in the previously-dead `error` field, then returns
`Err(e)`.

The replay reasoning is in a comment at the call site so nobody "unifies" this
with the engine-cancel path above it. An `ActivityFailed` **is** consumed by the
replay matcher, so the command after it lands on a fresh history position; a
`WorkflowCancelled` is **not**, which is why the cancel path must issue nothing.
Two tests hold the two halves apart:
`ac33_engine_cancellation_kills_the_run_without_issuing_another_command` demands
zero commands, `the_failure_finalizer_is_a_command_…` demands exactly one. A
change that unified them breaks one of the two by construction.

An intermediate red is worth recording, because it is the thing that proves the
claim rather than assuming it. The first version of the tests reused the
suite's existing `assert_replays`, which requires `ReplaySucceeded`:

```
thread 'the_failure_finalizer_is_a_command_and_not_a_second_engine_cancel_path' (13584) panicked at boidboard/tests/workflow.rs:143:5:
replay diverged: ReplayReport(exec=ffffd6b2-3ed6-4de8-87a8-601de9c29533, events_replayed=6, status=WorkflowFailed(event_index=6, error="activity failed: simulate_batch (attempt 1): boom"))
```

That is not a divergence — a workflow that legitimately fails replays to
`WorkflowFailed`, and `events_replayed=6` is the *whole* history, finalizer
included. So the failure-path tests use a narrower helper,
`assert_replays_without_diverging`, which rejects `NonDeterminismDetected` and
additionally asserts the replay consumed every event. That is the exact claim
being made.

```
$ cargo test -p boidboard --test workflow -- a_batch_failure a_failed_run the_failure_finalizer
test a_batch_failure_finalizes_the_run_as_failed_before_the_workflow_gives_up ... ok
test the_failure_finalizer_is_a_command_and_not_a_second_engine_cancel_path ... ok
test a_failed_run_keeps_the_ticks_its_completed_batches_reached ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 0.31s
```

---

## 1b — the backstop for terminations the workflow cannot see — FIXED

### The gap

1a covers everything the workflow survives long enough to react to. It cannot
cover a `SIGKILL`ed worker, an engine `terminate`, an engine-level cancel (which
deliberately issues no further commands), a history cap or a replay failure. In
all of those the *execution* reaches a terminal state and the `runs` row does
not.

### Shape

`src/reconcile.rs`, split three ways so each piece is testable on its own:

1. `stale_non_terminal_runs(conn, cutoff, limit)` — *find*. One indexed query.
   The cutoff is a **parameter**, which is what makes it testable: a test passes
   a cutoff in the future to make its own rows stale and one in the past to prove
   a fresh run is left alone, instead of sleeping for half an hour or fighting
   the `runs_set_updated_at` trigger (which, being `BEFORE UPDATE`, cannot be
   backdated from the application at all).
2. `finalize_for(run, view) -> Option<FinalizeRequest>` — *decide*. **Pure**, so
   every branch of the policy is a plain unit test with a hand-built
   `WorkflowResult` and no engine.
3. `reconcile_one(conn, run, view)` — *apply*. Delegates to `finalize_run_core`,
   whose terminal guard is what makes this compose with 1a with no coordination:
   whoever gets there first wins and everyone else's write is a no-op.

`EngineView` has three cases and collapsing any two would be a bug: `Snapshot`
(the engine answered), `NoExecution` (there is nothing to ask about, so nothing
will ever drive this run), and `Unavailable` (the engine could not be asked —
**not** an answer, because failing a healthy run over a storage blip is far worse
than waiting for the next sweep).

Wiring the Harvest client into the `#[scheduled]` task was **not** impractical,
so the database-only fallback was not needed: `WorkflowHandleClient` is already
an `AppState` extension (that is how `create_run` starts runs), and
`client.handle(exec_id).result_snapshot()` is the whole lookup.

A completed execution is finalized on the **workflow's own result payload**,
not on the engine's execution state. That distinction is load-bearing: a run
stopped by the `max_ticks` guardrail is a successfully completed *execution*
whose *run* status is `budget_exceeded`, and reading `COMPLETED` off the
execution state alone would quietly relabel it.

### Red

```
$ cargo test -p boidboard --test workflow -- reconcil the_sweep a_terminated_or a_completed_execution a_run_that_names
error[E0432]: unresolved import `boidboard::reconcile`
error[E0282]: type annotations needed
error[E0282]: type annotations needed
error[E0282]: type annotations needed
error[E0282]: type annotations needed
error[E0282]: type annotations needed
error: could not compile `boidboard` (test "workflow") due to 6 previous errors
```

### Green

```
$ cargo test -p boidboard --test workflow -- reconcil the_sweep a_terminated_or a_completed_execution a_run_that_names
test a_run_that_names_no_execution_is_failed_rather_than_left_queued_forever ... ok
test a_terminated_or_failed_execution_closes_its_run_out_with_a_reason ... ok
test the_reconciler_leaves_alone_every_run_it_has_no_business_touching ... ok
test a_completed_execution_is_finalized_on_the_workflows_own_answer ... ok
test the_reconciler_cannot_clobber_a_run_that_finished_normally ... ok
test the_sweep_selects_stale_non_terminal_runs_and_nothing_else ... ok
test reconciling_a_terminated_execution_clears_the_stranded_row_once ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 24 filtered out; finished in 0.10s
```

`reconciling_a_terminated_execution_clears_the_stranded_row_once` builds the
exact state a `SIGKILL`ed worker leaves behind — one batch of frames written,
`ticks_completed` advanced, status `running`, `error` NULL — and asserts the
sweep closes it out as `failed` with the engine's reason, keeps the 50 ticks it
really reached, and is a no-op on the second pass.

### The registration guard (written after, defect re-introduced to prove it)

`the_reconciler_is_registered_and_actually_mounted` was written *after* the
`.tasks(…)` wiring, so the wiring was removed again to capture the failure:

```
$ cargo test -p boidboard --test workflow -- the_reconciler_is_registered
thread 'the_reconciler_is_registered_and_actually_mounted' (1395) panicked at boidboard/tests/workflow.rs:1740:5:
the application must register the reconciler with `.tasks(tasks![…])`; an unscheduled `#[scheduled]` function is dead code
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 31 filtered out; finished in 0.12s
```

It guards precisely the failure mode that made AC-27 vacuous and left the cancel
route unreachable: **correct code with no caller**. It also asserts the sweep
interval is shorter than `STALE_AFTER`, so the two cannot drift apart.

---

## 2a — `POST /runs/{id}/cancel` had no button — FIXED

### Red

```
$ cargo test -p boidboard --test web ac33_
---- ac33_a_queued_run_can_be_cancelled_but_a_terminal_one_cannot stdout ----
thread 'ac33_a_queued_run_can_be_cancelled_but_a_terminal_one_cannot' (11493) panicked at boidboard/tests/web.rs:1154:9:
assertion `left == right` failed: `queued` is not terminal, so the run is still cancellable:
<h1>Highway · run #5</h1><div class="run-progress" id="run-progress" data-status="queued" … [elided: the full rendered page]
  left: 0
 right: 1

failures:
    ac33_a_live_run_offers_a_cancel_button_that_posts_to_the_cancel_route
    ac33_a_queued_run_can_be_cancelled_but_a_terminal_one_cannot

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 50 filtered out; finished in 0.13s
```

### Green

`views::cancel_form(run_id, csrf)`, rendered from `run_detail_page` **only while
the run is non-terminal** — which is exactly the set the route acts on, since
`cancel_run` redirects an already-terminal run unchanged. A `<form method="post">`
rather than a link, because a `GET` control is one a crawler or a link preview
can fire on its own. It uses `crate::routes::paths::cancel_run(id)`, so a route
path change cannot leave the button pointing at nothing.

Four tests: two view-level (`ac33_a_live_run_offers_a_cancel_button_that_posts_to_the_cancel_route`,
`ac33_a_queued_run_can_be_cancelled_but_a_terminal_one_cannot`), one route-level
through `TestApp` (`ac33_the_rendered_detail_page_offers_a_cancel_button_only_while_the_run_is_live`),
and the CSRF one below.

```
$ cargo test -p boidboard --test web ac33_
test ac33_a_live_run_offers_a_cancel_button_that_posts_to_the_cancel_route ... ok
test ac33_a_queued_run_can_be_cancelled_but_a_terminal_one_cannot ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 50 filtered out; finished in 0.00s
```

---

## 2b — AC-27 stuck detection had no caller — FIXED

### Red

```
$ cargo test -p boidboard --test web ac27_
error[E0433]: failed to resolve: could not find `analysis` in `boidboard`   (×7)
error[E0609]: no field `stuck` on type `RunDetail`                          (×2)
error: could not compile `boidboard` (test "web") due to 9 previous errors
```

### Green

`src/analysis.rs`, pure and holding to the same rule as `views`:

```rust
pub fn run_is_stuck(frames: &[Frame], world: &World) -> bool
```

It builds the centroid path with `boids_core::metrics::toroidal_centroid` over
the decoded frames and hands it to `is_stuck` with `STUCK_WINDOW = 20` and
`STUCK_STRAIGHTNESS = 0.2` (the middle of the useful band `is_stuck` documents).
Frames that do not decode are **skipped**, not substituted with an empty flock —
an empty flock's centroid is `(0,0)` and would fake an enormous jump. The
`decode_agents` here reads `SimState`'s wire form first for the same reason
`routes::decode_agents` does; reading it as `Vec<Agent>` silently yields an empty
flock.

The page renders `views::stuck_badge()` from a `bool` on `RunDetail`, not from a
computation the view performs — the frames are the handler's to load, and a flag
is what makes the badge testable.

Five tests, all with no database: `ac27_a_flock_that_keeps_making_progress_is_not_stuck`,
`ac27_a_flock_oscillating_in_place_is_reported_stuck`,
`ac27_stuckness_is_never_claimed_on_absent_evidence`,
`ac27_stuckness_is_read_from_the_wire_form_the_workflow_actually_writes`,
`ac27_the_run_detail_page_badges_a_stuck_run_and_only_a_stuck_run`.

```
$ cargo test -p boidboard --test web ac27_
test ac27_stuckness_is_never_claimed_on_absent_evidence ... ok
test ac27_a_flock_oscillating_in_place_is_reported_stuck ... ok
test ac27_a_flock_that_keeps_making_progress_is_not_stuck ... ok
test ac27_stuckness_is_read_from_the_wire_form_the_workflow_actually_writes ... ok
test ac27_the_run_detail_page_badges_a_stuck_run_and_only_a_stuck_run ... ok
test result: ok. 5 passed; 0 filtered out ... finished in 0.00s
```

`analysis.rs` is now held to the AC-50 purity rule by the same guard that holds
`views` to it.

---

## 3 — the CSRF token in the forms — FIXED (POST) / WON'T-FIX (the GET form)

### Approach

`ChangesetForm` was read first, as instructed. A full `ChangesetForm` migration
is the wrong tool here for a concrete reason rather than a vague one: its
`form_tag` emits `<form action method enctype>` and nothing else, and every form
in this app carries a **semantic class** (`form.new-run`, `form.cancel-run`) that
the tests and the stylesheet are both written against. So the framework's *token
source* and *field name* are used, and only the `<form>` element itself is still
hand-written:

```rust
// views/pages.rs — the same markup `autumn_web::form::form_tag_inner` emits
@if let Some(token) = self.token.as_deref() {
    input type="hidden" name=(self.field) value=(token);
}
```

`views::Csrf` carries the token **and the configured field name**, built from the
two request extensions `CsrfLayer` publishes — `CsrfToken` and `CsrfFormField` —
so a `security.csrf.form_field = "authenticity_token"` is honoured rather than
hardcoded. `Csrf::absent()` emits nothing at all: an invented or empty token
would look protected and be rejected the moment the layer was switched on.

### Red

First, the types:

```
$ cargo test -p boidboard --test web csrf_
error[E0433]: failed to resolve: could not find `Csrf` in `views`           (×11)
error[E0061]: this function takes 1 argument but 2 arguments were supplied  (×5)
error[E0609]: no field `csrf` on type `RunDetail`                           (×2)
error: could not compile `boidboard` (test "web") due to 18 previous errors
```

Then, to prove the route-level test really catches the original defect, the
hidden input was removed from `new_run_form` again:

```
$ cargo test -p boidboard --test web csrf_
test csrf_the_cancel_form_carries_the_token_too ... ok
test csrf_no_middleware_means_no_field_rather_than_an_invented_token ... ok
test csrf_the_detail_page_hands_its_token_to_the_cancel_form ... ok
test csrf_the_compare_form_stays_a_get_and_needs_no_token ... ok
test csrf_the_new_run_form_carries_the_token_the_middleware_supplied ... FAILED
test csrf_the_configured_field_name_is_honoured_rather_than_hardcoded ... FAILED
test csrf_the_rendered_new_run_form_submits_successfully_under_an_enabled_csrf_layer ... FAILED
test result: FAILED. 4 passed; 3 failed; 0 ignored; 0 measured; 57 filtered out; finished in 0.14s

$ cargo test -p boidboard --test web csrf_the_rendered
thread 'csrf_the_rendered_new_run_form_submits_successfully_under_an_enabled_csrf_layer' (5647) panicked at boidboard/tests/web.rs:1640:10:
no elements matched selector `form.new-run input[type="hidden"][name="_csrf"]`.
Parsed HTML:
<html lang="en">
  <head>
    <meta charset="utf-8">
    …
```

### Green

Seven tests. Six are pure-view; the seventh,
`csrf_the_rendered_new_run_form_submits_successfully_under_an_enabled_csrf_layer`,
is the one that would actually have caught this in CI: it mounts a real
`CsrfLayer` on a `TestApp`, renders `GET /runs/new`, pulls the token out of the
markup, submits **exactly what the rendered form produces** and asserts a `303`
— then submits the same body without the field and asserts a `403`, so the test
proves the protection is real rather than merely absent.

```
$ cargo test -p boidboard --test web csrf_
test csrf_the_cancel_form_carries_the_token_too ... ok
test csrf_no_middleware_means_no_field_rather_than_an_invented_token ... ok
test csrf_the_configured_field_name_is_honoured_rather_than_hardcoded ... ok
test csrf_the_detail_page_hands_its_token_to_the_cancel_form ... ok
test csrf_the_compare_form_stays_a_get_and_needs_no_token ... ok
test csrf_the_new_run_form_carries_the_token_the_middleware_supplied ... ok
test csrf_the_rendered_new_run_form_submits_successfully_under_an_enabled_csrf_layer ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 57 filtered out; finished in 0.11s
```

### WON'T-FIX: the compare form

The review listed `views.rs:1041` (the compare form) alongside the new-run form.
That one is `method="get"`, and `GET` is in `security.csrf.safe_methods` by
default (`["GET", "HEAD", "OPTIONS", "TRACE"]`), so it neither needs a token nor
should carry one — a `_csrf` value on a GET form lands in the query string, gets
bookmarked, and ends up in logs and `Referer` headers. It is a navigation
control, not a state change, so it stays a `GET`.
`csrf_the_compare_form_stays_a_get_and_needs_no_token` pins both halves: the form
is still a `GET`, and no token leaks into the page that carries it.

---

## 4a — activity failures were all `Database` and all retried — FIXED

### The gap

All three activity shells did `map_err(|e| HarvestError::Database(e.to_string()))`.
But `simulate_batch_core` returns `unprocessable_msg("run {} has invalid
parameters")` and `unprocessable_msg("…no checkpoint at tick…")` — permanent
failures that were retried three times with exponential backoff and then reported
to metrics as database faults.

### Red

```
$ cargo test -p boidboard --test workflow -- a_422_from_a_core the_error_types_are a_steer_that_invalidates resuming_from_a_checkpoint finalizing_into_a_non_terminal
error[E0432]: unresolved imports `boidboard::workflow::classify_activity_error`, `boidboard::workflow::error_type`
error[E0282]: type annotations needed
error[E0282]: type annotations needed
error: could not compile `boidboard` (test "workflow") due to 3 previous errors
```

### Green

The `_core`/shell split is the classification point, and the cores already carry
the answer: they use `AutumnError`'s status deliberately, and `unprocessable_msg`
(422) is how a core says "this request is wrong". So the policy is one line —
**422 is permanent, anything else is transient** — rather than a growing list of
string matches:

```rust
pub fn classify_activity_error(error: &AutumnError) -> ActivityFailure {
    if error.status() == StatusCode::UNPROCESSABLE_ENTITY {
        ActivityFailure::non_retryable(error_type::INVALID_CONFIG, error.to_string())
    } else {
        ActivityFailure::retryable(error_type::DATABASE, error.to_string())
    }
}
```

The three activities now return `Result<Value, ActivityFailure>`, which is the
return type the `#[activity]` macro recognises syntactically to route errors
through the typed encoding, so `error_type` and `non_retryable` survive into
workflow history and into `harvest.activity.failed{error.type=…}`. `app_conn`
was classified too: a missing `AppDbPool` is non-retryable (a worker that booted
without a pool will not grow one between retries), a checkout failure is not. A
malformed activity payload is `InvalidInput`, non-retryable.

Five tests: two pure (`a_422_from_a_core_is_a_permanent_failure_and_anything_else_is_transient`,
`the_error_types_are_stable_low_cardinality_names`) and three against the live
database that classify a **real** error produced by a real core rather than a
hand-built one — `a_steer_that_invalidates_the_config_is_not_retried`,
`resuming_from_a_checkpoint_that_does_not_exist_is_not_retried`,
`finalizing_into_a_non_terminal_status_is_not_retried`.

```
$ cargo test -p boidboard --test workflow -- a_422_from_a_core the_error_types_are a_steer_that_invalidates resuming_from_a_checkpoint finalizing_into_a_non_terminal
test a_steer_that_invalidates_the_config_is_not_retried ... ok
test the_error_types_are_stable_low_cardinality_names ... ok
test finalizing_into_a_non_terminal_status_is_not_retried ... ok
test resuming_from_a_checkpoint_that_does_not_exist_is_not_retried ... ok
test a_422_from_a_core_is_a_permanent_failure_and_anything_else_is_transient ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 0.18s
```

---

## 4b — `views.rs` split — FIXED

Done last, with everything else green, and purely mechanical: `views.rs` (1230
lines by then) became a directory along the seam its own section dividers already
showed.

| file | lines | contents |
|---|---|---|
| `views/mod.rs` | 50 | module docs + `pub use` re-exports |
| `views/svg.rs` | 429 | `num`, `fit`, `flock_svg`, `agent_mark`, `trajectory_svg`, `seam_split`, `points_attr`, `sparkline`, and the rendering budgets |
| `views/pages.rs` | 737 | layout, tables, panels, forms, fragments, `Csrf`, `metric` |
| `views/style.rs` | 86 | the `STYLESHEET` const |

Every public item is re-exported from `views`, so `views::flock_svg` and
`views::run_detail_page` are spelled exactly as before and **no caller changed** —
not `routes.rs`, not a single test assertion. `num` is `pub(super)` because the
preset cards print world dimensions with it; one implementation, one rounding
rule.

The AC-50 purity guard was the one thing that had to change, since it read
`../src/views.rs` by path. It now `concat!`s all three source files **and**
asserts `views/mod.rs` declares exactly three submodules — so a fourth view file
added later cannot quietly escape the purity check.

```
$ cargo test -p boidboard --test web
test result: ok. 65 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.91s
```

---

## 4c — `metrics_panel`'s anonymous 4-tuple — FIXED

`[(&str, &str, Vec<f64>, usize); 4]` became a named `MetricCard { field, label,
values, places }`. Three of the four members were genuinely confusable: `field`
and `label` are both strings and are deliberately *different* strings, and
`places` is a bare `usize` next to a `Vec<f64>`. `places: 0` reads as "collisions
are whole numbers"; `0` in the fourth tuple slot reads as nothing at all.

Behaviour-preserving, so the existing tests are the proof:

```
$ cargo test -p boidboard --test web -- ac44
test ac44_metrics_panel_renders_for_a_run_with_no_frames_yet ... ok
test ac44_metrics_panel_gives_every_headline_metric_its_own_sparkline ... ok
test ac44_sparkline_maps_the_series_across_a_stable_viewbox ... ok
test ac44_sparkline_survives_empty_single_and_flat_series ... ok
test ac44_run_detail_route_survives_a_run_with_no_frames_at_all ... ok
test ac44_run_detail_route_renders_one_sparkline_per_headline_metric ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.12s
```

---

## What `lib.rs` and `routes.rs` needed

Both files are owned elsewhere. The changes below are the **minimum** required to
make the new modules reachable, and each was applied with an exact-anchor edit so
nothing else in either file was touched. They are listed verbatim so they can be
re-applied if a concurrent edit drops them.

### `src/lib.rs` — three additions

```rust
pub mod analysis;                                              // in the module list, before `pub mod models;`
pub mod reconcile;                                             // in the module list, after `pub mod presets;`
        .tasks(autumn_web::tasks![reconcile::reconcile_stranded_runs])   // in `run()`, after `.routes(all_routes())`
```

`the_reconciler_is_registered_and_actually_mounted` fails if the `.tasks(…)` line
is missing.

### `src/routes.rs` — two handlers

`new_run_form` gains the two CSRF extractors and passes them on:

```rust
pub async fn new_run_form(
    csrf: Option<autumn_web::security::CsrfToken>,
    csrf_field: Option<autumn_web::security::CsrfFormField>,
) -> Markup {
    let csrf = views::Csrf::from_request_parts(csrf.as_ref(), csrf_field.as_ref());
    views::layout("New run", views::new_run_form(&presets::all(), &csrf))
}
```

`run_detail` gains the same two extractors, and two lines in the `RunDetail`
literal:

```rust
pub async fn run_detail(
    mut db: Db,
    Path(id): Path<i64>,
    csrf: Option<autumn_web::security::CsrfToken>,
    csrf_field: Option<autumn_web::security::CsrfFormField>,
) -> AutumnResult<Markup> {
```

```rust
    let detail = views::RunDetail {
        stuck: crate::analysis::run_is_stuck(&stored, &world),
        csrf: views::Csrf::from_request_parts(csrf.as_ref(), csrf_field.as_ref()),
        run,
        scenario_name,
        world,
        …
```

Both new fields go **first** in the literal, deliberately: struct-expression
fields are evaluated in written order, and `world` is moved by its own shorthand,
so `&world` after it would not compile.

`Option<CsrfToken>` rather than `CsrfToken`: the extractor is optional so the
handlers still work with no CSRF layer mounted, which is exactly the
configuration the pure-route tests boot.

---

## Two things adjacent to this work, for the record

* **`seed_run` in `tests/workflow.rs` had to become find-or-create.** A migration
  added in parallel makes `scenarios.config_hash` `UNIQUE`, and the helper saved
  a fresh scenario unconditionally — which broke `ac36_…` and the new reconciler
  tests the moment two runs shared a parameter set. It now looks the scenario up
  by config hash first, exactly as `create_run` does.
* **CSRF is now on for every profile** via `autumn.toml`, which is what makes the
  markup half of §3 load-bearing rather than merely tidy.

## Left alone, deliberately

The idiom review called these out as well-built and none of them was touched:
the cursor-only workflow invariant and its module doc; the `SimState`
exact-decimal wire form and the comments explaining it; the three idempotency
guards (`finalize_run_core`'s terminal guard, `record_signal_core`'s
jsonb-equality lookup, `simulate_batch_core`'s `.le(…)`-guarded update); the
`_core`/shell split; `progress_fragment`'s stopping condition; and the
degenerate-case handling in `sparkline`, `num` and `seam_split`.
