# Acceptance-criteria verification — final sign-off

Independent verification of every criterion in
[`docs/ACCEPTANCE_CRITERIA.md`](ACCEPTANCE_CRITERIA.md), performed by reading the
tests and the code rather than the TDD logs, running the suite, and driving the
running application over HTTP.

---

## Headline

**52 MET / 0 PARTIAL / 0 NOT MET** out of 52.

One criterion (**AC-38**) was **PARTIAL** on arrival and was closed during this
verification; see [Gaps closed](#gaps-closed-during-verification). The prior
audit's two weak criteria, **AC-14** and **AC-27**, were re-checked from scratch
and are genuinely closed.

### The real numbers

`cargo test --workspace`, run twice consecutively against the live
`boidboard_test` database, **after** the gap-closing test was added:

| Binary | Tests | Run 1 | Run 2 |
|---|---|---|---|
| `boidboard` unittests (`src/lib.rs`) | 11 | ok | ok |
| `boidboard` unittests (`src/main.rs`) | 0 | ok | ok |
| `boidboard` `tests/integration.rs` | 18 | ok | ok |
| `boidboard` `tests/persistence.rs` | 15 | ok | ok |
| `boidboard` `tests/web.rs` | 65 | ok | ok |
| `boidboard` `tests/workflow.rs` | 33 | ok | ok |
| `boids-core` unittests (`src/lib.rs`) | 238 | ok | ok |
| `boids-core` `tests/cross_process.rs` | 1 | ok | ok |
| `boids-core` `tests/purity.rs` | 12 | ok | ok |
| **Total** | **393** | **0 failed** | **0 failed** |

Doc-tests: 0 run, 28 ignored (24 `boidboard` + 4 `boidboard` merged, 0
`boids_core`) — all `no_run`/`ignore` illustrative blocks on the workflow and
activity items.

Both runs exited `0` and produced **identical** per-binary counts. Before the
new test the totals were 392/392 across two earlier consecutive runs, also
identical — so **AC-40's twice-consecutively requirement was observed four
times**, not inferred.

### Other gates, run live

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` | **exit 0**, on a forced fresh compile (`cargo clean -p boids-core -p boidboard` first, so this is not a cached verdict) |
| `cargo fmt --all --check` | **exit 0** |
| `unsafe_code = "forbid"` | present in `boids-core/Cargo.toml:20` and `boidboard/Cargo.toml:30` |

### The application, driven over HTTP

`AUTUMN_PROFILE=dev cargo run`, then real requests. Not a test harness — the
actual server, the actual dev database.

* `GET /health` → `200`, `{"status":"ok","profile":"dev",...}`.
* `GET /runs/new` → `200`, **4 preset cards**, a `_csrf` token in the form.
* `POST /runs` (preset `classic-flock`, seed 99, 300 ticks) → `303` to
  `/runs/799`. Polling the row: `running|200` → `completed|300` with
  `final_state_hash = c60a569697205f5b` — the run really executed, in ~2s.
* `GET /runs/799` → `200`, 57 KB of server-rendered HTML containing **120
  `polygon.agent` marks, each with its own `rotate(…)`**, 30 trajectory ribbon
  groups, 4 sparklines, a provenance panel printing seed `99`, config hash
  `557a45cd…`, kernel `boids-core@0.1.0` and final hash `c60a5696…` — matching
  the database row exactly. No `<canvas>`, no `.wasm`, no cancel form (the run
  is terminal), no stuck badge (the run is healthy).
* `GET /runs/799/progress` → `200`, a bare fragment with **no `hx-trigger`** —
  polling really does stop at terminal.
* `GET /runs` → `200`, 200 run rows each with a status badge.
* `GET /compare?a=799&b=795` → `200`, both flocks rendered.
* A second run (`nervous-swarm`, 5000 ticks) showed a **cancel form** and
  `hx-trigger="every 2s"` while live; `POST /runs/{id}/cancel` → `303`; the run
  reached `cancelled` at tick 50 with **51 frames retained**.
* `GET /api/harvest/workflows` and `/api/harvest/ui/workflows` → **404**. The
  management API is not mounted by default.

---

## The 52 criteria

Verdicts are **MET / PARTIAL / NOT MET**. Evidence is a specific test name and
file, an observed runtime behaviour, or a `file:line`. Where a test is stronger
or weaker than its criterion's literal wording, the row says so.

### A. Simulation kernel — geometry and determinism

| AC | What it requires (short) | Verdict | Evidence |
|---|---|---|---|
| **AC-1** | `Vec2` arithmetic; `normalize()` total, `ZERO.normalize()` is zero not `NaN` | **MET** | `boids-core/src/vec2.rs`: `add_sub_scale_dot_are_correct`, `length_matches_the_three_four_five_triangle`, `zero_normalize_is_exactly_zero_not_nan` (asserts `== Vec2::ZERO`, and `!is_nan()` on both components), `non_finite_normalize_is_exactly_zero`, `normalize_survives_extreme_magnitudes` |
| **AC-2** | `limit()` never exceeds the cap; shorter vectors untouched | **MET** | `vec2.rs`: `limit_never_exceeds_the_limit`, `limit_shortens_a_vector_over_the_limit`, `limit_leaves_a_shorter_vector_untouched`, `limit_leaves_a_vector_exactly_at_the_limit_untouched` (boundary is inclusive, no clamp), `limit_preserves_direction_when_it_clamps` |
| **AC-3** | Toroidal minimum-image distance; `x=1`↔`x=99` in a 100-wide world is 2 | **MET** | `boids-core/src/world.rs::distance_across_the_x_seam_is_the_short_way_round` asserts the AC's literal case: `w.distance((1,0),(99,0)) == 2.0`, symmetric, and `distance_squared == 4.0`. Also `distance_across_the_y_seam_…`, `displacement_never_exceeds_half_the_world`, `displacement_is_antisymmetric` |
| **AC-4** | `rem_euclid` wrapping for negative and multi-width offsets | **MET** | `world.rs::wrap_handles_multi_world_width_offsets` asserts `wrap((-250,350)) == (50,50)`, `wrap((350,-250)) == (50,50)`, `wrap((-1e6,1e6)) == ZERO`. Plus `wrap_handles_negative_coordinates`, `wrap_result_is_always_in_bounds`, `wrap_upper_bound_is_exclusive`, `wrap_is_idempotent` |
| **AC-5** | Seeded self-contained PRNG; no `rand::thread_rng` in the kernel | **MET** | `boids-core/src/rng.rs`: `same_seed_produces_same_sequence`, `different_seeds_produce_different_sequences`, `matches_published_splitmix64_vectors` (pinned to published SplitMix64 vectors, so it is a *known* PRNG, not an ad-hoc one). Absence of `rand` is machine-checked: `tests/purity.rs::boids_core_declares_no_forbidden_dependency` (`rand` is in `FORBIDDEN`) and `boids_core_dependencies_are_on_the_approved_list` (`APPROVED` is exactly `serde`, `serde-json`) |
| **AC-6** | Canonical `state_hash()`: order-independent, sensitive to every field | **MET** | `boids-core/src/hash.rs`: `hash_is_permutation_invariant` and `…_for_a_large_flock` (order-independence), `hash_changes_when_any_single_field_changes` and `hash_detects_a_one_ulp_change_in_every_agent_and_field` (sensitivity at 1 ULP, per agent, per field), `hash_treats_negative_zero_as_zero`, `hash_is_pinned_to_a_known_value`, `hash_distinguishes_different_flock_sizes` |

### B. Simulation kernel — neighbourhood queries

| AC | What it requires (short) | Verdict | Evidence |
|---|---|---|---|
| **AC-7** | Naive O(N²) query exact under toroidal distance; never returns self | **MET** | `boids-core/src/neighbors.rs`: `naive_never_returns_the_query_agent_itself`, `naive_returns_only_agents_inside_the_radius`, `naive_boundary_is_inclusive`, `naive_finds_a_neighbour_across_the_wrap_seam`, `naive_with_a_single_agent_returns_empty_not_itself`, `naive_with_all_agents_coincident_returns_everyone_else` |
| **AC-8** | A spatial-hash backend exists as a second implementation | **MET** | `SpatialHash` + `NeighborBackend::{Naive,SpatialHash}` in `neighbors.rs`; `spatial_hash_answers_the_same_query_shape_as_naive`, and `spatial_hash_actually_prunes_a_realistically_shaped_run` proves it is a real index rather than a renamed scan |
| **AC-9** | Randomised equivalence property test, **exact set equality** | **MET** | `neighbors.rs::spatial_hash_is_set_equal_to_naive_over_randomised_configurations`. **Checked in detail:** 256 cases from decorrelated seeds (golden-ratio stride); agent counts 0/1/2/3–40; four world shapes including tiny (0.5–4) and long-thin (20–300 × 1–10); six radius regimes including 0, 1e-9–1e-3, wider-than-world, and exactly `w/2`; positions deliberately biased onto the x-seam, the y-seam, corners, and onto *other agents* (coincident). For **every agent of every case** it sorts both results and `assert_eq!`s the vectors — exact equality, no tolerance, no sampling. Four corpus-coverage assertions (`empties > 0`, `coincident > 0`, `wider_than_world > 0`, `zero_radius > 0`) fail the test if the generator ever stops producing the hard shapes, so it cannot decay into an expensive no-op. This is a genuine property test, not a token one |
| **AC-10** | Multi-tick run: identical `state_hash()` under both backends | **MET** | `boids-core/src/sim.rs::both_neighbour_backends_produce_the_same_run` — 400 ticks, equal `state_hash`, **bit-identical** agent arrays via `assert_bit_identical`, and equal 400-sample metric series. `the_backends_agree_across_a_range_of_scenario_shapes` repeats it over 5 world/radius/density shapes. `neighbors.rs::both_backends_agree_for_every_agent_over_a_sequence_of_queries` covers the per-tick query pattern with a `total > 1_000` non-sparsity guard |

### C. Simulation kernel — steering forces

| AC | What it requires (short) | Verdict | Evidence |
|---|---|---|---|
| **AC-11** | Separation; coincident agents give a finite, deterministic, non-random force | **MET** | `boids-core/src/forces.rs`: `separation_pushes_away_from_a_close_neighbour`, `separation_pushes_harder_the_closer_the_neighbour`, `separation_of_coincident_agents_is_finite`, `separation_of_coincident_agents_is_deterministic`, `coincident_agents_are_pushed_in_opposite_directions`. The tie-break is keyed on agent identity (`mix64` of the id pair), so it is deterministic by construction; `a_pile_of_coincident_agents_is_finite_and_deterministic` covers the N-way pile |
| **AC-12** | Alignment to mean heading; antiparallel neighbours → zero, not `NaN` | **MET** | `forces.rs`: `alignment_steers_toward_the_mean_neighbour_heading`, `alignment_of_antiparallel_neighbours_is_zero_not_nan`, `alignment_vanishes_once_the_agent_matches_the_flock`, `alignment_with_no_neighbours_is_zero` |
| **AC-13** | Cohesion via **toroidal displacement**, so `x=1`/`x=99` cohere across the seam | **MET** | `forces.rs::cohesion_pulls_across_the_seam_not_toward_the_middle` — the direct falsifier of a coordinate-mean centroid. Plus `cohesion_pulls_toward_the_neighbour_centroid`, `cohesion_across_the_seam_stays_small` |
| **AC-14** | Goal seeking; with `goal_weight = 0` the goal is **provably irrelevant** | **MET** *(was WEAK; re-verified independently)* | Steering half: `forces.rs::goal_seek_steers_toward_the_goal`, `goal_seek_takes_the_short_way_round_the_seam`, `goal_seek_weakens_on_arrival`. **Zero-weight half — read line by line at `sim.rs:1708`:** `with_a_zero_goal_weight_the_goal_position_is_provably_irrelevant` runs 400 ticks for five goals (`None`, three in-world positions, and one at `(-4000, 7500)` far outside the world) and requires an **identical `state_hash`** for all five, having first asserted `validate` accepts each. It then adds an explicit non-vacuity clause: the *same* comparison at `w_goal = 0.4` must `assert_ne!`, so the test cannot pass against a goal force that was deleted outright. This is exactly the AC's wording ("two runs with different goals produce identical state hashes") and is stronger than it. Red phase captured at `docs/review-fixes-kernel.md:604` |
| **AC-15** | Obstacle avoidance: repels, pushes outward from inside, head-on broken deterministically | **MET** | `forces.rs`: `obstacle_avoidance_pushes_an_approaching_agent_away`, `obstacle_avoidance_pushes_an_agent_inside_radially_outward`, `obstacle_avoidance_pushes_harder_from_deeper_inside`, `a_head_on_agent_at_the_centre_is_pushed_sideways`, `obstacle_avoidance_at_the_exact_centre_is_deterministic`, `a_motionless_agent_at_the_centre_is_still_pushed_out`, `obstacle_avoidance_crosses_the_seam`, `an_obstacle_whose_influence_band_overflows_contributes_nothing` |
| **AC-16** | Weighted sum of five forces; each weight independently and monotonically affects the result | **MET** | `forces.rs::blend_is_the_weighted_sum_of_its_components` pins the blend to the exact linear combination of independently computed components — from which monotonicity in each weight follows. `each_weight_moves_the_blend_along_its_own_component` then checks it behaviourally for all five weights: raising `w` from 1.0 to 2.0 must move the blend with `dot(component) > 0` **and** `cross ≈ 0` (i.e. along that component's axis and no other). `blend_is_clamped_to_max_force`, `blend_never_exceeds_max_force` cover the cap. *Note:* the behavioural check is a single 1.0→2.0 increment per weight rather than a monotone sweep; the exact-linearity assertion is what actually carries the "monotonically" clause |
| **AC-17** | Zero neighbours → every flocking force zero; no erratic movement | **MET** | `forces.rs::with_no_neighbours_the_flocking_forces_are_exactly_zero` (exact `Vec2::ZERO`, not near-zero), `a_lone_agent_with_nothing_to_react_to_gets_no_force`, `with_no_neighbours_goal_and_avoidance_still_act` (the correct negative: only the *flocking* forces vanish), `a_neighbour_list_naming_the_agent_itself_is_ignored` |

### D. Simulation kernel — integration

| AC | What it requires (short) | Verdict | Evidence |
|---|---|---|---|
| **AC-18** | Double-buffered step: shuffle → step → un-permute is **bit-identical** | **MET** | `sim.rs::stepping_is_independent_of_agent_array_order` and `order_independence_survives_a_long_run`, both ending in `assert_bit_identical` (raw `f64::to_bits` comparison, not `approx`). `permuting_a_pile_of_coincident_agents_changes_nothing` was added by the kernel review specifically because the original permutation never exercised coincident agents, leaving the identity-keyed separation tie-break unenforced. `step_leaves_the_input_state_untouched` pins the "double-buffered" part directly |
| **AC-19** | `dt`-scaled integration; halving `dt` **converges** rather than changing the answer | **MET** | `sim.rs::halving_dt_converges_rather_than_changing_the_answer` runs the same 4.0 units of simulated time at `dt` = 0.5/0.25/0.125/0.0625 (exact binary fractions, with an assertion that each covers precisely the same simulated time), measures the largest per-agent disagreement between successive refinements, and requires the gaps to **strictly decrease** and to shrink by a factor below 0.75 each time — plus a non-vacuity guard that the coarsest pair disagrees by more than 1e-6 in the first place. `dt_scales_the_force_so_a_finer_step_is_not_a_slower_simulation` covers the force-scaling half |
| **AC-20** | Batch equivalence: 1×1000 == 10×100 with serialize/deserialize between | **MET** | `sim.rs::one_long_batch_equals_ten_checkpointed_batches` — the `checkpointed` helper forces the state through `serde_json` to a `String` and back at **every** seam, and the test requires equal `state_hash`, `assert_bit_identical` agent arrays, and equality of all **1000** metric samples tick-for-tick (not just endpoints). `batching_is_equivalent_for_every_way_of_cutting_a_run` repeats it for batch sizes 1, 2, 3, 7, 16, 60, 120, 240 including uneven partitions. `metrics_are_sampled_on_the_absolute_tick_not_the_batch_offset` and `a_batch_never_reports_the_state_it_was_handed` pin the two subtleties that make the contract hold |
| **AC-21** | Stability invariants on **every tick** of a long adversarial run | **MET** | `sim.rs::a_long_adversarial_run_never_breaks_its_invariants` — per tick, per agent, across multiple adversarial scenarios (≥ 20 000 agent-ticks each, asserted): position and velocity finite, `speed <= max_speed`, position inside the world, **and the steering force recomputed and asserted `<= max_force`** rather than assumed. Critically it also asserts *distance travelled* `<= max_speed * dt` under toroidal distance — the kernel review found that asserting stored speed alone left the classic clamp-order bug (clamp the stored velocity, integrate with the unclamped one) undetectable. A `wraps > 100` coverage guard stops the in-bounds assertion from being vacuous |
| **AC-22** | Reproducible **across OS processes** | **MET** | `boids-core/tests/cross_process.rs::the_same_scenario_hashes_identically_in_a_separate_process` — computes hashes in-process for 4 seeds, then spawns a genuinely separate process (`cargo run --example state_hash`, `examples/state_hash.rs`) with the tick count and seeds on the command line, and compares the 250-tick hex hashes. A distinctness guard (`4 distinct hashes`) stops a degenerate scenario making the comparison vacuous. Confirmed to have really executed here (`cargo` present; the binary took 0.97–1.82 s, consistent with a child spawn). *Caveat:* it self-skips with a loud `eprintln!` if `cargo` is unavailable — legitimate for a packaged binary, but it means CI must ensure a toolchain is present or the criterion silently goes unchecked |

### E. Metrics

| AC | What it requires (short) | Verdict | Evidence |
|---|---|---|---|
| **AC-23** | Polarization exactly 1.0 aligned, exactly 0.0 for 0°/90°/180°/270° | **MET** | `boids-core/src/metrics.rs::ac23_polarization_is_exactly_one_for_a_perfectly_aligned_flock` uses `assert_eq!(…, 1.0)` with four *different speeds* on one heading (so it measures direction, not magnitude). `ac23_polarization_is_exactly_zero_for_four_agents_at_right_angles` asserts both `< 1e-12` and `== 0.0` exactly, again at differing speeds. Plus `ac23_zero_velocity_agents_have_no_heading_and_never_produce_nan`, `ac23_polarization_is_always_within_the_unit_interval` |
| **AC-24** | Mean nearest-neighbour distance under toroidal distance | **MET** | `metrics.rs`: `ac24_mean_nearest_neighbor_distance_goes_the_short_way_round_the_seam`, `…_averages_per_agent_nearest_distances`, `…_ignores_the_agent_itself`, `…_of_fewer_than_two_agents_is_zero`, `…_never_exceeds_the_half_diagonal`, `…_is_finite_in_an_enormous_world` (the `+inf` → JSON `null` → undeserializable-row bug the kernel review found) |
| **AC-25** | Collisions counted as **unordered** pairs: 2 overlapping = 1 | **MET** | `metrics.rs::ac25_two_mutually_overlapping_agents_are_one_collision_not_two` is the AC's literal case. Plus `ac25_three_mutually_overlapping_agents_are_three_pairs`, `ac25_an_agent_is_never_counted_against_itself`, `ac25_collisions_are_counted_across_the_seam`, `ac25_the_radius_is_a_strict_upper_bound`, `ac25_collision_count_matches_an_independent_ordered_pair_scan` (independent oracle), `ac25_a_fully_coincident_flock_attains_the_pair_bound_exactly` (`n(n-1)/2`) |
| **AC-26** | Time-to-goal is an `Option`, `None` when never reached, never a sentinel | **MET** | `metrics.rs::ac26_time_to_goal_is_none_when_the_goal_is_never_reached` asserts `is_none()`, `== None`, **and explicitly `!= Some(usize::MAX)`** — it names and rejects the sentinel. Plus `ac26_time_to_goal_is_the_first_qualifying_tick`, `…_takes_the_first_crossing_not_a_later_one`, `…_threshold_is_inclusive`, `…_of_an_unreachable_threshold_is_none` |
| **AC-27** | Stuck detection fires on a trapped flock and **does not** fire on a healthy one | **MET** *(was WEAK; re-verified independently)* | **Kernel (positive + negative):** `metrics.rs::ac27_is_stuck_fires_on_a_centroid_oscillating_in_place` vs `ac27_is_stuck_does_not_fire_on_a_flock_cruising_steadily`; plus `…_looks_only_at_the_most_recent_window`, `…_handles_a_path_shorter_than_the_window`, `…_follows_a_path_across_the_seam`, `…_counts_a_full_lap_of_the_world_as_progress`, `…_is_monotone_in_the_threshold`. **Reachable in the product — this is what was missing and is now real:** `boidboard/src/analysis.rs::run_is_stuck` is the caller, and it is invoked from the run-detail handler at `boidboard/src/routes.rs:678`, feeding `RunDetail.stuck` which the page renders as `stuck_badge()` (`views/pages.rs:555,706`). Product tests, again both directions: `web.rs::ac27_a_flock_oscillating_in_place_is_reported_stuck` (**positive**) and `ac27_a_flock_that_keeps_making_progress_is_not_stuck` (**negative**), `ac27_stuckness_is_never_claimed_on_absent_evidence` (empty and short-of-window inputs), `ac27_stuckness_is_read_from_the_wire_form_the_workflow_actually_writes` (the frames are built through `SimState`'s real wire shape, so a decoder regression fails the test), and `ac27_the_run_detail_page_badges_a_stuck_run_and_only_a_stuck_run` (**exactly 1** badge when stuck, **exactly 0** when not). Live-verified: the healthy completed run 799 rendered **zero** `stuck-badge` elements. *Honest caveat:* the stimulus is a synthetic centroid path (oscillating vs cruising), not a flock simulated into a concave obstacle pen as the AC's prose describes. The parenthetical requirement — a positive and a negative assertion, so an always-"yes" detector fails — is fully met, and the synthetic path is a more controlled stimulus, but the literal scenario named in the AC is not simulated anywhere |
| **AC-28** | Metrics translation-invariant under any offset | **MET** | `metrics.rs::ac28_every_metric_is_invariant_under_translation` — **3 000** randomised flocks × 4 offsets each, where the offsets deliberately include ±5 000 (many world-widths), exact multiples of the world size, and negative multiples. Plus `ac28_fraction_arrived_is_invariant_when_the_goal_moves_with_the_world` and `ac28_stuck_detection_is_invariant_under_translation` |

### F. Durable workflow

| AC | What it requires (short) | Verdict | Evidence |
|---|---|---|---|
| **AC-29** | Workflow carries only `(run_id, next_tick)`; history O(1) in agent count | **MET** *(asserted, not assumed — checked as requested)* | `boidboard/tests/workflow.rs::ac29_workflow_history_carries_only_a_cursor_never_the_agent_array`. The `simulate_batch` mock genuinely seeds and hashes an `agent_count`-strong `SimState` on every call, so the flock demonstrably passes *through* the activity — only the cursor comes back. The test then drives the whole workflow at **10 agents and at 10 000** and asserts: the same number of events, the **same serialized history size in bytes**, and — via `assert_no_agent_state`, a recursive walk of every event payload — that no key named `agents`/`px`/`py`/`vx`/`vy` appears anywhere and that no JSON array anywhere exceeds 8 elements. The byte-equality assertion is the load-bearing one: a leaked flock cannot survive it |
| **AC-30** | Tested with `WorkflowTestEnv`, no live Postgres, mocked activities, virtual clock | **MET** | `workflow.rs::ac30_the_workflow_runs_with_no_database_and_only_the_virtual_clock` — `WorkflowTestEnv::new()` with no injected state, all three activities mocked; asserts the workflow's `finished_at` equals `env.now()` (i.e. it read the *virtual* clock, not `Utc::now()`) and that `outcome.elapsed() == Duration::zero()` |
| **AC-31** | Passes a replay determinism check | **MET** | `workflow.rs::ac31_the_workflow_replays_without_diverging` uses `outcome.replay_check(…)` and requires `ReplayStatus::ReplaySucceeded`. Stronger than the AC: the private helper `assert_replays` is called at the end of **every** workflow test in the file, so replay determinism is re-proved on every code path the suite exercises, and `assert_replays_without_diverging` covers the failure path (no `NonDeterminismDetected`, whole history consumed) |
| **AC-32** | `simulate_batch` idempotent under `UNIQUE (run_id, tick)`; no tick gaps | **MET** | `workflow.rs::ac32_a_duplicated_simulate_batch_leaves_one_set_of_frames_and_no_gaps` (live Postgres). Persistence half: `persistence.rs::ac32_reinserting_the_same_frames_is_a_no_op`, `ac32_partially_overlapping_batches_insert_only_the_new_ticks`, `ac32_tick_gaps_reports_every_missing_tick`, `ac32_gaps_are_scoped_to_one_run`. The constraint itself is verified against the live catalogue by `persistence.rs::ac37_frames_has_unique_run_id_tick`, which queries `information_schema` and asserts the UNIQUE columns are exactly `["run_id","tick"]` |
| **AC-33** | Cancel signal honoured at the next batch boundary; terminal `Cancelled`, partial results intact | **MET** | `workflow.rs::ac33_a_cancel_signal_stops_the_run_at_the_next_batch_boundary`, `ac33_a_cancelled_run_keeps_the_frames_its_completed_batches_wrote`, `ac33_engine_cancellation_kills_the_run_without_issuing_another_command`; UI half `web.rs::ac33_a_live_run_offers_a_cancel_button_that_posts_to_the_cancel_route` and `ac33_a_queued_run_can_be_cancelled_but_a_terminal_one_cannot`; end-to-end `integration.rs::cancelling_a_run_from_the_ui_stops_it_and_keeps_the_frames_it_earned`. **Live-verified:** run 839 cancelled via `POST /runs/839/cancel` → `303`, reached `cancelled` at tick 50, **51 frames retained** |
| **AC-34** | Steer signal changes later batches and is recorded as provenance | **MET** | `workflow.rs::ac34_a_steer_signal_changes_params_for_later_batches_and_records_itself`, `ac34_steer_overrides_change_the_named_parameters_and_nothing_else` (the "and nothing else" half — an override must not perturb unrelated fields), `ac34_a_steer_applies_only_recognised_parameters_but_records_the_whole_payload` (unknown keys are stored verbatim but not applied), `ac34_a_recorded_signal_survives_redelivery_and_keeps_its_payload_verbatim`, `a_steer_that_invalidates_the_config_is_not_retried` |
| **AC-35** | `max_ticks` budget guardrail terminates deterministically | **MET** | `workflow.rs::ac35_max_ticks_bounds_a_run_to_exactly_ceil_max_over_batch_batches` (exactly `planned_batches(450,100) == 5` scheduled activities, `status == completed`, finalized exactly once) and `ac35_a_cursor_that_stops_advancing_trips_the_budget_guardrail` — a mock that never advances still terminates in 5 batches with `status == "budget_exceeded"`, i.e. the guardrail is what stops it, and the run says so rather than claiming completion |
| **AC-36** | Crash resume reaches the same final state hash as an uninterrupted run | **MET** *(central promise — checked in detail)* | `workflow.rs::ac36_a_run_resumed_from_its_postgres_checkpoint_reaches_the_same_final_state_hash`. The reference is `in_memory_reference_hash()` — a **pure-kernel, no-database** run — and the comment records *why*: comparing two batched runs compares the checkpoint machinery with itself, and a mutation test proved a systematic off-by-one would corrupt both sides identically and still pass. The interrupted run's pre-crash state goes out of scope entirely, the cursor is **rediscovered** via `max_tick` (asserted `== 100`), and after resuming the run must reach the identical 64-bit hash. Non-vacuity: `hash_at_the_crash != reference_hash`, so equality at tick 300 is a statement about the simulation. Stronger than the AC: **every one of the 301 intermediate frame hashes** is compared between the two runs, and `tick_gaps` must be empty across the resume seam |

### G. Persistence

| AC | What it requires (short) | Verdict | Evidence |
|---|---|---|---|
| **AC-37** | `scenario`/`run`/`frame` tables via embedded migrations, `UNIQUE (run_id, tick)` | **MET** | `persistence.rs::ac37_migrations_create_the_four_tables` and `ac37_frames_has_unique_run_id_tick`, the latter reading `information_schema.table_constraints`/`key_column_usage` from the live database and asserting the constraint's columns are exactly `["run_id","tick"]` in order — a schema fact, not a code fact |
| **AC-38** | Full provenance record stored, **and re-running from provenance reproduces the state hash** | **MET** *(was **PARTIAL** — closed during this verification)* | Storage half: `persistence.rs::ac38_run_round_trips_its_full_provenance_record` re-reads the row from the database and asserts kernel version, config hash, seed, final state hash, config snapshot and execution id. Reproduce half — **added here**: `workflow.rs::ac38_re_running_a_completed_run_from_its_provenance_reproduces_its_state_hash` drives a real run to completion through `simulate_batch_core` and `finalize_run_core`, re-reads the row, and then re-runs the kernel using **only** `config_snapshot`, `seed` and `ticks_completed` off that row, requiring the hash to equal the stored `final_state_hash`. Three non-vacuity clauses (perturbed seed, perturbed config, perturbed tick count must each produce a *different* hash) keep it from being trivially true. See [Gaps closed](#gaps-closed-during-verification) |
| **AC-39** | Editing a scenario cannot mutate a completed run's stored config | **MET** | `persistence.rs::ac39_editing_a_scenario_cannot_mutate_an_existing_runs_config` — the regression test for the mutation bug: a run owns a *copy*, never a live view of its scenario. Route-level counterpart at `web.rs:1700` asserts the run created by `POST /runs` carries the preset's config as its own snapshot |
| **AC-40** | Repository round-trips against live Postgres; whole suite passes **twice consecutively** | **MET** *(observed, not asserted by a test — as it must be)* | `persistence.rs::ac40_full_repository_round_trip_against_live_postgres` covers the round-trip. The twice-consecutively property was **observed four times** during this verification: two consecutive `cargo test --workspace` runs before the new test (392/392, identical per-binary counts, exit 0 both) and two after (393/393, identical, exit 0 both). Isolation is structural, not incidental: every DB test builds `TestApp::with_transactional_db`, a single-connection pool already inside `begin_test_transaction`, rolled back on drop, so nothing is committed and no cleanup code exists to forget |

### H. Web interface

| AC | What it requires (short) | Verdict | Evidence |
|---|---|---|---|
| **AC-41** | Run list with status and headline metrics | **MET** | View: `web.rs::ac41_runs_table_shows_one_row_per_run_with_status_and_headline_metrics`, `ac41_status_badge_carries_the_status_as_data_and_text`, `ac41_runs_table_says_so_when_there_are_no_runs`. Route: `ac41_run_list_route_renders_a_row_per_run_with_status_and_metrics`, `ac41_run_list_route_renders_an_empty_state_rather_than_a_blank_page`. **Live:** `GET /runs` → 200, 200 `.run-row` rows each carrying a `.status-badge` |
| **AC-42** | New-run form fronted by named presets | **MET** | `web.rs::ac42_presets_offer_at_least_four_distinct_named_starting_points`, `ac42_presets_have_genuinely_different_character` (distinct steering-weight profiles — two presets with identical parameters are one preset with two names), `ac42_every_preset_is_a_valid_simulation_config` (each passes the kernel's own `validate`), `ac42_by_slug_round_trips_and_rejects_unknown_slugs`, `ac42_new_run_route_is_fronted_by_preset_cards`, `ac42_posting_a_preset_slug_creates_a_run_carrying_that_presets_config`, `ac42_posting_an_unknown_preset_is_rejected_rather_than_silently_substituted`. **Live:** `GET /runs/new` → 4 preset cards (`classic-flock`, `nervous-swarm`, `highway`, `scatter`) |
| **AC-43** | Run detail renders inline SVG — oriented marks + trajectory ribbons — no SPA, no WASM | **MET** | `web.rs::ac43_flock_svg_is_inline_svg_with_a_world_sized_viewbox`, `ac43_every_agent_is_an_oriented_mark_rotated_to_its_heading`, `ac43_trajectory_svg_draws_one_ribbon_group_per_agent`, `ac43_trajectory_ribbons_break_at_the_toroidal_seam`, plus route-level `ac43_run_detail_route_renders_inline_svg_with_oriented_marks_and_ribbons` and `ac43_trajectory_ribbons_are_broken_at_the_seam_on_the_rendered_page`. The "no SPA, no WASM" clause is asserted twice: at view level (`web.rs:274` — no `<script>`, no `<canvas>`, no `.wasm` inside the flock SVG) and at route level (`web.rs:1801-1813` — every `script[src]` in the document is same-origin, none ends in `.wasm`, and the only one is `/static/js/htmx.min.js`). **Live:** the run-799 page carried 120 `polygon.agent` marks, each with its own `rotate(…)` in a `transform`, 30 trajectory groups, and zero `.wasm`/`<canvas>` |
| **AC-44** | Metric sparklines over the run's history | **MET** | `web.rs::ac44_metrics_panel_gives_every_headline_metric_its_own_sparkline`, `ac44_sparkline_maps_the_series_across_a_stable_viewbox`, `ac44_sparkline_survives_empty_single_and_flat_series` (the three degenerate series), `ac44_metrics_panel_renders_for_a_run_with_no_frames_yet`, route-level `ac44_run_detail_route_renders_one_sparkline_per_headline_metric` and `ac44_run_detail_route_survives_a_run_with_no_frames_at_all`. **Live:** 4 sparklines on the run-799 page |
| **AC-45** | htmx polling for in-progress runs; **fragment endpoint directly asserted** | **MET** *(checked as requested)* | The fragment endpoint is asserted **directly**: `web.rs::ac45_progress_endpoint_returns_the_fragment_and_nothing_else` issues `GET /runs/{id}/progress` and asserts it is a *fragment* — `assert_no_selector("html")`, `("head")`, `("body")`, `("svg.flock")` — with exactly one `#run-progress`, `hx-get` equal to its own URL, `hx-trigger="every 2s"`, and a `.status-badge[data-status="running"]`. Polling really stops: `ac45_progress_fragment_stops_polling_once_the_run_is_terminal` covers **all four** terminal statuses (`completed`, `cancelled`, `failed`, `budget_exceeded`) requiring both `hx-trigger` and `hx-get` to be absent, and `ac45_the_page_polls_while_running_and_stops_once_terminal` proves it end-to-end at the route: the running run's page has `#run-progress[hx-trigger="every 2s"]`, the completed run's page has `#run-progress` with **no** `hx-trigger` and **no** `hx-get`, and the completed run's *fragment* likewise — so the final swap is what stops the loop. **Live:** running run's page had `hx-trigger="every 2s"`; run 799's fragment after completion had none |
| **AC-46** | Compare view renders two runs together | **MET** | `web.rs::ac46_compare_view_renders_both_runs_and_highlights_the_differences`, `ac46_config_diff_lists_only_the_fields_that_actually_differ`, `ac46_config_diff_reports_fields_present_on_only_one_side`, `ac46_compare_view_says_so_when_two_runs_share_a_config` (an empty diff is a *finding*, rendered as `.config-diff-empty`, not a blank table), `ac46_compare_route_renders_both_runs_and_the_config_difference`, `ac46_compare_route_404s_when_a_run_is_missing`, `ac46_detail_page_offers_a_compare_form_prefilled_with_this_run`. **Live:** `GET /compare?a=799&b=795` → 200 with both flocks and a `.config-diff-empty` (same preset) |
| **AC-47** | Reproducibility hash surfaced in the UI | **MET** | `web.rs::ac47_provenance_panel_shows_every_field_needed_to_reproduce_a_run`, `ac47_provenance_panel_marks_a_missing_final_state_hash_as_pending` (a run still in flight says "pending" rather than showing a blank or a lie), route-level `ac47_run_detail_route_surfaces_the_reproducibility_hash`, and `integration.rs::the_detail_page_of_a_completed_run_draws_the_flock_and_its_hash` which asserts `dd.prov-final-state-hash` has the run's actual hash as its text. **Live:** the panel printed seed `99`, config hash `557a45cd…`, kernel `boids-core@0.1.0`, final hash `c60a569697205f5b` — byte-identical to the `runs` row |
| **AC-48** | Route tests use `TestApp` and assert on **HTML structure**, not string matching | **MET** *(checked as requested)* | The route section of `boidboard/tests/web.rs` (from the `route layer (AC-48…)` banner to EOF, 18 `#[tokio::test]`s) uses `autumn_web::test::TestApp`/`TestClient` throughout and asserts structurally: **44** `assert_selector`, **15** `assert_selector_count`, **13** `assert_no_selector`, **5** `assert_attr`, **10** `assert_text`. Only six `.contains(` calls appear in that whole region and **none of them is a body assertion** — five are the AC-50 source-scanning guard reading `include_str!`'d module text, and one checks that a `transform` attribute's *value* contains `rotate(`, which no selector can express. `integration.rs` uses the same selector family. *Note:* the pure-view layer above the banner uses a local `dom` start-tag scanner instead, deliberately — it exists so views can be proved renderable with no `TestApp` at all (AC-50) |

### I. Engineering quality

| AC | What it requires (short) | Verdict | Evidence |
|---|---|---|---|
| **AC-49** | `boids-core` free of Diesel/HTTP/Autumn Web/Harvest, proven automatically; backend swap needs zero test changes | **MET** | `boids-core/tests/purity.rs::boids_core_declares_no_forbidden_dependency` parses the crate's own `Cargo.toml` (via `CARGO_MANIFEST_DIR`, so it does not depend on cwd) and rejects any dependency in the forbidden families, matching on family prefixes and normalising `-`/`_`. `boids_core_dependencies_are_on_the_approved_list` goes further and fails on *any* dependency outside `{serde, serde-json}`, so a new crate is a deliberate edit rather than a silent addition. Ten unit tests cover the manifest parser itself (every dependency table, target-specific tables, commented-out lines, `#` inside strings). Backend-swap half: `neighbors.rs::swapping_the_backend_changes_nothing_at_the_call_site` (byte-identical call site, only the enum value differs) and `sim.rs::switching_backend_is_the_only_difference_between_the_two_runs` (guards that the comparison isn't two identical configs). *Scope note:* the check is on **declared** manifest dependencies, not the transitive graph — appropriate for the AC's wording, but it would not catch a banned crate arriving through a permitted one |
| **AC-50** | No domain logic in handlers; handlers delegate to kernel and repositories | **MET** | `web.rs::ac50_handlers_do_no_rendering_and_views_do_no_io`. Negative half, by source scan: `routes.rs` may not contain `html!`, `<svg`, `viewBox`, `boids_core::sim` or `frame_metrics(`; `views/{mod,pages,svg,style}.rs` and `analysis.rs` may not contain `.await`, `diesel`, `AsyncPgConnection`, `PgRunRepository` or `AutumnResult`. The guard also asserts `views/mod.rs` declares **exactly three** submodules, so a fourth view file cannot escape the check by not being listed. Positive half: all 14 public views are rendered from plain data in the test, and none may emit `NaN`. Structurally reinforced — every view test in the file is a plain `#[test]` with no runtime, no `TestApp` and no connection, so a view that reached for the database could not compile there |
| **AC-51** | `clippy --workspace --all-targets -- -D warnings` clean; `unsafe_code` forbidden | **MET** *(run live, not taken on trust)* | `cargo clean -p boids-core -p boidboard` followed by `cargo clippy --workspace --all-targets -- -D warnings` → **exit 0** on a genuinely fresh compile of both crates (a cached "Finished" would have proved nothing). `cargo fmt --all --check` → exit 0 as well. `unsafe_code = "forbid"` is set under `[lints.rust]` in `boids-core/Cargo.toml:20-21` and `boidboard/Cargo.toml:30-31` — `forbid`, not `deny`, so it cannot be locally overridden. *Minor observation:* `boidboard/Cargo.toml:33-34` sets `[lints.clippy] disallowed_methods = "allow"`. There is no `clippy.toml` in the repository, so that lint currently has an empty configured list and suppresses nothing — but it would silently disable the lint if one were ever added. It dates from the initial scaffold commit |
| **AC-52** | Every feature built red → green → refactor, with the red phase evidenced | **MET** | Nine per-module TDD logs in `docs/tdd/` — `kernel-foundation.md`, `neighbors.md`, `forces.md`, `metrics.md`, `sim.md`, `persistence.md`, `web.md`, `workflow.md`, `integration.md` — each organised as per-AC sections and each containing **real captured failure output** (`panicked at …`, `error[E…]`, `assertion \`left == right\` failed`, `test result: FAILED`): 21, 6, 38, 14, 24, 17, 26, 27 and 9 such lines respectively. Three further logs cover the post-review work — `review-fixes-kernel.md` (28), `review-fixes-security.md` (11), `review-fixes-product.md` (29) — including mutation-test evidence that each new assertion bites. The one criterion closed during this verification carries its own captured red output, below |

