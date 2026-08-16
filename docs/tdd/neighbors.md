# TDD log — neighbourhood queries

Evidence for **AC-52** ("every feature was built red → green → refactor") covering
`boids-core/src/neighbors.rs`: the naive O(N²) query (**AC-7**), the spatial hash
(**AC-8**), the equivalence property test (**AC-9**), the all-agents sequence agreement
(**AC-10**, kernel half), and the backend-agnostic dispatch (**AC-49**).

Every **RED** block below is real captured output from `cargo test -p boids-core neighbors`
at the moment the test existed and the implementation did not, trimmed to the interesting
lines. Nothing here is reconstructed.

**Cycle order.** AC-7 → AC-8 → AC-9 → AC-10/AC-49. The naive backend is built first
because it is the *oracle*: AC-9 asserts the spatial hash equals it exactly, so the thing
being compared against has to exist and be trustworthy before the optimisation is written.

---

### AC-7 — naive toroidal neighbour query that never returns the agent itself

**RED** — `naive_returns_only_agents_inside_the_radius`,
`naive_never_returns_the_query_agent_itself`, `naive_boundary_is_inclusive`,
`naive_finds_a_neighbour_across_the_wrap_seam`, `naive_returns_indices_in_ascending_order`,
`naive_on_an_empty_world_returns_nothing`, `naive_with_a_single_agent_returns_empty_not_itself`,
`naive_with_all_agents_coincident_returns_everyone_else`,
`naive_with_zero_radius_returns_only_coincident_agents`,
`naive_with_a_degenerate_radius_returns_nothing`,
`naive_with_an_out_of_range_index_returns_nothing`

```
error[E0425]: cannot find function `neighbors_naive` in this scope
  --> boids-core/src/neighbors.rs:31:20
   |
31 |         assert_eq!(neighbors_naive(&agents, &w100(), 0, 10.0), vec![1, 2]);
   |                    ^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `neighbors_naive` in this scope
  --> boids-core/src/neighbors.rs:37:17
   |
37 |         let n = neighbors_naive(&agents, &w100(), 0, 50.0);
   |                 ^^^^^^^^^^^^^^^ not found in this scope

...

error: could not compile `boids-core` (lib test) due to 15 previous errors; 1 warning emitted
```

**GREEN** — Filter `0..agents.len()`, skipping `index` itself, keeping `j` where
`world.distance_squared(me.pos, agents[j].pos) <= r²`; the scan visits indices in order so
the ascending-order guarantee is free rather than a trailing sort.

**REFACTOR** — Pulled the degenerate-radius guard out into `radius_squared(radius) ->
Option<f64>` instead of inlining `radius * radius`. Two reasons, both correctness rather
than tidiness: squaring a *negative* radius silently turns "match nothing" into "match
everything within |r|", and the spatial hash must use the byte-identical comparison or
AC-9 will report set mismatches that are really rounding disagreements. One helper, one
semantics. Tests stayed green.

**Decisions recorded here because downstream modules depend on them:**

| Question | Decision |
|---|---|
| Boundary at exactly `radius` | **Inclusive** — `d <= r`, evaluated as `d² <= r²` |
| Agent as its own neighbour | **Never** — excluded by *index*, so a coincident twin is still found |
| `radius == 0.0` | Legal: matches only agents at the *identical* position |
| `radius < 0.0` or `NaN` | Empty set (not a panic, and not silently `|r|`) |
| `radius == INFINITY` | Every other agent |
| `index` out of range | Empty set, no panic |
| Result order | Ascending index, both backends |

---

### AC-8 — a spatial hash answering the same query shape

**RED** — `spatial_hash_answers_the_same_query_shape_as_naive`,
`spatial_hash_never_returns_the_query_agent_itself`, `spatial_hash_boundary_is_inclusive`,
`spatial_hash_returns_indices_in_ascending_order`,
`spatial_hash_cell_is_never_smaller_than_the_radius`,
`spatial_hash_handles_a_world_that_is_not_a_multiple_of_the_cell_size`,
`spatial_hash_on_an_empty_world_builds_and_returns_nothing`,
`spatial_hash_with_a_single_agent_returns_empty_not_itself`,
`spatial_hash_with_all_agents_coincident_returns_everyone_else`,
`spatial_hash_with_zero_radius_returns_only_coincident_agents`,
`spatial_hash_with_a_degenerate_radius_returns_nothing`,
`spatial_hash_with_an_out_of_range_index_returns_nothing`,
`spatial_hash_queried_wider_than_it_was_built_stays_exact`,
`spatial_hash_built_for_a_different_agent_slice_stays_exact`

