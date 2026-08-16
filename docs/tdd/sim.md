# TDD log — simulation integration

Evidence for **AC-52** ("every feature was built red → green → refactor") covering
`boids-core/src/sim.rs`, `boids-core/tests/cross_process.rs`, and
`boids-core/examples/state_hash.rs`: seeded initial state, the double-buffered step
(**AC-18**), `dt` convergence (**AC-19**), batch equivalence across a serialize/deserialize
boundary (**AC-20**), the stability invariants (**AC-21**), backend equivalence over a run
(**AC-10**), cross-process determinism (**AC-22**), and parameter validation.

Every **RED** block below is real captured output from `cargo test -p boids-core sim` (or
`cargo test -p boids-core cross_process`) at the moment the test existed and the
implementation did not, trimmed to the interesting lines. Nothing here is reconstructed.

**Cycle order.** `SimState::seeded` → lossless checkpointing → **AC-18** → **AC-19** →
**AC-20** → **AC-21** → **AC-10** → **AC-22** → `validate`. The seeded state comes first
because every later cycle needs a populated flock to step. AC-18 comes before everything
else about `step`, because double buffering is not a property that can be retrofitted:
once the integration is written in place, every subsequent test is written against wrong
numbers and the whole log would be evidence of the wrong thing.

---

### AC-D0 — a seeded starting state is reproducible, distinct per seed, and in bounds

**RED** — `seeded_is_reproducible_for_the_same_seed`, `seeded_differs_for_different_seeds`,
`seeded_starts_every_agent_inside_the_world`, `seeded_gives_every_agent_a_distinct_id`,
`seeded_state_survives_a_json_round_trip`

```
error[E0425]: cannot find type `SimState` in this scope
  --> boids-core/src/sim.rs:75:19
   |
75 |         let back: SimState = serde_json::from_str(&json).expect("deserialize");
   |                   ^^^^^^^^ not found in this scope

error[E0433]: failed to resolve: use of undeclared type `SimState`
  --> boids-core/src/sim.rs:12:17
   |
12 |         let a = SimState::seeded(&params, 0xB01D_5EED);
   |                 ^^^^^^^^ use of undeclared type `SimState`

...

error: could not compile `boids-core` (lib test) due to 8 previous errors; 1 warning emitted
```

**GREEN** — `SimState { tick, agents }` plus `seeded`: ids `0..agent_count`, positions
drawn uniform over the world and passed through `World::wrap`, velocities drawn from the
square `[-max_speed, max_speed]²` and clamped with `Vec2::limit`.

**REFACTOR** — None needed on the shape, but two deliberate decisions are worth recording:

* **No trigonometry.** The obvious spelling of a random velocity is `(speed·cos θ,
  speed·sin θ)`. It was rejected: `sin`/`cos` are not guaranteed bit-identical across
  platforms and libm versions, and this value is hashed. `forces.rs` refuses trigonometry
  in its tie-break for the same reason. Drawing the components directly and clamping is
  pure arithmetic, so the seeded state is reproducible everywhere, which is what AC-22
  actually demands.
* **`wrap` is not redundant.** `Rng::range(0.0, width)` is half-open in exact arithmetic,
  but `width * u` for `u` a hair under 1 can *round up* to exactly `width`. The half-open
  `[0,size)` bound is a hard contract (every spatial-hash bucket index depends on it), so
  the wrap stays.

---

### AC-20 (a) — a checkpointed `SimState` survives JSON **exactly**

This cycle was not planned. It was found by `seeded_state_survives_a_json_round_trip`
failing against an implementation that derived `Serialize`/`Deserialize` in the obvious
way, and it is the single most important discovery in this module: without it AC-20 is
unachievable and every resumed run would read as a divergence.

**RED** — `seeded_state_survives_a_json_round_trip`

```
thread 'sim::tests::seeded_state_survives_a_json_round_trip' panicked at boids-core/src/sim.rs:154:9:
assertion `left == right` failed
  left:  ... Agent { id: 1, pos: Vec2 { x: 34.01178883045828, y: 46.93292329267243 },
                     vel: Vec2 { x: 0.6835730415668584, y: 0.9099685516591491 } } ...
  right: ... Agent { id: 1, pos: Vec2 { x: 34.01178883045828, y: 46.93292329267243 },
                     vel: Vec2 { x: 0.6835730415668584, y: 0.9099685516591492 } } ...

failures:
    sim::tests::seeded_state_survives_a_json_round_trip

test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 181 filtered out
```

