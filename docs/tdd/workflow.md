# TDD log — the durable simulation workflow

Evidence for **AC-52** ("every feature was built red → green → refactor") covering
`boidboard/src/workflow.rs`: the `simulation_workflow`, its three activities, and the
cursor-only invariant that justifies the whole architecture.

Every **RED** block below is real captured output from `cargo test -p boidboard` at the
moment the test existed and the implementation did not. Nothing here is reconstructed.

**Cycle order.** AC-29 → AC-30 → AC-31 → AC-35 → AC-33 (workflow half) → AC-34 (workflow half)
→ AC-32 → AC-33 (persistence half) → AC-34 (provenance half) → AC-36 → activity registration.
That is dependency order rather than numeric order:

* AC-29 first, because the cursor-only invariant is the design decision every later cycle
  builds on — it has to be nailed down before there is a loop to hang signals off.
* AC-30 and AC-31 next: they are properties of the *harness*, and having them in place means
  every subsequent cycle inherits a replay check for free rather than bolting one on at the
  end.
* AC-35 before the signal cycles, because the batch loop's bound is what makes a
  "stops early" assertion mean anything — without it, "the run stopped" and "the run hung
  and the harness gave up" look the same.
* AC-33 before AC-34: cancel is the simpler boundary check, and it establishes *where* in
  the loop a signal is honoured; steering then reuses that boundary.
* AC-32 and AC-36 last, because they are the two criteria that need real Postgres and the
  real activity bodies, which only exist once the workflow above them has settled.

**Two kinds of test live in `boidboard/tests/workflow.rs`, deliberately.** The workflow
cycles (AC-29 … AC-31, AC-33 workflow half, AC-34 workflow half, AC-35) run entirely under
`WorkflowTestEnv` — no Postgres, no Docker, mocked activities, virtual clock. The activity
cycles (AC-32, AC-33 persistence half, AC-34 provenance half, AC-36) run against the **live**
test database, because an idempotency claim asserted against a mock asserts nothing. Both
kinds are ordinary `#[tokio::test]`s; nothing here is `#[ignore]`d.

---

### AC-29 — the workflow drives a run on a cursor alone, never the agent array

**RED** — `ac29_workflow_history_carries_only_a_cursor_never_the_agent_array`

```
error[E0432]: unresolved imports `boidboard::workflow::BatchCursor`, `boidboard::workflow::simulation_workflow_info`
 --> boidboard/tests/workflow.rs:7:27
  |
7 | use boidboard::workflow::{BatchCursor, simulation_workflow_info};
  |                           ^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^ no `simulation_workflow_info` in `workflow`
  |                           |
  |                           no `BatchCursor` in `workflow`

For more information about this error, try `rustc --explain E0432`.
error: could not compile `boidboard` (test "workflow") due to 1 previous error
```

**GREEN** — `SimulationInput` (`run_id`, `max_ticks`, `batch_ticks`, `metrics_every`),
`BatchCursor` (`run_id`, `next_tick`, `done`, `state_hash`, `frames_written`), and a
`simulation_workflow` whose loop passes `(run_id, from_tick)` to `simulate_batch` and takes a
cursor back, then calls `finalize_run`.

**REFACTOR** — the green also carried a `finished_at` field on the workflow result, which
nothing in AC-29 needed. It was **removed again** and AC-29 re-run green without it. Left in,
it would have handed AC-30 — whose whole point is the virtual clock — a test that could not
fail. Speculative code is worth deleting for its own sake; deleting it because it would have
faked the next cycle's RED is the stronger reason.

**Why this test is not trivially true.** A mock that returned a hand-written cursor would
prove only that the *test* declines to mention agents. So `flock_cursor_mock` seeds a real
`SimState` of `agent_count` agents and returns that flock's real `state_hash_hex()`: the
whole array demonstrably passes through the activity on every call, and the cursor is what
comes out. The test then runs the same workflow at 10 agents and at 10 000 and asserts the
histories are equal in event count *and* in serialized bytes, plus a recursive sweep of every
event payload for `agents`/`px`/`py`/`vx`/`vy` and for any array longer than 8 elements.

