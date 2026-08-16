# Boidboard — Divergent Brainstorming Session

> Status: ideation only. Nothing here is committed to. The purpose of this document is to
> widen the option space before narrowing it, and to record the reasoning behind the
> narrowing so a later reader knows what was considered and rejected.

---

## 0. Interrogating the starting hypothesis

**Hypothesis as given:** *Boidboard is a web app for defining, running, observing and
comparing boids flocking simulations as durable experiments.*

That is a reasonable and buildable product. It is also, as stated, weak in three specific
ways that are worth naming before we diverge.

### 0.1 The durability argument does not survive contact with the arithmetic

A boids tick with a spatial hash is roughly `O(N · k)` where `k` is the average neighbour
count (typically 6–20 because of the vision cone). In Rust, 1,000 agents × 10,000 ticks is
on the order of a few seconds of single-core CPU. A run of the size a user will actually
configure through a web form is *smaller than a typical HTTP request timeout*.

If a single simulation run is the unit of work, a durable workflow engine is not
architecture — it is decoration. This is the single biggest risk to the project: building
**durability theatre**, where Harvest is present because we wanted to use Harvest.

The honest resolution is that durability earns its place at a *different unit of work*.
Three units make it genuine:

| Unit | Duration | Why durable |
|---|---|---|
| A single run | seconds | ❌ Not durable-worthy on its own |
| A **sweep** (10²–10⁴ seeded runs) | minutes → hours | ✅ fan-out, partial failure, cancellation, aggregation |
| A **search** (iterative, generation-by-generation) | hours → days | ✅ multi-round stateful dependency graph, human steering |
| A **paced / eternal world** | days → forever | ✅ timers, signals, continue-as-new |

**Conclusion: the unit of work must not be the run.** See §1 and §7.

### 0.2 "Compare runs side by side" is a feature, not a product

Comparison is table stakes for anything experiment-shaped. It does not tell us who the user
is or why they would return tomorrow. A product needs a *question the user has* and a
*decision it changes*.

### 0.3 Single-run intuition is scientifically wrong, and the product should say so

Boids is a chaotic system. Two runs with identical parameters and different seeds can look
qualitatively different. A UI that shows one run per parameter set actively teaches users to
over-read noise. Whatever we build, **the seed dimension should be first-class**, not an
advanced option. This is a genuine differentiator hiding inside a correctness concern.

### 0.4 What survives

The hypothesis's substrate — scenarios, runs, metrics, visualisation — is correct and
necessary. What it lacks is a *spine*: a reason the thing is more than a nicely rendered
sandbox. §1 explores candidate spines.

---

## 1. Six-plus reframings

Each reframing states: **who**, **the core loop**, **why it is worth building**, and
**what it costs**.

### R1 — The Experiment Bench (the hypothesis, sharpened)

- **Who:** an engineer or researcher who has a question about emergent behaviour.
- **Loop:** author scenario → run → observe → measure → fork-and-tweak → compare.
- **Worth it:** it is the substrate everything else needs; runs must exist before sweeps do.
  Demonstrates the whole Autumn Web stack end to end.
- **Cost:** thin on its own; no reason to return after the novelty wears off.
- **Sharpening:** promote the **Study** (a named question with a hypothesis, a set of runs,
  and a written conclusion) above the Run. Users do not want a run; they want an answer.

### R2 — The Flock Fitter (parameter search as a service)

- **Who:** a game AI developer or technical artist who knows what they want the flock to
  *look like* but has no idea what the five weights should be.
- **Loop:** describe the target as an objective (polarization ≈ 0.9, mean NND ≈ 2.5 body
  lengths, zero collisions, time-to-goal < 400 ticks) → the system searches parameter space
  (random → hill-climb → CMA-ES) across thousands of seeded trials → returns a Pareto front
  of weight vectors → user auditions candidates side by side and exports the winner.
- **Worth it:** this is the *real* pain. Tuning boids weights by hand is universally
  described as fiddly and unprincipled. It is also the strongest durable-workflow story in
  the entire option space: multi-hour, fan-out heavy, resumable, cancellable, human-steerable.
- **Cost:** needs a credible objective language and a search algorithm; the objective
  language is the hard part (see W2 — humans judge flocks perceptually, not numerically).

### R3 — Boid CI (regression harness for steering algorithms)

- **Who:** an engine or AI-middleware developer who ships steering code and currently has no
  way to test it. Emergent behaviour is famously untestable with conventional unit tests.
- **Loop:** register a suite of golden scenarios → on each commit / algorithm version, run
  the suite → assert behavioural invariants (no collisions, no stuck agents, polarization
  within a band, cluster count stable, determinism hash unchanged) → diff against the
  baseline → block or flag regressions.
- **Worth it:** *snapshot testing for emergent behaviour* is a genuinely unmet need and the
  most defensible commercial framing here. It also gives scheduled DAGs and dead-lettering
  an obvious home.
- **Cost:** requires users to bring their own algorithm, which means either a declarative
  rule DSL or sandboxed code execution — the latter is a serious security project.

### R4 — Live Swarm Ops Console (digital twin)

- **Who:** a drone / AMR / warehouse-robot fleet operator.
- **Loop:** real agents stream telemetry → Boidboard runs a shadow boid model in lockstep →
  divergence between predicted and actual steering raises alarms (wind, sensor fault,
  adversarial interference) → operator pushes waypoint or weight changes back to the fleet.
- **Worth it:** highest per-seat value in the list; a live, always-on, per-fleet workflow is
  the most natural Harvest use case imaginable.
- **Cost:** requires real hardware and a real customer. Not v1. **Salvageable kernel:** the
  *expected-vs-actual divergence detector* is a great primitive for R3 as well.

### R5 — Boids 101 (teaching instrument / explorable explanation)

