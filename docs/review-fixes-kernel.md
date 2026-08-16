# Adversarial review fixes — `boids-core`

Every finding below was reproduced before it was fixed, and every new test was
run against the defect it guards. Where the kernel was already correct, the
defect was re-introduced deliberately, the failure captured, and the mutation
reverted — the standard the rest of this crate was built to.

All output in this document is real terminal output, pasted unedited.

**Scope.** `boids-core/**` only. Nothing in the `boidboard` crate, and no
`Cargo.toml`, was touched. No new dependencies.

## Result

| | before | after |
|---|---|---|
| `cargo test -p boids-core` (lib) | 220 passed | **238 passed** |
| `cargo test -p boids-core` (cross_process) | 1 passed | **1 passed** |
| `cargo test -p boids-core` (purity) | 12 passed | **12 passed** |
| `cargo clippy -p boids-core --all-targets -- -D warnings` | clean | **clean** |

```
$ cargo test -p boids-core
test result: ok. 238 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.78s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.43s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo clippy -p boids-core --all-targets -- -D warnings
    Checking boids-core v0.1.0 (/home/user/boidboard/boids-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.82s
```

`cargo check -p boidboard` still compiles against the changed kernel.

## Summary

| ID | Verdict | Where |
|---|---|---|
| H1 end-to-end reproducibility fingerprint unpinned | **FIXED** | `sim.rs` |
| M1 AC-21 asserts stored speed, not distance travelled | **FIXED** | `sim.rs` |
| M2 AC-18 never exercises coincident agents | **FIXED** | `sim.rs` |
| M3 `obstacle_avoidance` returns `NaN` for an accepted radius | **FIXED** | `forces.rs`, `sim.rs` |
| M4 `mean_nearest_neighbor_distance` can be `+inf` | **FIXED** | `metrics.rs` |
| M5 stale-grid test is vacuous | **FIXED** | `neighbors.rs` |
| L1 `toroidal_centroid` uses non-portable trigonometry | **DOCUMENTED + guarded by a new test** | `metrics.rs`, `sim.rs` |
| L2 `displacement` can exceed half the world | **DOCUMENTED + pinned by two new tests** | `world.rs` |
| L4 collision-count bound test is structurally vacuous | **FIXED (replaced)** | `metrics.rs` |
| L5 separation's exclusive radius undocumented/untested | **FIXED** | `forces.rs` |
| AC-14 zero-goal-weight clause had no test | **FIXED** | `sim.rs` |

---

## H1 — nothing pinned `run_batch(params, seed) -> hash` — FIXED

### The gap

`state_hash` is persisted (`runs.final_state_hash`, `frames.state_hash`), shown
in the UI, and AC-38 promises re-running reproduces it. The only golden
constants were in `hash.rs`, over a hand-built four-agent flock: they pin the
*hash function*, not the *pipeline*. `tests/cross_process.rs` runs the same code
in both processes, so it cannot detect a deliberate arithmetic change either.

### Verification of the reviewer's constants

Recomputed from the crate's own example binary before pinning anything, rather
than trusted:

```
$ cargo run --quiet --release --example state_hash -p boids-core -- 250 0 1
18ae2e5de42a28e0
f8c69f0714bb7c6e
```

Both match the reviewer's values exactly. The three fixture hashes were then
computed the same way (temporary print test, since the fixtures are private to
the `sim` test module) and removed again.

### Constants pinned

`sim::tests::golden_runs`, all after **250 ticks** with `metrics_every = 0`:

| scenario | seed | final `state_hash` |
|---|---|---|
| `SimParams::default()` | `0` | `0x18ae_2e5d_e42a_28e0` |
| `SimParams::default()` | `1` | `0xf8c6_9f07_14bb_7c6e` |
| `interacting()` | `0x000D_0B1E` | `0x4e7e_d35c_666f_64a0` |
| `adversarial()` | `0x0BAD_1DEA` | `0xae13_5289_3fb2_f48e` |
| `cruising()` | `0x0BAD_1DEA` | `0x7f10_1eb8_26ad_c0bd` |

