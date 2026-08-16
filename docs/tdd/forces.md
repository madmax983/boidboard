# TDD log — steering forces

Evidence for **AC-52** ("every feature was built red → green → refactor") covering
`boids-core/src/forces.rs`: the five steering behaviours (separation, alignment, cohesion,
goal seeking, obstacle avoidance) and the weighted blend.

Every **RED** block below is real captured output from `cargo test -p boids-core forces` at
the moment the test existed and the implementation did not. Nothing here is reconstructed.

**Cycle order.** AC-11 → AC-12 → AC-13 → AC-14 → AC-15 → AC-16 → AC-17, i.e. numeric order,
with AC-11 and AC-15 each split into two cycles: the ordinary behaviour first, then the
degenerate case (coincident agents / an agent exactly at an obstacle's centre). Splitting
them is what makes the log worth reading — the degenerate tests fail *against the working
ordinary implementation*, which is the failure that matters, and a single combined cycle
would have hidden it behind a "function does not exist" compile error.

**Three cycles carry a MUTATION CHECK, and two have no RED at all.** Where an acceptance
criterion exists to catch a *plausible wrong* implementation (AC-13's naive coordinate mean),
a compile-error RED proves nothing about that; and by the time AC-17 and the robustness
property were written, earlier cycles had already forced the behaviour into the code, so
their tests passed on the first run. Both situations are recorded as they happened rather
than dressed up: in each, the correct line was temporarily replaced with the wrong one and
the suite re-run, and that captured failure is the real evidence the test discriminates. The
mutation was reverted immediately afterwards each time.

---

### AC-11 (1/2) — separation steers away from close neighbours

**RED** — `separation_pushes_away_from_a_close_neighbour`,
`separation_ignores_a_neighbour_outside_the_radius`,
`separation_pushes_harder_the_closer_the_neighbour`, `separation_crosses_the_seam`

```
error[E0425]: cannot find function `separation` in this scope
  --> boids-core/src/forces.rs:53:17
   |
53 |         let f = separation(&agents, &w100(), 0, &[1], 5.0);
   |                 ^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `separation` in this scope
  --> boids-core/src/forces.rs:64:17
   |
64 |         let f = separation(&agents, &w100(), 0, &[1], 5.0);
   |                 ^^^^^^^^^^ not found in this scope

error: could not compile `boids-core` (lib test) due to 5 previous errors; 3 warnings emitted
```

**GREEN** — Accumulate, in the given slice order, one contribution per neighbour strictly
inside `separation_radius`: the unit vector from the neighbour to this agent, scaled by
`1/distance`. Every difference goes through `World::displacement`, so the seam test passes
for the right reason rather than by accident.

**REFACTOR** — The radius test was first written as `if !(distance < separation_radius) {
continue; }`, which clippy rejects (`neg_cmp_op_on_partial_ord`). Inverting it to a positive
`if distance < separation_radius { … }` is both clippy-clean and *more correct*: a `NaN`
distance now falls outside the radius instead of inside it. Tests stayed green.

---

### AC-11 (2/2) — coincident agents produce a finite, deterministic force

**RED** — `separation_of_coincident_agents_is_finite`,
`separation_of_coincident_agents_is_deterministic`,
`coincident_agents_are_pushed_in_opposite_directions`

```
---- forces::tests::separation_of_coincident_agents_is_finite stdout ----
thread 'forces::tests::separation_of_coincident_agents_is_finite' panicked at
boids-core/src/forces.rs:123:9:
produced NaN: Vec2 { x: NaN, y: NaN }

---- forces::tests::separation_of_coincident_agents_is_deterministic stdout ----
thread 'forces::tests::separation_of_coincident_agents_is_deterministic' panicked at
boids-core/src/forces.rs:138:9:
assertion `left == right` failed: tie-break is not deterministic
  left: Vec2 { x: NaN, y: NaN }
 right: Vec2 { x: NaN, y: NaN }

failures:
    forces::tests::coincident_agents_are_pushed_in_opposite_directions
    forces::tests::separation_of_coincident_agents_is_deterministic
    forces::tests::separation_of_coincident_agents_is_finite

test result: FAILED. 4 passed; 3 failed; 0 ignored; 0 measured; 91 filtered out
```

The exact shape of the bug is visible in that output. `Vec2::normalize` is total, so a
zero-length difference normalises to `ZERO` rather than exploding — and then `ZERO * (1/0)`
is `0 * inf`, which is `NaN`. The determinism failure is the same `NaN` viewed from a
different angle: `left` and `right` print identically and still compare unequal, because no
`NaN` equals itself. A totality guarantee one layer down did not save this layer.

**GREEN** — When the `1/d` weight is not finite (`d == 0`, or a subnormal separation `f64`
cannot divide by), fall back to `escape_direction`: the escape *axis* is one of four fixed
unit axes selected by a SplitMix64 avalanche over the unordered pair of agent **ids**, and
the *sign* is decided by which agent sorts lower. Magnitude is the weight a pair one
thousandth of a separation radius apart would get.

**REFACTOR** — none needed; the tie-break was extracted into `escape_direction` /
`coincident_weight` / `mix64` as it was written, and `separation` itself stayed a single
loop. Clippy clean.

Three properties of that rule are load-bearing, and each is a deliberate rejection of a
simpler alternative:

* **Keyed on ids, not slot indices.** The array order changes under the double-buffered
  step's permutation test (AC-18); identity does not. A tie-break keyed on position in the
  array would make a shuffled-then-unpermuted run diverge.
* **Signed by pair ordering, not by agent.** `escape_direction(a,b) == -escape_direction(b,a)`
  exactly, so a coincident pair flies apart. Keying the direction on the agent alone would
  give both agents the *same* direction and translate the pile intact for ever.
* **Integer arithmetic, no trigonometry.** Picking an angle with `sin`/`cos` would be
  deterministic within a process but is not guaranteed bit-identical across platforms, and
  the reproducibility contract compares state hashes bit-for-bit. A fixed table indexed by a
  wrapping-integer hash has no such exposure.

---

### AC-12 — alignment steers toward the mean neighbour heading

**RED** — `alignment_steers_toward_the_mean_neighbour_heading`,
`alignment_vanishes_once_the_agent_matches_the_flock`,
`alignment_of_antiparallel_neighbours_is_zero_not_nan`, `alignment_with_no_neighbours_is_zero`

```
error[E0425]: cannot find function `alignment` in this scope
   --> boids-core/src/forces.rs:253:17
    |                 ^^^^^^^^^ not found in this scope
error[E0425]: cannot find function `alignment` in this scope
   --> boids-core/src/forces.rs:267:20
    |                    ^^^^^^^^^ not found in this scope
error[E0425]: cannot find function `alignment` in this scope
   --> boids-core/src/forces.rs:279:17
    |                 ^^^^^^^^^ not found in this scope
error[E0425]: cannot find function `alignment` in this scope
   --> boids-core/src/forces.rs:289:13
    |             ^^^^^^^^^ not found in this scope
error: could not compile (lib test) due to 4 previous errors
```

**GREEN** — Mean of the neighbour velocities in slice order, minus the agent's own
velocity; `ZERO` when no entry is usable.

Subtracting the agent's own velocity is the decision worth recording. Alignment is a
*correction*, not a thrust: `alignment_vanishes_once_the_agent_matches_the_flock` pins that
a flock already in consensus receives no further push. Had the function returned the bare
mean, a perfectly aligned flock would be handed a constant force every tick for ever, and
would sit pinned against the `max_speed` clamp instead of cruising. The antiparallel case
needs no special handling at all — the mean is genuinely `(0,0)` and nothing divides by its
length, so `NaN` never arises.

**REFACTOR** — The skip-self / skip-out-of-range / accumulate loop was now written twice, so
it was extracted into `neighbor_fold`, which returns both the sum and the contributing
count, and `separation` was rewritten on top of it. The fold takes `(slot, &Agent)` because
separation's tie-break needs the neighbour's slot as well as the agent. The extraction also
puts the "left-to-right in the order given" summation rule in one documented place instead
of relying on two loops staying accidentally identical. Tests stayed green; clippy clean
after re-inverting the radius comparison the extraction had flipped back to
`!(distance < separation_radius)`.

---

### AC-13 — cohesion via toroidal displacement, not a coordinate mean

**RED** — `cohesion_pulls_across_the_seam_not_toward_the_middle`,
`cohesion_pulls_toward_the_neighbour_centroid`, `cohesion_across_the_seam_stays_small`,
`cohesion_with_no_neighbours_is_zero`

```
error[E0425]: cannot find function `cohesion` in this scope
   --> boids-core/src/forces.rs:356:17
    |                 ^^^^^^^^ not found in this scope
error[E0425]: cannot find function `cohesion` in this scope
   --> boids-core/src/forces.rs:369:17
    |                 ^^^^^^^^ not found in this scope
error[E0425]: cannot find function `cohesion` in this scope
   --> boids-core/src/forces.rs:378:17
    |                 ^^^^^^^^ not found in this scope
error[E0425]: cannot find function `cohesion` in this scope
   --> boids-core/src/forces.rs:390:20
    |                    ^^^^^^^^ not found in this scope
```

**GREEN** — Mean of `world.displacement(me.pos, other.pos)` over the slice, in order;
`ZERO` when nothing contributes.

**MUTATION CHECK** — A missing-function RED says nothing about whether these tests catch the
bug they exist for, so the correct line was swapped for the naive implementation
(`mean(other.pos) - me.pos`) and the suite re-run:

```
---- forces::tests::cohesion_pulls_across_the_seam_not_toward_the_middle stdout ----
thread 'forces::tests::cohesion_pulls_across_the_seam_not_toward_the_middle' panicked at
boids-core/src/forces.rs:382:9:
cohesion must pull across the seam (-x), got Vec2 { x: 98.0, y: 0.0 }

---- forces::tests::cohesion_across_the_seam_stays_small stdout ----
thread 'forces::tests::cohesion_across_the_seam_stays_small' panicked at
boids-core/src/forces.rs:404:9:
a nearby neighbour produced a huge pull (138.59292911256333), so the centroid was averaged
in raw coordinates: Vec2 { x: 98.0, y: 98.0 }

failures:
    forces::tests::cohesion_across_the_seam_stays_small
    forces::tests::cohesion_pulls_across_the_seam_not_toward_the_middle

test result: FAILED. 13 passed; 2 failed; 0 ignored; 0 measured; 54 filtered out
```

`+98.0` where `-2.0` was wanted: the naive centroid sends the agent 98 units the wrong way,
through the middle of the world, to reach a flockmate two units away. Note which test did
*not* fail — `cohesion_pulls_toward_the_neighbour_centroid`, the well-behaved case away from
any seam, passes happily under the broken implementation. A suite containing only that test
would have been fully green on a fundamentally wrong kernel. The mutation was reverted and
the suite re-run green (15 passed).

**REFACTOR** — none needed; `cohesion` is `neighbor_fold` plus a divide, and the fold
extracted during AC-12 already carried the ordering rule.

---

### AC-14 — goal seeking, toroidal, and absent when there is no goal

**RED** — `goal_seek_steers_toward_the_goal`, `goal_seek_takes_the_short_way_round_the_seam`,
`goal_seek_without_a_goal_is_zero`, `goal_seek_weakens_on_arrival`

```
error[E0425]: cannot find function `goal_seek` in this scope
   --> boids-core/src/forces.rs:424:17
    |                 ^^^^^^^^^ not found in this scope
error[E0425]: cannot find function `goal_seek` in this scope
   --> boids-core/src/forces.rs:432:17
    |                 ^^^^^^^^^ not found in this scope
error[E0425]: cannot find function `goal_seek` in this scope
   --> boids-core/src/forces.rs:440:20
    |                    ^^^^^^^^^ not found in this scope
error[E0425]: cannot find function `goal_seek` in this scope
   --> boids-core/src/forces.rs:451:13
    |             ^^^^^^^^^ not found in this scope
```

**GREEN** — `world.displacement(me.pos, target)`, or `ZERO` when the goal is `None`. A single
`let (Some(me), Some(target)) = … else` covers both the missing goal and an out-of-range
index.

Returning the displacement *unnormalised* is the design choice here. The magnitude is the
toroidal distance, so the pull fades as the agent closes in and is exactly `ZERO` on the
goal — arrival damping for free, pinned by `goal_seek_weakens_on_arrival`. A normalised
constant-magnitude pull would keep shoving at full strength on top of the goal and make
arrived agents jitter around it. The magnitude is bounded by half the world diagonal, so it
is always finite.

`None` meaning "no goal" rather than "a goal at the origin" is what lets AC-14's stronger
claim hold: with no goal, or with `w_goal = 0`, this behaviour cannot touch the blend at all.

**REFACTOR** — none needed; the function is two lines.

---

### AC-15 (1/2) — obstacle repulsion, inside and out

**RED** — `obstacle_avoidance_pushes_an_approaching_agent_away`,
`obstacle_avoidance_pushes_an_agent_inside_radially_outward`,
`obstacle_avoidance_pushes_harder_from_deeper_inside`,
`obstacle_avoidance_ignores_a_distant_obstacle`, `obstacle_avoidance_crosses_the_seam`,
`obstacle_avoidance_with_no_obstacles_is_zero`

```
error[E0425]: cannot find function `obstacle_avoidance` in this scope
   --> boids-core/src/forces.rs:492:17
    |                 ^^^^^^^^^^^^^^^^^^ not found in this scope
error[E0425]: cannot find function `obstacle_avoidance` in this scope
   --> boids-core/src/forces.rs:501:17
    |                 ^^^^^^^^^^^^^^^^^^ not found in this scope
error[E0425]: cannot find function `obstacle_avoidance` in this scope
   --> boids-core/src/forces.rs:512:22
    |                      ^^^^^^^^^^^^^^^^^^ not found in this scope
error[E0425]: cannot find function `obstacle_avoidance` in this scope
   --> boids-core/src/forces.rs:513:25
    |                         ^^^^^^^^^^^^^^^^^^ not found in this scope
error[E0425]: cannot find function `obstacle_avoidance` in this scope
   --> boids-core/src/forces.rs:522:13
```

**GREEN** — One `obstacle_push` per obstacle, summed in slice order. The push is linear
across an influence band reaching `OBSTACLE_INFLUENCE = 2` radii: zero at the outer edge,
one at the surface, and continuing past one inside, so `2` dead centre.

Two things fall out of choosing a *linear band* over the `1/d` law separation uses. The
force is bounded by construction — an agent deep inside a rock cannot generate an
astronomical push — and it is continuous at the surface, so an agent that tunnels in during
one tick does not see the force sign-flip or spike. The band is a multiple of the radius
rather than a fixed margin so that a big rock is dodged from proportionally further out.

**REFACTOR** — none needed; the per-obstacle push was extracted into `obstacle_push` as it
was written, keeping `obstacle_avoidance` a bare sum over the slice.

---

### AC-15 (2/2) — an agent exactly at the centre must still get out

**RED** — `obstacle_avoidance_at_the_exact_centre_still_pushes`,
`obstacle_avoidance_at_the_exact_centre_is_deterministic`,
`a_head_on_agent_at_the_centre_is_pushed_sideways`,
`a_motionless_agent_at_the_centre_is_still_pushed_out`

```
---- forces::tests::obstacle_avoidance_at_the_exact_centre_still_pushes stdout ----
thread 'forces::tests::obstacle_avoidance_at_the_exact_centre_still_pushes' panicked at
boids-core/src/forces.rs:617:9:
an agent dead centre must be pushed out: Vec2 { x: 0.0, y: 0.0 }

---- forces::tests::a_motionless_agent_at_the_centre_is_still_pushed_out stdout ----
thread 'forces::tests::a_motionless_agent_at_the_centre_is_still_pushed_out' panicked at
boids-core/src/forces.rs:658:9:
a stationary agent must not be stranded: Vec2 { x: 0.0, y: 0.0 }

failures:
    forces::tests::a_motionless_agent_at_the_centre_is_still_pushed_out
    forces::tests::obstacle_avoidance_at_the_exact_centre_still_pushes

test result: FAILED. 27 passed; 2 failed; 0 ignored; 0 measured; 54 filtered out
```

This RED is the argument for asserting more than "finite and deterministic" on a degenerate
case. `Vec2::normalize` is total, so the centred agent got a perfectly finite, perfectly
reproducible force of `(0,0)` — and the two tests that check only finiteness and determinism
**passed against the broken implementation**. Only `f.length() > 0.0` catches it. A silent
zero is the failure mode that matters here: the agent sits in the middle of the rock for
ever, and nothing in the run ever reports an error.

**GREEN** — When the outward direction has no length, fall through to `centre_escape`: a
moving agent is steered along the **left normal of its heading** (sideways — reversing it
along its own heading would only stall it on the axis it is stuck on), and a motionless
agent, which has no heading to turn from, takes an axis from the same integer id hash used
for coincident neighbours. Strength is the band value at distance zero, which is `2`.

**REFACTOR** — none needed; `centre_escape` was extracted as it was written. Clippy clean.
The heading test is `heading.length_squared() > 0.0` rather than `heading == Vec2::ZERO`,
which keeps a non-finite velocity (which `normalize` maps to `ZERO`) on the fallback branch
instead of trusting a float equality.

---

### AC-16 — the weighted blend, clamped

**RED** — `blend_is_the_weighted_sum_of_its_components`, `blend_is_clamped_to_max_force`,
`each_weight_moves_the_blend_along_its_own_component`, `blend_never_exceeds_max_force`

```
error[E0425]: cannot find function `blend` in this scope
   --> boids-core/src/forces.rs:761:13
    |             ^^^^^ not found in this scope
error[E0425]: cannot find function `blend` in this scope
   --> boids-core/src/forces.rs:770:17
    |                 ^^^^^ not found in this scope
error[E0425]: cannot find function `blend` in this scope
   --> boids-core/src/forces.rs:795:25
    |                         ^^^^^ not found in this scope
error[E0425]: cannot find function `blend` in this scope
   --> boids-core/src/forces.rs:796:22
    |                      ^^^^^ not found in this scope
```

**GREEN** — Call the five behaviours, scale each by its weight, sum them left to right in
the documented order, and finish with `Vec2::limit(params.max_force)`.

The equality test asserts **exact** equality against a sum written in the same order, not an
epsilon. That is deliberate: float addition is not associative, so a future refactor that
reorders the five terms — or gathers them into an array and sums it differently — changes
the last bits of every force in the run and therefore every downstream state hash. Pinning
it exactly turns a silent reproducibility break into a failing test.

The monotonicity test drives all five behaviours simultaneously (a neighbour inside the
separation radius, a goal, and an obstacle inside its influence band) with `max_force` set
to `1e9`, and first asserts every component is non-zero. Without that guard the test would
pass vacuously for any behaviour that happened to return `ZERO` in the fixture. It then
raises one weight from 1 to 2 and asserts the *change* in the blend both points along that
component (positive dot) and stays on its axis (near-zero cross product) — so a weight that
leaked into the wrong term would be caught, not just one that did nothing.

**REFACTOR** — none needed in the implementation. In the tests, clippy's `type_complexity`
rejected the inline `[(&str, fn(&mut SimParams, f64)); 5]` weight-setter table, which became
a named `SetWeight` alias, and three RNG seed literals were regrouped for
`unusual_byte_groupings`.

---

### AC-17 — zero neighbours

**RED** — none available, and this is recorded rather than manufactured.

`with_no_neighbours_the_flocking_forces_are_exactly_zero`,
`with_no_neighbours_goal_and_avoidance_still_act`,
`a_lone_agent_with_nothing_to_react_to_gets_no_force`, and
`a_neighbour_list_naming_the_agent_itself_is_ignored` were written before running anything,
and **all four passed on the first run** — the `count == 0` guards had already been forced
into `alignment` and `cohesion` by AC-12 and AC-13. Fabricating a red here would have meant
breaking working code to photograph the wreckage.

**MUTATION CHECK** — Instead, the guards were deleted and the suite re-run, which is the
honest way to show these tests hold something up:

```
---- forces::tests::with_no_neighbours_the_flocking_forces_are_exactly_zero stdout ----
assertion `left == right` failed
  left: Vec2 { x: NaN, y: NaN }
 right: Vec2 { x: 0.0, y: 0.0 }

---- forces::tests::alignment_with_no_neighbours_is_zero stdout ----
assertion `left == right` failed: an agent with no flockmates has nothing to align to, and
must not be told to brake
  left: Vec2 { x: NaN, y: NaN }
 right: Vec2 { x: 0.0, y: 0.0 }

---- forces::tests::with_no_neighbours_goal_and_avoidance_still_act stdout ----
assertion `left == right` failed: with no neighbours the blend is exactly the two task forces
  left: Vec2 { x: 0.0, y: 0.0 }
 right: Vec2 { x: 29.6, y: 0.0 }

failures:
    forces::tests::a_neighbour_list_naming_the_agent_itself_is_ignored
    forces::tests::alignment_with_no_neighbours_is_zero
    forces::tests::cohesion_with_no_neighbours_is_zero
    forces::tests::with_no_neighbours_goal_and_avoidance_still_act
    forces::tests::with_no_neighbours_the_flocking_forces_are_exactly_zero
```

Without the guard, a lone agent divides a zero sum by a zero count: `ZERO * (1/0)` is `NaN`
again. The third failure is the interesting one. `blend` reported `(0.0, 0.0)` — not `NaN` —
because `Vec2::limit` is total and quietly converts a poisoned force into a zero one. So the
bug would have surfaced in production as *agents that mysteriously stop steering*, with no
`NaN` anywhere to grep for. That is precisely why AC-17 asks for the goal and avoidance
forces to be asserted **present** alongside the three zeros, rather than only asserting the
zeros.

**GREEN / REFACTOR** — guards restored, suite green again (37 passing at that point).

The distinction the tests document: with an empty neighbour slice the three *flocking*
behaviours are exactly `Vec2::ZERO`, while goal seeking and obstacle avoidance keep acting —
they are relations between the agent and the world, not between the agent and its
flockmates. A lone agent with no goal and no obstacles receives exactly `ZERO`, so it coasts
rather than wandering.

---

### Robustness — every force finite and reproducible, over randomised flocks

**RED** — none available; like AC-17 these passed on first run, because totality was built
in from AC-11 onward. The mutation check below is the evidence.

`every_force_is_finite_for_every_random_configuration` runs 20 000 randomised worlds,
weights, radii, goals and obstacles, checking all six functions for every agent.
`every_force_is_reproducible_for_every_random_configuration` runs 2 000 more, asserting
`blend` is bit-identical when called twice. `a_pile_of_coincident_agents_is_finite_and_deterministic`
stacks five agents on one point.

The generator matters more than the iteration count. Positions are **snapped to a coarse
lattice** and a quarter of velocities are set to exactly zero, so coincident agents, agents
dead centre in an obstacle, and stationary agents occur many times per run. Sampling
positions from a continuous uniform distribution would have made every one of those cases
probability zero — the test would have looked thorough and exercised nothing degenerate.

**MUTATION CHECK** — the coincident guard in `separation` was forced to the ordinary branch:

```
---- forces::tests::every_force_is_finite_for_every_random_configuration stdout ----
thread 'forces::tests::every_force_is_finite_for_every_random_configuration' panicked at
boids-core/src/forces.rs:1055:21:
separation went non-finite: Vec2 { x: NaN, y: NaN } for agent 1 of [Agent { id: 0, pos: Vec2
{ x: 5.0, y: 35.0 }, ... }, Agent { id: 1, pos: Vec2 { x: 15.0, y: 30.0 }, ... }, Agent {
id: 2, pos: Vec2 { x: 15.0, y: 30.0 }, ... }, ...]

failures:
    forces::tests::a_pile_of_coincident_agents_is_finite_and_deterministic
    forces::tests::coincident_agents_are_pushed_in_opposite_directions
    forces::tests::every_force_is_finite_for_every_random_configuration
    forces::tests::separation_of_coincident_agents_is_deterministic
    forces::tests::separation_of_coincident_agents_is_finite

test result: FAILED. 36 passed; 5 failed; 0 ignored; 0 measured; 54 filtered out
```

Agents 1 and 2 are both at `(15, 30)` — the lattice did its job, and the property found the
collision on its own rather than being handed one. Guard restored, suite green.

---

## Final state

```
$ cargo test -p boids-core forces
running 41 tests
test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 145 filtered out
```

Whole crate at the time of writing: `cargo test -p boids-core` → **186 passed; 0 failed; 0
ignored; 0 measured; 0 filtered out**, plus 12 passing purity tests. That total moves as the
neighbour, metrics and integration modules land; the 41 above are this module's.
`cargo clippy -p boids-core --all-targets -- -D warnings` → clean.

### Degenerate-case tie-break rules, in one place

| Case | Rule | Why it is deterministic |
|---|---|---|
| Two coincident agents | Escape along one of four fixed unit axes, chosen by a SplitMix64 hash of the unordered pair of **ids**; sign from which agent sorts lower | Integer arithmetic only; no RNG, no trig, no array order. Antisymmetric, so the pair separates |
| Agent dead centre in an obstacle, moving | Left normal of its own heading | A fixed choice of side, derived from state already in the frame |
| Agent dead centre in an obstacle, at rest | Fixed axis from a SplitMix64 hash of its **id** | Same integer path as the coincident case |
| Distance too small to divide by | Treated as coincident | The `1/d` weight is tested with `is_finite()`, so subnormal separations take the same branch as exact zeros |

No rule consults an RNG, a clock, an address, or the order agents happen to sit in the
array, and none uses `sin`/`cos`, whose cross-platform bit-exactness is not guaranteed.
