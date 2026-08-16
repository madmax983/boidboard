# Boidboard — Specification and Acceptance Criteria

Boidboard is a **durable boids experiment bench**: you configure a flocking scenario, submit it, and a durable [Autumn Harvest](https://crates.io/crates/autumn-harvest) workflow executes the simulation in checkpointed tick batches while the [Autumn Web](https://crates.io/crates/autumn-web) UI shows it progressing, renders the flock, and lets you compare runs.

Planning artifacts for this issue live in `docs/planning/` (brainstorming, reverse brainstorming, six-hats).

## Why durable workflows genuinely belong here

A boids run is fast, so durability is not about crash recovery alone. The activity boundary **is** the checkpoint boundary, and checkpoints are what make the product work: they enable resuming a killed run, scrubbing back through history, steering parameters mid-flight via signals, and cancelling a runaway run. The design rule that follows is that the **workflow carries only a cursor** (`run_id`, `next_tick`), never bulk agent state — history stays O(1) in agent count — and **frames are owned by Postgres** under a `(run_id, tick)` uniqueness constraint so at-least-once activity retries are idempotent by construction.

## Architecture

| Crate | Role | Rule |
|---|---|---|
| `boids-core` | Pure simulation kernel | Zero dependencies on the web framework, workflow engine, or database. Enforced by a test. |
| `boidboard` | Autumn Web app + Harvest workflow | Owns persistence, routes, views, orchestration. |

---

# Acceptance Criteria

## A. Simulation kernel — geometry and determinism

- [ ] **AC-1** `Vec2` supports add/sub/scale/dot/length, and `normalize()` is **total**: `Vec2::ZERO.normalize()` returns zero rather than `NaN`.
- [ ] **AC-2** Force and speed clamping (`limit()`) never produces a vector longer than the limit, and leaves shorter vectors untouched.
- [ ] **AC-3** The world is toroidal and distance uses the **minimum-image convention**: in a world of width 100, agents at `x=1` and `x=99` are 2 apart, not 98.
- [ ] **AC-4** Position wrapping is correct for negative and multi-world-width offsets (`rem_euclid` semantics), so an agent driven far off-world still lands in bounds.
- [ ] **AC-5** The RNG is a seeded, self-contained PRNG. The same seed produces the same sequence, and two different seeds produce different sequences. No use of `rand::thread_rng` anywhere in the kernel.
- [ ] **AC-6** A canonical `state_hash()` over the world state is order-independent in representation but sensitive to every field: changing any single agent's position or velocity changes the hash.

## B. Simulation kernel — neighbourhood queries

- [ ] **AC-7** A naive O(N²) neighbour query returns exactly the agents within the neighbour radius under toroidal distance, and **never includes the agent itself**.
- [ ] **AC-8** A spatial-hash neighbour query exists as a second backend.
- [ ] **AC-9** **Equivalence property test**: over many randomised configurations (varying agent counts, radii, world sizes, including agents near and across the wrap seam), the spatial hash returns a neighbour set **exactly equal** to the naive backend. Exact set equality, not a tolerance.
- [ ] **AC-10** A multi-tick run produces an identical `state_hash()` under both neighbour backends, so the optimisation is provably behaviour-preserving.

## C. Simulation kernel — steering forces

- [ ] **AC-11** **Separation** steers away from close neighbours, and two **coincident** agents (distance 0) produce a finite, deterministic force — no divide-by-zero, no `NaN`, and no randomness in the tie-break.
- [ ] **AC-12** **Alignment** steers toward the mean neighbour heading; exactly antiparallel neighbours average to a zero vector and must not produce `NaN`.
- [ ] **AC-13** **Cohesion** steers toward the neighbour centroid computed **via toroidal displacement vectors**, so agents at `x=1` and `x=99` cohere across the seam rather than toward the middle of the world.
- [ ] **AC-14** **Goal seeking** steers toward the configured goal, and with `goal_weight = 0` the goal position is provably irrelevant: two runs with different goals produce identical state hashes.
- [ ] **AC-15** **Obstacle avoidance** repels from obstacles, pushes an agent that starts inside an obstacle radially outward, and breaks a perfectly head-on approach deterministically.
- [ ] **AC-16** The five forces are combined as a weighted sum `w1*sep + w2*align + w3*coh + w4*goal + w5*avoid`, and each weight independently and monotonically affects the result.
- [ ] **AC-17** With zero neighbours, every flocking force is zero and the agent does not move erratically.

## D. Simulation kernel — integration

- [ ] **AC-18** The step function is **double-buffered**: shuffling the agent array, stepping, and un-permuting yields **bit-identical** state. (This is the canonical in-place-update bug.)
- [ ] **AC-19** Integration is `dt`-scaled, and halving `dt` **converges** rather than changing the answer.
- [ ] **AC-20** **Batch equivalence**: 1 batch of 1000 ticks produces the same state hash and metric series as 10 batches of 100 ticks with serialize/deserialize between them. This is the formal contract that permits checkpointing at all.
- [ ] **AC-21** **Stability invariants** hold on every tick of a long adversarial run: all state finite (no `NaN`/`inf`), `speed <= max_speed`, and every applied force `<= max_force`.
- [ ] **AC-22** Simulation is reproducible **across OS processes**: the same scenario and seed run in a separate process yields an identical state hash.

## E. Metrics

- [ ] **AC-23** **Polarization** (order parameter) is 1.0 for a perfectly aligned flock and exactly 0.0 for 4 agents heading at 0°/90°/180°/270°.
- [ ] **AC-24** **Mean nearest-neighbour distance** is computed under toroidal distance.
- [ ] **AC-25** **Collision count** counts unordered pairs: 2 mutually overlapping agents is 1 collision, not 2.
- [ ] **AC-26** **Time-to-goal** is an `Option` that is `None` when the goal is never reached, never a sentinel like `-1`.
- [ ] **AC-27** **Stuck detection** fires on a flock trapped in a concave obstacle arrangement and does **not** fire on a healthy flocking run. (Both a positive and a negative assertion — a detector that always says "yes" must fail.)
- [ ] **AC-28** Metrics are translation-invariant: translating the whole world by any offset leaves every metric unchanged.

## F. Durable workflow

- [ ] **AC-29** A `simulation_workflow` drives a run to completion by repeatedly invoking a `simulate_batch` activity, carrying only `(run_id, next_tick)` — **never** the agent array — so workflow history stays O(1) in agent count.
- [ ] **AC-30** The workflow is tested with `WorkflowTestEnv` **without a live Postgres**, with mocked activities and the virtual clock.
- [ ] **AC-31** The workflow passes a **replay determinism check** (`replay_check` / `WorkflowReplayer`) proving it does not diverge on replay.
- [ ] **AC-32** `simulate_batch` is **idempotent**: invoking it twice for the same `(run_id, batch)` leaves exactly one set of frames, enforced by a `UNIQUE (run_id, tick)` constraint, and leaves no tick gaps.
- [ ] **AC-33** A **cancel signal** is honoured at the next batch boundary; the run reaches a terminal `Cancelled` state with partial results intact.
- [ ] **AC-34** A **steer signal** applied mid-run changes the simulation parameters for subsequent batches, and the signal is recorded so the run's provenance explains its own behaviour.
- [ ] **AC-35** A **budget guardrail** (`max_ticks`) terminates a run deterministically rather than letting it run unbounded.
- [ ] **AC-36** **Crash resume**: a run interrupted mid-flight resumes from its checkpoint and reaches the same final state hash as an uninterrupted run.

## G. Persistence

- [ ] **AC-37** `scenario`, `run`, and `frame` tables exist via embedded migrations, with `UNIQUE (run_id, tick)` on frames.
- [ ] **AC-38** A completed run stores a full **provenance record** (kernel version, scenario config hash, seed, final state hash), and re-running from provenance reproduces the state hash exactly.
- [ ] **AC-39** Editing a scenario cannot mutate the stored configuration of an already-completed run.
- [ ] **AC-40** Repository round-trip tests pass against a live Postgres, and the whole suite passes **twice consecutively** (proving test isolation, not accidental first-run success).

## H. Web interface

- [ ] **AC-41** A **run list** page shows runs with status and headline metrics.
- [ ] **AC-42** A **new run form** is fronted by named **presets** (e.g. Classic Flock, Nervous Swarm, Scatter) so the user never faces a blank parameter form.
- [ ] **AC-43** A **run detail** page renders the flock as inline **SVG** — oriented agent marks plus trajectory ribbons — with no SPA framework and no WASM.
- [ ] **AC-44** Run detail shows **metric sparklines** over the run's history.
- [ ] **AC-45** In-progress runs update via **htmx polling**, and the polling fragment endpoint is directly asserted in tests.
- [ ] **AC-46** A **compare view** renders two runs together so the effect of a parameter change is visible.
- [ ] **AC-47** The reproducibility hash is **surfaced in the UI**, so a user can see that a run is reproducible.
- [ ] **AC-48** Route tests use `autumn_web::test::TestApp` and assert on **HTML structure** (via `test_html` selectors), not on raw string matching.

## I. Engineering quality

- [ ] **AC-49** `boids-core` has **zero** dependencies on Diesel, HTTP, Autumn Web, or Harvest, proven by an automated check — and swapping neighbour backends requires zero test changes.
- [ ] **AC-50** No domain logic lives in route handlers; handlers delegate to the kernel and repositories.
- [ ] **AC-51** `cargo clippy --workspace --all-targets -- -D warnings` is clean and `unsafe_code` is forbidden.
- [ ] **AC-52** Every feature was built **red → green → refactor**: a failing test was observed before the implementation, and the red phase is evidenced in the PR.

---

## Out of scope for this issue

3D; GPU/WASM compute; real-time 30fps animation (polling is orders of magnitude short — the product is stills, trajectories and scrubbing); parameter sweeps and child-workflow fan-out; user-uploaded steering code; authentication and multi-tenancy; RVO/social-force models.