The doc comment on `golden_runs` states, at length, that changing a value here
invalidates every stored provenance hash and must be a deliberate, reviewed
decision with the stored data migrated — mirroring the treatment
`hash::tests::hash_is_pinned_to_a_known_value` already gives the hash function.

### New tests

- `sim::tests::end_to_end_run_hashes_are_pinned_to_known_values`
- `sim::tests::the_pinned_runs_are_distinct_from_one_another` — coverage guard;
  five scenarios collapsing onto one state would agree trivially
- `sim::tests::the_pinned_runs_are_reached_by_batching_too` — the same hashes
  via 37-tick batches with a JSON checkpoint at each seam, so a golden cannot
  be satisfied by an unbatchable path

### Proof the goldens bite

**Mutation A — reorder the five terms of `blend` (`forces.rs`):**

```
failures:
    sim::tests::end_to_end_run_hashes_are_pinned_to_known_values
    sim::tests::the_pinned_runs_are_reached_by_batching_too
test result: FAILED. 221 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.35s
```

```
thread 'sim::tests::end_to_end_run_hashes_are_pinned_to_known_values' panicked at boids-core/src/sim.rs:710:13:
assertion `left == right` failed: default/seed 0: run_batch(250 ticks, seed 0) hashed as a6dd9184b3a9ce2f but the pinned value is 0x18ae2e5de42a28e0. Every stored provenance hash produced by this kernel is now unreproducible; this is an arithmetic change, not a stale constant
  left: 12023926579285052975
 right: 1778409883652729056
```

**Mutation B — semi-implicit Euler → explicit Euler (`sim.rs`, integrate the
position with the *old* velocity):**

```
failures:
    sim::tests::end_to_end_run_hashes_are_pinned_to_known_values
    sim::tests::the_pinned_runs_are_reached_by_batching_too
test result: FAILED. 221 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.13s
```

```
thread 'sim::tests::end_to_end_run_hashes_are_pinned_to_known_values' panicked at boids-core/src/sim.rs:710:13:
assertion `left == right` failed: default/seed 0: run_batch(250 ticks, seed 0) hashed as 449eae8e1ee257d4 but the pinned value is 0x18ae2e5de42a28e0. ...
```

Both mutations were reverted. Note the counts: in each case exactly the two new
goldens failed and the other 221 tests passed, which is the reviewer's claim
reproduced.

---

## M1 — AC-21 asserted stored speed, not distance travelled — FIXED

### Change

A per-tick displacement invariant added to
`sim::tests::a_long_adversarial_run_never_breaks_its_invariants`, checked for
every agent on every tick of both adversarial scenarios:

```rust
let limit = params.max_speed * params.dt;
for (now, was) in state.agents.iter().zip(&previous.agents) {
    let moved = params.world.distance(was.pos, now.pos);
    assert!(moved <= limit + 1e-9, "...");
}
```

Toroidal distance, so a seam crossing is measured the short way and does not
false-positive.

### Proof it bites

Mutant: the classic clamp-order bug — clamp the velocity that gets *stored*,
integrate the position with the *unclamped* one.

```
failures:
    sim::tests::a_long_adversarial_run_never_breaks_its_invariants
    sim::tests::end_to_end_run_hashes_are_pinned_to_known_values
    sim::tests::the_pinned_runs_are_reached_by_batching_too
test result: FAILED. 220 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.55s
```

```
thread 'sim::tests::a_long_adversarial_run_never_breaks_its_invariants' panicked at boids-core/src/sim.rs:1340:21:
crowded tick 1: agent 0 moved 1.0188945372900537 > max_speed*dt = 0.5
```

It fires on **tick 1**, and it names the defect (positional overshoot) rather
than merely reporting a changed hash. Mutation reverted.

---

## M2 — AC-18 never exercised coincident agents — FIXED

### Changes

