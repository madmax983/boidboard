# TDD log — the web ↔ workflow seam

Evidence for **AC-52** covering `boidboard/tests/integration.rs`, `routes::create_run`,
`routes::cancel_run` and `routes::decode_agents`.

## What this file closes

The web layer and the workflow layer were built independently and nothing owned the join
between them. `POST /runs` created the `scenarios` and `runs` rows, returned `303`, and then
**nothing happened**: `status='queued'`, `ticks_completed=0`, `workflow_execution_id=NULL`,
zero frames, forever. `simulation_workflow` was registered, correct, replay-clean and
unreachable. Every existing test passed in that state, because each one proved a layer.

Two gaps turned out to live in that seam, not one. The second — the run detail page decoding
`frames.agents` in a shape the workflow never writes, so a completed run drew an empty world —
was found by the cycle-5 test below and is exactly the same failure mode: two correct halves,
never run against each other.

## How the red output below was produced

Every block is real captured `cargo test -p boidboard` output. Nothing is reconstructed.

The seam is one compilable unit — a route cannot start a workflow through a client that no
`AppState` yet carries — so the harness (`TestApp` + the real `HarvestPlugin`, i.e. a genuine
Harvest worker against a genuine Postgres queue) had to exist before any of these tests could
run at all. Cycles 1 and 2's red is therefore the true pre-change behaviour; cycles 3–5's red
was captured by **reverting the specific implementation slice each one covers** and re-running:

* **RED-A** — `create_run`'s dispatch call removed and `cancel_run` unregistered. This is the
  bug exactly as reported. Four of the five tests fail here.
* **RED-B** — dispatch restored, but the workflow id made per-request rather than derived from
  the run id. That is the naive first implementation cycle 3 exists to rule out, and the only
  red that proves the idempotency test *discriminates* rather than merely requiring a run.
* **RED-C** — dispatch restored and green; cycle 5's red is then the real, unreverted state of
  the tree, and it is where the second gap surfaced.

## Running order

Cycle 1 (something is dispatched) → 2 (it runs to completion) → 3 (exactly once) → 4 (cancel)
→ 5 (the result is visible). Dependency order: there is no point asserting a run *completes*
before anything starts it, no point asserting it starts *once* before it starts at all, and
nothing to cancel or draw until it runs.

---

### 1 — creating a run dispatches a durable workflow

**RED** — `creating_a_run_records_the_workflow_execution_it_started`

```
thread 'creating_a_run_records_the_workflow_execution_it_started' (25765) panicked at boidboard/tests/integration.rs:168:5:
creating run 3 must start its durable workflow and record the execution it started; the run row still says workflow_execution_id=NULL, which is the whole bug: the simulation will never run
```

**GREEN** — `create_run` takes `State<AppState>`, pulls the `WorkflowHandleClient` the Harvest
plugin installs into app state and a connection from its `HarvestDbPool`, calls
`start_or_load`, and writes the returned `exec_id` back to `runs.workflow_execution_id`.
In-process, not over the management API: same transaction-capable connection, no self-HTTP,
no serialization of a start the process could have made directly.

**REFACTOR** — the start was lifted out of the handler into
`routes::start_simulation_workflow(client, conn, run)`, taking its client and connection as
arguments instead of digging them out of `AppState`. That is what lets cycle 3 call the exact
code the route runs, twice, rather than a lookalike. The handler kept only `dispatch_run`,
which resolves the two pieces of state and logs.

The run is deliberately left `queued` by the route: `simulate_batch` already moves it to
`running` when a worker genuinely picks it up, so the status reflects work done rather than
work hoped for — and `tests/web.rs`'s existing "a new run is queued" assertion stays true.

---

### 2 — the workflow actually drives the run to completion

**RED** — `a_created_run_is_driven_to_completion_with_its_frames_in_postgres`

```
thread 'a_created_run_is_driven_to_completion_with_its_frames_in_postgres' (26322) panicked at boidboard/tests/integration.rs:168:9:
run 30 never reached a terminal state: status=queued ticks_completed=0/200 workflow_execution_id=None error=None — is a Harvest worker running?
```