Read the last digit: `...491` went out, `...492` came back. Eleven of the eighty agents
came back changed by one ULP.

**Diagnosis.** Not our arithmetic. `serde_json` *writes* floats with `ryu` (shortest,
exact), but with default features it *parses* them with a fast best-effort algorithm that
can be one ULP off; exact rounding lives behind the non-default `float_roundtrip` feature.
`serde_json` is a dev-dependency of this crate and its features are set in the workspace
manifest, which this module does not own — and in any case a checkpoint format whose
exactness depends on a downstream crate's feature flags is a trap, not a contract.

**GREEN** — `SimState` gained `#[serde(into = "WireState", from = "WireState")]`. The wire
form carries each coordinate as a decimal **string** written with `{:?}` (shortest
round-tripping decimal) and read back with `f64::from_str` (correctly rounded), so the pair
is an exact identity independent of the JSON library's float precision.

**REFACTOR** — Added `json_round_trip_is_exact_for_awkward_float_values`, which round-trips
600 agents built from random *bit patterns* (reaching subnormals and full-length mantissas
that no decimal literal would produce) plus hand-picked awkward cases — `-0.0`,
`f64::MIN_POSITIVE`, `f64::MAX`, `0.1 + 0.2` — and compares `to_bits()`, not `==`. That is
the level the state hash reads at, so it is the level the assertion is written at. Also
folded the per-field `String` allocation out of the serializer with
`collect_str(&format_args!(...))`. Tests stayed green.

**Consequences worth knowing** (the workflow agent will hit this):

| | |
|---|---|
| A JSON number is **not** a lossless `f64` channel | under `serde_json`'s default features |
| A decimal string **is** | `{:?}` out, `from_str` in |
| A bonus | a string can carry a non-finite value; a JSON number becomes `null` |

---

### AC-18 — the step is double-buffered, so agent array order cannot matter

The highest-value test in the module, and the one written before any line of `step`.

**RED (1/2)** — `stepping_is_independent_of_agent_array_order`,
`order_independence_survives_a_long_run`, `step_leaves_the_input_state_untouched`,
`step_advances_the_tick_by_one`, `step_preserves_identity_and_slice_order`

```
error[E0425]: cannot find function `step` in this scope
   --> boids-core/src/sim.rs:388:25
    |
388 |         let reference = step(&state, &params);
    |                         ^^^^ not found in this scope

...

error: could not compile `boids-core` (lib test) due to 9 previous errors
```

**RED (2/2) — the important one.** A missing function failing to compile proves nothing
about double buffering. So `step` was then written *deliberately wrong*, in the obvious
in-place spelling — clone the array, then walk it updating each agent where it sits:

```rust
let mut agents = state.agents.clone();
for i in 0..agents.len() {
    let neighbors = neighbors_naive(&agents, world, i, params.neighbor_radius);
    let acc = blend(params, &agents, world, i, &neighbors);
    let vel = agents[i].vel.add(acc.scale(params.dt)).limit(params.max_speed);
    agents[i].vel = vel;                                     // <- leaks into
    agents[i].pos = world.wrap(agents[i].pos.add(vel.scale(params.dt))); // this tick
}
```

That version passes every *other* test in the file — the tick advances, identity is
preserved, the input is not mutated, the state changes — and AC-18 catches it immediately:

```
---- sim::tests::order_independence_survives_a_long_run stdout ----
thread 'sim::tests::order_independence_survives_a_long_run' panicked at boids-core/src/sim.rs:446:13:
assertion `left == right` failed: orderings diverged at tick 1
  left: 15708240784395354266
 right: 1997548141020277569

---- sim::tests::stepping_is_independent_of_agent_array_order stdout ----
thread 'sim::tests::stepping_is_independent_of_agent_array_order' panicked at boids-core/src/sim.rs:420:13:
assertion `left == right` failed: permutation 0 changed the state hash
  left: 15363147296347811790
 right: 3625145393156752263

failures:
    sim::tests::order_independence_survives_a_long_run
    sim::tests::stepping_is_independent_of_agent_array_order

test result: FAILED. 9 passed; 2 failed; 0 ignored; 0 measured; 181 filtered out
```

Diverged at **tick 1**, on the very first permutation. That is the AC doing its job.