- **Who:** a student, a self-teacher, an educator building a lecture.
- **Loop:** guided labs. *Turn cohesion to zero — what happens? Now separation. Set the
  vision cone to 360° and watch the flock stop looking like birds. Watch tick time explode
  at N=2000, then switch on the spatial hash and watch it flatten.*
- **Worth it:** lowest-risk, highest-polish-per-effort framing. Server-rendered pages with
  htmx map perfectly onto "each lab step is a page". Excellent portfolio artifact.
- **Cost:** almost no durability need; content authoring is the real work; hard to monetise.
- **Salvageable kernel:** the **force decomposition inspector** (§2, F19) is the single best
  teaching artifact and also a genuinely useful debugging tool for R1/R2/R3. Build it once,
  it serves every framing.

### R6 — The Arena (benchmark leaderboard)

- **Who:** algorithm authors, students in a course, competitive tinkerers.
- **Loop:** submit a steering policy → it runs against a hidden scenario suite → scored on a
  composite metric → public, reproducible-by-seed leaderboard.
- **Worth it:** creates a reason to return and a community. The submission pipeline
  (validate → compile → sandbox → run N scenarios → score → publish) is a textbook workflow
  with retries, timeouts and dead letters.
- **Cost:** sandboxing untrusted code. Mitigable by restricting submissions to a declarative
  parameter/rule spec rather than arbitrary code — which collapses it into R2 plus a
  scoreboard, and that is fine.

### R7 — The Commons (shared persistent world)

- **Who:** hobbyists, a Discord, anyone who likes watching things.
- **Loop:** one world ticks forever server-side; anyone can drop obstacles, predators, goals,
  or spawn a flock; everyone watches the same stream.
- **Worth it:** the purest fit for durable workflow primitives — an eternal workflow with
  continue-as-new, timers for tick pacing, and signals for every user action. Also the most
  fun and the most shareable.
- **Cost:** no clear value capture; moderation/abuse surface; competes for attention with the
  analytical framings.

### R8 — The Cartographer (phase diagrams as the product)

- **Who:** anyone who wants to understand the *space* rather than a point in it. Complexity-
  curious engineers, researchers, educators.
- **Loop:** pick two axes (say separation weight × neighbour radius) and a metric → the
  system fans out a grid of seeded runs → renders a **phase diagram**: a heatmap of the
  order parameter with regime boundaries drawn on it (swarm / torus / dynamic parallel /
  highly parallel, after Couzin et al. 2002) → click any cell to watch that run.
- **Worth it:** this is the *signature artifact*. It is beautiful, it is scientifically
  meaningful, it is impossible to produce by hand, it is only possible because of durable
  fan-out, and nobody else ships it as a first-class object. It converts "I ran a
  simulation" into "I mapped a region of behaviour space."
- **Cost:** compute-hungry (a 20×20 grid × 10 seeds = 4,000 runs), which is exactly the
  problem Harvest is for.

### R9 — Egress (crowd safety / evacuation sandbox)

- **Who:** venue planners, architecture students, safety consultants.
- **Loop:** upload or draw a floorplan → place exits, obstacles, crowd density → run over
  many seeds → egress-time distribution, congestion heatmap, bottleneck ranking.
- **Worth it:** a real vertical with real budget; the metrics (time-to-evacuate, max local
  density) are legible to non-simulation people.
- **Cost:** plain boids is a poor pedestrian model (needs social-force / RVO); credibility
  requires domain validation we cannot provide.

### R10 — Steering-as-a-Service (headless API)

- **Who:** anyone with a slow-tick simulation: turn-based games, NPC servers, LLM-agent
  swarms, ambient art installations.
- **Loop:** POST agent states → receive steering vectors.
- **Worth it:** forces a clean separation between the pure kernel and the web layer, which is
  a good architectural discipline regardless.
- **Cost:** latency makes it absurd for real-time games; the market is thin. Keep as an
  architectural constraint, not a product.

### R11 — Steering for anything with a position and a velocity

- **Who:** speculative. Anyone doing multi-agent anything.
- **Loop:** reframe the three rules as domain-general dynamics — separation as diversity
  pressure, cohesion as consensus pull, alignment as trend-following — over any vector space:
  embeddings, portfolios, load-balancer weights, feature vectors.
- **Worth it:** probably not directly. But it implies one cheap week-one decision: make the
  kernel generic over the *space* (metric + boundary behaviour) and, if painless, over
  dimensionality. That costs nearly nothing now and is unaffordable to retrofit.

### 1.1 Recommended composite

None of these should be built alone.

> **Boidboard is a study bench for emergent steering behaviour. The unit of work is not a
> simulation — it is a question asked across parameter space, executed durably, answered
> with statistics rather than a single pretty animation.**
>
> **"Run the sweep, not the sim."**

- **Substrate:** R1, sharpened so that *Study* outranks *Run*.
- **Spine:** R8 (phase diagrams / sweep cartography) — the signature artifact, and the thing
  that makes durable fan-out load-bearing from day one.
- **Second spine, v2:** R2 (objective-driven search) — the same machinery pointed at a goal.
- **Commercial angle, later:** R3 (Boid CI) — the same machinery pointed at a baseline.
- **Free rider:** R5's force-decomposition inspector, which every framing wants anyway.

Note that R8, R2 and R3 are *the same execution engine with three different objective
functions*: map it, optimise it, or compare it to a baseline. That is a strong sign the
architecture is right.

### 1.2 Vocabulary

Getting the nouns right early is cheap and compounding.

| Noun | Meaning |
|---|---|
| **Scenario** | A validated, content-hashed parameter spec. Immutable. |
| **Trial** | One execution of one Scenario at one seed. The atom. |
| **Run** | Colloquial synonym for Trial; avoid in code. |
| **Sweep** | A generated set of Scenarios (grid / linspace / random) × seeds. |
| **Study** | A named question: hypothesis text + one or more Sweeps + a written conclusion. |
| **Board** | The dashboard surface listing Studies and live activity. |
| **Regime** | A qualitative classification of collective state (swarm / torus / parallel). |