```
error[E0433]: failed to resolve: use of undeclared type `SpatialHash`
   --> boids-core/src/neighbors.rs:210:20
    |
210 |         let grid = SpatialHash::build(&agents, &world, radius);
    |                    ^^^^^^^^^^^ use of undeclared type `SpatialHash`
    |
help: there is an enum variant `crate::NeighborBackend::SpatialHash`; try using the variant's enum
    |
210 -         let grid = SpatialHash::build(&agents, &world, radius);
210 +         let grid = crate::NeighborBackend::build(&agents, &world, radius);
    |

... 13 further `SpatialHash` resolution errors ...

error: could not compile `boids-core` (lib test) due to 15 previous errors
```

(14 of those 15 are the ones above. The 15th, `unresolved import
super::collision_count --> boids-core/src/metrics.rs:151`, is a *different* agent's red
phase in a module this cycle does not touch — the crate's test binary is shared, so a
concurrent red elsewhere shows up in the same output.)

**GREEN** — Counting sort into a CSR layout: `axis_cells` picks the largest cell count
whose cells are still at least `radius` across, `trim_to_budget` halves axes until there
are no more cells than agents, then one pass tallies per-cell counts, a prefix sum turns
them into offsets, and a second pass places agent indices. A query locates the agent's cell
and scans the 3x3 block around it, applying the same `radius_squared` predicate as the
naive backend, then sorts.

The 3x3 block in this first draft **clamped** at the grid edge (`cy.saturating_sub(1)..=(cy+1).min(rows-1)`),
which is correct for every test in this cycle — they are all interior configurations — and
wrong on a torus. That is left for the next cycle to catch rather than fixed on suspicion:
the point of AC-9 is to be the thing that finds it.

**REFACTOR** — none needed; the CSR build and the query were written in their final shape.

Two design decisions worth recording, both chosen so that a *wrong* answer is never the
failure mode:

* A query radius **larger** than the build radius cannot be covered by 3x3, so the grid
  falls back to `neighbors_naive` instead of under-reporting.