**GREEN** — `step` reads `&state.agents` and `map`s it into a fresh `Vec<Agent>`, so every
agent is steered by the old flock. Borrowck now enforces what the test asserts: with
`agents` held as a shared reference there is no way to write into it.

**A second cause, found by the same test.** Double buffering alone was *not* enough.
`stepping_is_independent_of_agent_array_order` still failed, because neighbour queries
return **slice indices** in ascending order — and a permuted array turns the same
neighbour *set* into a different neighbour *sequence*. `f64` addition is not associative,
so `forces::blend` summed them to a value differing in the last bits. The fix is one line
in `step`:

```rust
scratch.sort_unstable_by_key(|&j| (agents[j].id, j)); // identity order, not slice order
```

Deleting that line to check it is load-bearing rather than decorative reproduces the
failure exactly, with the long-run test still passing and only the permutation tests
falling over:

```
test sim::tests::order_independence_survives_a_long_run ... FAILED
test sim::tests::stepping_is_independent_of_agent_array_order ... FAILED
```

The line went back in. This is the design decision the module turns on: **the neighbour
fold order is part of the reproducibility contract, and it must be keyed on identity, not
on position in an array.** `forces.rs` was already built for it — its coincident-agent
tie-break hashes the unordered pair of *ids* precisely so the step can be
permutation-invariant.

**REFACTOR** — Three things, all still green afterwards:

* Hoisted the neighbour buffer out of the loop (`scratch.clear()` + `extend`) so a tick
  does one allocation instead of one per agent.
* Built the `SpatialHash` **once per tick**, before the loop, from the old positions —
  both the fast thing and the correct thing, since a grid rebuilt mid-tick would describe
  a half-moved flock. `NeighborBackend::Naive` builds none at all.
* `tick.saturating_add(1)` rather than `+ 1`, so a run that somehow reaches `u32::MAX`
  stops counting instead of wrapping to zero and looking like a fresh run.

Also fixed a weak assertion in the test itself: the "this permutation is not the identity"
guard originally compared only `agents[0].id` and tripped on seed 2, where slot 0 happened
to keep its occupant. It now compares the whole id vector.

---

### AC-19 — refining `dt` converges on one trajectory instead of changing the answer

**RED** — `halving_dt_converges_rather_than_changing_the_answer`,
`dt_scales_the_force_so_a_finer_step_is_not_a_slower_simulation`

The test runs the same scenario to the same *simulated time* at four step sizes, so it
needs the batch runner. That is where `run_batch` enters the module:

```
error[E0425]: cannot find function `run_batch` in this scope
   --> boids-core/src/sim.rs:657:17
    |
657 |                 run_batch(&smooth_state(), &params, ticks, 0).0
    |                 ^^^^^^^^^ not found in this scope

error: could not compile `boids-core` (lib test) due to 3 previous errors
```

**GREEN** — `run_batch` loops `step` and samples metrics; the `dt` scaling itself was
already in `step` from the AC-18 cycle, since the integrator had to do *something*.

**Does the test have teeth?** A test that passes the moment it compiles is not evidence.
So the scaling was removed — `me.vel.add(acc)` instead of `me.vel.add(acc.scale(params.dt))`
— and both assertions fail loudly:

```
---- sim::tests::halving_dt_converges_rather_than_changing_the_answer stdout ----
refinement 0 did not converge: gap grew from 1.0868461923522277e1 to 1.816728959007664e1
  (all gaps: [10.868461923522277, 18.16728959007664, 24.22383803708826])

---- sim::tests::dt_scales_the_force_so_a_finer_step_is_not_a_slower_simulation stdout ----
the two step sizes describe different journeys, not the same one at different resolutions:
coarse travelled 17.789960, fine travelled 71.039400, a relative difference of 2.993
```

The gaps **grow** — 10.9 → 18.2 → 24.2 — which is the signature of `dt` being a strength
knob rather than a resolution knob: the 64-tick run accelerates eight times as often as
the 8-tick one and ends up four times further along. The scaling went back in. With it:

```
all gaps: [0.532762147233567, 0.26410331321139185, 0.1315141468800961]
ratios:    0.496, 0.498
```

Halving the step halves the disagreement — first-order convergence, exactly what
semi-implicit Euler should show.

**REFACTOR** — Two things the first draft of the test got wrong, both about *measuring the
right thing*:

* **The scenario had to be made smooth.** Convergence order is meaningless across a
  discontinuity, and the kernel has several: the neighbour radius (the set changes), the
  separation radius (a `1/d` law with a step at its edge), the obstacle influence band, and
  `limit` at the speed and force caps. `smooth_params` switches every one of them off —
  neighbour radius wider than the world, caps out of reach, no separation, no obstacles —
  leaving alignment, cohesion and goal seeking, which are a smooth field. This is the
  difference between a test that measures integration error and one that measures where
  agents happened to cross a threshold.
* **The second assertion was rewritten to name its own property.** It first compared final
  positions with a 5% tolerance and failed at 8.7% — which was ordinary first-order
  truncation error, not a bug, so the test was wrong rather than the code. It now compares
  *distance covered* at the two step sizes, which is the quantity the defect actually
  wrecks (3.0 relative difference when broken, 0.087 when correct). Same intent, and now
  the number it asserts on is the number the bug moves.

---

### AC-20 — batch equivalence: **the contract that permits checkpointing at all**

The durable workflow keeps only a cursor in its history and reloads the flock from storage
at the start of every batch. That is sound only if cutting a run into batches — with a
serialize/deserialize at each seam — is *unobservable*. This is where that is established.

**RED** — `one_long_batch_equals_ten_checkpointed_batches`,
`batching_is_equivalent_for_every_way_of_cutting_a_run`,
`metrics_are_sampled_on_the_absolute_tick_not_the_batch_offset`,
`a_batch_never_reports_the_state_it_was_handed`,
`zero_ticks_is_a_no_op_and_zero_stride_collects_nothing`, `run_batch_agrees_with_stepping_by_hand`

These went green the moment they compiled, because `run_batch` had been written one cycle
earlier for AC-19 and the losslessness fix was already in. **A test that has never been
observed failing is not evidence**, so the red was produced by reintroducing each of the
two defects the contract stands on, one at a time.

**Teeth check 1 — the checkpoint round trip.** Drop the `#[serde(into/from)]` attribute so
`SimState` derives serde in the obvious way, i.e. floats as JSON numbers:

```
---- sim::tests::one_long_batch_equals_ten_checkpointed_batches stdout ----
assertion `left == right` failed: 1x1000 and 10x100-with-checkpoints reached different states
  left: 13867457747365382359
 right: 8350065581472483383

test sim::tests::batching_is_equivalent_for_every_way_of_cutting_a_run ... FAILED
test sim::tests::one_long_batch_equals_ten_checkpointed_batches ... FAILED
```

One ULP per checkpoint, ten checkpoints, and the run has visibly forked. This is the AC-20
(a) discovery arriving exactly where it matters, and it is the reason that cycle exists.

**Teeth check 2 — absolute vs batch-local metric sampling.** Sample on an offset counted
from the start of each batch (`for offset in 1..=ticks { if offset % metrics_every == 0 }`)
instead of on the absolute tick:

```
assertion `left == right` failed: batches of 1 produced a different metric series
  left: []
 right: [(10, FrameMetrics { polarization: 0.0994..., collisions: 3, ... }), (20, ...), ... (240, ...)]
```

Batches of one tick collect **nothing at all** — an offset of 1 is never a multiple of the
stride of 10 — while the single-batch run collects 24 samples. Note that
`one_long_batch_equals_ten_checkpointed_batches` *still passed* under this defect, because
a stride of 1 samples every tick either way. That is precisely why
`batching_is_equivalent_for_every_way_of_cutting_a_run` exists: one split and one stride
are not enough to pin the property.

**GREEN** — Both properties restored: the string wire format, and

```rust
if metrics_every > 0 && current.tick.is_multiple_of(metrics_every) {
```

keyed on `current.tick`, the **absolute** tick, so where the batch boundaries fall cannot
move where the samples fall.

**REFACTOR** — None needed on the implementation. The tests gained two decisions worth
recording:

* **A batch reports only the states it produces**, never the state it was handed
  (`a_batch_never_reports_the_state_it_was_handed`, asserting ticks `[1,2,3,4,5]`). Emitting
  the input state too would duplicate a row at every seam, which the persistence layer's
  `UNIQUE (run_id, tick)` constraint would reject — turning a modelling slip into a
  production error.
* **Eight different partitions of 240 ticks**, including uneven ones (1, 2, 3, 7, 16, 60,
  120, 240) that do not divide the total, all compared against one 240-tick run. Batch size
  is a tuning decision, so it must not be a modelling one.