---

## 2. Feature brainstorm

48 features across nine themes. MoSCoW is scoped to **first release**.
🔶 marks the non-obvious ones.

### A. Scenario authoring

| # | Feature | Priority |
|---|---|---|
| F1 | Scenario form: N, w₁–w₅, neighbour radius, FOV angle, max speed, max force, world size, boundary mode, seed, tick budget | **MUST** |
| F2 | Fork-and-tweak: clone any Scenario into a new one with the delta highlighted | **MUST** |
| F3 | Scenario presets (Couzin regimes, "murmuration", "corridor evacuation", "predator drill") | SHOULD |
| F4 | Obstacle painter — place circles/polygons on a canvas, posted back via htmx | SHOULD |
| F5 | Waypoint / path editor for goal-seeking and path-following | SHOULD |
| F6 | YAML/JSON import-export of the Scenario spec, with a canonical content hash | SHOULD |
| F7 | Heterogeneous cohorts — different weights per species, leader/follower roles | COULD |
| F8 | Predator agents with their own pursuit steering; prey flee force | COULD |

### B. Execution & control

| # | Feature | Priority |
|---|---|---|
| F9 | Submit a Trial → durable workflow executes in tick batches | **MUST** |
| F10 | Live progress: ticks completed / budget, ETA, current metric snapshot | **MUST** |
| F11 | Cancel a Trial or an entire Sweep | **MUST** |
| F12 | Pause / resume via signal | SHOULD |
| F13 | 🔶 **Live parameter steering** — change weights mid-flight via signal; the change is drawn as a marker on the metric timeline so you can see the flock reorganise | SHOULD |
| F14 | Budget guardrails: max ticks, max wall-clock, max agents, max sweep cardinality — enforced before submission | **MUST** |
| F15 | 🔶 **Fork from tick** — rehydrate a checkpoint at tick 5,000 and branch with new parameters; the parent and child share history up to the fork | SHOULD |
| F16 | Real-time pacing mode (wall-clock 30 Hz) vs as-fast-as-possible | COULD |

### C. Observation

| # | Feature | Priority |
|---|---|---|
| F17 | Canvas/SVG trajectory renderer with a playback scrubber | **MUST** |
| F18 | Metric timeseries charts: polarization, angular momentum, mean NND, collisions | **MUST** |
| F19 | 🔶 **Force decomposition inspector** — click any agent at any tick; see all five component forces and the blended result drawn to scale, with the neighbour set highlighted. Answers "why did it do that?" | SHOULD |
| F20 | Live tick streaming to the board (SSE or htmx polling), downsampled | SHOULD |
| F21 | Neighbour-graph overlay and spatial-hash cell overlay | COULD |
| F22 | Accumulated density/occupancy heatmap for the whole run | COULD |
| F23 | 🔶 **Highlight reel** — auto-detected interesting moments (flock split, first collision, stuck onset, polarization peak) surfaced as jump-to chips on the scrubber | COULD |
| F24 | Animated GIF/WebM export of a run for sharing and for slide decks | COULD |

### D. Analysis & comparison

| # | Feature | Priority |
|---|---|---|
| F25 | Comparison view: 2–N Trials, overlaid metric charts, parameter diff table | **MUST** |
| F26 | Sortable metrics table across all Trials in a Study | **MUST** |
| F27 | 🔶 **Seed-variance view** — same parameters × K seeds rendered as a distribution (violin/box), with an explicit warning when the user tries to conclude anything from a single seed | SHOULD |
| F28 | 🔶 **Regime classifier** — label each Trial swarm / torus / dynamic-parallel / highly-parallel from (polarization, angular momentum), shown as a coloured badge | SHOULD |
| F29 | Stuck / local-minimum report: which agents, at which tick, in which geometry | SHOULD |
| F30 | Cluster/flock count over time (did the flock fission?) | COULD |

### E. Sweeps & search

| # | Feature | Priority |
|---|---|---|
| F31 | Grid sweep: cartesian product of parameter ranges × seeds, fanned out to child workflows | **MUST** |
| F32 | **Phase-diagram heatmap** of a 2-D sweep, cells coloured by metric, clickable through to the underlying Trial | **MUST** (this is the signature artifact) |
| F33 | Sweep cost estimator and live progress (n complete / n failed / n remaining) | **MUST** |
| F34 | Objective-driven search: random → hill-climb → CMA-ES toward a target metric profile | COULD (v2) |
| F35 | Pareto front view for multi-objective targets (cohesion vs collision-freedom vs speed) | COULD |
| F36 | 🔶 **Early abort of dominated branches** — kill sweep cells that cannot beat the incumbent, freeing budget | COULD |

### F. Reproducibility & provenance

| # | Feature | Priority |
|---|---|---|
| F37 | Every Trial records engine version, scenario hash, seed, RNG algorithm, and a final state hash | **MUST** |
| F38 | 🔶 **Verified reproducibility badge** — re-execute on demand and compare state hashes; badge reads "verified bit-exact", or goes stale when the engine version changes | SHOULD |
| F39 | Permalink + read-only public share view for any Trial, Sweep or Study | SHOULD |
| F40 | Export bundle: params + metrics CSV + sampled trajectories + a citation block | COULD |

### G. Collaboration & narrative

| # | Feature | Priority |
|---|---|---|
| F41 | 🔶 **Study as a written question** — hypothesis field, linked sweeps, and a conclusion the author writes when done. The artifact people actually share | SHOULD |
| F42 | Timestamped annotations on runs and on specific ticks | COULD |
| F43 | Public gallery of published Studies | COULD |

### H. Platform & operations

