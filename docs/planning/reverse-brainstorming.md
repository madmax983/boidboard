# Boidboard — Reverse Brainstorming (Inversion / Pre-Mortem)

> **Method.** We do not ask "how do we make Boidboard succeed?" We ask **"if we were trying to guarantee Boidboard fails, what exactly would we do?"** — and we insist on naming the *mechanism*, not the vibe. "It might be slow" is worthless. "The neighbour query is O(N²) and the batch activity has a 60-second start-to-close timeout, so at N≈1200 the activity times out mid-batch, the engine retries the batch while the first attempt is still running, and two workers append the same ticks" is useful.
>
> Every failure mode below carries a stable ID (`P#`, `S#`, `W#`, `T#`, `C#`). §6 inverts **every single one** into a testable acceptance criterion. §6 is the deliverable; §1–§5 are the justification for it.

**Status:** planning artefact. No code exists yet. This document is written to be consumed directly as the source of acceptance criteria for the TDD build.

---

## Contents

- [0. The one-paragraph pre-mortem](#0-the-one-paragraph-pre-mortem)
- [1. How to guarantee the PRODUCT fails (P1–P16)](#1-how-to-guarantee-the-product-fails-p1p16)
- [2. How to guarantee the SIMULATION is wrong in ways tests won't catch (S1–S18)](#2-how-to-guarantee-the-simulation-is-wrong-in-ways-tests-wont-catch-s1s18)
- [3. How to guarantee the DURABLE WORKFLOW layer fails (W1–W16)](#3-how-to-guarantee-the-durable-workflow-layer-fails-w1w16)
- [4. How to guarantee the TDD process is a sham (T1–T13)](#4-how-to-guarantee-the-tdd-process-is-a-sham-t1t13)
- [5. How to guarantee the CODEBASE is unpleasant (C1–C10)](#5-how-to-guarantee-the-codebase-is-unpleasant-c1c10)
- [6. INVERSION — every failure mode as an acceptance criterion](#6-inversion--every-failure-mode-as-an-acceptance-criterion)
- [7. Top 10 ranked risks](#7-top-10-ranked-risks)
- [8. The four artefacts that carry most of the safety](#8-the-four-artefacts-that-carry-most-of-the-safety)

---

## 0. The one-paragraph pre-mortem

It is six months from now and Boidboard is dead. Here is the autopsy. It shipped as a form with fourteen unlabelled float boxes that submits a job, a table of runs, and a line chart. There is no picture of the flock, so nobody — not users, not the developers — ever *saw* whether the boids were flocking; the one time somebody rendered the trajectories by hand, half the agents were vibrating in place inside a concave obstacle and had been for eight hours while the dashboard said `Running · healthy`. Re-running the same seed gave different polarization in the third decimal because the spatial hash iterated a `HashMap`, so the "compare two runs" feature was never trusted and quietly stopped being used. The CI badge was green throughout: every test that touched Postgres was `#[ignore]`d because the runner had no database, and the spatial hash was never once compared against brute force. The final incident was a run at N=4000 that grew its workflow history to 6 GB by round-tripping the whole agent array through the engine every batch, took forty minutes to replay after a deploy, and could not be cancelled because nobody had wired a signal.

Everything in that paragraph is cheap to prevent and expensive to discover. The rest of this document is the prevention.

---

## 1. How to guarantee the PRODUCT fails (P1–P16)

### P1 — Make "is this run any good?" unanswerable
Record polarization = 0.62, mean NND = 4.1, collisions = 318. Provide no baseline, no reference scenario, no expected range, no comparison to a random-walk control. The user has no idea whether 0.62 is a tight flock or noise. (For reference: a random heading field gives Φ ≈ 1/√N ≈ 0.045 at N=500 — but if we never state that, 0.62 is a number, not a finding.) Every run terminates in a shrug.

### P2 — Ship without watching the flock
Boids are a *visual* phenomenon. Defer the trajectory/animation view to "phase 2" and ship charts only. Consequence beyond disappointment: **the team loses its own best debugging instrument**. Every simulation bug in §2 is instantly obvious on screen and near-invisible in aggregate metrics. Shipping metrics-first is how S1, S5, S6, S14 and S17 all survive to production.

### P3 — Promise reproducibility, store only the seed
Persist `seed = 42` and the five weights. Do not persist: the engine/kernel version, `dt`, the tick count per batch, the neighbour-search algorithm actually used, float width, the boundary mode, the obstacle list's ordering. Six weeks later "re-run" produces different numbers. The user cannot tell whether they found a real effect or a code change. **Trust is lost permanently on the first instance** — reproducibility is a claim you get to break exactly once.

### P4 — Present the parameters as fourteen bare floats
`separation_weight: [1.0]`, `alignment_weight: [1.0]`, `neighbor_radius: [1.0]` — no units, no valid ranges, no defaults, no coupling guidance (separation radius must be well below neighbour radius or the flock is a gas; `max_force · dt` must be small relative to `max_speed` or integration is unstable). Users type 1.0 into everything, get a blob or an explosion, and conclude the tool is broken rather than that their configuration was.

### P5 — Spinner forever
Submit shows `Running…` with no tick counter, no percentage, no last-progress timestamp, no worker heartbeat, no error surface. A dead worker (W9), a stuck flock (S16), a poison-retry loop (W4) and a genuinely-slow-but-healthy run are **pixel-identical**. Users learn to distrust the entire status column.

### P6 — No cancel, no delete, no budget
User fat-fingers `10_000_000` ticks at N=5000. There is no stop button. The run occupies a worker slot for days; every subsequent run queues behind it; the board appears frozen. The only remedy is a DBA deleting rows — which then makes the workflow replay fail.

### P7 — Make comparison a manual eyeball
"Compare" = open two runs in two browser tabs. No parameter diff highlighting exactly what changed, no overlaid metric series on shared axes, no normalisation when the runs have different tick counts or different `dt`, no per-metric delta with a sense of whether the delta exceeds run-to-run variation. Users cannot answer the *only* question they came with: "what did changing cohesion from 1.0 to 1.4 actually do?"

### P8 — Black box: aggregate-only, no scrubbing, no events
Store final/periodic metrics only. When polarization collapses at tick 4200, there is no way to look at tick 4200: no state snapshot, no scrub bar, no event log (`goal_reached`, `collision`, `stuck_detected`, `flock_split`). Diagnosis requires re-running locally with `println!`, which nobody will do.

### P9 — Store everything, or silently store almost nothing
Two symmetric deaths. **(a)** Persist every agent's position every tick: 5000 agents × 100k ticks × 16 bytes ≈ 8 GB *per run*, and the trajectory page tries to render it. **(b)** Panic about (a) and silently subsample to 500 frames without saying so, so the rendered trajectory smooths over exactly the jitter (S17) and tunnelling (S14) you needed to see, and the user believes they are looking at the simulation.

### P10 — Get live progress catastrophically wrong in either direction
Poll the whole page every 5 s: canvas resets, scroll position jumps, in-progress form input is destroyed. Or poll a heavy aggregate query every 250 ms per open tab: with ten viewers the board issues 40 full metric scans/second and Postgres degrades exactly when a run is interesting enough to watch.

### P11 — Let scenarios be mutable and shared by reference
Runs point at a `scenarios` row. User tweaks the scenario and re-runs. Every *completed historical run* now renders the new parameters next to its old results. The entire archive is retroactively falsified and nobody notices, because nothing looks broken.

### P12 — Leave every metric undefined at the UI
Show "collisions: 318" without stating the collision radius. "Time to goal: 812" without stating which agents (first? median? 90%?) or what counts as arrival. "Polarization" without stating it is `|mean(unit heading)|` over live agents only. Two people read the same dashboard and reach different conclusions; neither is wrong, because the number means nothing.

### P13 — No empty states, no failure states
Zero runs → blank white page (new users bounce). A failed run → a row saying `Failed` with no reason, no failing activity name, no dead-letter reference, no partial results, no retry. Errors are the highest-information events in the system and we throw them away.

### P14 — Make results unshareable
No stable per-run URL, no CSV/JSON export of metrics, no way to lift a chart into a doc. Findings never leave the app, so they never influence a decision, so the app is a toy.

### P15 — Make the known domain pitfalls invisible
The product owner named four: concave local minima, O(N²) neighbours, no formal formation, force-cancellation jitter. Instrument none of them. The flock parks in a concave obstacle and the board reports healthy progress for nine hours. Agents vibrate on the spot and no metric distinguishes that from cruising, because position variance is low in both.

### P16 — Hide the scaling cliff
Demo at N=200. User tries N=2000 with naive neighbours: 100× the pair work, run time goes from 20 s to 30 min with no warning, no pre-flight cost estimate, no admission control. The user's first real attempt is their worst experience.

---

## 2. How to guarantee the SIMULATION is wrong in ways tests won't catch (S1–S18)

These are ranked roughly by *stealth* — how well they survive a plausible test suite. A test asserting "after 500 ticks polarization > 0.8" passes for **every single one** of S1, S2, S5, S6, S7, S9, S11, S12.

### S1 — Update agents in place (order-of-update bias)
```
for i in 0..n { agents[i].vel += steer(&agents, i); agents[i].pos += agents[i].vel * dt; }
```
Agent 0 steers using all-old positions; agent N−1 steers using positions already advanced this tick. This is Gauss–Seidel where the model is Jacobi. It is *not* a crash — it produces a plausible-looking flock with subtly different dynamics, an artificial dependence on agent index order, and results that change if you ever sort or shuffle the agent array (e.g. for spatial locality). **Nothing in a "does it flock" test detects this.** The double-buffer requirement is the single cheapest correctness win in the project.

### S2 — Timestep dependence (`dt` missing or partial)
`vel += force` instead of `vel += force * dt`; or `dt` on position but not velocity; or `max_speed` clamped in units/tick rather than units/second. Symptom: halving `dt` should *refine* the trajectory toward a limit, but instead changes the answer — the flock is tighter or looser purely as a function of tick size. Every published parameter is then meaningless outside the exact `dt` it was tuned at, and comparison across runs with different `dt` (P7) is nonsense.

### S3 — Normalise a zero-length vector → NaN
`v / v.length()` where `length == 0`: a boid at rest normalising its heading; `desired − velocity` when they are exactly equal; a cohesion vector when the centroid coincides with the agent. Produces `NaN`, which then propagates through every subsequent tick and every metric. Worse, `NaN` comparisons are always false, so `assert!(pos.x < WORLD_W)` **fails** but `assert!(pos.x >= 0.0)` also fails while `assert!(!(pos.x > 1e9))` *passes* — NaN quietly satisfies negated bounds checks.

### S4 — Coincident agents → divide by zero in separation
Separation weighted by `1/d` or `1/d²` with `d == 0`. Two agents can be exactly coincident from: a seeded spawn on an integer grid, a small world with many agents (birthday paradox), a symmetric initial condition, or convergence into a point attractor. Result is `inf` acceleration, then `inf − inf = NaN` on the next sum.

### S5 — Toroidal wrap with naive vector subtraction
`delta = b.pos - a.pos` on a wrapped world. An agent at x=1 and one at x=W−1 are *2 units apart* on a torus but the code says W−2. Consequences: neighbours across the seam are silently invisible; the flock develops an invisible wall at the boundary; separation pushes the wrong way at the seam. **And the metrics are worse than the sim**: the arithmetic mean position of a wrapped flock is a meaningless point in the middle of the world — flock centroid and mean NND require the minimum-image convention and a circular mean. This bug renders correctly-looking on screen (the flock still flocks) and poisons every distance-based metric.

### S6 — Spatial-hash cell size vs neighbour radius mismatch
Cell size < neighbour radius while only scanning the 3×3 neighbourhood → neighbours in the 4th ring are silently dropped. Or cell size ≥ radius but the scan uses `<` on cell indices and misses the boundary column. Or the hash is built from positions *before* the update but queried *after* (a stale index). Every variant produces a flock that still flocks with a slightly wrong neighbour set. **Undetectable without a brute-force oracle** (T6). This is the highest-value differential test in the codebase.

### S7 — Non-deterministic iteration order
Iterating a `HashMap`/`HashSet` of spatial buckets and accumulating forces in bucket order: floating-point addition is not associative, so the summed force differs in the last bits per process (Rust's `RandomState` is seeded per-process). Same seed, different trajectory after ~10⁴ ticks of chaotic amplification. Same class of bug: `rayon` reductions over non-associative float adds; parallel iteration without a deterministic combine order.

### S8 — RNG that isn't actually seeded end-to-end
Variants, all plausible: a global/thread RNG (`thread_rng()`) used for tie-breaking or initial jitter; the RNG re-seeded from the run seed **at the start of every batch**, so batch 2 replays batch 1's noise; the seed stored as `i32` in Postgres and the kernel wanting `u64`; the RNG created inside the workflow body rather than passed into a deterministic activity; agents spawned by a parallel iterator so the draw order varies. Symptom is always the same and always devastating: reproducibility is claimed and false.

### S9 — f32 accumulation drift and cross-machine divergence
`pos += vel * dt` in `f32` over 10⁶ ticks with world coordinates ~10³: the increment is near the ULP and catastrophic cancellation eats it. Separately: LLVM may or may not contract `a*b + c` into an FMA depending on target features, so the *same binary source* gives different results on different CPUs — which turns "reproducible" into "reproducible on my laptop". Silent, gradual, and only visible in long runs that unit tests never do.

### S10 — Unclamped or wrongly-clamped forces
No `max_force` clamp; or clamping the *sum* of forces but letting an individual `1/d²` separation term reach 10⁸ first; or clamping force but not speed, so velocity ratchets up; or clamping speed but not force, so a single tick displaces an agent across the whole world (which then wraps, appearing "fine"). Divergence to `inf` typically takes 10⁴–10⁵ ticks — comfortably beyond any 100-tick unit test, comfortably inside every real run.

### S11 — Metrics computed on a stale snapshot
Metrics computed at the top of the batch (state as of the *previous* batch's end), or between the velocity update and the position update, so polarization and positions describe different instants. Time-to-goal is then quantised to the batch size and off by up to one batch. The tell: per-tick metrics and batch-boundary metrics disagree, and changing the batch size changes the metrics — a pure implementation detail leaking into results.

### S12 — Radius semantics inconsistency
One `neighbor_radius` in the config, but separation internally uses a different (often hard-coded) radius; or the radius is compared against a *squared* distance without squaring the radius (silently making the effective radius `√r`); or `<=` vs `<` at the boundary. All produce a working flock with parameters that don't mean what the label says, so every tuning session is fitting noise.

### S13 — Agent counts itself as a neighbour
Self at distance 0 → separation `inf` (S4 again) or, if guarded, a cohesion centroid biased toward self by `1/k` and an alignment vector biased toward the agent's own current heading — which is a hidden inertia term that *stabilises* the flock and makes alignment weight appear more effective than it is. Also silently changes averages (`/N` vs `/(N−1)`).

### S14 — Obstacle handling that lies
Two failure shapes. **(a) Positional correction**: on penetration, teleport the agent to the surface. This injects energy, breaks momentum conservation, and makes agents "pop". **(b) Tunnelling**: per-tick displacement `max_speed · dt` exceeds obstacle thickness, so a point-particle test finds no penetration on either side and the agent passes straight through — and `collision_count` reads **0**, which the dashboard proudly reports as a perfect run. Needs a swept (segment-vs-obstacle) test, not a point test.

### S15 — Goal-seeking that isn't a force
Implemented as "set velocity toward the goal", which overrides separation/alignment/cohesion entirely and defeats the premise of a weighted-force model. Or a goal force that is normalised to constant magnitude and never decays near the goal, so agents orbit the waypoint forever and `time_to_goal` never fires — the run burns its full tick budget one metre from success.

### S16 — Concave local minima with no stall detection
Goal force pulls east; obstacle-avoidance force from a concave pocket pushes west with equal magnitude; net force ≈ 0; the flock parks. **This is not a bug — it is the correct behaviour of the model**, and it is a named domain pitfall. The *failure* is that we run 9,999,000 more ticks reporting healthy progress. Without a stuck detector this is the single most common way a run wastes a day.

### S17 — Force-cancellation jitter that no metric captures
Two near-equal opposing forces flip the net sign each tick; agents vibrate on the spot. Position variance is low (looks like a settled flock), mean NND is stable (looks healthy), polarization is low-but-so-is-a-milling-swarm. **Every existing metric says "fine".** Detecting it requires a metric on heading *reversal* — mean per-tick heading change, or the fraction of agents whose velocity dot-product with their previous velocity is negative.

### S18 — Boundary mode inconsistent between simulation, obstacles, and metrics
Sim wraps toroidally; obstacle-intersection tests use Euclidean geometry, so an obstacle straddling the seam is invisible on one side; metrics use Euclidean distance (S5). Or the config offers "bounded" and "toroidal" but only one is actually implemented and the other silently falls through to the same branch.

---

## 3. How to guarantee the DURABLE WORKFLOW layer fails (W1–W16)

### W1 — Non-determinism inside the workflow body
`SystemTime::now()` to stamp progress; `rand` to jitter a retry; iterating a `HashMap` of batch results; reading a Diesel row directly in the workflow rather than through an activity. On replay after a worker restart, the workflow takes a different path than history records → history mismatch → the run wedges, or worse, silently continues from a different state. The rule: **the workflow body is a pure function of (input, activity results, signals)**. Clock, RNG, and I/O live in activities.

### W2 — Round-trip the whole agent state through workflow history
`let state = advance_batch(state).await?` where `state: Vec<Agent>`. At N=5000 that is ~160 KB serialised, per batch, in **both** the argument and the result, persisted forever. 1000 batches → ~320 MB of history for one run. Replay after a deploy reads all of it. Postgres row/TOAST limits, memory blowups on the worker, minutes-long replays, and eventually a workflow that cannot be resumed at all. State belongs in a `run_state` table keyed by `(run_id, checkpoint)`; history should carry a **handle and a hash**, not the payload.

### W3 — Non-idempotent activity + retry = duplicated ticks
`append_ticks(run_id, ticks)` succeeds, the ack is lost to a network blip, the engine retries, and the batch is appended twice. Tick 4000 now exists twice with different values; metrics double-count; the trajectory renders a ghost. Requires an idempotency key and a `UNIQUE (run_id, tick)` constraint that makes the double-write *impossible*, not merely unlikely.

### W4 — Poison run retries forever instead of dead-lettering
An activity panics on `NaN` (from S3/S10). Retry policy is exponential backoff with no maximum attempts. The run never completes, never fails, never alerts, and permanently occupies a worker slot. Multiply by five such runs and the whole system is wedged while every dashboard says "Running".

### W5 — No cancellation path
No cancel signal, no cancellation check between batches, no tick budget enforced in the workflow. The only way to stop a runaway is to kill the worker — which, being durable, resumes the run on restart. This is the specific mechanism by which P6 becomes unrecoverable.

### W6 — Signals dropped rather than buffered
A cancel signal sent before the workflow has started, or while the worker is down, or to a run that has just completed. If the engine drops rather than durably buffers it, the UI shows "Cancelling…" forever and the user learns the button is a lie. Also: signal sent to a stale workflow ID after a resubmit.

### W7 — Two sources of truth for run status
The web app writes `runs.status = 'running'` on submit; Harvest tracks workflow state in its own tables. The workflow completes but the app-side status update fails (or was never wired). The board shows `Running` for a run that finished yesterday. Symmetric failure: app marks `failed` on a transient submit error while the workflow is actually running fine, and now a live run is invisible.

### W8 — Migrations not applied, or applied in an incompatible order
Harvest's workflow tables don't exist in CI/prod, so submit 500s (best case) or a lazily-created table diverges from the expected schema (worst case). Related: app migrations and engine migrations maintained as separate streams that can interleave into a state where a foreign key references a table that doesn't exist yet. Nobody checks migration status at boot.

### W9 — No worker running, and no way to tell
The dev compose file starts the web app but not the worker; or the worker crashes on startup because of a config typo and its container restarts silently. Runs enqueue and sit at `Queued` **forever with no error anywhere in the UI**. This is the highest-frustration failure in the system because it is 100% invisible and 100% cheap to surface.

### W10 — Queue name / workflow type mismatch
Workflow registered on queue `boid-sim`; worker polls `boidsim`. Or the workflow *type name* is refactored (`SimulationWorkflow` → `BoidRunWorkflow`) and redeployed while runs are in flight, so the engine cannot resolve a definition for existing histories. Orphaned runs that no worker will ever pick up, with no "unroutable" error.

### W11 — Batch duration exceeds the activity timeout
A 10k-tick batch at N=2000 takes 90 s; the start-to-close timeout is 60 s. The engine times out and retries **while the first attempt is still running**. Two workers now simulate the same batch and both write (see W3). Symptoms: interleaved/duplicated ticks, nondeterministic results, and CPU burn that makes the timeout more likely — a positive feedback loop into total collapse under load.

### W12 — No workflow versioning across a kernel change
We fix S1 (in-place update). Runs are in flight. Their histories were produced by the old kernel; replay now computes different values for the same recorded steps. Either the engine detects a mismatch and wedges them, or it doesn't and we get a run that is half old-physics and half new-physics — and is still labelled with a seed, still claimed reproducible. This corrupts the archive, not just the in-flight run.

### W13 — No wall-clock or tick budget on the run as a whole
A stuck flock (S16) or a slow configuration (P16) runs until someone notices. No `max_ticks`, no `max_wall_clock`, no timer racing the simulation, no "this run has made no metric progress in 10 minutes" check.

### W14 — Split transaction between data write and progress marker
The activity writes tick rows in transaction A and updates `runs.last_completed_tick` in transaction B. Crash in between → progress says 5000, data has 4000, resume starts at 5001, and 1000 ticks are silently missing from the middle of the trajectory. The chart interpolates over the gap and looks perfect.

### W15 — Dead letters that nobody can see
Harvest dead-letters a poisoned workflow into a table. There is no ops page, no count on the dashboard, no alert, no replay button. The feature exists and is functionally equivalent to `/dev/null`.

### W16 — No concurrency limit or backpressure
Fifty runs submitted at once each grab a DB connection; the pool (default ~10–16) is exhausted; the *web app* now 500s, including its own health check, so the orchestrator restarts it, and the durable workflows resume immediately and exhaust the pool again. A self-sustaining outage triggered by ordinary success.

---

## 4. How to guarantee the TDD process is a sham (T1–T13)

The build is specified as strict red/green/refactor by AI agents. That is exactly the setting where TDD degrades into *test-shaped documentation written after the fact*. Here is how it happens.

### T1 — Tests written after the implementation
Git history shows `src/sim/forces.rs` and `tests/forces.rs` landing in one commit, with the test asserting precisely the constants the implementation happens to produce. The test can never fail for a reason that teaches anything; it is a snapshot of current behaviour wearing a test's clothes.

### T2 — A red phase that was never observed
The ritual is performed in the transcript ("now I'll write the failing test") but no failure is ever *executed and recorded*. Or the "red" was a **compile error**, not an assertion failure — which proves only that the function doesn't exist yet, not that the test discriminates right from wrong behaviour.

### T3 — Assertions on implementation details
`assert_eq!(hash.buckets.len(), 37)`; `assert_eq!(sim.internal_scratch.capacity(), 512)`. These break on every refactor (so they get deleted) and pass for wrong physics (so they protect nothing). The refactor step of red/green/refactor becomes impossible, which is how the codebase ends up as §5.

### T4 — Vacuous passes
```rust
for c in collisions { assert!(c.penetration > 0.0); }   // collisions is empty
assert!(metrics.iter().all(|m| m.polarization <= 1.0)); // metrics is empty
```
Both pass with an empty collection, and the collection is empty because the simulation never ran. This is the single most common way a green suite tests nothing.

### T5 — Epsilon laundering
`assert!((got - want).abs() < 1.0)` on a quantity whose whole range is [0, 1]. Or an epsilon widened *after* the test failed, which converts a caught bug into a documented one. Any tolerance loose enough to admit the wrong algorithm is worse than no test, because it buys false confidence.

### T6 — Never differentially test the spatial hash against brute force
The single highest-value test in the project — "for 1000 random configurations, the spatial-hash neighbour set is *exactly* the brute-force neighbour set, and a 200-tick run with each produces bit-identical state" — is also the easiest to skip because both implementations "obviously work". S6 lives or dies here.

### T7 — Postgres tests gated off in CI
Integration tests marked `#[ignore]`, or `#[cfg(feature = "db-tests")]`, or gated on `DATABASE_URL` being set — and CI doesn't set it. The persistence layer, the repositories, the migrations, and the entire workflow layer are then **untested**, while the badge is green and the coverage number is respectable. A column-name typo ships to production.

### T8 — Mock until nothing real is exercised
Workflow tests mock the activities; activity tests mock the repository; repository tests mock Diesel. Every layer is verified against a fiction of its neighbour. The integration seams — which is where 90% of the W-class bugs live — are the *only* thing never tested.

### T9 — Hand-picked examples where properties are free
The simulation is a rich source of invariants that property tests check for pennies: `speed ≤ max_speed` always; all components finite always; wrapped positions in `[0, W)` always; the neighbour relation symmetric; total momentum unchanged by a pure separation force in a closed system; results invariant under a global translation (in bounded mode) or a relabelling of agents. Replacing these with three hand-chosen examples is how S3, S4, S10 and S13 survive.

### T10 — No determinism test — or one that only tests within a process
The core product promise is "same seed ⇒ same result". Either it is untested, or it is tested as `run(cfg) == run(cfg)` in a single process — which passes even with a per-process-seeded `HashMap` (S7) because the hash seed is constant *within* a process. The test must cross a **process boundary** and, ideally, a **replay boundary**.

### T11 — Slow tests get disabled after the first CI timeout
The 100k-tick numerical-stability test (the only thing that catches S9 and S10) takes four minutes, times out CI once, and is marked `#[ignore]` "temporarily". It never comes back.

### T12 — Coverage theatre with no adversarial inputs
90% line coverage, entirely happy-path. No test for N=0, N=1, all agents coincident, `neighbor_radius = 0`, negative weights, `dt = 0`, obstacle exactly on the toroidal seam, goal inside an obstacle, `max_speed = 0`.

### T13 — CI that retries flaky tests
`--retries 3` or a rerun-on-failure job. A genuinely non-deterministic simulation (S7/S8) now passes CI two times in three. The retry setting converts the project's most important bug class into background noise.

---

## 5. How to guarantee the CODEBASE is unpleasant (C1–C10)

### C1 — Domain logic inside route handlers
The tick loop, the force computation, or the metric aggregation living inside a `#[post("/runs")]` body. Now the only way to test physics is to spin up HTTP + Postgres, which is slow, which means it isn't done.

### C2 — `unwrap()` / `expect()` / panic in handlers and activities
A panic in a request handler is a 500 with no context for the user and no structured log for us. A panic inside a workflow activity is *worse*: depending on engine semantics it may be an infinite retry (W4) or an immediate dead-letter, and either way the interesting information — which agent, which tick, what value — is in a backtrace nobody reads.

### C3 — God modules
`simulation.rs` at 3,000 lines containing vector math, neighbour search, five force rules, integration, obstacle geometry, metrics, and serde impls. Merge conflicts on every parallel task, no unit of the system testable in isolation, and every change has an unbounded blast radius.

### C4 — Fight the framework
Hand-rolled routing next to `#[get]`/`#[post]`; raw SQL strings next to `#[repository]`; `format!("<div>{}</div>", user_input)` next to Maud (which throws away compile-time template checking *and* auto-escaping, i.e. it also reintroduces XSS). The result is a codebase where the framework's guarantees are true in some files and not others, and nobody can tell which without reading.

### C5 — No pure simulation core
The sim crate/module depends on Diesel types, on the workflow context, or on the HTTP request. You cannot run a million ticks in a `cargo test` without a database. Consequence: the numerical tests in §2 are *impossible to write cheaply*, so they aren't written. **This one architectural choice determines whether §2 is preventable at all.**

### C6 — Primitive obsession: everything is an `f32`
Positions as `(f32, f32)`, weights as `f32`, radii as `f32`, status as `String`, run id as `i32`. Passing a radius where a weight was expected compiles. The type system — the main reason to be in Rust — is switched off exactly where the domain is most confusable.

### C7 — Parameter definitions duplicated across four layers
A new steering weight must be added to: the Maud form, the Diesel schema + migration, the workflow input struct, and the sim config struct. Someone will update three. The failure is silent — the parameter is accepted by the form and ignored by the kernel, or defaults to 0.0 and the flock quietly stops cohering.

### C8 — Untyped errors all the way up
`anyhow::Error` from kernel to handler, so the UI can only ever render "something went wrong" (P13). No distinction between "your configuration is invalid" (user fixes it), "the simulation diverged" (interesting!), and "the database is down" (page someone).

### C9 — htmx as a page-reload machine, or as a spa in disguise
`hx-get="/runs" hx-target="body"` every 5 s (destroys canvas and scroll — P10), or so much hand-written JS holding simulation state client-side that the "server-rendered" architecture is nominal and the client and server disagree about what tick it is.

### C10 — No seam for time or randomness
`Instant::now()` and `thread_rng()` called directly inside sim and metric code. Tests become time-dependent and flaky (feeding T13), and W1/S8 become unfixable without surgery, because the non-determinism is diffused through the codebase rather than injected at one edge.

---

## 6. INVERSION — every failure mode as an acceptance criterion

Requirements are phrased implementation-agnostically and MUST-style. "How a test proves it" names a specific, automatable check. Requirement IDs mirror failure IDs (`P1 → R-P1`).

### 6.1 Product

| Failure mode | Inverted requirement | How a test proves it |
|---|---|---|
| **P1** No way to judge a run | The system MUST ship ≥4 named **reference scenarios** (`ordered_flock`, `random_walk_control`, `concave_trap`, `dense_jitter`) with committed expected metric ranges, and every run view MUST display each metric against the control baseline for the same N. | Golden test: each reference scenario runs and every metric falls inside its committed `[lo, hi]`. A UI test asserts the run page renders a baseline value alongside each metric. |
| **P2** No visualisation | Every completed or running run MUST expose a rendered view of agent positions/trajectories over time, with a scrub control, available from the first shippable increment. | Route test: `GET /runs/{id}/frames?t=…` returns ≥1 frame with N position entries for any run with ≥1 recorded tick. Snapshot test on the rendered SVG/canvas payload for a fixed seed. |
| **P3** Reproducibility claimed, not stored | A run MUST persist a complete **provenance record**: seed, all parameters, `dt`, boundary mode, neighbour algorithm + its tuning, float width, kernel version, and a `config_hash` over all of it. Re-running from a provenance record MUST reproduce the state hash exactly. | Test: `rerun_from_provenance(run) → state_hash == run.state_hash`. Second test: mutating any one provenance field changes `config_hash`. |
| **P4** Bare float boxes | Every parameter MUST declare unit, valid range, default, and one-line meaning, sourced from a single machine-readable definition; the form MUST reject out-of-range values server-side with a field-level message; ≥3 named presets MUST be loadable in one click. | Test: for every parameter in the definition set, the rendered form contains its label, unit and default; submitting `range.max + 1` returns 422 with that field named. Preset test: loading each preset yields a config that passes validation. |
| **P5** Spinner forever | A running run MUST display current tick, total ticks, percent complete, wall-clock elapsed, **time since last progress update**, and worker liveness. A run with no progress for > `stall_threshold` MUST be visually flagged as stalled. | Test: freeze progress in a fake clock, advance past the threshold, assert the run view renders the `stalled` state and the stale-progress duration. |
| **P6** No cancel / no budget | Every run MUST be cancellable from the UI at any point after submission and MUST reach a terminal `Cancelled` state within one batch boundary. Every run MUST carry a `max_ticks` **and** `max_wall_clock`, both defaulted and both enforced. | Integration test: submit → cancel → poll; assert terminal `Cancelled` ≤ N seconds, no further ticks written after the cancel tick, partial results still readable. Budget test: a run configured past `max_ticks` terminates as `BudgetExceeded`. |
| **P7** Comparison by eyeball | The system MUST provide an N-run comparison view showing (a) a parameter diff highlighting only fields that differ, (b) metric series overlaid on shared normalised axes, (c) per-metric deltas, and (d) each delta contextualised against the seed-to-seed variation of the same config. | Test: compare two runs differing in one weight; assert exactly that field is flagged as differing and all others are not. Assert the response contains series for both runs on a common tick axis. |
| **P8** Black box | Runs MUST record a queryable **event log** (`goal_reached`, `collision`, `stuck_detected`, `budget_exceeded`, `diverged`, `flock_split`) with tick numbers, and MUST support fetching full agent state at any recorded checkpoint tick. | Test: a scenario engineered to trap the flock emits a `stuck_detected` event with a tick number; `GET /runs/{id}/state?tick=T` returns N agents for any checkpoint T. |
| **P9** Storage explodes / silent truncation | Trajectory persistence MUST be governed by an explicit, per-run recorded sampling policy (checkpoint interval, agent subsample), and any view rendering sampled data MUST state the sampling in the response. Storage per run MUST be bounded by a documented formula and asserted in test. | Test: run 50k ticks; assert row count ≤ `ceil(ticks/interval) · sampled_agents` and that the frames response includes the sampling descriptor. |
| **P10** Live-update pathology | Live progress MUST update via targeted partial swaps that never replace the visualisation container or reset scroll, at a documented interval ≥1 s, backed by a query proven O(1)-ish in run length. | Test: the polling endpoint returns only the progress fragment (assert the payload excludes the canvas/root element). Perf test: progress query latency at 10k recorded ticks is within X of latency at 100. |
| **P11** Mutable scenarios falsify history | A submitted run MUST snapshot its full configuration immutably; editing a scenario MUST NOT alter any existing run's stored config or displayed parameters. | Test: submit run → mutate the source scenario → assert the run's `config_hash` and rendered parameters are unchanged. Ideally enforced by an append-only constraint, also asserted. |
| **P12** Undefined metrics | Every metric MUST have a committed formal definition (formula + units + which agents are included + any threshold constants), surfaced in the UI, and the implementation MUST be tested against hand-computed values for at least one fixture. | Test: hand-computed fixture — 4 agents with known headings ⇒ exact expected polarization; a known 2-agent geometry ⇒ exact mean NND. Docs test: every metric key has a definition entry. |
| **P13** No empty/failure states | Zero-run and zero-result states MUST render actionable guidance. A failed run MUST display a typed reason, the failing stage, the last successful tick, a link to its dead-letter entry (if any), and any partial results. | Test: force an activity failure; assert the run page renders the typed reason and last-good tick, and that partial metrics remain retrievable. Empty-list test asserts the empty-state copy. |
| **P14** Unshareable | Every run MUST have a stable permalink and MUST export its metrics and provenance as CSV and JSON from the UI. | Test: `GET /runs/{id}.json` and `.csv` return 200 with a header row / provenance block; permalink resolves after a restart. |
| **P15** Domain pitfalls invisible | The system MUST compute and display, per run: a **stall/stuck** indicator (see R-S16), a **jitter** metric (see R-S17), a **flock-cohesion/split** count, and the neighbour algorithm's realised cost. Each MUST be able to fire in a purpose-built fixture. | One test per indicator, each with a scenario engineered to trigger it and a control scenario asserted *not* to trigger it (no false positives). |
| **P16** Hidden scaling cliff | Submission MUST show a pre-flight cost estimate (expected pair-operations and projected wall-clock) derived from N, ticks and the selected neighbour algorithm, and MUST warn or require confirmation above a configured budget. | Test: estimate for (N=2000, naive) is ≥ 50× the estimate for (N=200, naive) and above the warn threshold; assert the confirmation gate is returned. |

### 6.2 Simulation

| Failure mode | Inverted requirement | How a test proves it |
|---|---|---|
| **S1** In-place update bias | A tick MUST be computed as a pure function `step(state_t, params) -> state_{t+1}`; **no agent may observe another agent's tick-`t+1` state**. Results MUST be invariant to the order agents are processed and to permutation of the agent array. | Permutation test: shuffle the agent array with a known permutation, step once, un-permute; assert **bit-identical** state to the unshuffled step. This fails loudly under in-place update and passes only for double-buffering. |
| **S2** Timestep dependence | All forces and integration MUST be expressed in per-second units and scaled by `dt`; halving `dt` while doubling tick count MUST converge (trajectory difference decreases monotonically toward a limit), not merely change. | Convergence test: run to fixed simulated time `T` at `dt`, `dt/2`, `dt/4`; assert `‖x(T)_{dt} − x(T)_{dt/2}‖ > ‖x(T)_{dt/2} − x(T)_{dt/4}‖` and both below a bound. Dimensional test: doubling `dt` with halved ticks leaves total displacement within tolerance. |
| **S3** Zero-vector normalise → NaN | Vector normalisation MUST be total: normalising a zero-length vector MUST yield the zero vector (or a documented defined result), never NaN. No simulation state may contain a non-finite value at any tick. | Unit test on `normalize(0,0)`. Property test: after every tick of a 10k-tick randomised run, `state.iter().all(|a| a.pos.is_finite() && a.vel.is_finite())`, with a guard asserting the state is non-empty (defeats T4). |
| **S4** Coincident agents → inf | Separation MUST be well-defined at distance 0 (documented epsilon floor or a deterministic symmetry-breaking rule) and MUST produce a finite, bounded force. | Test: two agents at identical positions ⇒ separation force is finite and ≤ `max_force`; N=50 all coincident ⇒ 100 ticks with all state finite and no agent exceeding `max_speed`. |
| **S5** Naive delta on a torus | In toroidal mode, **all** pairwise displacement, distance, neighbour selection, and distance-based metrics MUST use the minimum-image convention; flock centroid MUST use a circular mean. | Test: agents at `x=1` and `x=W−1` in a world of width `W` report distance 2, and are neighbours when `radius=3`. Metric test: a flock straddling the seam has a centroid on the flock, not at `W/2`. Invariance test: translating the entire world state by an arbitrary offset (mod W) leaves all metrics unchanged. |
| **S6** Hash/radius mismatch drops neighbours | The accelerated neighbour search MUST return a set **exactly equal** to brute force for the same radius, for all configurations, and a full run under each MUST produce identical state. | Differential property test over randomised N, radius, world size and cell size: `sorted(hash_neighbors(i)) == sorted(brute_neighbors(i))` for every `i`. Plus: 500-tick run with each backend ⇒ identical state hash. Must include boundary cases (radius == cell size, radius = 0, agents on cell borders, on the seam). |
| **S7** Non-deterministic iteration order | Force accumulation MUST be performed in a deterministic, documented order independent of any hash-map iteration or thread scheduling; parallelism, if used, MUST use a deterministic combine. | Cross-process determinism test (see R-T10): the same seed run in two separate processes produces the same state hash. Additionally: a test that runs the kernel single- and multi-threaded and asserts identical state hashes. |
| **S8** RNG not seeded end-to-end | All randomness MUST derive from the run's stored seed via an explicitly-passed, deterministic generator; no global/thread RNG anywhere in the kernel; the generator's stream MUST be continuous across batch boundaries (or derived per-batch by a documented deterministic scheme). | Static check: CI greps/lints for `thread_rng`/`OsRng`/`SystemTime::now` in the kernel crate and fails on any hit. Behavioural test: a run executed as one 1000-tick batch and as ten 100-tick batches yields identical state hashes. |
| **S9** f32 drift / cross-machine divergence | Simulation state and accumulation MUST use `f64`; the float width MUST be recorded in provenance; long-run stability MUST be asserted; the build MUST NOT enable fast-math/unsafe-FP contraction. | Long-run test: 100k ticks; assert all state finite, energy/speed bounded, and no monotone drift in a conserved quantity beyond a tight bound. Determinism test compares state hashes across two build/run configurations. |
| **S10** Unclamped forces | Each steering force MUST be individually clamped to `max_force`, the summed steering MUST be clamped to `max_force`, and speed MUST be clamped to `max_speed`, every tick — enforced as an invariant, not a convention. | Property test after each tick of a randomised adversarial run (tiny separation distances, huge weights): `speed ≤ max_speed + ε` and every force magnitude `≤ max_force + ε`, for 100k ticks. Targeted test: two agents at distance 1e-9 do not produce a displacement > `max_speed·dt`. |
| **S11** Metrics on stale state | Metrics for tick `t` MUST be computed from the fully-committed state at the end of tick `t`, and MUST be **independent of the batch size** used to execute the run. | Test: run 1000 ticks as batches of 1000, 100 and 7; assert the full metric series are identical in all three. This single test kills S11 and half of W-class batching bugs. |
| **S12** Radius semantics inconsistency | Radius semantics MUST be explicit and uniform: distances compared against `radius²` only when squared, inclusive/exclusive boundary documented, and any per-rule radius exposed as its own named parameter rather than silently differing. | Test: at exactly `d == radius` the neighbour relation matches the documented boundary rule; setting `radius = r` yields a neighbour count matching an analytically-derived count for a uniform lattice within tolerance. |
| **S13** Self counted as neighbour | An agent MUST NOT be a member of its own neighbour set, in any backend. | Test: `neighbors(i)` never contains `i`, asserted for brute force and hash across randomised configs including all-coincident agents. Cohesion test: with 3 agents in a known triangle, the cohesion target equals the hand-computed centroid of the *other two*. |
| **S14** Obstacle lies (teleport / tunnelling) | Obstacle avoidance MUST be a force, never a positional teleport; collision detection MUST be **swept** (segment from `pos_t` to `pos_{t+1}`), so no agent can pass through an obstacle in a single tick regardless of speed. | Tunnelling test: a single agent aimed at a thin obstacle at `max_speed` with a large `dt` registers a collision and does not end up on the far side. Assert no tick produces a position discontinuity greater than `max_speed·dt`. |
| **S15** Goal overrides everything / orbiting | Goal-seeking MUST be a weighted force composed with the others (setting `goal_weight = 0` MUST make goal position irrelevant to the trajectory), and arrival MUST be defined by a documented radius that terminates the waypoint. | Test: two runs identical except goal position, with `goal_weight = 0`, produce identical state hashes. Arrival test: an agent aimed at a goal in open space reaches `arrival_radius` and emits `goal_reached` within a bounded tick count — it does not orbit. |
| **S16** Concave local minimum unnoticed | The system MUST detect stalls: if the flock centroid displacement and per-agent net displacement stay below thresholds over a window of `W` ticks while a goal is unreached, the run MUST emit `stuck_detected` and follow its configured stall policy (flag / terminate). | Test: the `concave_trap` reference scenario emits `stuck_detected` within a bounded number of ticks; the `ordered_flock` scenario never emits it over the same duration (no false positives). |
| **S17** Jitter invisible to all metrics | The system MUST compute a **jitter/heading-reversal** metric (e.g. fraction of agents per tick with `v_t · v_{t−1} < 0`, and mean per-tick heading change) and display it per run. | Test: a fixture with deliberately opposed forces of equal magnitude scores jitter above threshold while its position-variance and NND metrics remain in the "healthy" band — proving the new metric discriminates where the old ones do not. |
| **S18** Boundary mode inconsistent | Boundary mode MUST be a single explicit setting honoured identically by integration, neighbour search, obstacle geometry and metrics; both modes MUST be genuinely implemented and behaviourally distinguishable. | Test: identical config under `toroidal` vs `bounded` produces *different* state hashes (proves both branches are live). Seam test: an obstacle straddling the wrap boundary is detected from both sides. |

### 6.3 Durable workflow

| Failure mode | Inverted requirement | How a test proves it |
|---|---|---|
| **W1** Non-determinism in workflow body | The workflow body MUST be a pure function of (input, activity results, signals). No clock, RNG, environment, or direct DB access in the workflow body; all such effects MUST occur inside activities. | CI lint over the workflow module denying `SystemTime`, `Instant`, `rand`, and direct repository/connection types. Behavioural test: kill and restart the worker mid-run; assert the replayed run completes with the same state hash as an uninterrupted run. |
| **W2** State bloats workflow history | Simulation state MUST NOT be passed through workflow history. Activities MUST exchange a checkpoint **handle plus a content hash**; state lives in a `run_checkpoints` table. Recorded history size per run MUST be bounded and asserted. | Test: run 200 batches at N=2000; assert total serialized history size < a fixed budget (e.g. 256 KB) and that per-batch history entries are O(1) in N. Assert the checkpoint hash round-trips. |
| **W3** Non-idempotent activity double-appends | Every state-mutating activity MUST be idempotent under retry, keyed by `(run_id, batch_index)`, and the schema MUST enforce `UNIQUE (run_id, tick)` so duplication is impossible rather than improbable. | Test: invoke the append activity twice with the same key; assert exactly one set of ticks exists and the second call succeeds (not errors). Chaos test: force a post-commit/pre-ack failure; assert no duplicate ticks after retry. |
| **W4** Poison retries forever | Every activity MUST have a bounded retry policy (max attempts + max elapsed). On exhaustion the run MUST transition to a terminal `Failed` state with a typed reason and MUST appear in the dead-letter list. | Test: an activity that always fails causes the run to reach terminal `Failed` within `max_attempts`, and a dead-letter record exists with the failing activity name and error. |
| **W5** No cancellation | The workflow MUST check for cancellation at every batch boundary and MUST honour a cancel signal within one batch, releasing its worker slot and leaving partial results intact and readable. | Test: start a long run, send cancel, assert terminal `Cancelled` within one batch duration, no ticks written after the cancel point, and partial metrics retrievable. |
| **W6** Signals lost | Signals MUST be durably buffered: a signal delivered before workflow start, during worker downtime, or concurrently with completion MUST either be applied exactly once or rejected with an explicit, surfaced reason — never silently dropped. | Test matrix: signal-before-start, signal-with-worker-down-then-restarted, signal-after-completion. Each asserts a defined, observable outcome (applied, or explicitly rejected with reason shown in the UI). |
| **W7** Two sources of truth for status | Run status MUST have exactly one authoritative source, with the read model derived from it. The UI MUST NOT be able to show `Running` for a workflow the engine considers terminal. | Reconciliation test: for a set of runs driven through every lifecycle path, assert `ui_status(run) == engine_status(run)` for all. Invariant test asserting no run row can be terminal-in-engine but active-in-app. |
| **W8** Migrations missing / mis-ordered | Application boot MUST verify that all migrations (app **and** engine) are applied and fail fast with an explicit message otherwise. CI MUST run the full migration set from empty on every build. | CI job: migrate from an empty database, then run the integration suite. Test: boot against a database missing a migration and assert a clear startup failure, not a 500 at request time. |
| **W9** No worker, no signal | The system MUST expose worker liveness (last heartbeat, poll queue, in-flight count) and MUST flag any run queued longer than a threshold with no available worker as `NoWorkerAvailable` in the UI. | Test: enqueue a run with no worker running; after the threshold the run view shows `NoWorkerAvailable` (not a silent `Queued`). Health endpoint reports worker staleness. |
| **W10** Queue / type-name mismatch | Queue names and workflow type identifiers MUST be defined once as shared constants used by both registration and polling; renaming a workflow type MUST be gated behind an explicit versioning step. | Test: assert the registered type/queue set equals the polled set. Test: a workflow enqueued for an unregistered type surfaces an `Unroutable` error on the run within a bounded time rather than sitting queued. |
| **W11** Batch longer than the activity timeout | Batch size MUST be bounded such that the p99 batch duration is under a documented fraction (e.g. 50%) of the activity timeout; long batches MUST heartbeat; concurrent execution of the same batch MUST be impossible (via W3's key). | Test: measure batch duration at max supported N and assert `< 0.5 × timeout`. Concurrency test: two simultaneous executions of the same `(run_id, batch_index)` produce one committed result. |
| **W12** Kernel change corrupts in-flight replay | The simulation kernel MUST carry a version recorded in each run's provenance. A workflow MUST refuse to replay under a different kernel version, terminating with an explicit `KernelVersionChanged` reason rather than continuing. | Test: start a run, bump the kernel version, force replay; assert the run terminates with `KernelVersionChanged` and the archive's `config_hash`/results are untouched. Test that `config_hash` incorporates kernel version. |
| **W13** No run-level budget | Every run MUST enforce both `max_ticks` and `max_wall_clock` at the workflow level via a timer racing the simulation, terminating as `BudgetExceeded` with partial results retained. | Test with a fake clock: a run configured to exceed `max_wall_clock` terminates as `BudgetExceeded` and its partial ticks remain queryable. |
| **W14** Split transaction loses ticks | Tick data and the progress marker MUST be committed in a single transaction, so `last_completed_tick` is always exactly consistent with persisted tick rows. | Invariant test after a crash injected between the writes: `max(tick) == last_completed_tick`. Property test over resumed runs: the recorded tick sequence has no gaps and no duplicates from 0 to `last_completed_tick`. |
| **W15** Dead letters invisible | Dead-lettered workflows MUST be listed in an ops view with the failure reason, the failing activity, and a replay action; the count MUST be visible from the main board. | Test: force a dead-letter; assert it appears in the ops listing with reason and that the replay action re-enqueues it. |
| **W16** No backpressure | Concurrent in-flight runs MUST be capped by an explicit limit below the DB connection-pool size; excess runs queue visibly as `Queued`; the health endpoint MUST remain responsive under saturation. | Load test: submit 5× the concurrency limit; assert in-flight count never exceeds the cap, the health endpoint returns 200 throughout, and every run eventually completes. |

### 6.4 TDD integrity

| Failure mode | Inverted requirement | How a test proves it |
|---|---|---|
| **T1** Tests written after code | Each behavioural change MUST land as a test-first commit pair: a commit adding a failing test, then a commit making it pass. History MUST show the test existing before its implementation. | CI/history check over the PR: for each new test file/case, a commit exists where it is present and the suite is red. Reviewers reject squashed single-commit features. |
| **T2** Red never observed | The red phase MUST be recorded as an **assertion failure**, not a compile error, and the captured failure output MUST be retained in the PR. | The PR includes per-test red-phase output showing `assertion failed: left != right` with concrete values. A compile error is not an acceptable red. |
| **T3** Implementation-detail assertions | Tests MUST assert observable behaviour (state, metrics, responses, invariants), not internal structure. Internal fields MUST NOT be exposed solely for assertion. | Review gate plus a practical proof: a refactor of the neighbour backend (naive ⇄ hash) MUST require **zero** test changes. If tests change, they were testing implementation. |
| **T4** Vacuous passes | Every test asserting over a collection MUST first assert the collection is non-empty (and, where known, of the expected length). | Lint/review rule; plus a mutation-style spot check — deleting the body of `step()` must fail the suite. Any test still green under a no-op kernel is vacuous by definition. |
| **T5** Epsilon laundering | Numeric tolerances MUST be explicit, justified in a comment relative to the quantity's scale, and MUST NOT be widened to make a failing test pass. Determinism assertions MUST be **exact** (hash equality), not epsilon-based. | Review gate on any epsilon change (diff shows widening → reject). Determinism tests use exact hash comparison, which admits no epsilon. |
| **T6** No brute-force oracle | An equivalence test between the naive O(N²) neighbour search and every accelerated backend MUST exist, be property-based over randomised configurations, and run on every CI build. | The test in R-S6. CI MUST fail if this test is removed or ignored (assert its presence in the required-test manifest). |
| **T7** DB tests skipped in CI | CI MUST provision Postgres and run the full integration suite on every build. `#[ignore]` MUST be disallowed in CI; the build MUST fail if the count of executed tests drops below a committed floor. | CI runs with `--include-ignored` (or bans the attribute) and asserts executed-test count ≥ committed baseline. A test asserting `DATABASE_URL` is set fails the job when it isn't. |
| **T8** Over-mocking | At least one end-to-end test per layer boundary MUST exercise the real component: real Postgres for repositories, real engine for workflows, real kernel for activities. Mocks are permitted only for external services and injected time. | A designated `e2e` suite: submit a run over HTTP → real worker executes → assert metrics readable via the API and the state hash matches the pure-kernel run of the same config. |
| **T9** Examples where properties belong | The kernel MUST have property-based tests for its invariants: finiteness, speed bound, force bound, wrap containment, neighbour symmetry, permutation invariance, translation invariance. | Each invariant is a `proptest`/`quickcheck` case over randomised configs with a minimum case count, run in CI. |
| **T10** Determinism untested or intra-process only | "Same seed ⇒ same result" MUST be proven **across process boundaries** and **across a replay/restart boundary**, by exact state-hash equality. | Test harness spawns two separate processes with the same config and compares state hashes; a second test kills the worker mid-run, lets the workflow replay, and compares the final hash to an uninterrupted run. |
| **T11** Slow tests get disabled | Long-running numerical tests MUST remain in CI (a scheduled or tagged lane is acceptable; deletion and `#[ignore]` are not), with a documented budget. | The required-test manifest includes the long-run stability test; CI fails if a manifest entry did not execute in the last N builds. |
| **T12** Happy-path-only coverage | An adversarial-input suite MUST exist covering N=0, N=1, all-coincident agents, `radius=0`, `dt=0`, negative/zero weights, `max_speed=0`, goal inside an obstacle, and an obstacle on the wrap seam — each with a defined expected outcome (valid result or typed validation error, never a panic). | One test per case asserting either a well-formed result or a typed `ConfigError`; a blanket assertion that no case panics. |
| **T13** Flaky-tolerant CI | CI MUST NOT retry failing tests. A flaky test MUST be treated as a determinism defect and investigated, not re-run. | Assert the CI configuration contains no retry/rerun directives (config test or review gate). |

### 6.5 Codebase

| Failure mode | Inverted requirement | How a test proves it |
|---|---|---|
| **C1** Logic in handlers | Route handlers MUST only parse/validate input, call a domain service, and render. No simulation, metric, or orchestration logic in handler bodies. | Every behaviour reachable via HTTP MUST also have a handler-free unit test. Practical proof: the full kernel test suite runs with no HTTP and no database. |
| **C2** `unwrap()`/panic in handlers and activities | Handlers, repositories and activities MUST be panic-free: all fallible paths return typed errors mapped to HTTP status or workflow failure reasons. | CI lint denying `unwrap`/`expect`/`panic!`/indexing-panic patterns outside tests. Fuzz/adversarial test: malformed submissions return 4xx, never a 500-from-panic. |
| **C3** God modules | The simulation MUST be decomposed into separately-testable units (vector math, neighbour search, forces, integration, obstacles, metrics), each with its own tests; a module size ceiling MUST be enforced. | CI check on file/module length. Structural proof: each unit has a test file that compiles without the others' internals. |
| **C4** Fighting the framework | Routes MUST use the framework's typed route macros; all HTML MUST be produced by Maud templates (no ad-hoc string HTML); all persistence MUST go through the `#[model]`/`#[repository]` layer (raw SQL only in reviewed, justified exceptions). | CI lint for HTML-in-`format!`, raw SQL outside the repository layer, and hand-rolled routing. XSS test: a run name containing `<script>` renders escaped. |
| **C5** No pure core | The simulation kernel MUST be a separate crate/module with **zero** dependencies on Diesel, HTTP, or the workflow engine, capable of running 10⁶ ticks in-process with no external services. | Dependency test/CI check asserting the kernel's dependency set excludes those crates. Timing test: a 10⁶-tick run completes in the unit-test lane with no DB. |
| **C6** Primitive obsession | Domain quantities MUST be newtypes with meaningful names and validated construction (`Position`, `Velocity`, `Weight`, `Radius`, `Seed`, `RunStatus` as an enum). Invalid values MUST be unrepresentable or rejected at construction. | Compile-fail tests (`trybuild`) proving a `Radius` cannot be passed where a `Weight` is expected. Constructor tests rejecting NaN/negative where invalid. |
| **C7** Parameter definitions duplicated | Simulation parameters MUST be defined once and derived everywhere (form, validation, storage, workflow input, kernel config). Adding a parameter MUST require exactly one definition-site change. | Test: iterate the canonical parameter set and assert each appears in the rendered form, the validation rules, the persisted config, and the kernel config — failing loudly on any missing one. |
| **C8** Untyped errors | Errors MUST be typed and classified into at least `ConfigInvalid`, `SimulationDiverged`, `BudgetExceeded`, `Cancelled`, `InfrastructureError`, each with a user-facing message and a distinct HTTP/terminal-status mapping. | Test: each error variant maps to its expected status/reason and renders its specific message — never a generic "something went wrong". |
| **C9** htmx misuse | Live updates MUST swap only the smallest relevant fragment; the visualisation and form state MUST survive updates; client-side code MUST NOT hold authoritative simulation state. | Test: the progress-poll response contains only the fragment (assert absence of page chrome/root ids). Browser/DOM test asserting the canvas node identity is preserved across a progress swap. |
| **C10** No seam for time/randomness | Time and randomness MUST be injected at the boundary (a clock port and a seeded generator), never called ad hoc inside kernel or metric code. | CI lint denying `Instant::now`/`SystemTime::now`/`thread_rng` in kernel and workflow modules. Tests drive a fake clock — proof the seam exists is that timeout/stall tests need no `sleep`. |

---

## 7. Top 10 ranked risks

Scored `L` (likelihood this happens if we do nothing special, 1–5) × `I` (impact if it happens, 1–5). Impact is judged against the product's core promise: *a trustworthy, comparable, observable record of flocking experiments*.

| # | Risk | IDs | L | I | Score | The single mitigation that MUST NOT be cut |
|---|---|---|:-:|:-:|:-:|---|
| 1 | **Results are not reproducible.** Hash-map iteration order, a stray `thread_rng`, per-batch reseeding, or a kernel change mid-flight makes the same seed produce different numbers. Every comparison, every finding, and the product's entire reason to exist evaporates — and it fails *silently*. | S7, S8, W1, W12, P3 | 5 | 5 | **25** | **Cross-process + cross-replay state-hash determinism test in CI** (R-T10). Two processes, same seed, exact hash equality; plus kill-the-worker-mid-run replay equality. Not an epsilon comparison. Not intra-process. |
| 2 | **You cannot see the flock.** Visualisation deferred; the product is a table of numbers about an inherently visual phenomenon. Users can't judge runs, and *we* lose the instrument that makes S1/S5/S6/S14/S17 obvious in seconds. | P2, P8, P15 | 5 | 5 | **25** | **A trajectory/animation view with a scrub control in the first shippable increment** (R-P2). Visualisation is not a phase-2 nicety; it is the primary debugging tool for the whole build. |
| 3 | **The physics is quietly wrong.** In-place update bias, missing `dt`, or naive deltas on a torus. The flock still flocks, all tests stay green, every number produced is subtly meaningless. | S1, S2, S5, S11, S12 | 4 | 5 | **20** | **The three invariance tests: permutation invariance (R-S1), `dt`-convergence (R-S2), and world-translation invariance under minimum-image (R-S5).** They are cheap, they are exact, and each one is specifically undetectable by "does it flock" testing. |
| 4 | **The accelerated neighbour search silently drops neighbours.** Cell size vs radius mismatch or a boundary off-by-one. Produces a plausible flock with the wrong neighbour set and no error, ever. | S6, P16 | 4 | 5 | **20** | **Property-based differential test: hash neighbours ≡ brute-force neighbours, exactly, over randomised configs, plus identical state hashes for a full run under each backend** (R-S6/R-T6). This is the highest value-per-line test in the project. |
| 5 | **Workflow history explodes with agent state.** Passing `Vec<Agent>` through activity arguments/results. Replays take minutes, storage balloons, and eventually runs become unresumable — discovered only at the size where it's most expensive. | W2, P9 | 4 | 5 | **20** | **Activities exchange checkpoint handles + hashes only, with a CI-asserted per-run history-size budget that is O(1) in N** (R-W2). Assert the budget in a test, not in a doc. |
| 6 | **Runs sit queued forever with no visible error.** No worker deployed, worker crash-looping, or a queue-name typo. The app looks totally broken and the cause is invisible from every surface. | W9, W10, W8, P5 | 4 | 5 | **20** | **Worker liveness surfaced in the UI plus a `NoWorkerAvailable` state on any run queued past a threshold** (R-W9). Never let "queued" and "nothing is listening" look the same. |
| 7 | **CI is green while the persistence and workflow layers are untested.** Postgres tests `#[ignore]`d or env-gated; over-mocking. Column typos, migration gaps and idempotency bugs ship behind a passing badge. | T7, T8, W8 | 4 | 4 | **16** | **CI provisions Postgres, runs migrations from empty, bans `#[ignore]`, and fails if executed-test count drops below a committed floor** (R-T7). A test that doesn't run is a lie with a checkmark. |
| 8 | **A run cannot be stopped or bounded.** No cancel, no tick budget, no wall-clock budget, no stall detection — so a trapped or fat-fingered run consumes a worker indefinitely and blocks everyone else. | P6, W5, W13, S16 | 4 | 4 | **16** | **Cancellation honoured at every batch boundary, plus mandatory `max_ticks` and `max_wall_clock` defaults enforced in the workflow** (R-P6/R-W5/R-W13). Cancellation is not a feature; it is the pressure-relief valve for every other failure. |
| 9 | **Retries duplicate or lose ticks.** Non-idempotent append + retry (or a batch exceeding the activity timeout, running twice concurrently) duplicates ticks; a split transaction loses them. Metrics are wrong and the trajectory has ghosts or invisible gaps. | W3, W11, W14 | 3 | 5 | **15** | **`UNIQUE (run_id, tick)` plus single-transaction commit of data + progress marker** (R-W3/R-W14). Make duplication and gaps *structurally impossible*, not merely unlikely. |
| 10 | **Numeric explosion to NaN/inf mid-run.** Zero-vector normalisation, coincident agents, or unclamped `1/d²` separation. Typically appears after 10⁴–10⁵ ticks — past every unit test, inside every real run — then poisons all downstream metrics. | S3, S4, S10, S9 | 4 | 4 | **16*** | **Finiteness + speed-bound + force-bound property assertions on every tick of a 100k-tick adversarial run** (R-S3/R-S10), kept in CI and never `#[ignore]`d (R-T11). |

\* Ranked 10th on judgement rather than raw score: it is the most likely of these to be caught by ordinary development, since it eventually produces visibly broken output — unlike #1–#5, which fail silently and plausibly.

**Two cross-cutting notes on the ranking.** First, seven of the top ten fail *silently* — they produce output that looks correct. That is why almost every non-cuttable mitigation is an **exact equality assertion** (state hashes, neighbour-set equality, tick-sequence integrity) rather than a threshold or a tolerance: against silent failure, tolerances are the enemy. Second, risks #1, #3, #4 and #10 are all cheap to prevent *now* and effectively unfixable later, because by then an archive of results exists that was produced by the broken kernel and cannot be retroactively validated.

---

## 8. The four artefacts that carry most of the safety

If everything else in this document is negotiable, these four are not. Each one collapses a whole class of failure modes.

1. **A pure simulation kernel with no I/O dependencies** (R-C5). Makes 10⁶-tick numerical tests cheap, which is the only reason §2 is preventable at all. Every other simulation safeguard is downstream of this one architectural decision.

2. **A canonical state hash** — a stable, order-independent digest of the full agent state at a given tick. It is the mechanism behind determinism testing (R-T10), permutation invariance (R-S1), backend equivalence (R-S6), batch-size invariance (R-S11), replay integrity (R-W1) and provenance verification (R-P3). One primitive, six guarantees. Build it first.

3. **The provenance record + `config_hash`** (R-P3). Turns "reproducible" from a claim into a checkable property, and gives run comparison (R-P7) and immutability (R-P11) something concrete to compare and freeze.

4. **The reference scenario suite** — `ordered_flock`, `random_walk_control`, `concave_trap`, `dense_jitter` (R-P1). These are simultaneously the product's interpretability baseline *and* the regression corpus for stall detection (R-S16), jitter detection (R-S17), and metric sanity (R-P12). Every scenario must have both a positive assertion (the indicator fires) and a negative one (it doesn't fire on the control) — otherwise a detector that always says "yes" passes.

---

*Document produced by adversarial pre-mortem. Failure-mode IDs are stable and referenced by the inverted requirement IDs (`P1 → R-P1`); use them in issue titles and test names so traceability from acceptance criterion to test is mechanical.*