---

### AC-30 — the workflow runs with no Postgres, mocked activities, and a virtual clock

**RED** — `ac30_the_workflow_runs_with_no_database_and_only_the_virtual_clock`

```
---- ac30_the_workflow_runs_with_no_database_and_only_the_virtual_clock stdout ----

thread 'ac30_the_workflow_runs_with_no_database_and_only_the_virtual_clock' (30556) panicked at boidboard/tests/workflow.rs:187:5:
assertion `left == right` failed: the workflow timestamps itself from the virtual clock
  left: Null
 right: String("2026-08-16T05:52:44Z")
```

**GREEN** — stamp the workflow result with `ctx.now().to_rfc3339_opts(SecondsFormat::Secs,
true)`.

**REFACTOR** — none needed.

**What the assertion actually buys.** "Runs without a database" is hard to assert positively,
so the test asserts the three things that make it true and are checkable: the
`WorkflowTestEnv` is built with **no injected state**, so a workflow reaching for a pool would
find none; every activity is a closure; and the run's own finish timestamp equals `env.now()`,
the harness's virtual clock, to the second. Whole seconds rather than the default
`AutoSi` precision because a variable-width timestamp would have quietly broken AC-29's
byte-for-byte history comparison. Direct database IO inside the workflow body is not merely
untested but *unrepresentable* — `#[workflow]`'s HVG006 lint rejects it at compile time.

---

### AC-31 — the workflow replays without diverging

**RED** — none. `ac31_the_workflow_replays_without_diverging` passed the first time it ran:
by this point the workflow body was already a straight loop over `execute_activity_raw` with
no ambient input, so there was nothing for the replayer to disagree with. A green-on-first-run
test is not evidence that the test discriminates, so:

**MUTATION CHECK** — the classic replay bug was introduced deliberately: an activity call
guarded by `if !ctx.is_replaying()`, which is exactly how "only do this once" gets written by
someone who has not internalised that replay re-executes the whole body.

```
---- ac31_the_workflow_replays_without_diverging stdout ----

thread 'ac31_the_workflow_replays_without_diverging' (2616) panicked at boidboard/tests/workflow.rs:236:5:
the workflow completes: Err("non-deterministic replay: activity mismatch: expected ActivityScheduled(simulate_batch), got ActivityScheduled(record_signal)")
```

The mutation was reverted immediately and the suite re-run green. Note *where* it was caught:
in the live `WorkflowTestEnv` run, before `replay_check` was ever reached. That is not a
weakness of the check, it is how the harness works — the env drives a workflow by re-running
it against a growing history, so every iteration after the first is already a replay. The
mismatch names both sides (`expected ActivityScheduled(simulate_batch), got
ActivityScheduled(record_signal)`), which is what makes such a failure diagnosable in CI.

**GREEN** — no production change was needed; the deterministic implementation was already in
place.

**REFACTOR** — the replay assertion was extracted into `assert_replays(&outcome)` and wired
into **every** workflow test rather than only this one. AC-31 is a property of all code paths,
not of one happy-path history, and the check is free — it reuses the history the run already
built.

---

### AC-35 — `max_ticks` terminates a run deterministically rather than letting it run unbounded

**RED** — `ac35_max_ticks_bounds_a_run_to_exactly_ceil_max_over_batch_batches`,
`ac35_a_cursor_that_stops_advancing_trips_the_budget_guardrail`