| # | Feature | Priority |
|---|---|---|
| F44 | Thin Harvest ops surface: workflow status, retry counts, dead letters, manual replay | **MUST** (you need it for your own sanity on day one) |
| F45 | Per-user concurrency limits and a fair-share worker pool | SHOULD |
| F46 | Artifact retention policy: full trajectories for recent/pinned Trials, keyframes for the rest | SHOULD |
| F47 | Nightly scheduled benchmark DAG with baseline-drift alerting | COULD |

### I. Deliberately strange

| # | Feature | Priority |
|---|---|---|
| F48 | 🔶 **Adversarial scenario search** — invert the objective and search for the parameters that maximise collisions, stuck agents, or numerical divergence. "Chaos engineering for emergent AI." Nearly free once F34 exists | COULD |
| F49 | 🔶 **Naive-vs-accelerated equivalence checker** exposed as a *user-facing* tool: run both neighbour implementations and report any disagreement | COULD |
| F50 | Tick-cost profiler: ms/tick vs N, plotted, demonstrating the O(N²) → O(N) transition live | COULD |

### Explicit WON'T (first release)

3D. GPU or client-side WASM simulation. Real robot/hardware integration. Execution of
user-uploaded arbitrary code. Multi-tenant auth beyond a single owner. Mobile-first layout.
Social-force / RVO pedestrian models. Real-time collaborative editing.

---

## 3. Where durable workflows genuinely earn their place

Assessed honestly. Each entry states the Harvest primitive, and specifically why a plain
background job (a queue + a worker + a row in a table) would not suffice. Entries are graded:
**STRONG** (durability is load-bearing), **MODERATE** (durability is a real convenience),
**WEAK** (be honest — a cron job would do).

### D1 — Simulation execution in tick batches — MODERATE, but for a subtle reason

- **Primitive:** a workflow whose body loops over `advance_batch(cursor, 500 ticks) →
  checkpoint`, each batch a retryable activity; optional timer between batches for paced mode.
- **Naive argument:** "long-running compute needs durability." This is *false* at the sizes a
  web form will produce (see §0.1). A 4-second run does not need crash recovery.
- **Real argument:** the activity boundary *is* the checkpoint boundary, and checkpoints are
  what make the product's best features possible — scrubbing, fork-from-tick (F15),
  mid-flight parameter steering (F13), live progress (F10), and cancellation that doesn't
  lose partial results. **Durability here buys product capability, not just crash safety.**
- **When it becomes STRONG:** N ≥ 5,000; tick budgets in the millions; paced/real-time runs
  measured in hours; any run a human intends to interact with while it executes.
- **Design consequence:** the simulation must run *inside activities*, never in the workflow
  body. Workflow bodies must be deterministic across replay; floating-point simulation across
  heterogeneous workers and compiler versions is not a determinism guarantee we want to bet
  the engine's replay correctness on. The workflow holds only seed, params, cursor and hashes.

### D2 — Parameter sweeps (fan-out) — STRONG

- **Primitive:** child workflows, one per Trial, with a bounded-concurrency fan-out and a
  parent that awaits all children.
- **Why a job queue is not enough:**
  1. **Completion semantics.** The parent must know when all 4,000 children have reached a
     terminal state *including* the ones that failed and exhausted retries. A queue gives
     fire-and-forget; you would hand-roll a completion ledger — which is precisely what a
     workflow engine is, built worse.
  2. **Partial failure is a first-class outcome.** A phase diagram with 3 holes out of 400 is
     a valid, publishable result — but only if the holes are *known and marked*, never
     silently missing. Statistical conclusions from a set with unknown gaps are wrong.
  3. **Cascading cancellation.** "Cancel this sweep" must reliably stop 4,000 in-flight
     children and free their budget.
  4. **Backpressure.** Concurrency limits enforced by the parent, not by praying about queue
     depth.
  5. **Aggregation survives the aggregator.** The parent crashing mid-aggregation must not
     lose the children's results.

### D3 — Iterative objective-driven search — STRONG

- **Primitive:** a workflow loop: generation → fan-out children → await all → activity
  `propose_next_generation(results)` → repeat, for 20–200 generations over hours.
- **Why a job queue is not enough:** generation *k+1*'s inputs are a pure function of
  generation *k*'s outputs. This is a stateful, multi-round dependency graph with no natural
  home in a queue. You would build a state machine in Postgres plus a poller — an engine
  reimplementation. This is the canonical durable-workflow shape and it maps onto R2 exactly.
- **Bonus:** the loop naturally accepts human interruption ("stop, generation 7 looks right")
  via signal, without special casing.

### D4 — Human-in-the-loop control — STRONG

- **Primitive:** signals (`pause`, `resume`, `cancel`, `set_weights`, `inject_obstacle`,
  `spawn_predator`, `extend_budget`) received by a running workflow, plus `await_signal`.
- **Why a job queue is not enough:** a background job has no addressable durable mailbox. You
  would poll a `pending_commands` table from inside the loop and reinvent delivery semantics.
- **The killer detail:** because signals are recorded in the workflow history, **the signal
  log is the provenance record of what the human did to the run**. "At tick 12,400 the
  operator raised cohesion from 1.0 to 2.5" is not an audit afterthought — it is the same
  data structure that caused the behaviour. That is exactly the reproducibility story
  Boidboard is selling (F37/F38).

### D5 — Approval and quota gates — STRONG

- **Primitive:** `await_signal(approval)` with a timer-based timeout that auto-rejects after
  24 hours.
- **Why not:** a process that must sit idle for a day and then either proceed or compensate.
  Nothing about a job queue supports "wait for a human, but not forever."

### D6 — Scheduled benchmark DAG — STRONG

- **Primitive:** DAG schedule. `fan-out over golden scenario suite → aggregate scores →
  compare to stored baselines → alert on regression → publish`.