---

## Gaps closed during verification

### AC-38 — "re-running from provenance reproduces the state hash exactly"

**What was missing.** AC-38 has two clauses. The first — that a completed run
stores kernel version, config hash, seed and final state hash — was asserted by
`persistence.rs::ac38_run_round_trips_its_full_provenance_record`, but that test
stores the literal string `"f00dcafe"` as the hash: it proves the *column*
round-trips, not that the value means anything. The second clause was asserted
**nowhere**. The closest test, `ac36_…reaches_the_same_final_state_hash`,
constructs its reference `SimParams` in memory rather than reading them back out
of the stored `config_snapshot`, so a run whose snapshot recorded parameters
other than the ones it executed would still have passed the entire suite. That
is precisely the forgery the security review described (an unauthenticated
`steer` making a run simulate parameters its snapshot does not record), and the
fingerprint the detail page prints is worth nothing if it is not checkable from
the row alone.

This was a **coverage** gap, not a behavioural one: the production code was
already correct.

**What was added.** `boidboard/tests/workflow.rs` —
`ac38_re_running_a_completed_run_from_its_provenance_reproduces_its_state_hash`,
plus a helper `reproduce_from_provenance(&Run)` that deliberately takes only a
`Run` row and touches no other source. The test drives a real 240-tick run to
completion through the production `simulate_batch_core`, closes it out through
the production `finalize_run_core`, re-reads the row, and then reproduces the
hash from `config_snapshot` + `seed` + `ticks_completed` alone — exactly what a
third party holding nothing but that row could do. Three non-vacuity clauses
require a perturbed seed, a perturbed config weight and a perturbed tick count
each to produce a *different* hash.

