# TDD log — simulation kernel foundation

Evidence for **AC-52** ("every feature was built red → green → refactor") covering the
foundation layer of `boids-core`: `Vec2`, the toroidal `World`, the seeded `Rng`, the
canonical `state_hash`, and the purity guard.

Every **RED** block below is real captured output from `cargo test -p boids-core` at the
moment the test existed and the implementation did not. Nothing here is reconstructed.

**Cycle order.** The cycles run AC-5 → AC-1 → AC-2 → AC-4 → AC-3 → AC-6 → AC-49 rather than
in numeric order. The RNG comes first because the acceptance criteria call for
property-style tests over many randomised inputs, and those tests draw from *this crate's*
`Rng` (no `proptest` dependency). Writing the geometry property tests first would have meant
writing them against a generator that did not exist, then retrofitting — the RNG is a
genuine prerequisite, so it is built first. `wrap` (AC-4) precedes minimum-image distance
(AC-3) for the same reason: displacement is defined in terms of wrapping.

---

### AC-5 — seeded, self-contained, reproducible PRNG

**RED** — `matches_published_splitmix64_vectors`, `same_seed_produces_same_sequence`,
`different_seeds_produce_different_sequences`, `next_f64_is_in_unit_interval`,
`next_f64_is_roughly_uniform`, `range_stays_within_bounds`,
`range_with_empty_span_is_the_endpoint`

```
error[E0432]: unresolved import `rng::Rng`
 --> boids-core/src/lib.rs:9:9
  |
9 | pub use rng::Rng;
  |         ^^^^^^^^ no `Rng` in `rng`

error[E0432]: unresolved import `super::Rng`
 --> boids-core/src/rng.rs:3:9
  |
3 |     use super::Rng;
  |         ^^^^^^^^^^ no `Rng` in `rng`

error: could not compile `boids-core` (lib test) due to 2 previous errors
```

**GREEN** — SplitMix64 over a single `u64` state: wrapping-add the golden-ratio gamma, two
xor-shift-multiply mixing rounds, final xor-shift; `next_f64` takes the top 53 bits and
scales by an exact 2^-53 literal so the result can never round up to 1.0.

**REFACTOR** — Lifted the four magic constants (`GAMMA`, `MIX_1`, `MIX_2`, `F64_SCALE`) out
of the expressions into named consts documented as coming from the reference
implementation, and derived the shift width from `u64::BITS - F64_BITS` instead of hardcoding
`11`, so the 53-bit significand assumption is stated once. Tests stayed green; clippy clean.