- **Why not:** cron plus jobs can *trigger*, but not express node dependencies with per-node
  retry, nor give last night's execution a coherent identity you can open and inspect. The
  "compare" node must not run until every scenario node has settled; the "publish" node must
  not run if "compare" failed. That is a DAG, and hand-rolling DAG semantics is a project.

### D7 — Retry on flaky activities — MODERATE

- **Primitive:** per-activity retry policy with exponential backoff.
- **Candidates:** persisting large trajectory artifacts; rendering preview images/GIFs;
  outbound webhooks and email on sweep completion; fetching an imported floorplan.
- **Why not just try/catch:** retries alone are easy. What is hard is **idempotent retry
  across a crash**: "did I already write the artifact for Trial 47 before I died?" The
  workflow's deterministic execution ID answers that; ad-hoc retry logic does not.

### D8 — Dead-lettering a diverged run — STRONG

- **Primitive:** dead letter queue after exhausted retries, plus a human triage surface (F44).
- **Candidates:** NaN/Inf velocity blow-up (a real failure mode when `maxForce` is large and
  `dt` is coarse); agents escaping world bounds; activity timeout because N was mis-estimated;
  a replay-determinism mismatch.
- **Why not:** a queue's DLQ preserves the failed *message*. A workflow's DLQ preserves the
  failed *process* — every prior step's inputs and outputs — so a human can diagnose it,
  fix the engine, and re-drive the exact same execution. For a product whose entire value
  proposition is reproducibility, "we lost why it broke" is disqualifying.

### D9 — Replay as a determinism oracle — STRONG and unusual

- **Primitive:** workflow replay.
- **Idea:** re-execute a completed workflow from its recorded history. If the recomputed
  per-batch state hashes diverge from the recorded ones, then *either* the engine *or* the
  simulation kernel is non-deterministic — and we want to know immediately, because the whole
  product rests on determinism.
- **Why this is notable:** it inverts the usual relationship. Replay is normally an
  implementation detail of durability; here it becomes a **correctness oracle for the domain
  code**, and it directly implements the user-facing reproducibility badge (F38). This is the
  single most elegant justification for the engine in the whole document.

### D10 — Fan-out over seeds for variance estimation — STRONG

- **Primitive:** child workflows, identical Scenario × K seeds, aggregated into a distribution.
- **Why not:** identical to D2, but with a sharper edge — **statistics require completeness**.
  A silently dropped child does not produce a smaller sample, it produces a *biased* one, and
  nothing downstream can detect it. Completion guarantees here are a correctness requirement,
  not an operational nicety.

### D11 — Eternal world workflow — STRONG (only if R7 is built)

- **Primitive:** continue-as-new to bound history growth, timers for tick pacing, signals for
  every user interaction.
- **Why not:** a process that must survive weeks, deploys, and worker replacement while
  holding live shared state.

### D12 — Saga / compensation on sweep cancellation — MODERATE

- **Primitive:** compensating activities. On cancellation: release reserved compute quota,
  delete orphaned artifacts, mark the Sweep partial rather than failed.
- **Why not:** you must know precisely *what was allocated* to undo it, and the workflow
  history is the only place that record is guaranteed complete.

### D13 — Multi-step ingestion (floorplan → vectorise → navmesh → bake index) — MODERATE

- **Primitive:** sequential activities with independent retry and resumability.
- Only relevant if R9 features land. Genuinely workflow-shaped when it does.

### D14 — Artifact retention sweeps — WEAK

- **Primitive:** long timers ("in 30 days, downsample this trajectory to keyframes").
- **Honest assessment:** a nightly cron that scans a table by `created_at` is simpler,
  cheaper, and easier to reason about. Do not use a durable timer per artifact. Listed here
  to be explicitly rejected.

### 3.1 Where durability explicitly does NOT belong

CRUD on Scenarios and Studies. Page rendering. Computing a metric from already-stored
trajectory data. Listing, filtering, sorting. Auth and sessions. Validating a form. Any of
these routed through a workflow is a smell.

### 3.2 Two architectural rules that fall out of the above

1. **Simulation runs in activities; the workflow holds only seed, params, cursor, and
   hashes.** Workflow bodies must replay deterministically, and cross-machine floating-point
   arithmetic is a bad thing to bet replay correctness on.
2. **Never pass bulk state through workflow history.** A 10,000-agent frame must not appear
   in a workflow input or output. Pass an artifact ID; store bytes in Postgres/blob storage.
   *Workflow history is a ledger, not a data store.* Violating this is the most common way
   these systems fall over.

---

## 4. The pure-domain core: what is delightful to test-drive

Strict red/green/refactor needs a large surface of deterministic, side-effect-free logic.
Boids is unusually generous here — nearly all of it is pure functions over value types.

Notation: **E** = example-based test, **P** = property test (proptest/quickcheck), **⚠** =
an edge case that is a known real-world bug source.

### 4.1 Vector math (`Vec2`)

| Test | Kind | Assertion |
|---|---|---|
| Magnitude | E | `Vec2::new(3.0, 4.0).length() == 5.0` |
| Normalize | P | for non-zero `v`: `normalize(v).length() ≈ 1.0` and `dot(v, normalize(v)) > 0` |
| ⚠ Normalize of zero | E | `Vec2::ZERO.normalize()` must be finite. **Decide the contract in a test** — return `Vec2::ZERO`, or return `Option::None` and force callers to handle it. This choice propagates into every steering rule. |
| Limit — under | P | if `\|v\| ≤ m` then `limit(v, m) == v` *exactly*, no floating drift |
| Limit — over | P | `\|limit(v, m)\| ≤ m + ε` for all `v`, `m > 0` |
| ⚠ `angle_between` antiparallel | E | opposing vectors give exactly `π`, not `NaN`. Requires clamping `dot/(\|a\|\|b\|)` to `[-1, 1]` before `acos` — floating error pushes it to `-1.0000000000000002` and `acos` returns NaN. Classic. |
| Scale distributivity | P | `(a + b) * s ≈ a*s + b*s` |
| ⚠ Overflow | E | `length()` of `(1e200, 1e200)` — decide whether to accept `inf` or use `hypot` |