1. `stepping_is_independent_of_agent_array_order` and
   `order_independence_survives_a_long_run` now run over **both** `interacting()`
   and `adversarial()` (via a new `order_independence_scenarios` fixture),
   widening permutation coverage to saturated clamps, obstacle interiors and a
   near-neighbourhood-wide separation radius.
2. New test `sim::tests::permuting_a_pile_of_coincident_agents_changes_nothing`:
   24 agents at *exactly* the same point, 8 shuffles, 5 ticks each, all required
   to produce an identical `state_hash` and bit-identical agents.

### An important correction to the finding

Adding `adversarial()` to the permutation tests does **not** catch the
slot-keyed mutant on its own. `adversarial()` crowds agents to within
`collision_radius`, which is not the same as putting two of them on the same
`f64` point, and `escape_direction` only fires when `1/distance` is non-finite.
Neither do the H1 goldens — none of those five scenarios contains an exactly
coincident pair. Only the dedicated pile test reaches it. That limitation is
now written into the doc comment on `order_independence_scenarios` so the next
reader does not over-trust the generic scenarios.

### Proof it bites

Mutant: re-key `escape_direction` on slot indices instead of agent ids.

```
failures:
    sim::tests::permuting_a_pile_of_coincident_agents_changes_nothing
test result: FAILED. 223 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.30s
```

```
thread 'sim::tests::permuting_a_pile_of_coincident_agents_changes_nothing' panicked at boids-core/src/sim.rs:958:13:
assertion `left == right` failed: permutation 0 of a coincident pile changed the state hash
  left: 2570544883740635400
 right: 928335025013249804
```

223 passed / 1 failed: the new test is the *only* thing in the crate that sees
this defect. Mutation reverted.

---

## M3 — `obstacle_avoidance` returned `NaN` for a radius `validate` accepts — FIXED

### Fix

Both halves, because they answer different questions:

- **`forces::obstacle_push`** now returns `Vec2::ZERO` when the computed
  influence band is non-finite. The kernel's totality rule says no input
  produces a `NaN`, and the existing radius check cannot catch this because the
  overflow is a property of the *band*, not of the radius.
- **`sim::validate`** now reports such an obstacle, because silently ignoring an
  obstacle a user configured is exactly the "your scenario does not say what you
  think it says" case `validate` exists for. It asks the force law via a new
  `forces::obstacle_influence_radius` rather than re-deriving
  `OBSTACLE_INFLUENCE`, so the two cannot drift apart.

### New tests

- `forces::tests::an_obstacle_whose_influence_band_overflows_contributes_nothing`
- `forces::tests::an_obstacle_with_an_overflowing_influence_band_does_not_disable_the_flock`
- `forces::tests::the_influence_radius_is_the_force_laws_own_band`
- `sim::tests::validate_rejects_an_obstacle_whose_avoidance_band_overflows` —
  including that `radius = 8.9e307` (the largest with a finite band) is still
  **accepted**, so the rule is about the band overflowing and not about "big"

### Proof they bite

With the new `!influence.is_finite()` guard removed:

```
failures:
    forces::tests::an_obstacle_whose_influence_band_overflows_contributes_nothing
    forces::tests::an_obstacle_with_an_overflowing_influence_band_does_not_disable_the_flock
test result: FAILED. 236 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.74s
```

```
thread 'forces::tests::an_obstacle_whose_influence_band_overflows_contributes_nothing' panicked at boids-core/src/forces.rs:859:13:
radius 1e308 produced a non-finite force: Vec2 { x: NaN, y: NaN }

thread 'forces::tests::an_obstacle_with_an_overflowing_influence_band_does_not_disable_the_flock' panicked at boids-core/src/forces.rs:880:9:
one absurd obstacle silently switched off all five behaviours: Vec2 { x: 29.8, y: 32.8 } became Vec2 { x: 0.0, y: 0.0 }
```

The second message is the finding's real severity: `blend`'s `limit` swallows
the `NaN`, so the run is not visibly broken — it just has no steering at all.
Guard restored.

