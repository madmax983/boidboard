# TDD log — metrics module

Evidence for **AC-52** ("every feature was built red → green → refactor") covering
`boids-core/src/metrics.rs`: AC-23 through AC-28, plus the `FrameMetrics` aggregate and
the toroidal centroid the stuck detector is fed from.

Every **RED** block below is real captured output from `cargo test -p boids-core metrics`
at the moment the test existed and the implementation did not. Output is trimmed
(surrounding cargo noise, unrelated modules' tests) but never edited or reconstructed.

**Cycle order.** AC-23 → AC-24 → `toroidal_centroid` → AC-25 → `FrameMetrics` → AC-26 →
AC-27 → AC-28. This is numeric order with two insertions. `toroidal_centroid` lands after
AC-24 because both are the same lesson (a mean taken in raw coordinates is wrong on a
torus) and the centroid is what AC-27's stuck detector consumes. `FrameMetrics` lands
before AC-26 because `time_to_goal` takes a `&[FrameMetrics]`, so the type is a genuine
prerequisite rather than a convenience. AC-28 is deliberately last: it is a property over
*every* metric, so it can only be written once every metric exists.

---

### AC-23 — polarization is the magnitude of the mean unit heading

**RED** — `ac23_polarization_is_exactly_one_for_a_perfectly_aligned_flock`,
`ac23_polarization_is_exactly_zero_for_four_agents_at_right_angles`,
`ac23_zero_velocity_agents_have_no_heading_and_never_produce_nan`,
`ac23_polarization_is_always_within_the_unit_interval`

```
error[E0432]: unresolved import `super::polarization`
  --> boids-core/src/metrics.rs:15:9
   |
15 |     use super::polarization;
   |         ^^^^^^^^^^^^^^^^^^^ no `polarization` in `metrics`

For more information about this error, try `rustc --explain E0432`.
error: could not compile `boids-core` (lib test) due to 1 previous error
```

**GREEN** — sum `vel.normalize()` over the flock, divide by the number of agents that had
a heading, take the length; `.min(1.0)` clamps the one-ULP overshoot that floating-point
rounding can produce above the triangle-inequality bound.

**REFACTOR** — none needed; the function is already a single accumulation loop.

**Zero-velocity decision (the AC asks for one, explicitly documented).** A stationary agent
is **excluded from both the numerator and the divisor**, not folded in as a zero vector.
Folding it in would conflate "standing still" with "pointing the wrong way", so a frozen
flock would report the same `0.0` as a maximally disordered one and polarization would stop
being a pure measure of heading agreement. The exclusion falls out of `Vec2::normalize`
already being total (zero-length and non-finite both map to `ZERO`), so the implementation
needs no special case beyond skipping `ZERO`. Consequences, all asserted: an empty flock and
an all-stationary flock both return `0.0`; a flock with a single moving agent returns `1.0`.

---

### AC-24 — mean nearest-neighbour distance under toroidal distance

**RED** — `ac24_mean_nearest_neighbor_distance_goes_the_short_way_round_the_seam`,
`ac24_mean_nearest_neighbor_distance_averages_per_agent_nearest_distances`,
`ac24_mean_nearest_neighbor_distance_of_fewer_than_two_agents_is_zero`,
`ac24_mean_nearest_neighbor_distance_ignores_the_agent_itself`,
`ac24_mean_nearest_neighbor_distance_never_exceeds_the_half_diagonal`

```
error[E0432]: unresolved import `super::mean_nearest_neighbor_distance`
  --> boids-core/src/metrics.rs:59:17
   |
59 |     use super::{mean_nearest_neighbor_distance, polarization};
   |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ no `mean_nearest_neighbor_distance` in `metrics`

For more information about this error, try `rustc --explain E0432`.
error: could not compile `boids-core` (lib test) due to 1 previous error
```

**GREEN** — for each agent, scan every other agent for the smallest
`World::distance_squared`, `sqrt` the winner, average over the flock; `len() < 2` short
circuits to `0.0`.

**REFACTOR** — renamed the inner accumulator `nearest` to `nearest_squared`, since it holds
a squared distance until the `sqrt` at the end of each outer iteration and the short name
invited a future reader to compare it against a plain radius. Tests stayed green.

Two deliberate choices were made in the green step rather than after it: the inner loop
compares **squared** distances and takes a single `sqrt` per agent instead of one per pair,
and self-exclusion is by **slice index** (`i == j`) rather than by `Agent::id`, so duplicate
or reused ids cannot let an agent match itself.

The half-diagonal property test is the cheap insurance against a non-toroidal
implementation: a raw coordinate subtraction is unbounded by the half-diagonal, so it fails
that assertion over randomised flocks long before AC-28 gets a chance to.

---

### `toroidal_centroid` — the circular mean, not the coordinate mean

**RED** — `toroidal_centroid_of_a_seam_straddling_pair_is_the_seam_not_the_middle`,
`toroidal_centroid_of_a_clustered_flock_matches_the_plain_mean`,
`toroidal_centroid_is_always_inside_the_world`,
`toroidal_centroid_of_an_empty_flock_is_the_origin`,
`toroidal_centroid_moves_with_the_flock`

```
error[E0432]: unresolved import `super::toroidal_centroid`
  --> boids-core/src/metrics.rs:96:63
   |
96 |     use super::{mean_nearest_neighbor_distance, polarization, toroidal_centroid};
   |                                                               ^^^^^^^^^^^^^^^^^ no `toroidal_centroid` in `metrics`

For more information about this error, try `rustc --explain E0432`.
error: could not compile `boids-core` (lib test) due to 1 previous error
```

**GREEN** — per axis, map each coordinate to an angle (`x * TAU / width`), sum the unit
vectors, and read the mean direction back with `atan2`, folding the result into
`[0,size)`. Positions are `World::wrap`ped first, which is what keeps a non-finite input
from poisoning the flock's centroid through `cos`/`sin`.

**REFACTOR** — none needed. Two things were got right in the green step rather than
after it: the accumulated `(cos_sum, sin_sum)` is **not** divided by the agent count,
because `atan2` reads only the direction and the division would add a pointless rounding
step; and the `[0,size)` fold repeats `world.rs`'s guard that `rem_euclid` can return
exactly `size` for tiny negative inputs, which would break the half-open invariant.

The pair of tests is the point: `..._is_the_seam_not_the_middle` fails for a plain
coordinate mean, and `..._matches_the_plain_mean` fails for anything that gets the easy
non-seam case wrong while chasing the hard one. `toroidal_centroid_moves_with_the_flock`
generalises both — a tight cluster shifted by a random offset (including offsets that drag
it over a seam) must move its centroid by exactly that offset.

---

### AC-25 — collisions are unordered pairs

**RED** — `ac25_two_mutually_overlapping_agents_are_one_collision_not_two`,
`ac25_three_mutually_overlapping_agents_are_three_pairs`,
`ac25_an_agent_is_never_counted_against_itself`,
`ac25_collisions_are_counted_across_the_seam`,
`ac25_the_radius_is_a_strict_upper_bound`,
`ac25_collision_count_never_exceeds_the_number_of_pairs`

```
error[E0432]: unresolved import `super::collision_count`
   --> boids-core/src/metrics.rs:151:9
    |
151 |         collision_count, mean_nearest_neighbor_distance, polarization, toroidal_centroid,
    |         ^^^^^^^^^^^^^^^ no `collision_count` in `metrics`
```

*(Trimmed: the same build also carried `E0433 use of undeclared type SpatialHash` errors
from `neighbors.rs`, which a concurrent agent had in its own red phase. Those are not in
this file. The green below was therefore confirmed against an isolated harness crate that
symlinks the identical `metrics.rs`/`world.rs`/`vec2.rs`/`rng.rs`/`config.rs` sources, and
re-confirmed in the workspace once `neighbors.rs` went green — see the final run at the
bottom of this log.)*

**GREEN** — nested loop over `j > i` only, comparing `World::distance_squared` against
`radius * radius`; non-positive and non-finite radii short circuit to `0`.

**REFACTOR** — none needed. The `j > i` iteration is expressed as `&agents[i + 1..]` rather
than an index guard, so "unordered pair" is structural — there is no `i != j` check that a
later edit could weaken back into double counting.

Two design points the tests pin down. `radius` is a **strict** upper bound: agents exactly
`radius` apart are not colliding, matching the separation force treating that distance as
the point where repulsion has just faded out. And a non-positive radius admits nothing at
all, including coincident agents — squaring a negative radius would otherwise turn `-1.0`
into a *wider* collision test than `0.5`.

---

### `FrameMetrics` — the persisted per-frame aggregate

**RED** — `frame_metrics_reports_every_field_of_the_worked_example`,
`frame_metrics_fraction_arrived_is_zero_without_a_goal`,
`frame_metrics_fraction_arrived_measures_arrival_across_the_seam`,
`frame_metrics_arrival_radius_is_inclusive`,
`frame_metrics_of_an_empty_flock_is_all_zeros`,
`frame_metrics_are_always_finite`,
`frame_metrics_serialises_under_its_persisted_field_names`

```
error[E0432]: unresolved imports `super::FrameMetrics`, `super::frame_metrics`
   --> boids-core/src/metrics.rs:186:9
    |
186 |         FrameMetrics, collision_count, frame_metrics, mean_nearest_neighbor_distance, polarization,
    |         ^^^^^^^^^^^^                   ^^^^^^^^^^^^^ no `frame_metrics` in `metrics`
    |         |
    |         no `FrameMetrics` in `metrics`

For more information about this error, try `rustc --explain E0432`.
error: could not compile `boids-core` (lib test) due to 1 previous error
```

**GREEN** — the `FrameMetrics` struct with its five documented fields, plus `frame_metrics`
delegating to the three functions already built and to two new private helpers, `mean_speed`
and `fraction_arrived`.

**REFACTOR** — none needed; `frame_metrics` is a five-line struct literal by construction.
`mean_speed` and `fraction_arrived` were kept **private**: they are pure projections into
`FrameMetrics`, and every caller wants the whole frame, so exporting them would widen the
kernel's surface for nothing.

The worked example is one four-agent flock reused across the assertions, chosen so every
number is exact and every toroidal path is exercised: agents `0`/`1` collide directly and
have both arrived, agents `2`/`3` collide **across the seam** and have not, and agent `3` is
stationary so the polarization divisor is 3 rather than 4. Expected values are `sqrt(5)/3`,
`1.0`, `2`, `2.25`, `0.5`.

`frame_metrics_serialises_under_its_persisted_field_names` exists because the field names
are a database contract owned jointly with the persistence agent — it asserts the exact
sorted key set of the serialized JSON object, so a rename fails the build here rather than
at a `frames.metrics` read months later.

---

### AC-26 — time-to-goal is an `Option`, never a sentinel

**RED** — `ac26_time_to_goal_is_the_first_qualifying_tick`,
`ac26_time_to_goal_is_none_when_the_goal_is_never_reached`,
`ac26_time_to_goal_takes_the_first_crossing_not_a_later_one`,
`ac26_time_to_goal_threshold_is_inclusive`,
`ac26_time_to_goal_of_an_unreachable_threshold_is_none`

```
error[E0432]: unresolved import `super::time_to_goal`
   --> boids-core/src/metrics.rs:261:9
    |
261 |         time_to_goal, toroidal_centroid,
    |         ^^^^^^^^^^^^ no `time_to_goal` in `metrics`

For more information about this error, try `rustc --explain E0432`.
error: could not compile `boids-core` (lib test) due to 1 previous error
```

**GREEN** — `series.iter().position(|m| m.fraction_arrived >= fraction)`. `position` returns
`Option<usize>` and stops at the first match, which is precisely the specified behaviour, so
the sentinel question never arises at the type level.

**REFACTOR** — clippy's `unnecessary_map_or` fired on the test's
`result.map_or(true, |t| t < series.len())`; rewritten as `result.is_none_or(..)`. No
production-code change. Note also that the `NaN` threshold case needs **no branch**: `>=`
against `NaN` is false for every frame, so `position` runs off the end and yields `None` on
its own.

The AC asks for proof there is no sentinel, so
`ac26_time_to_goal_is_none_when_the_goal_is_never_reached` asserts three separate things —
`is_none()`, `== None`, and `!= Some(usize::MAX)` — and adds
`is_none_or(|t| t < series.len())`, which fails for any out-of-range index an implementation
might smuggle back through the `Option`.
`ac26_time_to_goal_takes_the_first_crossing_not_a_later_one` guards the other direction: a
flock that arrives, scatters, and re-arrives must be credited with tick 1, not tick 4.

---

### AC-27 — stuck detection, by tortuosity

**RED** — `ac27_is_stuck_fires_on_a_centroid_oscillating_in_place`,
`ac27_is_stuck_does_not_fire_on_a_flock_cruising_steadily`,
`ac27_is_stuck_looks_only_at_the_most_recent_window`,
`ac27_is_stuck_handles_a_path_shorter_than_the_window`,
`ac27_is_stuck_ignores_a_motionless_flock`,
`ac27_is_stuck_follows_a_path_across_the_seam`,
`ac27_is_stuck_is_monotone_in_the_threshold`

```
error[E0432]: unresolved import `super::is_stuck`
   --> boids-core/src/metrics.rs:283:55
    |
283 |         FrameMetrics, collision_count, frame_metrics, is_stuck, mean_nearest_neighbor_distance,
    |                                                       ^^^^^^^^ no `is_stuck` in `metrics`

For more information about this error, try `rustc --explain E0432`.
error: could not compile `boids-core` (lib test) due to 1 previous error
```

**GREEN** — over the last `window` samples, sum the minimum-image step vectors to get net
displacement and sum their lengths to get path length; `straightness = |net| / path_length`,
and stuck is `straightness < ratio_threshold`.

**REFACTOR** — see the mutation check below, which is what the refactor step turned up.

**Algorithm and thresholds.** Straightness is in `[0,1]`: `1.0` is a dead-straight run,
`~0.0` means the flock covered ground and ended where it started. The **ratio** is what makes
both halves of the AC satisfiable at once — an oscillating centroid walks a long path to no
end and scores ~0, while a cruising flock scores 1.0 whether it is fast or slow, so speed
alone can never be mistaken for progress. The useful threshold band is `0.1`-`0.3`; the
tests use `0.2`. A frozen flock (`path_length == 0`) is `0/0` and returns `false`
deliberately: that is a different diagnosis, and `FrameMetrics::mean_speed` is the metric
that names it. Insufficient evidence — a path shorter than `window`, or `window < 2` —
also returns `false` rather than panicking or guessing.

**Mutation check (the refactor step's real finding).** Writing the negative assertions is
not enough on its own: a detector that always returns `true` fails
`ac27_is_stuck_does_not_fire_on_a_flock_cruising_steadily`, but that test does **not**
discriminate between summing the minimum-image *steps* and simply taking the minimum-image
distance between the window's *endpoints*, because its 20-sample window spans only 28.5
units of a 100-wide world — less than half a lap, so the two formulations agree. Neither
does `ac27_is_stuck_follows_a_path_across_the_seam`, for the same reason.

So the implementation was reverted to the endpoint formulation to check whether anything
caught it. Nothing did, which is why
`ac27_is_stuck_counts_a_full_lap_of_the_world_as_progress` was added: an 80-sample window
spanning 118.5 units laps the world, endpoint distance folds that to 18.5, and the naive
version reports a straightness of 0.156 — "stuck" — for a flock flying dead straight. With
the endpoint mutation in place:

```
test metrics::tests::ac27_is_stuck_counts_a_full_lap_of_the_world_as_progress ... FAILED

failures:

---- metrics::tests::ac27_is_stuck_counts_a_full_lap_of_the_world_as_progress stdout ----

thread 'metrics::tests::ac27_is_stuck_counts_a_full_lap_of_the_world_as_progress' (20800) panicked at boids-core/src/metrics.rs:958:9:
lapping the world is progress; endpoint distance folds it away

test result: FAILED. 39 passed; 1 failed; 0 ignored; 0 measured; 121 filtered out
```

Exactly one test failed, confirming both that the new test discriminates and that the seven
original ones did not. Restoring the step-summing implementation returns all 40 to green.

`ac27_is_stuck_is_monotone_in_the_threshold` covers the remaining degenerate detector: over
500 randomised drifting random walks, a lenient threshold firing where a strict one did not
is impossible for any implementation that actually reads `ratio_threshold`.

---

### AC-28 — every metric is translation invariant

**RED (by mutation).** AC-28 is a property *over the metrics already built*, so by the time
it could be written the implementation satisfied it — writing the test first would have
produced a green run, which is no evidence at all. The AC states its own purpose: it "is the
test that catches any non-toroidal distance calculation". So the red phase here is a
**mutation check**: each toroidal call was reverted, one at a time, to the raw coordinate
subtraction a careless implementation would have used, and the test had to catch it.

**Mutation A** — `mean_nearest_neighbor_distance` using `a.pos.sub(b.pos).length_squared()`
instead of `World::distance_squared`:

```
test metrics::tests::ac24_mean_nearest_neighbor_distance_goes_the_short_way_round_the_seam ... FAILED
test metrics::tests::ac28_every_metric_is_invariant_under_translation ... FAILED
test metrics::tests::ac24_mean_nearest_neighbor_distance_never_exceeds_the_half_diagonal ... FAILED
thread 'metrics::tests::ac28_every_metric_is_invariant_under_translation' panicked at src/metrics.rs:1041:17:
mean NND moved under offset Vec2 { x: 1338.612461828905, y: 1375.15104273636 }: FrameMetrics { polarization: 0.30024177270365937, mean_nearest_neighbor_distance: 28.86998418413989, collisions: 0, mean_speed: 2.21525064058992, fraction_arrived: 0.0 } -> FrameMetrics { polarization: 0.30024177270365937, mean_nearest_neighbor_distance: 15.886473722042064, collisions: 0, mean_speed: 2.21525064058992, fraction_arrived: 0.0 }
test result: FAILED. 39 passed; 4 failed; 0 ignored; 0 measured; 54 filtered out
```

**Mutation B** — `collision_count` using the same raw subtraction:

```
---- metrics::tests::ac28_every_metric_is_invariant_under_translation stdout ----

thread 'metrics::tests::ac28_every_metric_is_invariant_under_translation' panicked at src/metrics.rs:1047:17:
assertion `left == right` failed: collision count moved under offset Vec2 { x: 17.827091789347982, y: 63.924164188473064 }
  left: 3
 right: 1
test result: FAILED. 39 passed; 4 failed; 0 ignored; 0 measured; 54 filtered out
```

**GREEN** — restoring `World::distance_squared` in both places returns all 43 tests to
green. No production change was needed beyond the restore, which is the point: the metrics
were routed through `World` from the start, and this is the evidence that it mattered.

**REFACTOR** — none needed.

**Tests** — `ac28_every_metric_is_invariant_under_translation`,
`ac28_fraction_arrived_is_invariant_when_the_goal_moves_with_the_world`,
`ac28_stuck_detection_is_invariant_under_translation`.

3,000 randomised flocks, each re-checked under four offsets drawn from four families:
large random offsets (+-5,000, i.e. fifty world-widths out), exactly one world size,
an exact negative multiple (`-3w, 7h`), and a small in-world offset. Polarization, mean
speed and collisions are asserted **exactly** or to `1e-12`; mean NND to `1e-9`, which is
the `sqrt` rounding budget.

`fraction_arrived` needed a separate test and a word of justification: it is measured
*against the goal*, so translating the agents alone is **supposed** to change it — that is
correct behaviour, not a bug, and folding it into the main test would have forced the wrong
assertion. The invariance that does hold is translating the agents **and** the goal
together, which is what that test asserts.

---

## Final state

`cargo test -p boids-core metrics`:

```
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 138 filtered out; finished in 0.13s
```

Whole crate, `cargo test -p boids-core`, with the neighbouring modules landed:

```
test result: ok. 181 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.40s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo clippy -p boids-core --all-targets -- -D warnings` exits 0, with no diagnostics
anywhere in the crate.

### A note on the concurrent workspace

`neighbors.rs`, `forces.rs` and `sim.rs` were being written by other agents throughout, and
a crate's `--lib` test binary is a single compilation unit — so whenever one of those
modules was mid-red, `cargo test -p boids-core metrics` could not link, regardless of the
filter. Three of the runs above were therefore captured from an isolated harness crate that
**symlinks** the identical `metrics.rs`, `world.rs`, `vec2.rs`, `rng.rs` and `config.rs`
sources and depends on nothing else. Where that applies it is stated inline; the only
difference in the output is the `--> src/metrics.rs` path prefix in place of
`--> boids-core/src/metrics.rs`. Every result was re-confirmed in the real workspace once
the neighbouring modules were green.