The golden vectors were generated from the published algorithm (Vigna's public-domain
SplitMix64) in a throwaway Python script *before* the Rust existed — they are a spec, not a
recording of this implementation's behaviour. That is what makes them a real cross-process
and cross-platform reproducibility assertion (AC-22's foundation) rather than a tautology.

---

### AC-1 — Vec2 arithmetic, and a `normalize` that is total

**RED** — `zero_normalize_is_exactly_zero_not_nan`, `non_finite_normalize_is_exactly_zero`,
`normalize_yields_unit_length_and_keeps_direction`, `normalize_survives_extreme_magnitudes`,
`add_sub_scale_dot_are_correct`, `length_matches_the_three_four_five_triangle`,
`length_squared_is_the_square_of_length`, `zero_is_the_additive_identity`,
`is_finite_detects_nan_and_infinity`, `operators_agree_with_the_named_methods`,
`negation_is_its_own_inverse`, `serde_round_trips`

```
error[E0432]: unresolved import `vec2::Vec2`
  --> boids-core/src/lib.rs:11:9
   |
11 | pub use vec2::Vec2;
   |         ^^^^^^^^^^ no `Vec2` in `vec2`

error[E0432]: unresolved import `super::Vec2`
 --> boids-core/src/vec2.rs:3:9
  |
3 |     use super::Vec2;
  |         ^^^^^^^^^^^ no `Vec2` in `vec2`

error[E0282]: type annotations needed
  --> boids-core/src/vec2.rs:75:24
   |
75 |             assert_eq!(v.normalize(), Vec2::ZERO, "normalize({v:?}) must be zero");
   |                        ^ cannot infer type

error: could not compile `boids-core` (lib test) due to 3 previous errors
```

**GREEN** — `Vec2 { x: f64, y: f64 }` with the inherent arithmetic, the `std::ops` impls
delegating to it, and a `normalize` that returns `ZERO` whenever the length is zero or
non-finite. `limit` was deliberately *not* written here — it is AC-2's cycle.

Two implementation choices were forced by `normalize_survives_extreme_magnitudes`, which is
exactly what writing that test first bought: `length` uses `f64::hypot` rather than
`sqrt(x*x + y*y)`, because the naive square-sum overflows to infinity for components above
~1e154 and the total-normalize guard would then silently convert that into `ZERO` — a
*wrong* answer for a perfectly well-defined direction. Symmetrically, `normalize` divides
instead of multiplying by `1.0 / len`, because that reciprocal overflows for subnormal
lengths and would put infinities back into the output of the very function whose job is to
keep them out. `length_squared` keeps the cheap square-sum, since neighbour queries compare
it against a squared radius.

**REFACTOR** — `cargo clippy` rejected the green implementation even though every test
passed:

```
error: method `add` can be confused for the standard trait method `std::ops::Add::add`
  --> boids-core/src/vec2.rs:36:5
   |
36 | /     pub fn add(self, o: Vec2) -> Vec2 {
   = note: `-D clippy::should-implement-trait` implied by `-D warnings`

error: method `sub` can be confused for the standard trait method `std::ops::Sub::sub`
error: could not compile `boids-core` (lib) due to 2 previous errors
```

The contract requires *both* spellings, so this is a narrow, justified suppression:
`#[expect(clippy::should_implement_trait, reason = ...)]` on those two methods only —
`expect` rather than `allow`, so it becomes an error if the lint ever stops applying.
Doc comments were added explaining why the inherent form earns its place next to the
operator. Tests stayed green.

---

### AC-2 — `limit()` never returns a vector longer than the cap

**RED** — `limit_shortens_a_vector_over_the_limit`,
`limit_leaves_a_vector_exactly_at_the_limit_untouched`,
`limit_leaves_a_shorter_vector_untouched`, `limit_never_exceeds_the_limit`,
`limit_preserves_direction_when_it_clamps`, `limit_to_zero_is_zero`,
`limit_with_a_degenerate_maximum_stays_finite`, `limit_of_a_degenerate_vector_stays_finite`

```
error[E0599]: no method named `limit` found for struct `Vec2` in the current scope
   --> boids-core/src/vec2.rs:257:19
    |
 17 | pub struct Vec2 {
    | --------------- method `limit` not found for this struct
...
257 |         let l = v.limit(2.0);
    |                   ^^^^^ method not found in `Vec2`

error: could not compile `boids-core` (lib test) due to 12 previous errors
```

**GREEN** — Guard the degenerate cap, guard a non-finite length, return `self` untouched
when `len <= max`, otherwise scale by `max / len`.

The three required cases (well over / exactly at / under the cap) are each their own test,
and `limit_never_exceeds_the_limit` states AC-2 as a property over 50,000 randomised
vector/cap pairs drawn from this crate's `Rng`. The "exactly at" case asserts **bit
identity** (`assert_eq!`), not approximate equality: an implementation that unconditionally
scaled by `max / len` would return a value a rounding step away from its input, and that
perturbation applied every tick to every under-speed agent would break the reproducibility
contract further downstream.

**REFACTOR** — `limit_preserves_direction_when_it_clamps` and
`normalize_yields_unit_length_and_keeps_direction` had grown the same open-coded
cross-product-and-dot-product check. Extracted it to an `assert_same_direction` test helper
so the "parallel *and* same-facing" definition lives in one place. 27 tests green, clippy
clean.

---

### AC-4 — `wrap()` is correct for negative and multi-world-width offsets

**RED** — `wrap_handles_negative_coordinates`, `wrap_handles_multi_world_width_offsets`,
`wrap_upper_bound_is_exclusive`, `wrap_never_returns_the_upper_bound`,
`wrap_result_is_always_in_bounds`, `wrap_is_idempotent`,
`wrap_shifts_by_a_whole_number_of_world_widths`, `wrap_leaves_in_bounds_positions_untouched`,
`wrap_of_a_degenerate_world_stays_finite`, `wrap_of_a_non_finite_position_stays_finite`,
`new_stores_the_dimensions`, `serde_round_trips`

```
error[E0432]: unresolved import `world::World`
  --> boids-core/src/lib.rs:13:9
   |
13 | pub use world::World;
   |         ^^^^^^^^^^^^ no `World` in `world`

error[E0432]: unresolved import `super::World`
 --> boids-core/src/world.rs:3:9
  |
3 |     use super::World;
  |         ^^^^^^^^^^^^ no `World` in `world`

error: could not compile `boids-core` (lib test) due to 2 previous errors
```

**RED (second observation)** — The obvious one-liner `Vec2::new(p.x.rem_euclid(self.width),
p.y.rem_euclid(self.height))` was written first, specifically to check that the edge-case
tests were not vacuous. They were not — three of them failed:

```
thread 'world::tests::wrap_never_returns_the_upper_bound' panicked at boids-core/src/world.rs:97:13:
wrap(-1e-18) returned the exclusive upper bound: Vec2 { x: 100.0, y: 100.0 }

thread 'world::tests::wrap_of_a_non_finite_position_stays_finite' panicked at boids-core/src/world.rs:164:13:
wrap(Vec2 { x: NaN, y: 0.0 }) produced Vec2 { x: NaN, y: 0.0 }

failures:
    world::tests::wrap_never_returns_the_upper_bound
    world::tests::wrap_of_a_degenerate_world_stays_finite
    world::tests::wrap_of_a_non_finite_position_stays_finite

test result: FAILED. 37 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
```

This is the cycle's real finding, and it is worth stating plainly because the bug is
invisible by inspection: **`f64::rem_euclid` can return exactly its modulus.** For a tiny
negative input it evaluates `v + size`, and `-1e-18 + 100.0` rounds to `100.0`. A position
of exactly `width` violates the half-open `[0,width)` interval that every downstream bucket
index and seam calculation assumes — it would put an agent in a spatial-hash cell that does
not exist. An agent decelerating across the origin hits this, so it is reachable in a real
run, not just in a test.

**GREEN** — Extracted a `wrap_axis(v, size)` helper applying `rem_euclid` and then closing
all three holes: non-positive or non-finite `size`, non-finite `v`, and the
`wrapped >= size` rounding case.

**REFACTOR** — The guard was first written `if !(size > 0.0)`, which is NaN-safe but reads
as a double negative, and clippy rejected it:

```
error: unnecessary boolean `not` operation
  --> boids-core/src/world.rs:46:8
   |
46 |     if !(size > 0.0) || !v.is_finite() {
   |        ^^^^^^^^^^^^^
   = note: `-D clippy::neg-cmp-op-on-partial-ord` implied by `-D warnings`
```

Rewritten as `!size.is_finite() || size <= 0.0 || !v.is_finite()`, which states the NaN case
outright instead of relying on the reader knowing that `!(x > 0.0)` and `x <= 0.0` differ for
NaN. 40 tests green, clippy clean.

---

### AC-3 — minimum-image toroidal distance

**RED** — `distance_across_the_x_seam_is_the_short_way_round`,
`distance_across_the_y_seam_is_the_short_way_round`,
`displacement_points_the_short_way_and_is_signed`, `displacement_across_a_diagonal_seam`,
`displacement_does_not_require_wrapped_inputs`,
`distance_within_the_world_is_ordinary_euclidean`, `distance_to_self_is_zero`,
`displacement_never_exceeds_half_the_world`, `displacement_is_antisymmetric`,
`stepping_along_the_displacement_arrives_at_the_target`,
`distance_is_symmetric_and_matches_its_square`, `distance_never_exceeds_the_half_diagonal`,
`distance_is_translation_invariant_on_the_torus`, `distance_of_a_degenerate_world_stays_finite`

```
error[E0599]: no method named `distance` found for struct `World` in the current scope
   --> boids-core/src/world.rs:201:22
error[E0599]: no method named `distance_squared` found for struct `World` in the current scope
   --> boids-core/src/world.rs:203:22
error[E0599]: no method named `displacement` found for struct `World` in the current scope
   --> boids-core/src/world.rs:221:22

error: could not compile `boids-core` (lib test) due to 27 previous errors
```

**GREEN** — `displacement` reduces each raw axis difference by
`size * round(d / size)`; `distance` and `distance_squared` are `length()` and
`length_squared()` of that vector. The three AC-3 cases are pinned exactly (`assert_eq!`,
no epsilon): x-seam, y-seam, and a diagonal crossing where both components wrap at once.

Two decisions worth flagging for the code that builds on this. First, the round-and-subtract
form handles differences of **any** magnitude, so `displacement` does not require pre-wrapped
inputs — `displacement_does_not_require_wrapped_inputs` locks that in, because a neighbour
query that had to remember to wrap first would eventually forget. Second, at exactly half the
world size both images are equally short and the tie has to break *somewhere*;
`f64::round` breaks away from zero, which is arbitrary but deterministic, and
`displacement_is_antisymmetric` confirms the tie-break stays consistent in both directions
rather than making `displacement(a,b)` and `displacement(b,a)` disagree.

**REFACTOR** — `min_image_axis` had been written with the same three-clause degenerate guard
that `wrap_axis` already carried. Extracted both to a shared `axis_is_degenerate(v, size)`
predicate with the reasoning documented once. Building `World` across two cycles had also
left it with two separate `impl` blocks; merged into one. 54 tests green, clippy clean.

---

### AC-6 — canonical, permutation-invariant, field-sensitive state hash

**RED** — `hash_is_stable_for_the_same_input`, `hash_is_permutation_invariant`,
`hash_is_permutation_invariant_for_a_large_flock`, `hash_changes_when_any_single_field_changes`,
`hash_detects_a_one_ulp_change_in_every_agent_and_field`,
`hash_distinguishes_different_flock_sizes`, `hash_of_an_empty_flock_is_defined`,
`hash_treats_negative_zero_as_zero`, `hash_is_pinned_to_a_known_value`,
`hex_matches_the_numeric_hash`, `hex_is_sixteen_lowercase_hex_digits`,
`distinct_states_mostly_produce_distinct_hashes`

```
error[E0432]: unresolved import `world::Agent`
  --> boids-core/src/lib.rs:14:17
   |
14 | pub use world::{Agent, World};
   |                 ^^^^^ no `Agent` in `world`

error[E0432]: unresolved imports `super::state_hash`, `super::state_hash_hex`
 --> boids-core/src/hash.rs:3:17
  |
3 |     use super::{state_hash, state_hash_hex};
  |                 ^^^^^^^^^^  ^^^^^^^^^^^^^^ no `state_hash_hex` in `hash`
  |                 |
  |                 no `state_hash` in `hash`

error[E0432]: unresolved import `crate::world::Agent`
 --> boids-core/src/hash.rs:6:9
  |
6 |     use crate::world::Agent;
  |         ^^^^^^^^^^^^^^^^^^^ no `Agent` in `world`

error: could not compile `boids-core` (lib) due to 1 previous error
```

**GREEN** — Added `Agent { id, pos, vel }` to `world.rs`, then hand-written FNV-1a over an
explicit little-endian byte stream: agent count, then agents in ascending canonical-key
order, each contributing `id` and four canonicalised `f64` bit patterns.

The golden value in `hash_is_pinned_to_a_known_value` was computed from the documented
encoding by a separate Python implementation *before* the Rust was written, the same way the
SplitMix64 vectors were. It matched on the first green run, which is the actual evidence that
the Rust implements the stated spec rather than the spec being a description of whatever the
Rust happened to do. A hash captured from its own output would pin nothing.

Two things this cycle's tests forced that are easy to miss:

- **`-0.0` must hash as `0.0`.** They compare equal and denote the same position, but their
  bit patterns differ — and `wrap()` genuinely produces `-0.0`, since `(-0.0).rem_euclid(w)`
  is `-0.0`. Without `canonical_bits`, two physically identical flocks would hash
  differently and a resumed run would be reported as a divergence from an uninterrupted one.
  `NaN` is canonicalised for the same reason: it has many bit patterns and which one an
  operation yields is not guaranteed across platforms.
- **Sorting on the whole key, not just `id`.** Permutation invariance would otherwise depend
  silently on ids being unique. Ordering the complete `(id, pos, vel)` tuple makes the
  invariant unconditional.

`hash_detects_a_one_ulp_change_in_every_agent_and_field` flips a single mantissa bit in each
of the 4 fields of each of the 4 agents in turn — 16 assertions — so an implementation that
hashed only the first or last element cannot pass.

**REFACTOR** — The `(u32, u64, u64, u64, u64)` tuple appeared in three signatures with no
indication of what its fields were. Introduced a `CanonicalKey` type alias documenting that
it *is* the hashed encoding and that its derived `Ord` is what "ascending canonical order"
means. 66 tests green, clippy clean.

---

### AC-49 — automated purity guard on the kernel's dependencies

**RED** — `finds_dependencies_in_every_dependency_table`,
`finds_dependencies_declared_as_their_own_tables`, `finds_target_specific_dependencies`,
`ignores_non_dependency_tables`, `ignores_commented_out_dependencies`,
`a_hash_inside_a_string_does_not_start_a_comment`, `detects_every_forbidden_dependency`,
`detects_forbidden_dependencies_by_family_prefix`,
`treats_underscores_and_hyphens_as_the_same_crate`, `a_pure_manifest_reports_nothing`,
`boids_core_declares_no_forbidden_dependency`, `boids_core_dependencies_are_on_the_approved_list`

```
error[E0432]: unresolved imports `super::banned_dependencies`, `super::declared_dependencies`
  --> boids-core/tests/purity.rs:15:17
   |
15 |     use super::{banned_dependencies, declared_dependencies};
   |                 ^^^^^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^ no `declared_dependencies` in the root

error[E0425]: cannot find function `banned_dependencies` in this scope
   --> boids-core/tests/purity.rs:170:17

error[E0425]: cannot find function `normalize` in this scope
   --> boids-core/tests/purity.rs:201:41

error: could not compile `boids-core` (test "purity") due to 4 previous errors
```

**GREEN** — A hand-written manifest parser (no `toml` crate — taking a dependency in order to
police dependencies would rather undercut the point): quote-aware comment stripping,
quote-aware header splitting, and recognition of all three dependency tables plus
`[dependencies.name]` sub-tables and `[target."cfg(..)".dependencies]`.

A guard like this has a specific failure mode: it passes because it found nothing, and it
found nothing because it was looking in the wrong place. **A test asserting the real
`Cargo.toml` is clean would pass on day one no matter how broken the parser was.** So the
detector is driven by synthetic manifests that *do* contain `axum`, `diesel`, `tokio`, and
friends, and `detects_every_forbidden_dependency` asserts each one is caught. Only then is
the same function pointed at the real manifest. This also avoids editing `Cargo.toml` to
prove the point, which would have broken the concurrent builds of other crates.

Matching is by crate **family** (`autumn-harvest` catches `autumn-harvest-macros`) and
normalizes `_` to `-`, since Cargo treats those spellings as the same crate and a guard that
did not could be side-stepped by a rename.

**REFACTOR** — Two fixes, one of which was found by the guard turning on its author:

1. `boids_core_dependencies_are_on_the_approved_list` failed on first run:

```
thread 'boids_core_dependencies_are_on_the_approved_list' panicked at boids-core/tests/purity.rs:345:5:
boids-core declares unapproved dependencies: ["serde_json"]
```

   `serde_json` is a legitimate dev-dependency, but `normalize` rewrites it to `serde-json`
   while the approved list held the underscored spelling — the comparison could never match.
   A real bug in the guard, caught because the assertion was exercised against real input
   rather than assumed correct. The list now stores normalized spellings with a comment
   justifying each entry.

2. Clippy rejected the nested conditional in the parser
   (`-D clippy::collapsible-if`); rewritten using a let-chain.

The approved-list test is deliberately stricter than AC-49 requires. A deny-list only stops
the crates someone thought to name, so `hyper` or `axum-core` would sail past it; requiring
each dependency to be listed with a justification makes adding one a visible decision rather
than a silent one. The failure message says where to add the entry and where the dependency
probably belongs instead.

---

## Summary

| AC | Behaviour | Where |
|---|---|---|
| AC-1 | `Vec2` arithmetic; total `normalize` | `src/vec2.rs` |
| AC-2 | `limit()` never exceeds the cap | `src/vec2.rs` |
| AC-3 | Minimum-image toroidal distance | `src/world.rs` |
| AC-4 | `wrap()` for negative and multi-width offsets | `src/world.rs` |
| AC-5 | Seeded reproducible PRNG, no `rand` | `src/rng.rs` |
| AC-6 | Canonical order-independent state hash | `src/hash.rs` |
| AC-49 | Automated dependency purity guard | `tests/purity.rs` |

Three bugs in this foundation were found by a test rather than by inspection, and all three
would have been near-invisible in review:

1. **`f64::rem_euclid` can return exactly its modulus** for tiny negative inputs, breaking
   the half-open `[0,width)` interval that every spatial-hash bucket index depends on.
2. **`-0.0` and `0.0` have different bit patterns**, and `wrap()` produces `-0.0`, so a
   bitwise state hash would report two identical flocks as divergent.
3. **`sqrt(x*x + y*y)` overflows** for components above ~1e154, which would make
   `normalize()` return `ZERO` for a perfectly well-defined direction.

That is the argument for the discipline, not the log itself: each was found by writing the
adversarial case down before the code existed.