---

## M4 — `mean_nearest_neighbor_distance` could be `+inf` — FIXED

### Choice made, and why

**Make the metric robust; do not bound the world in `validate`.**

`mean_nearest_neighbor_distance` is `pub`, so its contract has to hold for any
caller, not only for parameter sets that went through `validate`. And an
enormous world is a legitimate (if eccentric) scenario, not a mistake —
rejecting it would be `validate` overreaching, where the kernel's stated rule is
totality. Two independent overflows had to be closed:

1. **`distance_squared` saturates.** `World::distance_squared` is `x*x + y*y`
   and overflows above a world size of roughly `1.3e154`, where
   `World::distance`'s `hypot` still succeeds. The per-agent scan keeps the fast
   squared comparison and falls back to a `hypot` rescan only when the squared
   minimum came out non-finite — so the common path is unchanged and the
   pathological path is exact. A final saturation to the world's half-diagonal
   (which is always finite and is the bound the doc already claimed) makes the
   guarantee unconditional.
2. **The sum overflows where the mean does not.** Each nearest distance is
   bounded by the half-diagonal and so is their mean, but a sum of `n` of them
   is not. The division moved inside the loop. The same one-line treatment was
   applied to `mean_speed`, which has the identical defect.

`FrameMetrics`' finiteness claim was rewritten to be **precise and true** rather
than aspirational: no field is ever `NaN` for any input; `polarization`,
`fraction_arrived` and `mean_nearest_neighbor_distance` are finite
unconditionally; `mean_speed` is finite whenever the individual speeds are —
a velocity of `(f64::MAX, f64::MAX)` has an infinite *length*, which no
averaging can repair, and which `sim::step`'s `max_speed` clamp rules out for
every frame a run produces. The doc also now states *why* it matters: JSON has
no infinity, so `+inf` becomes `null` and then fails to deserialize.

### New / changed tests

- `metrics::tests::ac24_mean_nearest_neighbor_distance_is_finite_in_an_enormous_world`
  — worlds of `1e155`, `1e200`, `1e300`, `f64::MAX/2`, `f64::MAX`
- `metrics::tests::a_non_finite_metric_cannot_survive_the_persisted_json_round_trip`
  — demonstrates the actual damage: `+inf` serializes to `null` and then fails
  to parse back, leaving an unreadable `frames.metrics` row
- `metrics::tests::mean_speed_is_finite_even_where_the_total_speed_is_not`
- `metrics::tests::frame_metrics_are_always_finite` **extended beyond its 100x100
  sampling** to eight world scales from `1e-300` to `1e307`, with
  `collision_radius`, goal and arrival radius scaled to match, and each frame
  additionally required to serialize and deserialize successfully

### Proof they bite

Against the original implementation (hypot fallback removed):

```
thread 'metrics::tests::ac24_mean_nearest_neighbor_distance_is_finite_in_an_enormous_world' panicked at boids-core/src/metrics.rs:638:13:
world 1e155: mean NND was inf

thread 'metrics::tests::frame_metrics_are_always_finite' panicked at boids-core/src/metrics.rs:887:17:
non-finite metric in a 1e200-scale world: FrameMetrics { polarization: 0.15012801940175313, mean_nearest_neighbor_distance: inf, collisions: 10, mean_speed: 2.1330554173240635, fraction_arrived: 0.1 }
```

Against the original `mean_speed` (sum then divide):

```
thread 'metrics::tests::mean_speed_is_finite_even_where_the_total_speed_is_not' panicked at boids-core/src/metrics.rs:935:9:
mean speed was inf
```

Both reverted.

### Side observation (not fixed, out of scope)