**Red output.** Because the behaviour already existed, the test passed on first
run — so its red phase was produced by mutation instead, mutating
`workflow.rs::load_checkpoint` to seed from a constant rather than `run.seed`:

```
thread 'ac38_re_running_a_completed_run_from_its_provenance_reproduces_its_state_hash' panicked at boidboard/tests/workflow.rs:1480:5:
assertion `left == right` failed: AC-38: a run must be reproducible from the provenance it stores. The detail page prints this hash as a promise that the seed, the config snapshot and the kernel version are enough to re-create the run; if they are not, the promise is false.
  left: "1b06f60b6e7440c8"
 right: "890e60d055f74c26"
```

The mutation was reverted immediately; `git diff` after the work touches
`boidboard/tests/workflow.rs` only. For honesty about how much *unique* coverage
this adds: under that particular mutation `ac36_…` fails too. The new test's
distinct contribution is that it is the only place the AC's second clause is
stated as an assertion, traversing the stored provenance record itself rather
than parameters reconstructed alongside it.

**Verification after the change.** Full workspace suite run twice
consecutively: 393 passed, 0 failed, both times, with identical per-binary
counts. Clippy and rustfmt still clean. No existing test was weakened, renamed
or deleted.

---

## Remaining gaps

**None at the level of the acceptance criteria.** Every one of AC-1…AC-52 is
MET. The items below are not gaps against the contract but are things a reviewer
should know; they are stated plainly rather than buried.