### 4.2 Space / boundary handling

The most under-tested and most bug-prone area in every boids implementation.

| Test | Kind | Assertion |
|---|---|---|
| ⚠⚠ Toroidal displacement | E | world width 100, `a = (1,0)`, `b = (99,0)` → `displacement(a,b) == (-2,0)`, **not** `(98,0)`. Getting this wrong silently tears flocks apart at the seams and is nearly invisible in a small demo. |
| Displacement bound | P | `\|displacement(a,b)\| ≤ worldDiagonal / 2` for all a, b |
| Antisymmetry | P | `displacement(a,b) == -displacement(b,a)` |
| Wrap idempotence | P | `wrap(wrap(p)) == wrap(p)` |
| ⚠ Multi-wrap | E | `wrap(250.0)` in a 100-wide world → `50.0`. A single subtraction is not enough. |
| ⚠ Negative coordinates | E | `wrap(-1.0)` → `99.0`. Requires `rem_euclid`, not `%`, in Rust. |
| Bounce reflection | P | post-bounce position is strictly inside bounds; `\|v\|` unchanged (elastic) |
| Boundary exact | E | position exactly on the wall — define whether it wraps or stays |

### 4.3 Neighbour queries

| Test | Kind | Assertion |
|---|---|---|
| Naive query basics | E | returns all `j ≠ i` with `dist ≤ r`; self never included |
| ⚠ Zero agents | E | `neighbors_of(i)` on an empty world → empty, no panic, no index arithmetic |
| ⚠ Single agent | E | empty neighbour set |
| ⚠ Radius 0 | E | define: empty, or only exactly-coincident agents |
| FOV cone filter | E | a neighbour directly behind is excluded when `fov < 2π` |
| FOV equivalence | P | `fov == 2π` produces exactly the same set as no filter |
| ⚠ FOV with zero velocity | E | an agent with no velocity has no heading — define the fallback (include all / retain last heading) and test it |
| **⚠⚠ Spatial hash ≡ naive** | **P** | **The crown jewel.** For arbitrary random agent sets, world sizes, radii and cell sizes: `spatial_hash_neighbors(i, r)` as a *set* equals `naive_neighbors(i, r)`. This one property test replaces dozens of examples and catches every off-by-one in cell traversal. |
| ⚠ Cell smaller than radius | P | generator must produce `r > cellSize` so the implementation is forced to scan more than a 3×3 block |
| ⚠ Cell larger than world | E | degenerate single-cell case stays correct |
| ⚠ Agents on cell boundaries | E | an agent at exactly `(cellSize, cellSize)` is found from both adjacent cells |
| ⚠ All agents coincident | E | every agent in one cell — correct (if quadratic) result, no panic |
| ⚠ Toroidal hash | P | cell indices must wrap; only a property test in a wrapping world will catch this |

### 4.4 Steering rules, each in isolation

Each is a pure `fn(agent, neighbors, params) -> Vec2`.

**Separation**
- E: one neighbour at distance `d` → force direction is exactly `normalize(self - other)`.
- P: force magnitude is monotonically non-increasing in `d` (with `1/d` weighting).
- P: two neighbours placed symmetrically at `±x` produce exactly cancelling contributions.
- ⚠⚠ E: **coincident agents (`d == 0`) → divide by zero.** Must define behaviour: return zero,
  or apply a *deterministic* nudge derived from agent index/seed. Never `rand()` — that
  destroys reproducibility. Test it explicitly; this happens constantly at spawn.
- ⚠ E: zero neighbours → exactly `Vec2::ZERO`, never `NaN`.

**Alignment**
- E: all neighbours share heading `h` → desired velocity direction is `h`.
- ⚠⚠ E: two neighbours with exactly antiparallel velocities → mean velocity is the zero
  vector → `normalize` of zero. Must return zero force. This *will* happen in a real run.
- ⚠ E: all neighbours stationary → zero force.
- ⚠ E: zero neighbours → zero force.

**Cohesion**
- E: single neighbour → direction is exactly `normalize(other - self)`.
- ⚠⚠ E: **toroidal centroid.** In a 100-wide world with neighbours at x=1 and x=99, the naive
  mean is 50 — the exact opposite side of the world. The centroid must be computed by
  averaging *displacement vectors from self*, then adding back. Test this; it is the second
  seam bug and it makes flocks explode at the boundary.
- ⚠ E: centroid coincides with self → zero force, no NaN.
- ⚠ E: zero neighbours → zero force.

**Goal seeking / arrival**
- E: `desired = normalize(goal - pos) * maxSpeed`; `steer = limit(desired - vel, maxForce)`.
- ⚠ E: already exactly at the goal → zero force.
- P: within the slowing radius, desired speed ramps monotonically to exactly 0 at the goal.

**Path following**
- E: advances to the next waypoint when within capture radius.
- ⚠ E: empty path → zero force, no panic. Single waypoint. Capture radius 0.
- E: behaviour at the final waypoint (hold vs loop) — pin the contract.

**Obstacle avoidance**
- E: obstacle directly behind → zero avoidance force.
- ⚠⚠ E: **head-on collision course** — the perpendicular escape direction is genuinely
  ambiguous. The tie-break must be deterministic and tested, or two identical runs diverge.
- ⚠ E: agent already *inside* an obstacle → push out radially, do not produce NaN.
- ⚠ E: zero-radius obstacle; obstacle at exactly whisker length.
- P: force magnitude increases as clearance decreases (continuity, no cliff).