While extending `frame_metrics_are_always_finite` I found that `FrameMetrics`
does **not** round-trip bit-exactly through `serde_json` for arbitrary values —
it is written as plain JSON numbers, whose parser is an ULP loose. (`SimState`
solves this by serializing floats as exact decimal *strings*; `FrameMetrics`
does not.) The existing assertion `"FrameMetrics must round-trip exactly"` in
`frame_metrics_serialises_under_its_persisted_field_names` holds only because
the worked example's values are tidy. My new assertion therefore checks that a
frame is *storable and reloadable*, not bit-exact. This is defensible for
metrics — the module doc already says a frame's metrics can be recomputed from
stored state at any time — but it is a real difference between the two persisted
types and worth a decision rather than an accident. No existing assertion was
weakened.

---

## M5 — `spatial_hash_built_for_a_different_agent_slice_stays_exact` was vacuous — FIXED

### Changes

- Renamed to
  `neighbors::tests::a_grid_built_for_a_different_length_slice_falls_back_to_an_exact_scan`,
  and the extra agent moved from `(4.0, 4.0)` — outside the flock's
  `[20, 72.5]²` span with radius 12, so it had no neighbours and was nobody's
  neighbour — to `(45.0, 45.0)`, in the middle of the flock.
- Two explicit non-vacuity preconditions added: the intruder must have
  neighbours, and at least one existing agent must see the intruder.
- New test
  `neighbors::tests::the_length_guard_does_not_detect_a_same_length_flock_that_moved`,
  which states the known limitation as an executable fact: the guard is
  length-only, so a same-length but *moved* flock still gets wrong answers from
  a stale grid. What actually covers that is the "rebuild every tick" contract
  in `sim::step`, which the test's comment names.

### Proof it bites

Mutant: delete the `agents.len() != self.agent_count` clause from
`SpatialHash::neighbors`.

```
failures:
    neighbors::tests::a_grid_built_for_a_different_length_slice_falls_back_to_an_exact_scan
test result: FAILED. 232 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.68s
```

```
thread 'neighbors::tests::a_grid_built_for_a_different_length_slice_falls_back_to_an_exact_scan' panicked at boids-core/src/neighbors.rs:963:13:
assertion `left == right` failed: agent 19
  left: [10, 11, 12, 18, 20, 26, 27, 28]
 right: [10, 11, 12, 18, 20, 26, 27, 28, 64]
```

The stale grid drops agent 64 — exactly the "drop agents it never saw" failure
the test claims to guard. Guard restored.

---

## L1 — `toroidal_centroid` uses non-portable trigonometry — DOCUMENTED, plus a new guard

### Verdict: documented, not rewritten

Computing a circular mean without trigonometry means inverting the angle, and
the trig-free alternatives (sorting the coordinates and searching for the
minimum-variance rotation) compute a *different* statistic with different
degenerate behaviour. Swapping in a different definition to fix a portability
hazard for a value nothing currently consumes is a worse trade than naming the
hazard loudly, so: documented.

### What changed

- `toroidal_centroid` carries a heading:
  **"NOT BIT-PORTABLE — never hash, fingerprint, or persist this value"**, and
  spells out that `sin`/`cos`/`atan2` are not correctly-rounded IEEE 754
  operations and are free to disagree between libm implementations, CPUs and
  optimisation levels; that it is safe to *display* and unsafe to compare
  across machines; and that anything hashed needs a trig-free circular mean,
  written as a different function.
- `circular_mean_axis` carries a one-line cross-reference.
- The `sim.rs` module doc's "no trigonometry ... anywhere in the pipeline" was
  not literally inaccurate (`toroidal_centroid` is not on that path), but it was
  vague about which path. It now names the path — `SimState::seeded` → `step` →
  `hash::state_hash` — explains *why* trigonometry specifically is called out,
  names the single exception, and points at the test that keeps it true.

### New guard

`sim::tests::the_kernel_uses_trigonometry_in_one_place_only` scans every
`boids-core/src/*.rs` for trigonometric calls, in the same source-scanning
style `tests/purity.rs` uses to police dependencies, and fails if any appears
outside `metrics.rs`. It also asserts the scanner still finds the *known*
occurrences in `metrics.rs`, so it cannot pass silently by having stopped
matching.