```
---- ac35_max_ticks_bounds_a_run_to_exactly_ceil_max_over_batch_batches stdout ----

thread 'ac35_max_ticks_bounds_a_run_to_exactly_ceil_max_over_batch_batches' (6977) panicked at boidboard/tests/workflow.rs:300:5:
assertion `left == right` failed: a run that spends its whole tick budget completed; it did not overrun
  left: Null
 right: String("completed")

---- ac35_a_cursor_that_stops_advancing_trips_the_budget_guardrail stdout ----

thread 'ac35_a_cursor_that_stops_advancing_trips_the_budget_guardrail' (6976) panicked at boidboard/tests/workflow.rs:342:5:
assertion `left == right` failed: a run stopped by the guardrail says so, rather than claiming completion
  left: Null
 right: String("budget_exceeded")
```

**GREEN** — a `terminal` status initialised to `BUDGET_EXCEEDED` and set to `COMPLETED` only
on the path that breaks out because the run reached its tick budget, threaded into both the
`finalize_run` input and the workflow result.

**REFACTOR** — the `for _ in 0..planned_batches(..)` bound itself needed no change; it was
already there from AC-29, because without it the AC-29 loop would not have terminated at all.
What AC-35 added was the *distinction* between exhausting the loop and finishing the work.

**Two tests, because the criterion has two halves.** The first is the ordinary case: 450 ticks
in batches of 100 issues exactly `ceil(450 / 100) = 5` `simulate_batch` calls — not four (a
truncating division would drop the last 50 ticks) and not six. The second is the case the word
*guardrail* is actually about: a `simulate_batch` mock that never advances its cursor, which
without a bound is an infinite loop. The run stops after 5 batches with `budget_exceeded` and
`ticks_completed = 0`, and says so rather than reporting completion.

**Why `completed` and not `budget_exceeded` for the ordinary case.** `max_ticks` is a run's
configured length, so reaching it *is* success — the run list showing "budget exceeded" for
every normal run would be a lie the UI then has to explain. `budget_exceeded` is reserved for
the guardrail firing, which matches `models::run::status`'s own documentation: `COMPLETED` is
"reached `max_ticks` or its own termination condition", `BUDGET_EXCEEDED` is "stopped by the
`max_ticks` guardrail". The guardrail is still derived from `max_ticks` — it is
`ceil(max_ticks / batch_ticks)` batches — so both halves of AC-35 are `max_ticks` doing the
terminating.

---

### AC-33 — a cancel signal is honoured at the next batch boundary, partial results intact

**RED** — `ac33_a_cancel_signal_stops_the_run_at_the_next_batch_boundary`,
`ac33_engine_cancellation_kills_the_run_without_issuing_another_command`

```
---- ac33_a_cancel_signal_stops_the_run_at_the_next_batch_boundary stdout ----

thread 'ac33_a_cancel_signal_stops_the_run_at_the_next_batch_boundary' (10458) panicked at boidboard/tests/workflow.rs:398:5:
assertion `left == right` failed: the in-flight batch completes, and no further batch is dispatched
  left: 5
 right: 1

---- ac33_engine_cancellation_stops_the_run_before_it_spends_another_batch stdout ----

thread 'ac33_engine_cancellation_stops_the_run_before_it_spends_another_batch' (10459) panicked at boidboard/tests/workflow.rs:462:41:
the workflow terminates: "non-deterministic replay: activity mismatch: expected ActivityScheduled(simulate_batch), got WorkflowCancelled"
```

**GREEN** — a non-blocking `ctx.try_wait_for_signal(CANCEL_SIGNAL)` at the end of each loop
iteration: it records the signal via `record_signal`, sets the terminal status to `cancelled`,
and breaks. Plus an `is_cancelled()` guard at the top of the loop.

**REFACTOR** — the `record_signal` call was extracted to `record_signal_for(ctx, run_id, tick,
kind, payload)` while writing it, because AC-34's steer path needs the identical shape with a
different `kind`.

**The second RED changed the design, and that is the interesting part of this cycle.** The
first draft treated an engine-level cancel exactly like a signal: stop the loop, then call
`finalize_run` to tidy the row. The replay engine rejected it —