**Fleeing**
- P: exactly zero beyond the threat radius, and *continuous* at the boundary — sample either
  side of `r` and assert the difference is small. A discontinuity here produces visible
  popping.

### 4.5 Force blending

| Test | Kind | Assertion |
|---|---|---|
| Linear sum | E | `F = Σ wᵢFᵢ` then `limit(F, maxForce)` |
| All weights zero | E | zero force |
| Saturation semantics | E | doubling all weights ≠ doubling the result once saturated — pin this deliberately |
| Prioritised — budget | P | total magnitude never exceeds `maxForce` |
| Prioritised — dominance | P | a higher-priority force is never attenuated by a lower-priority one |
| Prioritised — starvation | E | if the top force alone saturates the budget, all lower forces are dropped *exactly* to zero |
| Context steering — masking | P | a direction masked as dangerous is never selected |
| ⚠ Context steering — equivariance | P | rotating the entire scenario by θ rotates the selected direction by ≈θ (up to bin quantisation). A beautiful property test that catches indexing bugs no example test would. |
| Cross-blender agreement | P | with a single force below `maxForce`, all three blenders return the same vector |

### 4.6 Integration step

| Test | Kind | Assertion |
|---|---|---|
| Speed clamp invariant | P | after any sequence of steps, `\|v\| ≤ maxSpeed + ε` |
| Zero acceleration | P | `pos(t) ≈ pos₀ + v·t` — catches integrator errors immediately |
| `dt = 0` | E | exact no-op |
| ⚠ Negative `dt` | E | pin the contract (reject / debug-assert) |
| ⚠ Huge `dt` | E | tunnelling straight through an obstacle — define and test the guard (sub-stepping or a swept test) |

### 4.7 Determinism and seeding — the contract with Harvest

This section is the highest-value block in the whole document, because it is where the pure
core meets the durable engine.

| Test | Kind | Assertion |
|---|---|---|
| ⚠⚠ **Batch equivalence** | P | **Advancing 1,000 ticks in one call produces a bit-identical state to advancing 10 batches of 100 ticks with a full serialize/deserialize round-trip between each.** This is the single most important test in the product: it is the formal contract that lets Harvest checkpoint the simulation at all. Everything in §3 D1 depends on it. |
| ⚠⚠ **Order independence** | P | shuffle the agent array, run N ticks, sort results by stable ID → identical to the unshuffled run. This forces **double-buffered updates** (compute all forces from state *T*, then commit *T+1*). In-place updating is *the* classic boids bug: it makes agent 0 see the old world and agent 999 see the new one, and it silently breaks determinism. |
| Same seed, same result | P | two invocations of the same `(scenario, seed)` produce identical state hashes |
| Seed actually matters | P | different seeds produce different trajectories with overwhelming probability — guards against a seed being accepted and then ignored |
| Spawn purity | P | the initial layout is a pure function of `(seed, N, world)` |
| ⚠ RNG discipline | E | a pinned, explicitly-versioned PRNG (PCG/xoshiro), seeded per-run, never thread-local, never `SystemRandom` |
| ⚠ No hash iteration in arithmetic | E | assert (by construction and review) that no `HashMap`/`HashSet` iteration order feeds a floating-point accumulation — a notorious source of non-reproducibility in Rust |

### 4.8 Metrics

| Metric | Kind | Assertion |
|---|---|---|
| Polarization `φ = \|Σ v̂ᵢ\| / N` | E | all headings aligned → exactly 1.0 |
| | E | N=4 at 0°/90°/180°/270° → exactly 0.0 |
| | P | always within `[0, 1]` |
| | ⚠ E | N=0 → `None` (not a divide-by-zero, not `-1`); N=1 → 1.0; agents with zero velocity have no heading and must be excluded or explicitly defined |
| Angular momentum `\|Σ (r̂ᵢ × v̂ᵢ)\| / N` | E | a perfect ring moving tangentially → 1.0; distinguishes torus from parallel |
| | P | always within `[0, 1]` |
| Mean nearest-neighbour distance | P | invariant under global translation and rotation |
| | P | scales linearly under uniform scaling of positions |
| | ⚠ E | N=1 → `None`; coincident agents → 0.0 |
| Collision count | ⚠ E | N=2 overlapping agents → **1**, not 2. Pair counting, not per-agent. |
| | P | cumulative count is monotonically non-decreasing |
| Time-to-goal | ⚠ E | never reached → `None`, not a `-1` sentinel. Test the *type*, not just the value. |
| ⚠⚠ Stuck detection | E | an agent parked *exactly at its goal* is **not** stuck — goal satisfaction must be checked first |
| | E | an agent oscillating in a tight loop **is** stuck, despite non-zero speed → the metric must be **tortuosity** (net displacement ÷ path length over a window), not speed |
| Cluster count (union-find on the neighbour graph) | E | N isolated agents → N clusters; all within radius → 1; a barbell configuration → 2 |
| Regime classifier | E | table-driven at all four corners of (polarization, angular momentum), *plus* exactly-on-threshold cases |
| Online accumulators (Welford) | P | streaming mean/variance matches the batch computation within ε |
| | ⚠ E | n=0 and n=1 → variance is `None`, not 0.0 |

### 4.9 Scenario validation

- `validate(spec) -> Result<ValidScenario, Vec<ValidationError>>`, pure and total.
- E: N=0; radius ≤ 0; maxSpeed 0; maxForce 0; world size 0; all weights zero; tick budget 0
  or above the cap; goal outside the world; obstacles overlapping the spawn region.
- E: neighbour radius > world size is a **warning**, not an error — it means "everyone is a
  neighbour", which is a legitimate (and instructive) configuration.
- P: never panics on arbitrary input.
- P: a valid Scenario round-trips through serialize → deserialize unchanged.
- ⚠ P: the canonical content hash is stable under key reordering and *insensitive to
  non-semantic fields* (name, notes, description). This is what makes "we already ran this
  exact Scenario, here is the result" possible.