1. **AC-27's stimulus is synthetic, not simulated.** The criterion's prose names
   "a flock trapped in a concave obstacle arrangement". Both the kernel and the
   product tests instead feed the detector hand-built centroid paths — one
   oscillating in place, one cruising steadily. The criterion's own parenthetical
   (a positive *and* a negative assertion, so an always-"yes" detector fails) is
   fully satisfied, and a synthetic path is the more controlled stimulus, but no
   test drives a real simulation into a concave pen and watches the detector
   fire. Closing that would mean tuning a scenario until a flock reliably wedges
   itself, which is a flaky-test risk for little added confidence — hence
   documented rather than attempted.

2. **AC-22 can silently skip.** `cross_process.rs` returns early with an
   `eprintln!("SKIPPED: …")` when `cargo` is not on the PATH. That is the right
   behaviour for a packaged binary running its own suite, but it means CI must
   guarantee a toolchain or the cross-process reproducibility claim goes
   unchecked while the suite still reports green. It genuinely executed during
   this verification.

3. **AC-16's "monotonically" rests on the linearity assertion.** The behavioural
   test checks one 1.0 → 2.0 increment per weight (direction and axis purity),
   not a monotone sweep. Monotonicity follows from
   `blend_is_the_weighted_sum_of_its_components`, which pins the blend to the
   exact linear combination — but it is derived, not directly swept.