```
---- ac33_engine_cancellation_stops_the_run_before_it_spends_another_batch stdout ----

thread 'ac33_engine_cancellation_stops_the_run_before_it_spends_another_batch' (13610) panicked at boidboard/tests/workflow.rs:462:41:
the workflow terminates: "non-deterministic replay: activity mismatch: expected ActivityScheduled(finalize_run), got WorkflowCancelled"
```

— because Harvest's `WorkflowCancelled` event has no workflow-command counterpart and is never
consumed by the matcher, so **any** command issued past it lands on that event and diverges.
The tidy-up call was not a nicety with a cost; it was a latent non-determinism bug that would
have surfaced only in production replay. So the two cancels are now deliberately different
mechanisms, and the code says why:

* the **`cancel` signal** is the graceful stop and the one the product uses — it lets the batch
  in flight finish, writes provenance, and finalizes the run row with `cancelled`;
* an **engine cancel** is a kill — the workflow issues no further command and returns
  `HarvestError::Cancelled`, letting Harvest terminate the execution from its own history.

The engine-cancel test now asserts the *absence* of any scheduled activity, so a future
"helpful" finalize call fails the suite instead of production.

**Partial results, concretely.** The signal test asserts the run stops after exactly one of its
five allowed batches, and that the tick-100 checkpoint that batch produced is what
`finalize_run` receives — `ticks_completed = 100`, `final_state_hash` equal to the real hash of
the real flock, not a reset to zero. It also asserts the complete set of activity names in the
history is exactly `{simulate_batch, record_signal, finalize_run}`, so a rollback or
compensation step could not have run unnoticed. The persistence half — that those frames are
still in Postgres afterwards — is asserted for real in the AC-32/AC-36 cycles below.

---

### AC-34 — a steer signal changes later batches' parameters, and records itself

**RED (1/2)** — `ac34_steer_overrides_change_the_named_parameters_and_nothing_else`

```
error[E0425]: cannot find function `steer_overrides` in module `boidboard::workflow`
   --> boidboard/tests/workflow.rs:573:42
    |
573 |     let overrides = boidboard::workflow::steer_overrides(&json!({
    |                                          ^^^^^^^^^^^^^^^ not found in `boidboard::workflow`

error[E0425]: cannot find function `apply_overrides` in module `boidboard::workflow`
   --> boidboard/tests/workflow.rs:585:26
    |
585 |     boidboard::workflow::apply_overrides(&mut params, &overrides);
    |                          ^^^^^^^^^^^^^^^ not found in `boidboard::workflow`

For more information about this error, try `rustc --explain E0425`.
error: could not compile `boidboard` (test "workflow") due to 2 previous errors
```

**RED (2/2)** — with the two pure functions in place but the workflow loop not yet reading the
signal, the behavioural failure appears:

```
---- ac34_a_steer_signal_changes_params_for_later_batches_and_records_itself stdout ----

thread 'ac34_a_steer_signal_changes_params_for_later_batches_and_records_itself' (21128) panicked at boidboard/tests/workflow.rs:503:5:
assertion `left == right` failed
  left: Object {}
 right: Object {"w_cohesion": Number(2.5)}

---- ac34_a_steer_applies_only_recognised_parameters_but_records_the_whole_payload stdout ----

thread 'ac34_a_steer_applies_only_recognised_parameters_but_records_the_whole_payload' (21127) panicked at boidboard/tests/workflow.rs:555:5:
assertion `left == right` failed
  left: Object {}
 right: Object {"w_cohesion": Number(2.0)}
```

**GREEN** — `STEERABLE` (nine scalar knobs), `steer_overrides` (extract), `apply_overrides`
(apply), and a second `try_wait_for_signal` at the batch boundary that merges the extracted
overrides into the accumulator and records the raw payload.

**REFACTOR** — none needed; `record_signal_for` had already been extracted during AC-33 in
anticipation of exactly this second caller.