* A grid handed a slice of a different length than it was built from is stale; it also
  falls back. (A same-length slice with moved positions is undetectable — hence the
  "rebuild every tick" contract in the type's doc comment.)

---

### AC-9 — THE EQUIVALENCE PROPERTY TEST

**RED** — `spatial_hash_is_set_equal_to_naive_over_randomised_configurations`

256 randomised configurations from `Rng::seeded`, each reproducible from a printed seed,
varying agent count (including 0, 1, 2), world shape (square, non-square, long-and-thin,
and worlds only fractions of a unit across), and radius (0, 1e-9-ish, exactly half the
world, and larger than the whole world), with agents deliberately parked within 2% of a
seam, on both seams at once, and at coincident positions. The first configuration that
disagreed:

```
thread 'neighbors::tests::spatial_hash_is_set_equal_to_naive_over_randomised_configurations'
panicked at boids-core/src/neighbors.rs:711:17:
assertion `left == right` failed: backends disagree — reproduce with random_case(0x1439da59bcbed67e)
agent 0 at Vec2 { x: 1.1694546217779789, y: 0.40037957253018447 }
World { width: 3.473434030831799, height: 2.230029367246862 } radius 0.7330387711985906
grid (4, 3) cells of Vec2 { x: 0.8683585077079498, y: 0.7433431224156206 }
agents: [ ... 38 agents elided ... ]
  left: [1, 4, 13, 19, 20, 21, 32, 37]
 right: [1, 4, 32, 37]
```

Exactly the predicted failure: agent 0 sits at `y = 0.400` in a world only `2.230` tall,
and agents 13, 19, 20 and 21 sit at `y ≈ 2.21` — a toroidal distance of about `0.42`, well
inside the `0.733` radius, but two grid rows away in raw index terms. The naive backend
(`left`) sees them; the clamped 3x3 block (`right`) never looks at row 2.

**GREEN** — Replaced the clamped index range with `axis_span(c, n)`, which returns the
**distinct** wrapped cell indices `{c-1, c, c+1} mod n`. Two halves to the fix, and the
generated corpus needed both: wrapping (cell 0's left neighbour is cell `n-1`) and
distinctness (with `n <= 2` the three offsets collide, and a cell visited twice would
report its agents twice — a *duplicate*, not a missing entry, which the exact-equality
assertion catches just as loudly).

**REFACTOR** — none needed at this step; `axis_span` was extracted as its own documented
function rather than inlined into the loop precisely because the "distinct **and** wrapped"
requirement is the subtle part.

**Result**: 26 tests green, including all 256 configurations x every agent in each.

---

### AC-10 (kernel half) + AC-49 — every agent, every backend, one call site

**RED** — `both_backends_agree_for_every_agent_over_a_sequence_of_queries`,
`swapping_the_backend_changes_nothing_at_the_call_site`,
`neighbors_with_is_exact_even_without_a_prebuilt_grid`,
`the_naive_backend_ignores_a_supplied_grid`

```
error[E0425]: cannot find function `neighbors_with` in this scope
   --> boids-core/src/neighbors.rs:784:29
    |
 57 | pub fn neighbors_naive(agents: &[Agent], world: &World, index: usize, radius: f64) -> Vec<usize> {
    | ------------------------------------------------------------------------------------------------ similarly named function `neighbors_naive` defined here
...
784 |                 let naive = neighbors_with(NeighborBackend::Naive, &agents, &world, None, i, radius);
    |                             ^^^^^^^^^^^^^^
    |
help: a function with a similar name exists
    |
784 -                 let naive = neighbors_with(NeighborBackend::Naive, &agents, &world, None, i, radius);
784 +                 let naive = neighbors_naive(NeighborBackend::Naive, &agents, &world, None, i, radius);
    |

... 4 further `neighbors_with` resolution errors ...

error: could not compile `boids-core` (lib test) due to 5 previous errors
```

**GREEN** — `neighbors_with(backend, agents, world, grid, index, radius)` matches on the
`NeighborBackend` enum: naive ignores the grid, spatial-hash uses the supplied one, and
with no grid supplied it builds a throwaway one so the answer is right even when the
caller has nothing prepared.

The AC-10 test does what a tick does: one grid, built once, queried for **every** agent in
a 200-agent world at four radii (3, 12.5, 40, 90 — from "almost nobody" to "over half the
world"), then queried again in reverse order to prove a grid is read-only state that gives
the same answers on the second pass. Both backends are called through the *same* call site
with only the enum value differing, which is the concrete form of AC-49's "swapping
backends requires zero changes at call sites". The multi-tick `state_hash` half of AC-10
belongs to `sim.rs` and is not written here.

**REFACTOR** — Three cleanups, all driven by `cargo clippy -p boids-core --all-targets -- -D warnings`:

* `!(ideal >= 1.0)` and `!(radius <= self.radius)` were flagged by
  `clippy::neg_cmp_op_on_partial_ord` — negated comparisons on floats hide their `NaN`
  behaviour. Both were rewritten to state the `NaN` case out loud (`ideal < 1.0`;
  `self.radius.is_nan() || radius > self.radius`), which is the same behaviour said
  plainly.
* `r.next_u64() % 2 == 0` became `.is_multiple_of(2)`.
* `rustfmt --edition 2024` over the file.

Also added `spatial_hash_actually_prunes_a_realistically_shaped_run` during this pass. It
is a **guard, not a driver**, and it passed the moment it was written — recorded honestly
rather than dressed up as a cycle. It exists because every other test in the module would
still pass if the grid collapsed to one cell and quietly became the naive scan: correctness
tests cannot see the difference, so the optimisation needs its own tripwire (a
200x200 world, 80 agents, radius 25 must yield a grid where the 3x3 block is at most a
quarter of the cells).

---

## Summary

```
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 146 filtered out
```

| AC | Proven by |
|---|---|
| AC-7 | `naive_returns_only_agents_inside_the_radius`, `naive_never_returns_the_query_agent_itself`, `naive_boundary_is_inclusive`, `naive_finds_a_neighbour_across_the_wrap_seam`, plus the empty / single / coincident / zero-radius / degenerate-radius / out-of-range cases |
| AC-8 | `spatial_hash_answers_the_same_query_shape_as_naive`, `spatial_hash_cell_is_never_smaller_than_the_radius`, `spatial_hash_handles_a_world_that_is_not_a_multiple_of_the_cell_size`, and the same edge-case battery as AC-7 |
| AC-9 | `spatial_hash_is_set_equal_to_naive_over_randomised_configurations` — 256 seeded configurations x every agent, exact set equality |
| AC-10 (kernel half) | `both_backends_agree_for_every_agent_over_a_sequence_of_queries` |
| AC-49 (query half) | `swapping_the_backend_changes_nothing_at_the_call_site`, `neighbors_with_is_exact_even_without_a_prebuilt_grid`, `the_naive_backend_ignores_a_supplied_grid` |

### Constraints downstream modules must respect

1. **Rebuild the grid every tick**, after positions move. A grid is a snapshot; a
   same-length slice with moved agents is undetectably stale.
2. **Do not query wider than you built.** It still returns the right answer, but by
   falling back to an O(N) scan — build with the largest radius the tick will ask for
   (typically `neighbor_radius`, then filter down to `separation_radius` in `forces`).
3. **Do not reorder the returned indices.** They are ascending so that force summation is
   reproducible; `f64` addition is not associative, and the state hash is compared
   bit-for-bit.
4. **The boundary is inclusive** (`d <= r`) in both backends. A force that re-tests
   distance must use `<=` too, or an agent exactly at the radius will be counted as a
   neighbour with zero influence.
5. **`radius == 0` is legal** and matches coincident agents; a negative or `NaN` radius
   matches nothing rather than panicking.