Proof it works — trigonometry deliberately introduced into `Vec2::length`:

```
thread 'sim::tests::the_kernel_uses_trigonometry_in_one_place_only' panicked at boids-core/src/sim.rs:723:9:
trigonometry on the reproducible path — a persisted state hash can now differ between two machines running the same code:
vec2.rs:77: let a = self.y.atan2(self.x);
vec2.rs:78: (self.x * a.cos() + self.y * a.sin()).abs()
```

Reverted.

---

## L2 — `World::displacement` can exceed half the world — DOCUMENTED, plus two new tests

### Verdict: fix the doc, not `validate`

Bounding `goal` and `obstacle.center` by magnitude in `validate` would reject
scenarios that are merely silly rather than unsafe, and would not make
`displacement` — a `pub` method anyone can call — any more truthful. The doc was
the thing that was wrong.

### Characterisation

The reduction is `d - size * round(d / size)`. It stops cancelling cleanly once
`d / size` runs out of low bits. Measured worst `|component| / (size/2)` by
separation magnitude:

| `|b - a| / size` | worst ratio |
|---|---|
| `1e0`–`1e10` | `1.000002` |
| `1e11`–`1e12` | `1.000115` |
| `1e12`–`1e13` | `1.001727` |
| `1e13`–`1e14` | `1.033396` |
| `1e14`–`1e15` | `1.316573` |
| `1e15`–`1e16` | `3.994749` |