**Splitting the cycle in two was the useful decision.** The compile-error RED proves nothing
about behaviour, so the pure functions went in first and the workflow wiring second. That
second run is the one that shows the batch inputs actually being `{}` when they should carry
the steer — the failure that a reader can believe.

**One steer per boundary, not a drained burst.** `try_wait_for_signal` claims the single oldest
buffered steer at each boundary rather than `drain_signals_raw` claiming all of them. Each
steer therefore gets its own `run_signals` row against the exact tick from which it took
effect. Draining a burst into one merged parameter change would be marginally faster and would
destroy the one-to-one correspondence between an intervention and the stretch of trajectory it
explains — which is the entire point of recording them.

**What is deliberately not steerable.** `agent_count`, `world` and `backend` are excluded.
Changing any of them mid-run splits a run into two incompatible halves and silently invalidates
every frame stored before the change. A typo'd or non-numeric key is dropped rather than
rejected — an operator's slip must not kill a running experiment — but the payload is recorded
verbatim, so the slip is still discoverable. `ac34_a_steer_applies_only_recognised_parameters_
but_records_the_whole_payload` pins both halves of that.

---

### AC-32 — a duplicated `simulate_batch` leaves one set of frames and no gaps

**RED** — `ac32_a_duplicated_simulate_batch_leaves_one_set_of_frames_and_no_gaps`

```
error[E0282]: type annotations needed
   --> boidboard/tests/workflow.rs:707:17
    |
707 |       let first = simulate_batch_core(&mut conn, &request)
    |  _________________^
708 | |         .await
    | |______________^ cannot infer type

error[E0282]: type annotations needed
   --> boidboard/tests/workflow.rs:715:18
    |
715 |       let second = simulate_batch_core(&mut conn, &request)
    |  __________________^
716 | |         .await
    | |______________^ cannot infer type

Some errors have detailed explanations: E0282, E0432.
For more information about an error, try `rustc --explain E0282`.
warning: `boidboard` (test "workflow") generated 1 warning
error: could not compile `boidboard` (test "workflow") due to 3 previous errors; 1 warning emitted
```

(The E0432 the note refers to is the unresolved import of `BatchRequest` and
`simulate_batch_core` themselves; `cargo` truncated it above the captured window. The two
E0282s are its downstream consequence — with the function absent, nothing can infer what
`first` and `second` are.)

**GREEN** — `BatchRequest` and `simulate_batch_core(&mut AsyncPgConnection, &BatchRequest)`:
load the run, deserialize `config_snapshot` into `SimParams`, apply steer overrides, load the
checkpoint at `from_tick` (or seed at tick 0), simulate, `insert_frames_idempotent`, advance
`runs.ticks_completed`.

**REFACTOR** — none needed.

**The shape is the point.** The activity is a thin shell around a `_core` function taking an
explicit `&mut AsyncPgConnection`, so the test calls the real code against the real database
twice in a row — which is the only way an idempotency claim means anything. Asserting it
against a mock would assert that the mock is idempotent.

**Two design decisions this cycle forced.**

*`from_tick` is the authority, not the database.* The obvious implementation resumes from
`max_tick(run_id)`. That is wrong under retry: a redelivered batch would find the frames its
own first delivery wrote and run the *next* batch instead of repeating itself. The
`UNIQUE (run_id, tick)` constraint cannot save you from that, because the second run writes
genuinely new ticks. Taking the start tick from the request means a duplicate recomputes
byte-identical frames and every insert conflicts away — which is what `frames_written == 0` on
the second call asserts.