The scenario used is deliberately unpleasant — 90 agents in a 120x120 world with a goal
pulling them into a pile, ending at ~215 simultaneous collisions and a mean
nearest-neighbour distance of 1.3. That means the run spends most of its 1000 ticks inside
`forces.rs`'s coincident-agent tie-break, so the equivalence is asserted over exactly the
states where a non-deterministic implementation would come apart.

---

### AC-21 — stability invariants hold on every tick of a long adversarial run

**RED** — `a_long_adversarial_run_never_breaks_its_invariants`,
`the_adversarial_scenario_actually_stresses_the_clamps`,
`an_absurdly_coarse_timestep_still_produces_a_finite_world`,
`a_flock_stacked_on_a_single_point_stays_finite`

```
error[E0063]: missing fields `collision_radius` and `goal_arrival_radius` in initializer of `SimParams`
error: could not compile `boids-core` (lib test) due to 1 previous error
```

Then, with the scenario compiling, the coverage guard failed rather than the invariant —
the first honest signal that the "adversarial" scenario was not adversarial:

```
thread 'sim::tests::the_adversarial_scenario_actually_stresses_the_clamps' panicked:
no agent is speed-clamped; scenario is tame
```

**GREEN** — The invariants themselves passed from the start: `Vec2::limit` and `World::wrap`
are total, and `forces::blend` already clamps. The work in this cycle was making the test
capable of failing.

**Does the test have teeth?** Three separate defects were injected:

1. Drop `.limit(params.max_speed)` from the velocity update:

```
crowded tick 1: agent 0 exceeded max_speed: 2.0377890745801066 > 1
```

2. Drop `world.wrap(...)` from the position update — **this one initially passed**, which
   was the most useful result in the cycle. See below.

3. Both scenarios run 200 ticks × 120 agents = 24,000 agent-ticks each, and every
   assertion is made inside the tick loop, so an invariant that breaks at tick 3 is
   reported at tick 3.

**REFACTOR — three findings, each of which changed the test rather than the code:**

* **The scenario settled, so it stopped testing anything.** The first `adversarial()` had
  `max_speed: 5.0` and a heavy goal weight; the flock collapsed onto the goal within a few
  dozen ticks and then sat almost still, giving 260 speed-clamped agent-ticks out of
  24,000. Retuned so `max_force * dt` (1.5) comfortably exceeds `max_speed` (1.0) — now
  almost any force saturates the clamp, and the invariant is asserted where it actually
  binds.
* **`the_adversarial_scenario_actually_stresses_the_clamps` counts across the whole run**,
  not at the end. Its first version inspected only the final frame, which for a settling
  flock is the calmest frame in the run. It now requires >1,000 speed-clamped agent-ticks,
  >1,000 force-clamped agent-ticks, and >1,000 colliding pairs (~10 overlapping pairs on
  every tick), so the coincident-agent tie-breaks in `forces.rs` are continuously live.
* **The in-bounds assertion was vacuous, and injecting a bug proved it.** Deleting
  `world.wrap` from `step` left the whole test **passing**: the crowded flock collapses to
  the middle of a 40x40 world and never reaches an edge, so nothing ever needed wrapping.
  A second scenario, `cruising` — no goal to fall into, alignment weight 200, so the flock
  picks a heading and laps the world — was added, plus a counter of seam crossings and a
  guard that at least 100 occur. With it, the same injected bug now fails immediately:

```
cruising tick 1: agent 3 left the world at Vec2 { x: 38.116578394002495, y: -0.050913330069493445 }
```

  This is the cycle's real lesson: an invariant test over a scenario that cannot violate
  the invariant is not a test. The guard that the scenario reaches the states it claims to
  is as load-bearing as the assertion itself.

---

### AC-10 — both neighbour backends produce the same run, not merely the same query

**RED** — `both_neighbour_backends_produce_the_same_run`,
`the_backends_agree_across_a_range_of_scenario_shapes`,
`switching_backend_is_the_only_difference_between_the_two_runs`

```
error: invalid suffix `PT_1M15E_u64` for number literal
error: could not compile `boids-core` (lib test) due to 1 previous error
```

Once compiling, these passed immediately — and that is the honest and expected result.
`neighbors.rs` already establishes exact set equality per query (AC-9), and `step` folds
neighbours in identity order, so equality over a run *follows*. AC-10's job here is not to
discover the property; it is to assert that the per-query equality **compounds correctly
over 400 ticks of feedback**, where a single disagreement on a single tick would fan out
through the whole flock. It is regression protection for the optimisation, which is
precisely what an AC of this shape should be.

