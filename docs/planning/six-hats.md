# Boidboard — Six Thinking Hats Session

> A structured pre-build thinking session using Edward de Bono's Six Thinking Hats.
> Each hat is a dedicated, uncontaminated pass. Read them in order.
>
> **Subject**: Boidboard — a web app for defining, running, observing and comparing
> boids flocking simulations as durable experiments.
> **Stack (fixed)**: Rust, Autumn Web 0.6, Autumn Harvest 0.5, Postgres, Maud + htmx.
> **Date**: 2026-08-16

---

## ⚪ White Hat — Facts and Information

*Neutral data only. What we know, what we don't, and which gaps actually block the build.*

### F1. Facts about the boids algorithm

The boids model (Craig Reynolds, SIGGRAPH 1987, *"Flocks, Herds, and Schools: A
Distributed Behavioral Model"*) is fully specified in the public literature. These are
facts, not choices:

- Flocking emerges from three steering rules applied per-agent over a local
  neighbourhood: **separation** (steer away from crowding neighbours),
  **alignment** (steer toward the average heading of neighbours), and
  **cohesion** (steer toward the average position of neighbours).
- Each rule produces a *steering force*, not a velocity. The canonical formulation is
  Reynolds' steering model: `steer = desired_velocity - current_velocity`, where
  `desired_velocity` is normalised to `max_speed`.
- Forces are summed with per-rule weights, the total is clamped to `max_force`,
  applied to velocity, and velocity is clamped to `max_speed`. Position integrates
  by Euler: `pos += vel * dt`.
- The neighbourhood is defined by a radius (and optionally a field-of-view angle).
  Separation conventionally uses a smaller radius than alignment/cohesion.
- Naive neighbour search is O(n²) per tick. Spatial hashing / grid binning reduces
  this, and is a well-known optimisation, not a research problem.
- The model is **fully deterministic** given initial conditions. All stochasticity
  enters through initial position/velocity seeding.

Standard measured quantities (also established, not invented here):

- **Polarization / order parameter**: `|Σ v̂ᵢ| / N`, the magnitude of the mean unit
  heading. Ranges `[0, 1]`. ~0 = disordered swarm, ~1 = perfectly aligned flock.
  This is the standard Vicsek order parameter.
- **Mean nearest-neighbour distance**: mean over agents of the distance to the
  closest other agent. Indicates flock density/spacing.
- **Collision count**: pairs closer than some threshold. Definition is a *choice*
  (see gaps).
- **Time-to-goal**, **stuck detection**: application-level, not standard boids.
  These are our inventions and we define them.

### F2. Facts about the stack (verified against vendored crate sources)

These were confirmed by reading the actual crate sources present in the local cargo
registry at `/root/.cargo/registry/src/index.crates.io-*/`, not from memory:

**Environment**
- `rustc 1.94.1` / `cargo 1.94.1`. Autumn requires 1.88.0+ (edition 2024). **Satisfied.**
- Postgres is **live and accepting connections** on `/var/run/postgresql:5432`
  (`pg_isready` confirms). `psql` is on PATH.
- No `DATABASE_URL` is currently set in the environment — it must be configured.
- The `docker` binary exists on PATH; the daemon's state was not verified.
- Crates present in the registry cache: `autumn-web-0.6.0`, `autumn-macros-0.6.0`,
  `autumn-harvest-0.5.0`, `autumn-harvest-macros-0.5.0`, `autumn-harvest-plugin-0.5.0`.
- The repository `/home/user/boidboard` is a git repo with a single commit containing
  only `LICENSE`, `.gitignore`, and an empty `docs/planning/`. **There is no Cargo
  project yet.** Everything is greenfield.

**Autumn Web 0.6.0**
- Default features include `maud`, `htmx`, `tailwind`, `db`, `cache-moka`,
  `http-client`, `reporting`, `flash`. Maud is on by default.
- Provides `#[get]`/`#[post]`/`routes![]`/`#[autumn_web::main]`, `#[model]`,
  `#[repository]`, embedded migrations, `#[scheduled]`, `#[job]`,
  `/health` + `/actuator/*`.
- **`autumn_web::test`** provides `TestApp` / `TestClient` / `TestResponse` —
  a Spring-Boot-`MockMvc`-shaped in-process HTTP test harness with fluent
  assertions (`assert_status`, `assert_body_contains`).
- **`autumn_web::test_html`** provides a dependency-free HTML parser and CSS
  selector matcher backing *structural* assertions. Supported: tag, class, id,
  attribute selectors (`[a=v]`, `^=`, `$=`, `*=`), compound selectors, descendant
  and child combinators. Not supported: pseudo-classes, sibling combinators, XPath.
  It deliberately parses fragments *literally*, so a bare `<tr>` htmx swap survives.
- **`TestDb`** exists but is a **Postgres testcontainer** requiring Docker; the
  crate's own tests that use it are marked `#[ignore = "requires Docker"]`.
- `autumn_web::system_test` provides a **real browser** harness (`SystemTest`,
  `Page`, `.click()`, `.fill()`, `.expect_text()`, `.expect_hx_settle()`,
  `.expect_sse_event()`, `.snapshot()`, `.console_errors()`).
- `src/sse.rs` provides first-class SSE (`Sse`, `Event`, `keep_alive`) integrated
  with `state.channels()`.
- **`src/live.rs`** provides a `LiveFragment` trait: implement it on a `#[model]`,
  add `broadcasts = true` to `#[repository]`, and generated `save`/`update`/`delete_by_id`
  automatically publish rendered `hx-swap-oob` HTML fragments to a named channel.
- `src/stories/` provides a `/_stories` widget gallery and a `story!` macro.
- Autumn ships an `examples/flock` — a "literary boids" WASM island using Yew CSR
  with a custom `'wasm-unsafe-eval'` CSP. **The domain is already a known Autumn
  example**, but via a client-side WASM island, which our constraints exclude.

**Autumn Harvest 0.5.0**
- Default features: `db`, `unified-dag-execution`. Also `testing`, `schema`,
  `metrics-rs`, `wasm-activities`.
- `#[workflow]`, `#[activity]`, `#[query]`, `#[update]`, `#[dag]`, and the
  `workflows![]` / `activities![]` / `queries![]` / `updates![]` collection macros.
- `HarvestPlugin::new().workflows(...).activities(...).api("/api/harvest")` wires
  into `autumn_web::app().plugin(...)`.
- `WorkflowContext` surface confirmed to include: `execute_activity`,
  `execute_activity_raw`, `execute_local_activity`, `timer`, `sleep_until`,
  `start_timer`/`cancel_timer`/`reset_timer`, `spawn_child_workflow`,
  `receive_signal`, **`try_receive_signal` (non-blocking)**, `drain_signals`,
  `wait_for_signal_timeout`, `side_effect`, `system_now`, `new_uuid`,
  `random_u64`/`random_f64`/`random_range`, `continue_as_new`,
  `should_continue_as_new`, `history_event_count`.
- **Determinism guardrails** are a real, enumerated catalog in `src/guardrail.rs`:
  HVG001 WallClock, HVG002 Randomness, HVG003 ProcessEnv, HVG004 SleepTimer,
  HVG005 BackgroundTask, HVG006 DirectIo, HVG007 ProcessGlobal,
  HVG008 NonDeterministicPredicate, HVG009 UnsafeLogging (Warning),
  HVG010 SelectMacro, HVG011 NonDeterministicIteration (HashMap/HashSet order).
  All are `HardBlocker` except HVG009. `guardrail.rs` is a catalog only, **but**
  `autumn-harvest-macros/src/determinism_lint.rs` is a real AST linter invoked by
  the `#[workflow]` macro that detects randomness paths and `Rng` method calls
  (`gen`, `gen_range`, …), plus `det_check.rs` DET010 at runtime.
- **`autumn_harvest::testing::WorkflowReplayer`** replays a workflow against a
  history. `replay_from_events(Vec<WorkflowEvent>)` and `replay_from_json(&str)`
  **require no database**; only `replay_from_db` needs the `db` feature. It returns
  a `ReplayReport` with a typed `ReplayStatus` and `NonDeterminismKind`
  (`ActivityScheduleMismatch`, `TimerMismatch`, `SignalMismatch`,
  `ChildWorkflowMismatch`, `SideEffectMismatch`, …). There is also a
  `ReplayVerifier` for batch fixture directories with CI report formatting.
- **Default payload offload threshold is 256 KiB** (`builder.rs`,
  `DEFAULT_PAYLOAD_OFFLOAD_THRESHOLD`). Larger activity payloads are offloaded via
  `PayloadOffloader`, which must be configured.
- **Default `continue_as_new` soft threshold is 10,000 history events**, with an
  optional `history_event_hard_cap` that DLQs the execution with
  `HistoryCapExceeded`.
- Activities are **at-least-once**. The README is explicit: a worker crash or
  timeout triggers a retry; use `ctx.idempotency_key()` to make downstream effects
  safe.
- Task dispatch uses `SKIP LOCKED` + `LISTEN/NOTIFY`. `result_raw()` blocks on
  LISTEN/NOTIFY wakeups rather than polling.
- Harvest ships its own `migrations/` directory.

### F3. Facts about the constraints

- Fixed: Rust, Autumn Web, Autumn Harvest, Postgres, server-rendered Maud + htmx.
  No React/SPA.
- Fixed process: strict red → green → refactor TDD by AI agents, then multi-angle
  code review, then a PR.
- Fixed bar: must actually build; tests must actually pass against a live local
  Postgres.
- Fixed budget: completable in a single focused build session.

### F4. Information gaps

**Gaps that genuinely BLOCK the build** (must be decided before code):

1. **Where simulation state lives between batches.** Either the workflow carries
   the full agent array as JSON through its history (bounded by the 256 KiB offload
   threshold and the 10,000-event budget), or Postgres owns frames and the workflow
   carries only `(run_id, tick)`. These produce materially different schemas,
   different activity signatures, and different failure modes. Not defaultable.
2. **The idempotency contract for the batch activity.** Activities are at-least-once
   (F2). If the activity appends frames, a retry duplicates them. The dedup strategy
   (natural primary key + upsert vs. explicit idempotency key) shapes the schema.
   Not defaultable.
3. **Frame sampling rate and per-run storage cap.** Drives the schema, the SVG
   payload size, the compare page, and the run duration. Not defaultable because
   every downstream size estimate depends on it.

**Gaps that can be DEFAULTED** (pick a sane value, note it, move on):

- World topology: toroidal wrap vs. reflecting bounds vs. soft steer-back.
  → Default: **toroidal wrap** (simplest, no edge-case forces, classic).
- Separation radius vs. alignment/cohesion radius. → Default: one `neighbour_radius`
  with separation using `radius * 0.5`.
- Field-of-view angle. → Default: **omit** (360° vision). Reduces parameter count.
- Collision threshold. → Default: agents closer than `separation_radius * 0.25`.
- `dt`. → Default: fixed `1.0` (unit timestep); avoids a whole class of
  integration-stability questions.
- Neighbour search algorithm. → Default: **O(n²)**. At n ≤ 500 and batched ticks
  this is microseconds; a spatial grid is a refactor-step optimisation, not a
  requirement.
- RNG choice. → Default: a small deterministic PRNG seeded from the scenario seed.
  Must be reproducible across platforms, so avoid anything whose stream is
  unspecified.
- Obstacle geometry. → Default: **circles only** (centre + radius). Polygons are a
  scope trap.
- Goal semantics. → Default: a single goal point, "reached" when the flock centroid
  is within a radius.

**Gaps that are genuinely unknown and must be discovered by doing:**

- Whether `autumn-web` and `autumn-harvest` compile cleanly together in *this*
  sandbox with *these* versions, and how long a cold build takes. Nothing in the
  sources contradicts it, and the plugin exists precisely for this pairing, but it
  is unverified until attempted. **This is the single largest unknown.**
- Whether the `#[workflow]` determinism lint produces false positives against
  ordinary numeric simulation code.
- Cold `cargo build` wall-time for this dependency graph, which directly consumes
  the session budget.

### F5. Explicit fact/assumption separation

| Statement | Status |
|---|---|
| Boids = separation + alignment + cohesion, steering-force formulation | **Fact** (literature) |
| Polarization = `\|Σ v̂ᵢ\|/N`, range [0,1] | **Fact** (Vicsek) |
| Harvest activities are at-least-once | **Fact** (README, verified) |
| Payload offload threshold = 256 KiB | **Fact** (source, verified) |
| `WorkflowReplayer::replay_from_events` needs no DB | **Fact** (source, verified) |
| Postgres is live in this sandbox | **Fact** (`pg_isready`, verified) |
| `TestDb` requires Docker | **Fact** (source, verified) |
| The two crates compile together here | **Assumption** (unverified) |
| O(n²) neighbour search is fast enough | **Assumption** (well-founded, untested) |
| Server-rendered SVG is a viable flock renderer | **Assumption** (sized in Green hat) |
| A single session suffices for the full working concept | **Assumption — and the Black hat disputes it** |

---

## 🔴 Red Hat — Emotions, Intuition, Gut Reaction

*No justification. Feelings only.*

**The excitement is real.** Boids is one of the genuinely *delightful* algorithms.
You write about eighty lines of vector math and something that looks alive falls out.
Everyone who has implemented it remembers the moment the flock first turned as one.
Building a *board* around that — a place to run experiments and compare them — feels
like giving that moment a laboratory. I want to use this.

**"Boidboard" is a great name.** It's fun to say. It sounds like a product. That
matters more than it should.

**The trajectory ribbons will be the thing people screenshot.** Long-exposure paths
of two hundred agents braiding through a world, in one static SVG. I already want to
see it. If we only ship one visual, ship that one.

**What feels wrong:** the parameter form. My heart sinks slightly at the thought of
eleven number inputs in a column. That's a *configuration screen*, and configuration
screens are where joy goes to die. If the first thing a user meets is a tax form,
we've lost them before the flock ever moves.

**What feels worrying:** the phrase "in batches of ticks." It's doing a lot of quiet
work in the concept. It's the seam where the elegant pure simulation meets the
durable-workflow machinery, and seams are where projects bleed time. I have a
low-grade dread about it.

**The scope feels like too much. Distinctly too much.** Reading the working concept,
I count: a scenario editor, a run list, live progress, trajectory rendering, five
metrics, obstacles, goal waypoints, *and* side-by-side comparison. That's a product
roadmap wearing a single build session as a disguise. My honest gut number is that
about half of it fits, and the half that fits is the good half. There's a real risk
of ending the session with nine things at 70% and nothing that runs.

**What would make a developer's heart sink** opening this repo: finding the boids
math tangled into a Diesel model, or into a workflow function. If `step()` can't be
called without a database connection, the project is spiritually over — the one
genuinely beautiful, pure, testable thing in the domain will have been ruined for no
reason. Also: a `src/` with twenty files and no obvious entry point. Also: mock-heavy
tests that assert the mock was called.

**What would make a developer's heart lift:** a `boids-core` crate with no
dependencies but a PRNG, forty fast unit tests, and a property test that says *same
seed, same flock, forever*. That's a repo you trust on sight.

**What would make a user go "oh, that's lovely":** dragging a scrubber and watching
the flock re-form, with no JavaScript anywhere. Or the contact sheet — twelve frames
in a grid — where you can *see* order emerging left-to-right across the page. Or
running the same seed twice and getting pixel-identical output, which is quietly
reassuring in a way people feel before they can articulate.

**The nagging doubt:** is the durable-workflow angle actually load-bearing, or is it
a framework demo wearing a boids costume? If a reviewer can say "you could have done
this with a for-loop and a background job," the whole premise deflates. That needs an
answer, not a hope. *(The Green hat owes me one.)*

**The quiet confidence:** boids is deterministic. Harvest demands determinism. Those
two facts want to be friends. There's something genuinely there.

---

## ⚫ Black Hat — Caution, Critical Judgement

*Rigorous criticism of THE PLAN's viability. Not a bug list — a judgement of whether
this plan, as stated, can succeed.*

### B1. The scope is over-committed by roughly 2×. This is the primary risk.

The working concept enumerates, as though they were one deliverable: a scenario
editor with eleven-plus parameters including obstacles and goal waypoints; durable
batched execution; five distinct metrics; a run list; live progress; trajectory
rendering; and side-by-side comparison. Under *strict* red→green→refactor TDD — where
every one of those needs a failing test first — this is not a single-session artefact.

The TDD constraint is not a small multiplier. It is roughly a 2–3× multiplier on
naive implementation time, and it is a *fixed* constraint, so it cannot absorb
overrun. The compressible thing is scope, and scope is currently uncompressed.

Concretely, the following are each a session's work on their own, not a bullet point:
obstacle avoidance (a whole second force model with its own failure modes), goal
waypoints (path state, arrival semantics, ordering), and side-by-side comparison
(a second rendering path plus a diffing UI).

### B2. Cold build time is an unpriced, unrecoverable tax.

`autumn-web` pulls Axum, Diesel, diesel-async, deadpool, Maud, Tokio, reqwest,
moka, and more. `autumn-harvest` adds its own Diesel/tokio-postgres/wasmtime-adjacent
graph. A cold build of this dependency set is plausibly 5–15 minutes, and every
subsequent incremental test cycle pays proc-macro expansion for `#[model]`,
`#[repository]`, `#[workflow]`, `#[activity]`, *and* the `#[workflow]` determinism
AST linter.

Strict TDD means many, many compile cycles. If the red-green loop costs 60–90 seconds
instead of 5, the session's effective budget collapses. **This risk is invisible in
the plan and is not mentioned anywhere in the concept.** It is also front-loaded: it
hits before any value is produced.

### B3. The Autumn ↔ Harvest integration is assumed, not verified.

`HarvestPlugin` is designed for exactly this pairing, and the version pairing
(web 0.6 / harvest 0.5) is the current one — so this is a *reasonable* assumption. But
it is still an assumption, with specific ways to go wrong:

- **Two migration sets.** Harvest ships `migrations/`; Autumn has embedded
  migrations. Both must run against the same database in the right order. Nothing
  guarantees they compose without configuration.
- **Two Diesel surfaces.** Both crates depend on Diesel/diesel-async. A version skew
  between them is a hard, unfixable-in-session wall. They must resolve to one
  version, and `Cargo.lock` will decide silently.
- **Pool contention.** Harvest documents "separate worker/web connection pools with a
  shared ceiling." Misconfigure it and the worker starves the web tier, or vice
  versa — and it will present as an intermittent hang under test, which is the most
  expensive class of bug to diagnose under time pressure.
- **Feature-flag interaction.** Harvest's `db` feature is default-on; `autumn-web`'s
  `db` is default-on. Turning either off to speed builds will break the other.

If integration fails at hour four, there is no fallback and the session produces
nothing. **This is the highest-severity risk even though B1 is the highest-likelihood
one.**

### B4. The workflow will be non-deterministic in ways the linter does not catch.

This is the subtlest and most dangerous technical risk, and it is *specific*:

- **`rand` inside `#[workflow]` is a compile-time HardBlocker** (HVG002, enforced by
  a real AST lint). Any plan that seeds the flock inside the workflow body fails to
  compile. Seeding must happen inside the activity, from the scenario's stored seed.
- **HVG011 — `HashMap`/`HashSet` iteration order — is a HardBlocker.** Simulation
  code reaches for maps constantly (neighbour buckets, agent lookup, per-agent
  metrics). Any such map whose iteration drives a scheduling decision in the workflow
  is a latent replay bomb. The lint downgrades command-free loops to a warning, which
  means *the dangerous cases are exactly the ones that stay HardBlocker* — but it also
  means developers will learn to ignore the warning class.
- **Floating-point non-determinism is not covered by any guardrail.** The lint catalog
  has no rule for it because it isn't a Harvest concern — but it is a *boids* concern.
  If the batch activity's output is replayed or compared across machines, differences
  in FMA contraction, SIMD autovectorisation, or `f32` vs `f64` accumulation order
  produce divergent flocks from identical seeds. Summing neighbour contributions in a
  different order changes the result. **"Same seed → same flock" is a promise the plan
  makes implicitly and has no mechanism to keep** unless summation order is pinned
  and the reduction is written deterministically.

The failure mode for all three is the worst kind: it passes locally, and surfaces as a
replay non-determinism error later, under a different code path.

### B5. The history-event and payload budgets are easy to blow, and the plan is silent on both.

Arithmetic the concept does not do:

- Each `execute_activity` writes multiple durable events (scheduled / started /
  completed). Call it ~3.
- The soft `continue_as_new` threshold is **10,000 events**; a hard cap DLQs the run.
- Therefore the budget is on the order of **~3,000 activity calls per execution**.

One activity per tick means a 3,000-tick simulation is already at the rotation
threshold, and a 10,000-tick run *fails*. Batching is therefore not an optimisation —
it is **load-bearing correctness**, and the concept treats it as an implementation
detail ("in batches of ticks").

Independently, the **256 KiB payload offload threshold** constrains the other design:
500 agents × 4 `f64` as JSON is roughly 40–60 KiB of text — comfortably under, but
only about 4× headroom. Raise the agent count to 2,000 and every activity round-trip
silently crosses into offload, which requires a configured `PayloadOffloader` that
the plan never mentions. **The plan has no stated agent-count ceiling**, so nothing
prevents a user from typing 5,000 into the form and breaking the engine.

### B6. Server-rendered flock visualization: genuinely hard, but *less* hard than it looks.

Honest assessment, with numbers.

**What is fine:**
- A single frame of 200 agents as `<circle>` elements is ~40 bytes each ≈ **8 KB**.
  As rotated `<polygon>` triangles, ~100 bytes each ≈ **20 KB**. Both trivial. SVG
  compresses ~10:1, so over the wire this is nothing.
- Metric sparklines are single `<polyline>` elements. Trivial.
- A contact sheet of 12 frames × 200 circles ≈ **96 KB** uncompressed, ~10 KB gzipped.
  Acceptable.

**What is not fine, and the plan does not acknowledge:**
- **Naive trajectory ribbons do not fit.** 200 agents × 200 frames × ~12 bytes per
  coordinate pair ≈ **480 KB of SVG in one response**, with 200 polylines of 200
  points each. That is a slow parse and a heavy DOM. The money shot from the Red hat
  *does not work at full resolution* and must be subsampled — e.g. 30 agents × 60
  frames ≈ 22 KB, which is fine. **This is a real constraint on the headline visual
  and needs to be a decision, not a discovery.**
- **Smooth animation is genuinely out of reach.** Real-time flocking is 30–60 fps.
  htmx polling at 1–2 Hz is two orders of magnitude short. Any expectation of
  "watching the flock fly" in the browser is unmeetable within the constraints. The
  product must reframe around *stills, trajectories, and scrubbing* rather than
  animation. If a stakeholder is imagining a live animated flock, that expectation
  will be violated, and it is better to break that now than at demo time.
- **Live progress via polling is a load multiplier.** `hx-trigger="every 1s"` on a run
  detail page means a full re-render and re-query per second per open tab.
  Acceptable for a demo, and worth stating as a known limitation rather than
  discovering it under review.

**Verdict:** the visualization is achievable and can be genuinely beautiful, but only
if the product commits to *static, sampled, scrubable* views and explicitly abandons
animation. That is a scope decision the plan currently leaves dangerously implicit.

### B7. The testing story has a real hole where it matters most.

The good news is documented in the White hat: `TestApp`/`TestClient` for HTTP,
`test_html` for structural assertions, and `WorkflowReplayer::replay_from_events`
with **no database required**. That is a strong foundation, and better than typical.

The hole is in the middle layer:

- **`TestDb` requires Docker**, and Autumn's own tests using it are `#[ignore]`d.
  Our constraint says "live local Postgres", which is present — so tests must be
  wired to a `DATABASE_URL` against the running cluster, *not* to `TestDb`. Choosing
  `TestDb` by reflex costs an hour and then fails.
- **Test isolation is unsolved by the plan.** Multiple tests writing runs and frames
  to one live database will interfere. Someone must decide: transaction-rollback
  per test, unique-schema per test, or truncation between tests. Under `cargo test`'s
  default parallelism this is not optional — it is the difference between a green
  suite and a flaky one. Flaky suites under time pressure get "fixed" by disabling
  parallelism or deleting assertions.
- **End-to-end durability is the least testable and most important property.** The
  claim "durably executes and survives restart" is precisely what a unit test cannot
  demonstrate. Testing it requires starting a worker, killing it mid-run, restarting,
  and asserting completion — an orchestration harness that is itself a meaningful
  chunk of the session budget. **The headline feature is the hardest thing to prove,
  and the plan allocates no time to proving it.**
- The browser `SystemTest` harness requires a browser binary and driver in the
  sandbox. Unverified. Planning to rely on it is planning on an unknown.

### B8. The premise risk: is durability actually load-bearing?

Stated plainly, because it is the question a sharp reviewer asks first: a boids
simulation is a pure, fast, deterministic function. Ten thousand ticks of five hundred
agents is **seconds of CPU**. It needs no retries (nothing can fail), no external
calls, no compensation, and no human-in-the-loop approval.

So the honest criticism is: **the canonical justification for a durable workflow
engine — expensive, unrepeatable, failure-prone, externally-dependent work — does not
apply here.** A `#[job]` and a loop would produce the same user-visible result with a
fraction of the machinery. Harvest's own README positions itself for exactly the
opposite case: *"long-running orchestrations that survive process restarts, retries
with durable history, signal-driven waits."* Boids has one of those four.

If the answer is only "it's a nice demo of the engine," the project is a framework
showcase, and should be honest about that. If there is a real answer, it has to be
*designed in* — deliberately — not asserted. **The Green hat must answer this
directly, and if it cannot, the premise should be restated rather than defended.**

---

## 🟡 Yellow Hat — Optimism, Value, Benefits

*Why this is genuinely a good idea, and what the best realistic outcome looks like.*

### Y1. The pure core is a TDD dream, and that is not a small thing.

The boids algorithm is the rare domain that is simultaneously: mathematically
specified, dependency-free, fast, deterministic, and *visually verifiable*. Every one
of those properties is a gift to test-driven development.

You can write `separation_steers_away_from_a_close_neighbour` before any
infrastructure exists, and it will still be a meaningful test a year later. There is
no mocking, no fixture scaffolding, no async, no clock. Red-green cycles here are
sub-second once compiled. **The most valuable part of this product is also the
easiest part to build correctly** — that alignment is rare and should be exploited
ruthlessly.

### Y2. Determinism is the thread that ties boids to Harvest — and it is not contrived.

This is the answer to the Black hat's premise challenge (B8), and it is stronger than
it first appears.

Harvest's central demand on workflow authors is: **be deterministic, or replay will
catch you.** It ships an entire guardrail catalog (HVG001–011), a compile-time AST
lint, and a `WorkflowReplayer` whose whole job is proving that re-executing history
yields identical commands.

Boids' central *property* is: **deterministic given a seed.**

These are the same discipline viewed from two directions. A boids simulation is
arguably the ideal pedagogical workload for a replay-based engine, because
non-determinism in the domain is *visible* — a divergent flock looks different. Most
workflow examples (billing, onboarding) hide non-determinism behind side effects you
can't see. Here, a determinism bug produces a visibly different picture. **That is a
genuinely elegant pairing, not a contrived one**, and it is the intellectual core of
the product.

### Y3. Reproducibility is a real user value, not a technical detail.

The actual pain in simulation work is: *"I got an interesting result last Tuesday and
I can't reproduce it."* Boidboard's persistence layer means every run stores its full
parameter set and seed, immutably, alongside its metrics. Re-running a scenario gives
byte-identical output. Comparing two runs compares two *records*, not two memories.

That is the durable-workflow value proposition restated in the user's language:
**the experiment log is the product.** A researcher's notebook that can't lie.

### Y4. Harvest earns its place through the things that *are* true of long simulations.

Even granting B8, several Harvest features map onto real needs here rather than
decorative ones:

- **Crash-resume**: a long parameter sweep genuinely should not restart from tick zero
  because a deploy happened. This is real, and it is the demo.
- **Signals for live steering**: mid-flight parameter changes (raise cohesion at tick
  500 and watch the flock tighten) are *scientifically interesting* and map exactly
  onto `try_receive_signal` between batches. This is a feature users would want even
  if durability were free.
- **Cancellation**: killing a long run cleanly is a genuine need, and workflows give
  it a correct implementation rather than a `AtomicBool` and hope.
- **Queryable progress**: `#[query]` handlers expose live tick counts without polling
  the database.
- **Observability for free**: `/actuator/*`, `/health`, the Harvest management API,
  the DLQ, and the worker fleet view all exist without us writing them. On day one
  the app has better operational surface than most production services.

### Y5. The stack genuinely shines on exactly this shape of app.

- **Maud emits arbitrary markup, so SVG is a first-class citizen.** There is no
  templating impedance: `html! { svg { circle cx=(x) cy=(y) r="2" {} } }` is
  compile-time checked Rust. Rendering a flock is a pure function
  `&Frame -> Markup` — testable, no browser, no DOM.
- **`LiveFragment` + `broadcasts = true`** gives htmx `hx-swap-oob` updates *generated
  by the repository layer*. Live progress without writing a WebSocket or an SSE
  handler is a genuine framework win.
- **`test_html`'s CSS-selector assertions** let us assert `svg circle` count equals the
  agent count. That is a *direct, structural* test of the visualization — the thing
  usually deemed untestable.
- **`/_stories`** gives a widget gallery where a reviewer can see the SVG components
  in isolation.
- Rust makes the simulation fast enough that batching thousands of ticks per activity
  is trivial, which is exactly what B5 requires for the event budget. **The
  performance headroom directly buys correctness headroom.**

### Y6. The best realistic outcome.

A reviewer clones the repo and finds:

- A `boids-core` crate with zero framework dependencies, ~40 fast tests including
  property tests for determinism and physical invariants, readable enough to serve as
  a reference implementation of Reynolds' model.
- A thin Harvest workflow that is *obviously* correct because it does almost nothing:
  loop, call activity, check signal, repeat.
- A handful of server-rendered pages with no JavaScript beyond bundled htmx.
- A trajectory SVG that is genuinely beautiful and that they screenshot.
- A test suite that passes on a live Postgres, first try.

And the demo: submit a scenario, watch progress tick up, **kill the worker process**,
restart it, watch the run resume mid-flight and complete, then compare it against a
previous run and see the order parameter curves diverge. **That demo is memorable,
it is honest, and it makes the durability point in fifteen seconds** — which is worth
more than any amount of feature breadth.

---

## 🟢 Green Hat — Creativity, Alternatives, New Ideas

*Solutions to the specific problems the Black hat raised.*

### (a) Visualizing a flock compellingly with server-rendered HTML + htmx

Solving B6. The unlock is to **stop trying to animate and start using the strengths of
static vector graphics** — which are considerable and, for this domain, arguably
better than animation.

**G1 — Trajectory ribbons ("long exposure"). The headline visual.**
One `<polyline>` per agent over sampled frames, with opacity ramping from 0.1 to 1.0
along the path so the flock's history fades behind it like a light-painting
photograph. This shows *the whole run in one static image* — where the flock formed,
where it turned, where it split. B6 says naive resolution is 480 KB, so **subsample by
design**: ~30 representative agents × ~60 frames ≈ 22 KB. Choosing 30 agents
deterministically (every `n/30`th index) keeps it reproducible.
*This is the single most valuable visual and it is a pure function of stored data.*

**G2 — The contact sheet (small multiples).**
A CSS grid of 12 frames sampled across the run, each a miniature SVG, reading
left-to-right like a comic strip. Order emerging across the page is *more legible than
animation* because you can compare frame 1 and frame 12 simultaneously — an animation
forces you to remember. This is how flocking is presented in papers, and it is
essentially free once a single-frame renderer exists.

**G3 — The zero-JavaScript scrubber.**
```html
<input type="range" min="0" max="199"
       hx-get="/runs/42/frame" hx-target="#stage" hx-trigger="input changed delay:80ms"
       name="tick">
```
Drag the slider, the server renders that frame as SVG, htmx swaps it in. No JS
written by us. It *feels* like scrubbing a video and it is about six lines. Combined
with `hx-push-url`, a specific frame becomes a shareable link.

**G4 — The CSS flipbook (zero-JS animation, the party trick).**
Emit N frames as `<g>` layers inside one SVG, all hidden, each with the same
`steps(N)` keyframe animation and a staggered **negative** `animation-delay`:
```css
.frame { animation: flip 4s steps(1,end) infinite; opacity: 0; }
.frame:nth-child(1) { animation-delay: 0s; }    /* -0.2s, -0.4s, … */
```
The browser animates a real flipbook with zero JavaScript. Per B6's arithmetic, keep
it to ~24 frames at reduced agent count (~100) ≈ 96 KB and ~2,400 DOM nodes — viable.
**Ship it as an optional "animate" toggle on the run page**, never as the primary
view, so the page stays fast and the trick is a delight rather than a dependency.

**G5 — Boids that look like boids.**
Render each agent as a small rotated triangle
(`<polygon points="0,-3 -2,3 2,3" transform="translate(x,y) rotate(θ)">`) where θ comes
from the velocity heading. Heading is *the* thing that matters in flocking and a
circle throws it away. This is ~60 extra bytes per agent for an enormous gain in
legibility — you can see alignment happening.

**G6 — Metric sparklines and the "flock fingerprint".**
Each metric as a 120×24 inline `<polyline>` sparkline. Then compose them into a small
multi-metric strip that acts as a run's visual signature. Two runs' fingerprints
side by side communicate difference instantly, before any number is read.

**G7 — Compare as an overlay, not two panels.**
For comparison, superimpose run A's centroid trajectory in one hue and run B's in
another within a *single* SVG, plus overlaid metric sparklines. One image beats two
images the eye must saccade between. This is also *cheaper* to build than a two-panel
layout, which is the rare case where the better design is the smaller one.

**G8 — Density heatmap fallback.**
For very large agent counts (where B6's arithmetic breaks), bin positions into a
coarse grid server-side and emit one `<rect>` per occupied cell with opacity by count.
Payload becomes independent of agent count. This is the graceful-degradation path
that lets the agent-count ceiling be generous.

### (b) Making the durable workflow feel essential rather than bolted on

Answering B8 head-on. Three ideas, in increasing order of strength.

**G9 — Signals as live experiment steering. (The feature that justifies itself.)**
Between batches, the workflow calls the non-blocking `try_receive_signal`. A user
watching a run can, mid-flight, **change a weight** — raise cohesion at tick 500 — and
watch the flock tighten in the very next frames. The run record stores the
intervention as a timestamped event, so the trajectory ribbon can be annotated with
"cohesion 1.0 → 2.5 here."

This reframes the product from *batch simulator* to **interactive experiment**, and it
is a feature you would want even if durability were free. Crucially, it is exactly
what signals are *for*, so the machinery stops being decorative. It is also cheap:
`try_receive_signal` + a form post.

**G10 — Crash-resume as a first-class, *demonstrable* product feature.**
Don't bury durability in the architecture — **surface it in the UI**. The run detail
page shows a "Durability" panel: batches completed, last checkpoint tick, worker
restarts survived. Then make the demo an explicit, scripted step: kill the worker,
restart, watch the run continue from its last checkpoint with the tick counter
resuming mid-flight.

A user who sees a run survive `kill -9` understands the value proposition instantly.
This converts B8's criticism into the product's most memorable moment.

**G11 — Make runs long enough that durability is *obviously* necessary.**
The honest weakness in B8 is that boids is fast. So make the unit of work honest:
support runs long enough that restarting from zero would be genuinely painful, and let
the workflow's `should_continue_as_new` / `continue_as_new` handle history rotation
for the long tail. A run that takes minutes and survives a deploy is a real
justification; a run that takes 200 ms is not. **Choose the workload to fit the tool,
rather than pretending the tool fits the workload.** If runs stay short, be honest in
the README that durability is demonstrative — that honesty is itself a quality signal
to a reviewer.

### (c) Keeping scope achievable while still feeling complete

**G12 — Vertical slice, not horizontal layers.** One scenario type, end to end, working
beautifully, beats five half-built features. Completeness is a *feeling* produced by a
path that never dead-ends — not by feature count.

**G13 — Presets instead of a parameter form. (Solves the Red hat's "tax form" dread
*and* saves scope.)**
Ship three or four named presets — **"Classic Flock", "Nervous Swarm", "Highway",
"Scatter"** — as buttons that fill the form. The user clicks one and gets an
interesting result in one action. This simultaneously: removes the blank-form problem,
gives QA fixed known-good scenarios, gives the compare feature something meaningful to
compare *by default*, and gives tests deterministic fixtures. **One idea paying four
debts.** Advanced users can still edit fields; the form just isn't the front door.

**G14 — Frames are an append-only, idempotent table keyed by `(run_id, tick)`.**
This solves B7's isolation problem *and* the at-least-once retry problem in one
stroke: `INSERT … ON CONFLICT (run_id, tick) DO NOTHING`. Retrying a batch is a no-op
by construction — no idempotency keys, no dedup logic, no compensation. **The schema
enforces correctness so the code doesn't have to.**

**G15 — The activity owns the state, the workflow owns only the cursor.**
The workflow carries `(run_id, next_tick)` — a few dozen bytes. The activity loads
the last frame from Postgres, simulates a batch, upserts new frames, and returns a
*compact metrics summary*. This makes B5's event budget and payload budget both
disappear: history stays tiny, payloads stay far under 256 KiB regardless of agent
count, and the workflow becomes trivially deterministic because it contains no
simulation state at all.

**G16 — Cut obstacles and goals to a single optional circular obstacle, or drop both.**
They are the two most expensive items per unit of demo value (B1). A flock is
compelling *without* obstacles. If time remains, one circular obstacle is a contained
addition; if it doesn't, nothing is missing.

**G17 — Time-box integration first, not last.**
Spend the first block of the session proving `autumn-web` + `autumn-harvest` compile
and boot together with a trivial workflow — a "walking skeleton" — *before* any boids
code. B3 is the highest-severity risk and it is also the fastest to falsify. If it
fails, we learn at hour one with time to adapt, rather than at hour four with none.

### (d) Making the app self-evidently correct to a reviewer

**G18 — Behavioural tests that prove the boids are really boids.**
Not "does `separation()` return a vector" but tests that assert *emergent* properties,
each of which fails loudly if the model is wrong:
- Alignment-only, identical initial headings → heading is invariant.
- Separation-only, two agents at distance ε → distance strictly increases.
- Cohesion-only → mean distance-to-centroid strictly decreases.
- Flocking parameters → polarization rises above 0.9 within N ticks.
- Random/zero weights → polarization stays near `1/√N`.

That last pair is the killer: **it tests that flocking actually emerges.** A reviewer
reading these test names understands the model is correct without reading the
implementation.

**G19 — Property tests for the invariants.**
With `proptest`: for arbitrary valid parameters and seeds — speed never exceeds
`max_speed`; polarization always lies in `[0,1]`; positions stay finite (no NaN);
and **same seed ⇒ byte-identical frame sequence** (directly addressing B4's
floating-point risk by turning it into an enforced, executable promise rather than an
assumption).

**G20 — Golden SVG snapshot committed to the repo.**
Render a fixed seed/scenario to SVG and commit it. The test asserts the render matches.
A reviewer can *open the file in a browser and look at the flock.* Correctness you can
see is worth ten assertions.

**G21 — A committed replay fixture.**
Export one real workflow history to JSON, commit it, and run
`WorkflowReplayer::replay_from_events` against it in CI. Per the White hat this needs
**no database**, so it runs anywhere and guards against determinism regressions
forever. This is the cheapest high-value test in the whole plan.

**G22 — Determinism visible in the UI.**
Store and display a hash of the final frame on the run detail page. Two runs with the
same seed and parameters show the same hash. A reviewer can verify reproducibility
*from the browser*, without running a test.

---

## 🔵 Blue Hat — Process, Synthesis, Control

*Managing the thinking. Synthesising five hats into a decision.*

### What the hats collectively established

The White hat found a stronger factual position than expected: Postgres is live, the
toolchain is current, and the test harnesses (`TestApp`, `test_html`,
`WorkflowReplayer` without a DB) are better than typical. It also found three
genuinely blocking unknowns, all about *where state lives*.

The Red and Black hats **agree**, independently, that scope is the dominant risk — and
agreement between an intuitive and an analytical pass is the strongest signal in this
method. The Black hat additionally surfaced three risks the concept never mentions:
cold build time (B2), the history-event budget (B5), and the trajectory payload
ceiling (B6).

The Yellow and Green hats converged on the same resolution to the premise challenge
(B8): durability becomes essential only if we **design it in** via signals (G9) and
**demonstrate it** via crash-resume (G10). Absent that, it is decoration.

Decision: **cut hard, build the pure core first, and make durability visible.**

### The decision — recommended scope

#### MUST (this is the product)

1. **`boids-core`**: a pure crate, no framework dependencies. `Vec2`, `Boid`,
   `Params`, `World`, seeded PRNG, O(n²) neighbourhood, separation + alignment +
   cohesion, force/speed clamping, toroidal wrap, `step()`.
2. **Metrics (pure)**: polarization, mean nearest-neighbour distance, and
   centroid — computed as pure functions over a frame.
3. **Persistence**: `scenario` and `run` tables, plus a `frame` table keyed
   `(run_id, tick)` with `ON CONFLICT DO NOTHING` (G14).
4. **One Harvest workflow + one activity**: workflow carries only
   `(run_id, next_tick)`; the activity simulates a batch, upserts frames, returns a
   compact metrics summary (G15).
5. **A cancel signal**, checked between batches via `try_receive_signal`.
6. **Pages**: run list; new-run form **fronted by presets** (G13); run detail with
   live progress via htmx polling.
7. **Visualization**: single-frame SVG with rotated-triangle agents (G5);
   **trajectory ribbons, subsampled** (G1); metric sparklines (G6).
8. **Tests**: behavioural boids tests (G18), determinism property test (G19),
   `test_html` structural assertions on the SVG, repository round-trip against live
   Postgres, and one committed replay fixture (G21).

#### SHOULD (only if the MUST list is green and time remains, in this order)

9. Compare two runs as a **single overlaid SVG** (G7).
10. A **steer signal** that changes a weight mid-run (G9) — the strongest single
    justification for the whole architecture.
11. The zero-JS scrubber (G3).

#### WON'T (explicit, non-negotiable cuts)

- **Obstacle avoidance** and **goal waypoints / time-to-goal** — the two most
  expensive features per unit of demo value (B1, G16).
- **Collision counting** and **stuck detection** — extra metrics with definitional
  ambiguity (F4) and no visual payoff.
- **Parameter sweeps / child workflows / DAG schedules** — the classic scope killer.
- **Smooth animation** and the CSS flipbook (G4) — deliberately deferred despite
  being a delightful trick; B6 shows real-time is unreachable and the flipbook is a
  bonus, not a foundation.
- **SSE and `LiveFragment` broadcasts** — polling is simpler and directly testable
  with `TestClient`.
- **Any WASM/Yew island**, notwithstanding Autumn's own `examples/flock`.
- **`TestDb` / testcontainers** — use the live local Postgres via `DATABASE_URL`.
- **The browser `SystemTest` harness** — unverified dependency, not worth the risk.
- Auth, multi-user, spatial-grid optimisation, retention/janitor configuration.

### Build order, optimised for TDD (pure before IO)

**Step 0 — Walking skeleton (do this FIRST, before any domain code).**
A Cargo workspace where `autumn-web` + `autumn-harvest-plugin` compile and boot
together, one trivial workflow runs end to end, `/health` responds. This falsifies the
highest-severity risk (B3) at the cheapest moment and absorbs the cold-build tax (B2)
before it can ambush the session (G17). **Do not proceed until this is green.**

**Step 1 — `boids-core`, pure, no async, no DB.** The densest value per minute in the
session, with sub-second red-green cycles. Order: `Vec2` → neighbourhood query →
each rule in isolation → weighted sum and clamping → `step()`. Drive with the
behavioural tests (G18) so emergence itself is under test.

**Step 2 — Metrics, pure.** Polarization and mean NN distance as free functions over a
frame slice. Add the determinism property test (G19) here, closing B4's
floating-point risk while the code is still small enough to fix cheaply.

**Step 3 — SVG rendering, pure.** `&Frame -> Markup`. Still zero IO. Test with
`test_html` selectors (agent count, `viewBox`, polyline count) and commit the golden
SVG (G20).

*Everything above this line is pure, fast, and needs no infrastructure. It should be
the majority of the test suite.*

**Step 4 — Persistence.** Models, migrations, repository. Round-trip tests against
live Postgres. Establish the test-isolation strategy here, once, deliberately (B7).

**Step 5 — The activity.** Thin: deserialize → call `boids-core` → upsert frames →
return metrics summary. Testable as a plain function.

**Step 6 — The workflow.** Loop, call activity, check cancel signal. Verify with
`WorkflowReplayer::replay_from_events` (no DB), then one live end-to-end run.

**Step 7 — Routes and pages.** `TestApp`/`TestClient` with structural assertions.

**Step 8 — Live progress polling, then the SHOULD list.**

### Definition of done

- `cargo build` and `cargo clippy` clean; `cargo test` green against the live local
  Postgres, **run twice consecutively** to prove isolation and idempotency.
- `boids-core` has no dependency on `autumn-web`, `autumn-harvest`, or Diesel —
  verifiable by reading one `Cargo.toml`.
- The behavioural suite proves flocking emerges (polarization > 0.9 for flocking
  params, near-baseline for disordered params).
- Same seed + same parameters ⇒ byte-identical frames, enforced by a property test
  and surfaced as a hash in the UI (G22).
- A committed replay fixture passes with no database.
- End to end in a browser: pick a preset → submit → progress advances → run completes
  → trajectory SVG and sparklines render → the frame hash is displayed.
- **The crash-resume demo works**: kill the worker mid-run, restart, the run resumes
  from its last checkpoint and completes (G10). *If durability isn't demonstrated,
  the premise isn't delivered.*
- `README.md` documents setup, the `DATABASE_URL` requirement, how to run the tests,
  the demo script, **and an honest statement of the durability rationale** per G11.
- One PR, reviewed from multiple angles.

### The top 3 decisions the coordinator must make before any code is written

**Decision 1 — State ownership and the idempotency contract.**
*Recommendation: adopt G15 + G14.* The workflow carries only `(run_id, next_tick)`;
Postgres owns frames via a `(run_id, tick)` primary key with
`ON CONFLICT DO NOTHING`. This is the highest-leverage decision available: it
simultaneously eliminates the history-event budget risk (B5), the 256 KiB payload
risk (B5), the at-least-once duplication risk (B7), and most workflow determinism
risk (B4) — because a workflow holding no simulation state cannot be
non-deterministic about it. **One decision retires four risks.**

**Decision 2 — Frame sampling rate, agent ceiling, and run length.**
*Recommendation: store every 10th tick, cap ~200 frames per run, cap agents at 500,
default runs to ~2,000 ticks.* This must be fixed *before* the schema, because the
SVG budgets (B6), the compare page, and the durability narrative (G11) all derive
from it. Left implicit, it will be discovered during rendering, when it is expensive
to change. Pair it with the deterministic 30-agent subsample for trajectory ribbons.

**Decision 3 — Live-update transport.**
*Recommendation: htmx polling (`hx-trigger="every 2s"`) on a progress fragment.*
Not SSE, not `LiveFragment` broadcasts. Polling is directly assertable with
`TestClient` in a normal `#[tokio::test]`; SSE requires stream lifecycle management
and the browser harness we have explicitly cut. Accept the known load cost (B6) and
document it as a demo-scale limitation. **Choose the transport that is testable over
the one that is elegant.**

### Closing control note

Two structural instructions for the build session, arising from the process rather
than the content:

1. **Step 0 is a gate, not a task.** If the walking skeleton is not green, stop and
   re-plan rather than building boids code against an unproven integration. The
   Black hat's B3 is the only risk that can produce a zero-value session.
2. **Cut from the SHOULD list without ceremony.** The MUST list was constructed to be
   a complete, coherent product on its own. Both the Red and Black hats warned that
   this plan's characteristic failure is many things at 70%. Shipping the MUST list
   fully is a success; shipping the MUST list plus a broken compare page is not.