*A frame per tick, not per metrics sample.* `metrics_every` subsamples **metrics**, not
**frames**. Every simulated tick gets a row, so the stored sequence is contiguous and
`tick_gaps` means what it says — a gap is a lost batch, not a sampling stride. Metrics for an
unsampled tick are stored as JSON `null` (valid in a `JSONB NOT NULL` column) because they are
a pure function of the frame and always recomputable, while `mean_nearest_neighbor_distance` is
O(N²) and not worth paying on every tick of a 10 000-agent run. The read-side budget lives in
`frames_for_run(.., Some(n))`, which subsamples where the rendering constraint actually is.

---

### AC-33 (persistence half) — a cancelled run keeps what its completed batches wrote

**RED** — `ac33_a_cancelled_run_keeps_the_frames_its_completed_batches_wrote`, and
`ac34_a_recorded_signal_survives_redelivery_and_keeps_its_payload_verbatim` alongside it

```
error[E0432]: unresolved imports `boidboard::workflow::FinalizeRequest`, `boidboard::workflow::SignalRecord`, `boidboard::workflow::finalize_run_core`, `boidboard::workflow::record_signal_core`
  --> boidboard/tests/workflow.rs:30:32
   |
30 |     BatchCursor, BatchRequest, FinalizeRequest, SignalRecord, finalize_run_core,
   |                                ^^^^^^^^^^^^^^^  ^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^ no `finalize_run_core` in `workflow`
   |                                |                |
   |                                |                no `SignalRecord` in `workflow`
   |                                no `FinalizeRequest` in `workflow`
31 |     record_signal_core, simulate_batch_core, simulation_workflow_info,
```

**GREEN** — `FinalizeRequest` / `finalize_run_core` and `SignalRecord` /
`record_signal_core`.

**REFACTOR** — none needed.

**What the test proves that the workflow-level half could not.** Two batches run for real,
101 frames land in Postgres, the operator cancels, and the run is finalized `cancelled` — and
the frames are still there, contiguous, with `max_tick = 100` and `final_state_hash` equal to
the checkpoint the second batch actually reached. Then the test replays a *straggler*: a
duplicate delivery of batch 1 arriving after the cancellation. The run stays `cancelled`. That
last assertion is why `simulate_batch_core`'s `ticks_completed` update carries
`.filter(status.eq_any([QUEUED, RUNNING]))` — without it, an at-least-once tail could put a
terminated run back into `running` and the UI would show a cancelled run apparently still
going.

---

### AC-34 (provenance half) — a recorded signal survives redelivery, verbatim

Covered by the same RED as above.

**GREEN** — `record_signal_core` looks for an existing `(run_id, tick, kind, payload)` row and
reuses it, otherwise inserts.

**REFACTOR** — none needed.

**Why a lookup and not a constraint.** Frames get idempotency free from
`UNIQUE (run_id, tick)`. Signals cannot: two genuinely different steers may legitimately share
a tick, so there is no unique key to add. The check is therefore an explicit lookup — made
exact by Postgres's *semantic* `jsonb` equality rather than by string-comparing serialized
payloads, so `{"a":1,"b":2}` and `{"b":2,"a":1}` are correctly one intervention and not two.

---

### AC-36 — crash resume reaches an identical final state hash

**RED** — none. `ac36_a_run_resumed_from_its_postgres_checkpoint_reaches_the_same_final_state_
hash` passed the first time it ran, because `simulate_batch_core` was already written to resume
from a stored checkpoint. This is the headline durability claim of the whole project, so a
green-on-first-run test was not acceptable as evidence:

**MUTATION CHECK (1/2)** — the checkpoint encoding was changed from `SimState`'s exact-decimal
wire form to a plain `Vec<Agent>` serialization, which is the lossy path the kernel's own docs
warn about (`serde_json`'s default float parser can land one ULP from the value written, and
one ULP changes the canonical hash).

**The test passed anyway — and that was the useful finding.** The mutation was harmless not
because the encoding is safe but because the test was weak: it compared an *uninterrupted
batched* run against a *resumed batched* run, and both go through the same checkpoint code. Any
systematic bug in that code corrupts both sides identically and the assertion still holds. A
second mutation confirmed it — an off-by-one making a batch resume from the frame *before* the
one its cursor names also passed.