### 4.10 Sweep expansion

- P: `|expand(sweep)| == Π|axisᵢ| × seedCount`.
- P: every generated Scenario is unique.
- P: expansion order is deterministic, so grid cell `(i, j)` maps to a stable index — the
  phase diagram (F32) depends on this.
- ⚠ E: an empty axis → an **empty** product, not a panic and not a one-element result.
- ⚠ E: `linspace(a, b, n=1)` → `[a]`; `n=0` → `[]`; `a == b` → all identical.
- E: `estimated_cost(sweep)` is a pure function, testable independently of execution.
- Trajectory decimation: P — preserves the first and last frame; `len ≤ max`; tick order
  preserved and strictly monotone.

### 4.11 Fork / branch

- P: forking at tick 0 with unchanged parameters reproduces the parent exactly.
- P: the fork's first tick equals what the parent's next tick would have been, given
  unchanged parameters.

---

## 5. Wild ideas

### W1 — The Flock Fuzzer

Invert the search objective. Instead of finding parameters that produce beautiful flocking,
durably search for the parameters and geometry that **break** your steering: maximum
collisions, maximum stuck agents, fastest route to numerical divergence, worst-case tick cost.
Ship it as "chaos engineering for emergent AI."

**Kernel worth keeping:** this is nearly free once objective-driven search (F34) exists — it
is the same machinery with a negated objective. It also produces exactly the golden-failure
scenarios that R3 (Boid CI) needs as regression fixtures. Failure-finding may be more
commercially legible than beauty-finding.

### W2 — A perceptual metric learned from human votes

Nobody actually wants polarization = 0.87. They want "it looks like starlings." Serve blind
A/B pairs of short flock clips, collect votes, fit a model mapping computed metrics → a
naturalness score, then optimise against the *learned* objective.

**Kernel worth keeping:** even a crude naturalness score built from a few hundred votes is a
better search target than any single order parameter. And the voting pipeline — fan out
comparison tasks, await human signals with timeouts, aggregate under partial response — is a
textbook durable-workflow shape that would otherwise be a genuine pain to build.

### W3 — Inverse boids

Given an observed trajectory — real starling tracking data, a hand-drawn path, a clip from a
film — infer the weight vector that best reproduces it. Turn "what parameters make this?"
around into "what parameters made *that*?"

**Kernel worth keeping:** this is F34 with a trajectory-distance objective instead of a metric
objective. Zero new execution machinery. And "upload a video of a flock, get the parameters"
is the most immediately compelling demo in this entire document.

### W4 — Digital-twin divergence alarm

Run the model in lockstep with a real system and alert when reality diverges beyond a
threshold. Aimed at drone fleets (R4), but the primitive is general.

**Kernel worth keeping:** *expected-vs-actual trajectory divergence* is precisely the
assertion primitive R3 needs. Build the divergence metric for CI; it upgrades to a twin later
without redesign.

### W5 — The executable paper

A Study renders as a citable document in which every figure is regenerated on demand from its
workflow. A "verify" button re-executes the sweep and confirms bit-identical results. The
reproducibility badge automatically goes stale when the engine version changes, and the
document says so, in public, on the figure.

**Kernel worth keeping:** F38 plus engine-version staleness is genuinely shippable and
essentially nobody does it. "This figure has not been verified against engine v0.4" printed
next to the figure is a small feature with an outsized credibility payoff.

### W6 — Boids for non-spatial agents

Treat any vector space as the world. Separation becomes diversity pressure; cohesion becomes
consensus; alignment becomes trend-following. Steer swarms of LLM agents in embedding space,
or portfolio allocations in feature space.

**Kernel worth keeping:** almost certainly not a product. But it implies one nearly-free
week-one decision — make the kernel generic over a `Space` trait (metric + boundary
behaviour), and don't hard-code 2-D assumptions into the steering rules. That costs a day now
and is unaffordable to retrofit later.

---

## 6. Synthesis: the recommended first slice

**Framing.** A study bench for emergent steering behaviour. The unit of work is a *question
asked across parameter space*, not a simulation. Nouns: Study → Sweep → Trial.

**The v1 spine** is the sweep-to-phase-diagram path, because it is simultaneously the
signature user-facing artifact and the thing that makes durable fan-out load-bearing rather
than decorative:

> author a Scenario → expand a 2-D sweep × K seeds → fan out to child workflows →
> aggregate → **phase diagram** → click a cell → watch that Trial → fork and tweak.

**MUST features:** F1 scenario form, F2 fork-and-tweak, F9 durable batched execution,
F10 live progress, F11 cancellation, F14 budget guardrails, F17 trajectory renderer,
F18 metric charts, F25 comparison view, F26 metrics table, F31 grid sweep fan-out,
F32 phase diagram, F33 sweep progress, F37 provenance record, F44 thin Harvest ops surface.

**Top durable-workflow justifications:** D2 (sweep fan-out with completeness guarantees),
D10 (seed fan-out, where completeness is a *correctness* requirement), D4 (signals as the
provenance record of human intervention), D8 (dead-lettering diverged runs with full
context), D9 (replay as a determinism oracle), D6 (scheduled benchmark DAG), D3 (iterative
search, v2).

**Two rules that must not be broken:** simulation lives in activities, never in workflow
bodies; bulk state travels by artifact reference, never through workflow history.

**Top risks, in order:**
1. **Durability theatre** — mitigated by making the sweep, not the run, the unit of work.
2. **Visualisation sink** — trajectory rendering can absorb unbounded effort. Timebox it;
   the phase diagram is the differentiator, not the animation.
3. **Single-seed misinterpretation** — mitigated by making the seed dimension first-class in
   the UI from day one (F27).
4. **Scope creep into 3D, GPU, or user-supplied code** — all three are explicit WON'Ts.