4. **AC-49's purity check is on declared dependencies only.** It parses
   `boids-core/Cargo.toml`; it does not walk the transitive graph. A banned crate
   arriving through a permitted dependency would not be caught. Given the
   approved list is exactly `{serde, serde-json}`, the exposure is small.

---

## Known limitations and deliberate scope exclusions

A reader should not infer more from "52/52" than is there.

### Declared out of scope by the specification

3D; GPU/WASM compute; real-time 30 fps animation (the product is stills,
trajectories and scrubbing — polling is orders of magnitude short of animation);
parameter sweeps and child-workflow fan-out; user-uploaded steering code;
**authentication and multi-tenancy**; RVO/social-force models.

### Consciously accepted, and worth stating explicitly

* **There is no authentication.** This is a deliberate scope decision, not an
  oversight — but it means *anyone who can reach the HTTP port can create,
  cancel and steer runs*. What the security review changed is narrower: the
  Harvest management API is no longer mounted anonymously. The application's own
  routes remain unauthenticated by design. Do not deploy this on a reachable
  network without putting something in front of it.

* **The Harvest admin API is not mounted by default.** It appears only when
  `BOIDBOARD_HARVEST_ADMIN_TOKEN` is set, and then behind a bearer token
  (`boidboard/src/lib.rs`, `the_management_api_is_opt_in_and_off_by_default`,
  `the_token_comparison_accepts_only_the_whole_token`). Verified live: both
  `/api/harvest/workflows` and `/api/harvest/ui/workflows` return **404**. That
  token is a root credential — engine-level control over every execution, with
  no per-user attribution — and belongs in a secret store, never a tracked file.