**The test was rewritten.** The reference is now `in_memory_reference_hash()`: one pure kernel
call, `SimState::seeded(..)` then `run_batch(.., 300, 0)`, with no database involved at all.
Both database runs are compared against *that*. Re-running the two mutations against the
strengthened test:

```
---- ac36_a_run_resumed_from_its_postgres_checkpoint_reaches_the_same_final_state_hash stdout ----

thread 'ac36_a_run_resumed_from_its_postgres_checkpoint_reaches_the_same_final_state_hash' (727) panicked at boidboard/tests/workflow.rs:994:5:
assertion `left == right` failed: cutting a run into batches must not change its answer
  left: "2f501dee47215562"
 right: "b7bd1e33ccf5a464"
```

(the off-by-one resume), and

```
---- ac36_a_run_resumed_from_its_postgres_checkpoint_reaches_the_same_final_state_hash stdout ----

thread 'ac36_a_run_resumed_from_its_postgres_checkpoint_reaches_the_same_final_state_hash' (1548) panicked at boidboard/tests/workflow.rs:994:5:
assertion `left == right` failed: cutting a run into batches must not change its answer
  left: "3977f2bbc332d49e"
 right: "b7bd1e33ccf5a464"
```

(the lossy `Vec<Agent>` encoding). Both mutations were reverted immediately and the suite
re-run green. So the exact-decimal checkpoint encoding is **demonstrably** necessary here, not
merely defensive — at this seed and flock size, plain `Agent` serialization really does change
the final hash.

**GREEN** — no production change; the correct implementation was already in place. The
deliverable of this cycle was a test worth believing.

**REFACTOR** — the reference moved out of the database, as described.

**What the test asserts, in order.** A pure in-memory run of 300 ticks fixes the answer. A
checkpointed run of six batches reaches it — so batching alone changes nothing. A second run is
then interrupted after two batches inside a block scope, and every variable that scope held goes
out of scope at its closing brace; the resume cursor is *rediscovered* with
`max_tick(run_id)` rather than remembered. The resumed run reaches the same hash; every one of
the 301 frames matches the uninterrupted run's tick for tick, including the two either side of
the seam; and `tick_gaps` is empty. A final `assert_ne!` compares the tick-100 state with the
tick-300 state, so "identical" is a statement about a flock that genuinely moved rather than
about a constant.

---

### Misrouted cursors — a doc comment that was not yet true

Not an acceptance criterion, but a cycle worth logging because it started as a **documentation
bug**. `BatchCursor::run_id` was documented as being "echoed back so a misrouted result is
detectable rather than silently applied to the wrong run" — and nothing checked it. A comment
that describes a guarantee the code does not provide is worse than no comment, so the claim got
a test.

**RED** — `a_cursor_for_the_wrong_run_fails_the_workflow_rather_than_being_applied`

```
---- a_cursor_for_the_wrong_run_fails_the_workflow_rather_than_being_applied stdout ----

thread 'a_cursor_for_the_wrong_run_fails_the_workflow_rather_than_being_applied' (11033) panicked at boidboard/tests/workflow.rs:542:10:
a cursor for another run must not be applied: Object {"batches": Number(5), "final_state_hash": String("0000000000000000"), "finished_at": String("2026-08-16T06:23:02Z"), "run_id": Number(4242), "status": String("budget_exceeded"), "ticks_completed": Number(100)}
```

The workflow drove all five batches on a cursor belonging to run 4243 and reported success.

**GREEN** — compare `cursor.run_id` with `input.run_id` and fail the run naming both.

**REFACTOR** — none needed.

A stopped run is recoverable; a run whose frames came from two different simulations is not,
and there would be nothing afterwards to tell you which frames came from where.

---

### Activity registration

**RED** — `the_registered_activity_names_match_the_names_the_workflow_schedules`