**GREEN** — no new production code: cycle 1's dispatch is what this asserts the *consequences*
of. What this cycle added is the proof — a real worker (`TestApp` runs `HarvestPlugin`'s
startup hook, which boots the runtime), a real queue, and assertions on rows nothing in the
test wrote: terminal `completed`, `ticks_completed == max_ticks`, a 16-hex-digit
`final_state_hash`, 201 frames for ticks `0..=200`, no gaps, and the run's final hash equal to
the last frame's `state_hash`.

**REFACTOR** — none needed.

---

### 3 — one run is driven by exactly one execution

**RED-A** — `a_run_is_dispatched_once_however_many_times_the_start_is_delivered`, against the
undispatched tree:

```
thread 'a_run_is_dispatched_once_however_many_times_the_start_is_delivered' (26323) panicked at boidboard/tests/integration.rs:272:10:
the run was dispatched
```

**RED-B** — the discriminating one. Dispatch restored, but with the naive per-request workflow
id (`run-{id}-{nanos}`) a first pass would plausibly write:

```
thread 'a_run_is_dispatched_once_however_many_times_the_start_is_delivered' (1074) panicked at boidboard/tests/integration.rs:291:5:
assertion `left == right` failed: the second start must resolve to the execution already driving run 39
  left: "00003d09-708e-4f2c-aae7-82a4bcc4b6f4"
 right: "0000f375-f836-4182-b489-245d34c15939"
```

**GREEN** — `workflow_id_for_run(run_id) -> "run-{run_id}"`, plus
`WorkflowIdReusePolicy::AllowDuplicate`. The id is a pure function of the run, so a
resubmitted form, a retried request or a redelivered start all resolve to the same
`(simulation_workflow, run-N)` pair and Harvest hands back the live execution
(`created == false`) instead of racing a second one at the same `(run_id, tick)` rows.

The test asserts all three: same `exec_id`, `!created`, and — read straight out of
`harvest_workflow_executions`, not inferred from the `runs` row, which can only ever name one
execution anyway — exactly one execution for that `workflow_id`, before and after the run
finishes.

**REFACTOR** — none needed.

---

### 4 — cancel is reachable from the UI

**RED** — `cancelling_a_run_from_the_ui_stops_it_and_keeps_the_frames_it_earned`

```
thread 'cancelling_a_run_from_the_ui_stops_it_and_keeps_the_frames_it_earned' (26324) panicked at boidboard/tests/integration.rs:333:10:
assertion `left == right` failed: expected status 303, got 404 Not Found.
Body: {"type":"https://autumn.dev/problems/not-found","title":"Not Found","status":404,"detail":"No route matches /runs/34/cancel","instance":"/runs/34/cancel","code":"autumn.not_found","request_id":"645fda1b-562b-4000-8656-b18e1995573d","errors":[]}
```

**GREEN** — `POST /runs/{id}/cancel` (`#[public]`, like every other route) resolves the run's
`workflow_execution_id` and calls `autumn_harvest::signal::send_signal(conn, exec_id,
"cancel", …)`.

A **signal**, not `WorkflowHandle::cancel`, and the difference is the point. An engine cancel
kills the execution where it stands: the in-flight batch's frames survive (the activity
committed them) but the `runs` row is never finalized, so it claims to be `running` forever —
and `simulation_workflow`'s own comment explains why it cannot tidy up past a
`WorkflowCancelled` event without failing the next replay. The `cancel` signal is read at a
batch boundary, which is what lets the workflow record the intervention to `run_signals` and
finalize the row honestly. AC-33 asks for the terminal *cancelled* state with partial results
intact; only the signal delivers it.

An already-terminal run redirects unchanged rather than erroring — cancelling a finished run is
a double click, not a fault.

**REFACTOR** — `harvest_conn(&AppState)` was extracted, since `create_run` and `cancel_run`
both need a connection to Harvest's storage (deliberately *not* the app pool: in a split
deployment the queue lives in a different database from `runs`).

---

### 5 — what the workflow produced is actually visible

**RED-A** — against the undispatched tree, there is nothing to draw:

```
thread 'the_detail_page_of_a_completed_run_draws_the_flock_and_its_hash' (26350) panicked at boidboard/tests/integration.rs:168:9:
run 36 never reached a terminal state: status=queued ticks_completed=0/200 workflow_execution_id=None error=None — is a Harvest worker running?
```

**RED-C** — and this is the one that mattered. With cycles 1–4 green, a real completed run,
201 real frames in Postgres and a real hash on the page, the flock was still **empty**:

```
thread 'the_detail_page_of_a_completed_run_draws_the_flock_and_its_hash' (2918) panicked at boidboard/tests/integration.rs:415:10:
expected 120 element(s) matching selector `svg.flock polygon.agent`, found 0.
```

The second gap in the same seam. `simulate_batch` stores `SimState`'s own wire array —
`{"id":0,"px":"262.4078154059424","py":"35.6917378162098","vx":"-0.34…","vy":"-0.17…"}`, flat,
with coordinates as exact decimal **strings**, because a JSON float can return one ULP off and
one ULP changes the canonical state hash. `routes::decode_agents` deserialized `Vec<Agent>` —
nested `pos`/`vel`, numeric — and, being a tolerant reader (`unwrap_or_default`), turned every
real frame into an empty flock without a whisper. Both halves were tested; each against its
own fixtures. `tests/web.rs` seeds `serde_json::to_value(Vec<Agent>)`, a shape the application
never writes, so it drew boids for a decoder that could not read production data.

**GREEN** — `decode_agents` rebuilds the frame through `SimState` — the same door it left by,
exactly as `load_checkpoint` does on the resume path — and falls back to the direct
`Vec<Agent>` form for hand-written or older frames. The test asserts the full chain: a
`completed` badge, `svg.flock` with **120** `polygon.agent` marks (the preset's flock, drawn
from rows only a Harvest activity wrote), trajectory ribbons, and
`dd.prov-final-state-hash` carrying the run's hash with no `prov-pending` marker.

**REFACTOR** — none needed.

---

## Isolation and the two-consecutive-run rule (AC-40)

These tests **commit**: a workflow that never commits is a workflow no worker can see, so the
transactional isolation the other files use is unavailable here by construction. Two things
keep AC-40 true anyway:

* **No assertion is global.** Every one is scoped to the run the test itself created — counts,
  ticks, gaps, signals and execution counts are all `WHERE run_id = …`. The file passes
  repeatedly against an accumulating database because it never asks how many rows exist.
* **The tests run one at a time**, behind a `tokio::sync::Mutex`. Each boots its own worker,
  and a finishing test drops its runtime — killing that worker wherever it happens to be,
  including mid-batch in another test. Harvest recovers from exactly that (the lease expires,
  the task is redelivered, `simulate_batch` is idempotent by construction — AC-32), but it is
  not worth paying for on every run.

## What is *not* covered

**The cancel button itself.** `POST /runs/{id}/cancel` exists and is proven end to end, but
`views.rs` is out of scope for this change, so nothing on the run detail page posts to it yet.
The route is reachable; the button is a one-line follow-up in the view layer.

**A workflow that dies without finalizing leaves the run row lying.** Found while capturing the
red output above, by repeatedly killing test processes mid-batch: Harvest counts a worker that
dies holding a task as a crash strike and, after three, seals the execution `FAILED` with a
`PoisonPill` error. `finalize_run` only ever runs from *inside* the workflow body, so nothing
writes that outcome back — the `runs` row is stranded at `running` forever, and the UI keeps
claiming a run is in progress that no worker will ever touch again. Four such rows are sitting
in the dev database right now (runs 43, 49, 54, 61: `status='running'`, part-spent budgets,
`error` NULL, executions `FAILED`).

This is a third gap in the same seam and it is **not closed here**. A single worker crash is
recovered normally — the lease expires, the task is redelivered, `simulate_batch` is idempotent
— so this only bites a genuinely poisoned workflow. Closing it needs a terminal-failure path
back into `runs` (a completion callback, or a scanner reconciling non-terminal runs against
their executions), which is a change of its own shape and deserves its own cycle.