**Does the test have teeth?** A behavioural difference was injected into the spatial-hash
path only — capping its neighbour list at twelve entries, a plausible shape for a
"just prune the far ones" optimisation gone wrong:

```rust
if params.backend == NeighborBackend::SpatialHash && scratch.len() > 12 {
    scratch.truncate(12);
}
```

Both tests fail at once:

```
---- sim::tests::both_neighbour_backends_produce_the_same_run stdout ----
assertion `left == right` failed: the two backends produced different runs
  left: 3014713271764420426
 right: 8330947738971960962

---- sim::tests::the_backends_agree_across_a_range_of_scenario_shapes stdout ----
assertion `left == right` failed: backends diverged in a 200x200 world, radius 25, 80 agents
```

**GREEN / REFACTOR** — No implementation change was needed. Two things were added to the
tests instead:

* **Five scenario shapes, not one.** The grid's cell count is derived from the world
  dimensions and the radius, so those are exactly the axes along which a spatial hash
  fails: a long thin world (300x12), a radius wider than the world (which collapses the
  grid to a single cell), a fine grid with a dense flock, and a tiny 10x10 world with a
  0.5 radius. A single default-shaped scenario would test one cell geometry and call it
  proof.
* **A guard that the comparison is real.**
  `switching_backend_is_the_only_difference_between_the_two_runs` asserts the two
  `SimParams` differ in the `backend` field and *nothing else* — otherwise a copy-paste
  slip could leave the test comparing a run against itself and passing forever.

---

### AC-22 — the same scenario and seed hash identically in a **separate OS process**

**RED** — `the_same_scenario_hashes_identically_in_a_separate_process`
(`boids-core/tests/cross_process.rs`)

```
child process failed: exit status: 101
error: no example target named `state_hash` in `boids-core` package

failures:
    the_same_scenario_hashes_identically_in_a_separate_process

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

**GREEN** — Added `boids-core/examples/state_hash.rs`: it takes a tick count and a list of
seeds on the command line, runs `SimState::seeded` + `run_batch` over
`SimParams::default()`, and prints one hex hash per line. The test computes the same hashes
in-process, spawns `cargo run --quiet --example state_hash -p boids-core`, and compares.

It really runs here — 4 seeds compared, no skip taken:

```
running 1 test
test the_same_scenario_hashes_identically_in_a_separate_process ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo run --quiet --example state_hash -p boids-core -- 250 0 1 13195404531918017076 18446744073709551615
18ae2e5de42a28e0
f8c69f0714bb7c6e
601427375f0f9940
a0aa66bdd44ffb84
```

**Does the test have teeth?** The defect this AC exists for is state that is stable within
a process but varies between them, so exactly that was injected — one character's worth,
in `SimState::seeded`:

```rust
let mut rng = Rng::seeded(seed ^ u64::from(std::process::id()));
```

```
assertion `left == right` failed: seed 0 hashed as 2c993eb3df2a27cf in this process
and 96852d3f7b3f206f in a separate one, after 250 ticks
```

Note that **every in-process test in the crate still passes** under that injection — 189 of
them — because within one process the hash is perfectly stable. This is the one test in the
suite that can see the difference, which is the whole argument for its existence.

**REFACTOR** — Four decisions, recorded because they are what makes the test meaningful
rather than ceremonial:

* **The test owns the experiment, the example is a dumb executor.** Ticks and seeds are
  passed as arguments rather than duplicated as constants on both sides. Two copies of a
  scenario definition drift, and a drifted copy makes this test compare two different
  experiments and pass.
* **Four seeds in one spawn**, including `0` and `u64::MAX`. One seed could agree by luck
  if a defect touched only part of the state space; batching keeps the cost at a single
  process spawn.
* **250 ticks, not one.** Per-process nondeterminism has to be given hundreds of
  opportunities to compound, rather than being rounded away in the first frame.
* **A vacuity guard.** The four hashes must be *distinct* from each other. Without it, a
  degenerate scenario — an empty flock, say — would make every seed agree trivially and the
  test would pass while proving nothing.

The skip path (`cargo` unavailable) prints a loud `SKIPPED:` to stderr and returns, so a
packaged environment without a toolchain does not fail the suite — but a skip is visible
rather than silent.

---

### `validate` — reject nonsense parameters, and report **every** fault at once

**RED** — `validate_accepts_the_default_parameters`,
`validate_accepts_every_scenario_this_module_simulates`,
`validate_rejects_a_world_with_no_area`, `validate_rejects_a_non_positive_timestep`,
`validate_rejects_an_empty_flock`,
`validate_rejects_negative_or_non_finite_distances_and_limits`,
`validate_rejects_non_finite_weights`,
`validate_rejects_a_separation_radius_wider_than_the_neighbourhood`,
`validate_rejects_degenerate_obstacles_and_names_which_one`,
`validate_rejects_a_non_finite_goal`, `validate_reports_every_problem_at_once`,
`validation_messages_say_what_is_wrong_and_what_is_required`,
`a_validated_scenario_actually_runs`

```
error[E0425]: cannot find function `validate` in this scope
error[E0425]: cannot find function `validate` in this scope
error[E0425]: cannot find function `validate` in this scope
error[E0425]: cannot find function `validate` in this scope
error[E0425]: cannot find function `validate` in this scope