(The reviewer's observed 1.19x sits inside this curve; it is not the worst case.)

### What changed

`World::displacement`'s doc replaces the unqualified "each component is at most
half the corresponding world dimension" and "inputs need **not** be pre-wrapped"
with the precise statement: the bound holds while `|b - a|` is within roughly
`1e10` world-widths — which covers everything the simulation produces, since
`wrap` keeps agents in `[0,size)` — and past that the guarantee *degrades*
rather than failing, staying finite and deterministic throughout, with wrapping
either input restoring the exact bound. It also names the exposure: `goal` and
`obstacle.center` are the only unwrapped values the kernel feeds in, and
`validate` checks them for finiteness but deliberately not for magnitude.

### New tests

- `world::tests::displacement_keeps_the_half_world_bound_out_to_a_documented_magnitude`
  — 50 000 cases, world sizes across six orders of magnitude, separations across
  ten; the bound must hold exactly
- `world::tests::displacement_of_absurdly_out_of_world_inputs_degrades_but_stays_total`
  — 20 000 cases at `1e13`–`1e16` widths out; every result must be finite and
  deterministic, wrapping must restore the bound, and **at least one case must
  breach half the world**, so the documented degradation is asserted to be real
  rather than assumed. If the reduction is ever made exact at these magnitudes
  the test fails and says so, pointing at the doc to update.

---

## L4 — `ac25_collision_count_never_exceeds_the_number_of_pairs` was vacuous — FIXED (replaced)

`c <= C(n,2)` is guaranteed by the *shape* of `for b in &agents[i + 1..]`. No
mutation of the loop body could break it; the test asserted the loop bounds.

Replaced with `metrics::tests::ac25_collision_count_matches_an_independent_ordered_pair_scan`,
which checks the count against a reference written the other way round — every
**ordered** pair, using `World::distance` rather than the implementation's
squared comparison, then halved — and retains the bound as a corollary. Plus a
non-vacuity guard that at least one sampled flock actually collided.

Added `metrics::tests::ac25_a_fully_coincident_flock_attains_the_pair_bound_exactly`:
`n` agents on one point must be exactly `C(n,2)` collisions for `n` in `0..12`,
so the bound is shown to be reachable rather than merely respected.

### Proof it bites where the old one could not

Mutant chosen so the **old** assertion still passes (the count only ever goes
*down*, so `c <= C(n,2)` still holds) and so the hand-written `n <= 4` cases are
untouched: cap each inner scan at four partners.

```
thread 'metrics::tests::ac25_collision_count_matches_an_independent_ordered_pair_scan' panicked at boids-core/src/metrics.rs:769:13:
assertion `left == right` failed: counted 7 unordered pairs, but an ordered scan found 18 ordered ones among 11 agents
  left: 7
 right: 9
```

Reverted.

---

## L5 — separation's exclusive radius was undocumented and untested — FIXED

### Doc

`forces::separation` gains a section explaining the asymmetry rather than
leaving it as an inconsistency: `neighbor_radius` decides **membership of a
set**, where the closed ball is right because an agent exactly on the boundary
is as visible as one just inside; `separation_radius` marks **where a behaviour
has switched off**, so the boundary belongs to the "off" side. It also notes
what `separation_radius == neighbor_radius` therefore means (the whole
neighbourhood repels except its outermost rim) and that
`metrics::collision_count` uses the same strict `<`, so "just touching" is
consistently not-yet-interacting across the kernel.

### New test

`forces::tests::separation_treats_its_radius_as_a_strict_threshold` — a pair
exactly `5.0` apart must produce exactly `Vec2::ZERO`; one ULP inside must
repel (so the first assertion is about the boundary, not about an inert
scenario); and `neighbors_naive` at that same distance must return the agent,
pinning both halves of the asymmetry in one place.

### Proof it bites

Mutant: `distance < separation_radius` → `distance <= separation_radius`.

```
failures:
    forces::tests::separation_treats_its_radius_as_a_strict_threshold
test result: FAILED. 234 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.73s
```

```
thread 'forces::tests::separation_treats_its_radius_as_a_strict_threshold' panicked at boids-core/src/forces.rs:504:9:
assertion `left == right` failed: a neighbour at exactly the separation radius must not repel
  left: Vec2 { x: -0.2, y: 0.0 }
 right: Vec2 { x: 0.0, y: 0.0 }
```

234 passed / 1 failed: the new test is the only thing that sees it. Reverted.

---

## AC-14 coverage gap — FIXED

AC-14's second clause — *"with `goal_weight = 0` the goal position is provably
irrelevant: two runs with different goals produce identical state hashes"* — had
no test. The suite only asserted the single-evaluation fact that `goal_seek`
returns `ZERO` for `None`.

New test `sim::tests::with_a_zero_goal_weight_the_goal_position_is_provably_irrelevant`:
five goals including `None` and one far outside the world, everything else
identical, `w_goal = 0.0`, **400 ticks** each, identical `state_hash` required.
It also carries a non-vacuity control — with `w_goal = 0.4` the same two goals
must produce *different* hashes — so the test cannot pass against a goal-seeking
behaviour that had simply been deleted.

### Proof it bites

Mutant: `goal.scale(params.w_goal)` → `goal.scale(params.w_goal + 1e-12)`, i.e.
the weight no longer switches the behaviour fully off.

```
thread 'sim::tests::with_a_zero_goal_weight_the_goal_position_is_provably_irrelevant' panicked at boids-core/src/sim.rs:1628:13:
assertion `left == right` failed: a goal at Some(Vec2 { x: 0.0, y: 0.0 }) changed the run despite w_goal = 0
  left: 9110278597731460989
 right: 17937124528323517888
```

Reverted. (The reviewer's expectation held: this was a missing test, not a bug.)

---

## Rules observed

- No new dependencies; `f64` throughout; no `HashMap`/`HashSet` iteration
  introduced (the trig scanner sorts its directory listing before use, since
  `read_dir` order is not deterministic).
- No existing assertion was weakened. `mean_speed` and
  `mean_nearest_neighbor_distance` changed their summation order, which shifts
  persisted metric values in the last bits; metrics are not part of the state
  hash and no golden metric constant exists, and both existing exact
  expectations (`1.0` and `13/3` on tidy inputs) still hold.
- Every new test was run against the defect it guards, with the real failure
  captured above, and every mutation was reverted before the next step.