* **A `prod` boot needs configuration a `dev` boot does not.** Migrations are not
  auto-applied under `prod` (`database.auto_migrate_in_production` defaults
  false) — run `autumn migrate` first. `[security.trusted_hosts]` must name the
  real hostnames or every request gets `400 Invalid Host header`.
  `[security.signing_secret]` must be set or each process and replica generates
  its own ephemeral key and invalidates the others' cookies. `database.url` and
  the `log` settings deliberately live in `autumn-dev.toml`, not `autumn.toml`,
  so they must be supplied for prod. This is documented in
  `docs/review-fixes-security.md`; the tests do not enforce it.

* **`FrameMetrics` does not round-trip bit-exactly through JSON.** `SimState`
  serializes coordinates as exact decimal *strings* precisely because
  `serde_json`'s float parser can land one ULP away, and one ULP changes the
  state hash. `FrameMetrics` writes plain JSON numbers and has no such
  protection, so a stored metric is *storable and reloadable* but not
  bit-exact. This is defensible — metrics are recomputable from stored state at
  any time and are never hashed — but it is a real asymmetry between the two
  persisted types. Recorded in `docs/review-fixes-kernel.md` under "Side
  observation (not fixed, out of scope)".

* **The spatial hash's staleness guard is length-only.** `SpatialHash::neighbors`
  falls back to an exact scan when `agents.len() != self.agent_count`, but a
  same-length flock whose members have *moved* still takes the fast path and
  reads buckets describing where those agents used to be. This is pinned as an
  executable fact by `the_length_guard_does_not_detect_a_same_length_flock_that_moved`.
  What actually covers it is the "rebuild every tick" contract in `sim::step`,
  which builds one grid per tick and never carries one across a step.