error: could not compile `boids-core` (lib test) due to 7 previous errors
```

**GREEN** — `validate(&SimParams) -> Result<(), Vec<String>>`, accumulating into a `Vec`
and returning `Ok(())` only when it is empty. Each check pushes a message naming the field,
quoting the value, and stating the rule.

**REFACTOR** — None needed on the implementation; the tests carry the decisions.

**What is deliberately *not* rejected**, each asserted as an explicit acceptance so a later
"tightening" cannot quietly remove a legitimate experiment:

| Accepted | Why |
|---|---|
| A **negative weight** | Inverts a behaviour — agents that flee the flock rather than join it. `forces.rs` is property-tested over negative weights precisely because they are legal. |
| **Zero** radius, speed or force | Switches that behaviour off, which is how a scenario isolates the others. |
| `separation_radius == neighbor_radius` | The whole neighbourhood repels. Only *greater than* is a contradiction. |
| `goal: None` | A scenario without a goal, not a missing field. |

**Three test decisions worth recording:**

* **Every field is named individually.** `validate_rejects_negative_or_non_finite_distances_and_limits`
  drives six fields × three bad values through a table of setters, so a validator that
  checks four of the six cannot pass by accident — which a single "reject a bad params
  struct" test would let through.
* **Obstacles are reported by index**, and the test asserts `obstacle 1`, `obstacle 2` and
  `obstacle 3` are named while the valid `obstacle 0` is *not*. "An obstacle is invalid" is
  not actionable in a scenario with forty of them.
* **The messages themselves are asserted**, not just the count
  (`validation_messages_say_what_is_wrong_and_what_is_required`): each message must exceed
  20 characters, state a requirement, and quote the offending value — the `dt` message must
  literally contain `-0.5` and the word `must`. This is the difference between validation a
  user can act on and a red box saying "invalid parameter".

Two tests tie validation back to the rest of the module rather than leaving it as an
isolated string generator:

* `validate_accepts_every_scenario_this_module_simulates` — all four scenarios used by the
  AC tests above must validate, so `validate` cannot drift away from what the kernel
  actually runs.
* `a_validated_scenario_actually_runs` — anything `validate` accepts must survive a real
  60-tick run with the AC-21 invariants intact. Acceptance has to mean something.

**Why this is a usability feature and not a safety one:** every kernel function is total,
so invalid parameters produce a finite, deterministic, *meaningless* run rather than a panic
or a `NaN`. Nothing here prevents a crash. It exists to tell a user that their scenario does
not say what they think it says — which is why the wording of the messages is tested as
carefully as the conditions.

---

## Final state

```
running 220 tests
test result: ok. 220 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.26s   # lib
test result: ok.   1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out                      # cross_process
test result: ok.  12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out                      # purity
```

`cargo clippy -p boids-core --all-targets -- -D warnings` is clean.

**The recurring lesson of this module**, in the order it was learned: a test that has never
been observed failing is not evidence. Five separate times — the in-place step, the missing
identity sort, the unscaled force, the derived serde, the batch-local metric stride, the
vacuous in-bounds assertion — the defect was injected deliberately to confirm the test could
see it. Twice that exercise found the *test* to be wrong rather than the code (AC-19's
tolerance, AC-21's scenario coverage), which is exactly the value of doing it.