```
error[E0425]: cannot find function `simulate_batch_info` in module `boidboard::workflow`
    --> boidboard/tests/workflow.rs:1089:30
     |
1089 |           boidboard::workflow::simulate_batch_info().name,
     |                                ^^^^^^^^^^^^^^^^^^^
```

**GREEN** — the three `#[activity]` shells, each finding an `AppDbPool` in activity state and
delegating to its `_core`.

**REFACTOR** — the connection checkout was extracted to `app_conn(ctx)`, shared by all three.

**Why this test exists at all.** `execute_activity_raw("simulate_batch", …)` names its target
with a string. A rename that misses one side is invisible until a production run hangs forever
waiting for a handler nobody registered. Asserting the macro-generated `{fn}_info().name`
against the literal the workflow schedules turns that into a test failure.

---

## Final verification

`cargo clippy -p boidboard --all-targets -- -D warnings` — clean. `boidboard/src/workflow.rs`
is also clean under `-W clippy::pedantic` and under `-D missing_docs`.

`cargo test -p boidboard`, run twice consecutively with no cleanup step in between. The
database tests commit nothing — every one of them runs inside `TestApp::with_transactional_db`'s
rolled-back transaction — so the second run starts from exactly the state the first one did
(**AC-40**).

Run 1:

```
     Running tests/persistence.rs (target/debug/deps/persistence-1c5597b6c6af2195)
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s
     Running tests/web.rs (target/debug/deps/web-74b239dd49c57ecd)
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s
     Running tests/workflow.rs (target/debug/deps/workflow-6ba5514827e65d7e)
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.36s
```

Run 2:

```
     Running tests/persistence.rs (target/debug/deps/persistence-1c5597b6c6af2195)
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s
     Running tests/web.rs (target/debug/deps/web-74b239dd49c57ecd)
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.45s
     Running tests/workflow.rs (target/debug/deps/workflow-6ba5514827e65d7e)
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.35s
```

## Which test proves which criterion

| AC | Test |
|---|---|
| AC-29 | `ac29_workflow_history_carries_only_a_cursor_never_the_agent_array` |
| AC-30 | `ac30_the_workflow_runs_with_no_database_and_only_the_virtual_clock` (and `assert_replays`' DB-free environment in every other workflow test) |
| AC-31 | `ac31_the_workflow_replays_without_diverging`, plus `assert_replays` in every workflow test |
| AC-32 | `ac32_a_duplicated_simulate_batch_leaves_one_set_of_frames_and_no_gaps` |
| AC-33 | `ac33_a_cancel_signal_stops_the_run_at_the_next_batch_boundary`, `ac33_a_cancelled_run_keeps_the_frames_its_completed_batches_wrote`, `ac33_engine_cancellation_kills_the_run_without_issuing_another_command` |
| AC-34 | `ac34_a_steer_signal_changes_params_for_later_batches_and_records_itself`, `ac34_a_steer_applies_only_recognised_parameters_but_records_the_whole_payload`, `ac34_steer_overrides_change_the_named_parameters_and_nothing_else`, `ac34_a_recorded_signal_survives_redelivery_and_keeps_its_payload_verbatim` |
| AC-35 | `ac35_max_ticks_bounds_a_run_to_exactly_ceil_max_over_batch_batches`, `ac35_a_cursor_that_stops_advancing_trips_the_budget_guardrail` |
| AC-36 | `ac36_a_run_resumed_from_its_postgres_checkpoint_reaches_the_same_final_state_hash` |

Two tests carry no AC of their own —
`the_registered_activity_names_match_the_names_the_workflow_schedules` and
`a_cursor_for_the_wrong_run_fails_the_workflow_rather_than_being_applied`. Both exist because a
failure they catch is silent: an unregistered handler makes a run hang forever with no error,
and a misrouted cursor corrupts a run's frames with no error at all.