* **`toroidal_centroid` is the kernel's one piece of non-bit-portable
  arithmetic** — it is the only function using trigonometry. That is exactly why
  the stuck answer is a *displayed badge* and is never hashed, persisted, or
  compared across machines (`boidboard/src/analysis.rs:62-65`).

* **Frame volume is bounded by query, not by policy.** The run-detail page uses
  a real query bound (`DETAIL_FRAME_BUDGET`, subsampled server-side) rather than
  loading every frame and trimming in memory — but a run still writes one frame
  row per tick, by design, so `tick_gaps` means what its name says. Capacity is
  guarded at submission time by `MAX_ACTIVE_RUNS` and a `max_ticks` ceiling, not
  by any retention policy. Nothing prunes old frames.

* **`disallowed_methods = "allow"`** in `boidboard/Cargo.toml` is inert today
  (no `clippy.toml` exists) but would silently disable that lint if one were
  added. It came from the initial scaffold.

---

## What a reviewer should look at most closely

1. **`boidboard/tests/workflow.rs::ac38_re_running_a_completed_run_from_its_provenance_reproduces_its_state_hash`**
   — the only change in this PR. Check that `reproduce_from_provenance` really
   reads nothing but the `Run` row, and that the three non-vacuity clauses are
   the right three.

2. **AC-27's product wiring** — `boidboard/src/analysis.rs`, its single call site
   at `boidboard/src/routes.rs:678`, and the badge at
   `boidboard/src/views/pages.rs:555,706`. This is the criterion that was
   previously satisfied by a unit test with no caller; the thing to confirm is
   that the path from stored frame → decoded flock → centroid → badge is
   unbroken, and that the synthetic-stimulus caveat above is acceptable.

3. **AC-9's generator, `boids-core/src/neighbors.rs:696-811`** — the entire
   strength of the spatial-hash equivalence claim rests on `random_case`
   continuing to produce seams, coincident agents, zero radii and
   radii-wider-than-the-world. The four corpus assertions guard that today; a
   future edit to the generator is the one change that could quietly turn a
   256-case property test into an expensive tautology.
