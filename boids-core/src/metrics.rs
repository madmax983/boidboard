//! Flock-level metrics for a single simulation frame.
//!
//! Everything here is a pure function of an agent slice plus the parameters,
//! so a frame's metrics can be recomputed from stored state at any time rather
//! than being trusted as an opaque number written once.
//!
//! **All geometry is toroidal.** Every distance goes through
//! [`World::distance`](crate::world::World::distance) or
//! [`World::displacement`](crate::world::World::displacement); a raw
//! coordinate subtraction anywhere in this module would silently report two
//! agents straddling a seam as being a world apart.

use crate::config::SimParams;
use crate::vec2::Vec2;
use crate::world::{Agent, World};

/// Metrics for one simulation frame.
///
/// Serialized into the `frames.metrics` JSONB column, so **the field names are
/// a persisted contract**: renaming one silently invalidates every stored
/// frame and every chart reading them. Add fields rather than rename them.
///
/// Every field is finite for every input — an empty frame reports zeros, never
/// `NaN` — so a caller charting a series never has to filter the data.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FrameMetrics {
    /// Order parameter in `[0,1]`: magnitude of the mean unit heading.
    /// See [`polarization`] for the treatment of stationary agents.
    pub polarization: f64,
    /// Mean distance from each agent to its nearest other agent (toroidal).
    /// `0.0` for fewer than two agents. See
    /// [`mean_nearest_neighbor_distance`].
    pub mean_nearest_neighbor_distance: f64,
    /// Count of **unordered** pairs closer than
    /// [`SimParams::collision_radius`]. See [`collision_count`].
    pub collisions: usize,
    /// Mean speed across agents; `0.0` for an empty frame.
    pub mean_speed: f64,
    /// Fraction in `[0,1]` of agents within
    /// [`SimParams::goal_arrival_radius`] of the goal, or `0.0` when there is
    /// no goal or no agents.
    pub fraction_arrived: f64,
}

/// Compute every metric for one frame.
///
/// A pure function of the agent slice and the parameters, so a stored frame's
/// metrics can be recomputed and checked at any time.
///
/// **Degenerate case.** An empty flock yields all-zero metrics rather than
/// `NaN`s, which keeps a run's metric series chartable from tick zero.
#[must_use]
pub fn frame_metrics(agents: &[Agent], params: &SimParams) -> FrameMetrics {
    FrameMetrics {
        polarization: polarization(agents),
        mean_nearest_neighbor_distance: mean_nearest_neighbor_distance(agents, &params.world),
        collisions: collision_count(agents, &params.world, params.collision_radius),
        mean_speed: mean_speed(agents),
        fraction_arrived: fraction_arrived(agents, params),
    }
}

/// Mean speed across the flock. `0.0` for an empty flock.
fn mean_speed(agents: &[Agent]) -> f64 {
    if agents.is_empty() {
        return 0.0;
    }
    let total: f64 = agents.iter().map(|a| a.vel.length()).sum();
    total / agents.len() as f64
}

/// Fraction of agents within `goal_arrival_radius` of the goal, in `[0,1]`.
///
/// `0.0` when there is no goal or no agents. The radius is **inclusive**: an
/// agent exactly on it has arrived.
fn fraction_arrived(agents: &[Agent], params: &SimParams) -> f64 {
    let Some(goal) = params.goal else {
        return 0.0;
    };
    if agents.is_empty() {
        return 0.0;
    }
    let arrived = agents
        .iter()
        .filter(|a| params.world.distance(a.pos, goal) <= params.goal_arrival_radius)
        .count();
    arrived as f64 / agents.len() as f64
}

/// Polarization, the flock's order parameter: the magnitude of the mean
/// **unit** velocity.
///
/// Range `[0,1]`. `1.0` is a perfectly aligned flock, `0.0` a perfectly
/// disordered one (for example four agents heading at 0/90/180/270 degrees,
/// whose unit headings sum exactly to zero). Speed is irrelevant — only
/// heading is measured.
///
/// **Degenerate cases.** A zero-velocity agent has no heading at all, so it is
/// *excluded from both the sum and the divisor* rather than being folded in as
/// a zero vector. Counting it as a zero vector would conflate "standing still"
/// with "pointing the wrong way" and would make a motionless flock report the
/// same 0.0 as a maximally disordered one. Consequently an empty slice, and a
/// flock in which every agent is stationary, both return `0.0`; a flock with
/// one moving agent reports that agent's alignment with itself, `1.0`.
/// Non-finite velocities are treated as headingless for the same reason.
/// The result is never `NaN`.
#[must_use]
pub fn polarization(agents: &[Agent]) -> f64 {
    let mut sum = Vec2::ZERO;
    let mut heading_count = 0usize;
    for a in agents {
        // `normalize` is total: it returns ZERO for a zero-length or
        // non-finite velocity, which is exactly the "no heading" case.
        let u = a.vel.normalize();
        if u == Vec2::ZERO {
            continue;
        }
        sum = sum.add(u);
        heading_count += 1;
    }
    if heading_count == 0 {
        return 0.0;
    }
    let mean = sum.scale(1.0 / heading_count as f64);
    // The triangle inequality bounds this by 1 exactly; floating-point
    // rounding can still land an ULP above, and callers treat the range as a
    // hard contract.
    mean.length().min(1.0)
}

/// Mean distance from each agent to its nearest *other* agent, under toroidal
/// distance.
///
/// A crowding measure: small values mean a tight flock, large values a
/// dispersed one. Bounded above by the world's half-diagonal, because that is
/// the largest distance the torus admits.
///
/// **Degenerate cases.** Fewer than two agents returns `0.0` — with nobody
/// else present there is no nearest neighbour to measure, and `0.0` is the
/// value a caller charting the series wants rather than a `NaN` gap.
/// An agent is never its own nearest neighbour: the comparison is by slice
/// index, so duplicate `id`s or coincident positions cannot make an agent
/// match itself.
#[must_use]
pub fn mean_nearest_neighbor_distance(agents: &[Agent], world: &World) -> f64 {
    if agents.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    for (i, a) in agents.iter().enumerate() {
        let mut nearest_squared = f64::INFINITY;
        for (j, b) in agents.iter().enumerate() {
            if i == j {
                continue;
            }
            // Compare squared distances: one `sqrt` per agent instead of one
            // per pair, and the ordering is identical.
            let d2 = world.distance_squared(a.pos, b.pos);
            if d2 < nearest_squared {
                nearest_squared = d2;
            }
        }
        total += nearest_squared.sqrt();
    }
    total / agents.len() as f64
}

/// Number of **unordered** agent pairs closer than `radius`, under toroidal
/// distance.
///
/// Two mutually overlapping agents are **one** collision, not two: the pair
/// `{i,j}` is counted once, by scanning only `j > i`. Three mutually
/// overlapping agents are therefore three collisions, and `n` mutually
/// overlapping agents are `n*(n-1)/2`. The result is bounded by `C(n,2)`.
///
/// `radius` is a **strict** upper bound — agents exactly `radius` apart are
/// not colliding — which keeps the count consistent with the separation force
/// treating that same distance as the point where repulsion has just faded out.
///
/// **Degenerate cases.** Fewer than two agents, or a non-positive or
/// non-finite `radius`, returns `0`. An agent is never counted against itself:
/// the pairing is by slice index, so duplicate `id`s and coincident positions
/// are both handled. Non-finite positions simply never satisfy the comparison.
#[must_use]
pub fn collision_count(agents: &[Agent], world: &World, radius: f64) -> usize {
    if !radius.is_finite() || radius <= 0.0 {
        return 0;
    }
    // Compare squared distances so the per-pair `sqrt` disappears; `radius` is
    // already known positive and finite, so squaring cannot flip the ordering.
    let r2 = radius * radius;
    let mut count = 0;
    for (i, a) in agents.iter().enumerate() {
        for b in &agents[i + 1..] {
            if world.distance_squared(a.pos, b.pos) < r2 {
                count += 1;
            }
        }
    }
    count
}

/// Index of the first frame at which at least `fraction` of the flock had
/// arrived at the goal, or `None` if that never happened.
///
/// The index is the tick, given that `series[i]` is tick `i`. The comparison
/// is **inclusive** (`>= fraction`) and returns the **first** crossing, so a
/// flock that arrives, scatters, and re-arrives is credited with its first
/// arrival.
///
/// **Never a sentinel.** "Did not reach the goal" is [`None`]; there is no
/// `-1` and no `usize::MAX` anywhere in this module. A caller cannot
/// accidentally chart or average a miss as if it were a very large time.
///
/// **Degenerate cases.** An empty series is `None`. A `fraction` of `0.0` or
/// less is satisfied by the first frame, since no arrivals still clears a bar
/// of none. A `fraction` above `1.0`, or `NaN`, is unsatisfiable and yields
/// `None` rather than panicking.
#[must_use]
pub fn time_to_goal(series: &[FrameMetrics], fraction: f64) -> Option<usize> {
    // `>=` against a NaN threshold is false for every frame, so the NaN case
    // falls out as `None` without a special branch.
    series.iter().position(|m| m.fraction_arrived >= fraction)
}

/// Whether the flock has stopped making progress despite still moving.
///
/// Measured as **tortuosity** over the last `window` samples of the centroid
/// path: compare the net displacement across the window against the total
/// path length walked to get it. Their ratio is the *straightness*, in
/// `[0,1]`: `1.0` is a dead-straight run, and a value near `0.0` means the
/// flock covered a lot of ground and ended up where it started. The flock is
/// stuck when straightness is **strictly below** `ratio_threshold`.
///
/// A ratio rather than a bare displacement is what distinguishes the two
/// cases the AC cares about: an agent oscillating tightly in a trap walks a
/// long path to no end and **is** stuck, while a flock cruising steadily has
/// net displacement equal to its path length and is **not** — no matter how
/// fast or slow it happens to be going. A `ratio_threshold` of `0.1`-`0.3` is
/// the useful band; `0.2` is a reasonable default.
///
/// **Toroidal.** Net displacement is the length of the **sum of the
/// minimum-image step vectors**, not the minimum-image distance between the
/// window's endpoints. That distinction matters: a flock cruising steadily
/// once round the world returns to where it began, and endpoint distance
/// would call that stuck. Summing steps unwraps the path instead, so laps
/// count as the progress they are. This is exact as long as the centroid
/// moves under half a world per sample, which `max_speed` guarantees.
///
/// **Degenerate cases.** Returns `false` — never panicking, and never
/// claiming stuckness on absent evidence — when the path is shorter than
/// `window`, when `window < 2`, or when the flock did not move at all over
/// the window. That last case is a *frozen* flock rather than a trapped one:
/// tortuosity is `0/0` there, and the caller should diagnose it from
/// [`FrameMetrics::mean_speed`], which is the metric that actually describes
/// it.
#[must_use]
pub fn is_stuck(
    centroid_path: &[Vec2],
    world: &World,
    window: usize,
    ratio_threshold: f64,
) -> bool {
    // Fewer than two samples describe no motion at all, so there is nothing
    // to take a ratio of.
    if window < 2 || centroid_path.len() < window {
        return false;
    }
    let recent = &centroid_path[centroid_path.len() - window..];

    let mut path_length = 0.0;
    let mut net = Vec2::ZERO;
    for pair in recent.windows(2) {
        let step = world.displacement(pair[0], pair[1]);
        path_length += step.length();
        net = net.add(step);
    }

    if path_length <= 0.0 || !path_length.is_finite() {
        return false;
    }
    let straightness = net.length() / path_length;
    // A `NaN` threshold makes this false, which is the safe answer.
    straightness < ratio_threshold
}

/// Centroid of the agent positions **on the torus**, wrapped into the world.
///
/// A plain coordinate mean is wrong here: agents at `x=1` and `x=99` in a
/// width-100 world average to `x=50`, which is the point furthest from both of
/// them. This uses the circular mean instead — map each coordinate onto a
/// circle, average the resulting unit vectors, and read the angle back — so
/// the same pair yields `x≈0`, the point they actually straddle.
///
/// **Degenerate cases.** An empty flock returns [`Vec2::ZERO`]. A flock spread
/// evenly around an axis has no meaningful centre on that axis; the averaged
/// vector is then the origin and `atan2(0,0) = 0` puts the result at
/// coordinate `0.0` — arbitrary, but deterministic, which is what the
/// reproducibility contract requires. A non-positive or non-finite world
/// dimension collapses that axis to `0.0`. The result is always finite and
/// always inside `[0,width) x [0,height)`.
#[must_use]
pub fn toroidal_centroid(agents: &[Agent], world: &World) -> Vec2 {
    if agents.is_empty() {
        return Vec2::ZERO;
    }
    // `wrap` is total, so a non-finite position becomes 0.0 here rather than
    // poisoning the whole flock's centroid through `cos`/`sin`.
    let wrapped: Vec<Vec2> = agents.iter().map(|a| world.wrap(a.pos)).collect();
    Vec2::new(
        circular_mean_axis(wrapped.iter().map(|p| p.x), wrapped.len(), world.width),
        circular_mean_axis(wrapped.iter().map(|p| p.y), wrapped.len(), world.height),
    )
}

/// Circular mean of coordinates living on a circle of circumference `size`.
///
/// Returns a value in `[0,size)`, or `0.0` when the axis is degenerate or the
/// values cancel exactly.
fn circular_mean_axis(values: impl Iterator<Item = f64>, count: usize, size: f64) -> f64 {
    if count == 0 || !size.is_finite() || size <= 0.0 {
        return 0.0;
    }
    let to_angle = std::f64::consts::TAU / size;
    let mut cos_sum = 0.0;
    let mut sin_sum = 0.0;
    for v in values {
        let theta = v * to_angle;
        cos_sum += theta.cos();
        sin_sum += theta.sin();
    }
    // No need to divide by `count`: `atan2` only cares about the direction of
    // (cos_sum, sin_sum), and skipping the division avoids a rounding step.
    let mean_angle = sin_sum.atan2(cos_sum);
    // `atan2` yields (-PI, PI]; fold back into [0,size).
    let coord = mean_angle / to_angle;
    let wrapped = coord.rem_euclid(size);
    if wrapped >= size { 0.0 } else { wrapped }
}

#[cfg(test)]
mod tests {
    use super::{
        FrameMetrics, collision_count, frame_metrics, is_stuck, mean_nearest_neighbor_distance,
        polarization, time_to_goal, toroidal_centroid,
    };
    use crate::config::SimParams;
    use crate::vec2::Vec2;
    use crate::world::{Agent, World};

    /// Tight enough that any genuine geometry error shows up; loose enough to
    /// absorb the final rounding of a `sqrt`.
    const EPS: f64 = 1e-12;

    fn w100() -> World {
        World::new(100.0, 100.0)
    }

    fn at(id: u32, x: f64, y: f64) -> Agent {
        Agent {
            id,
            pos: Vec2::new(x, y),
            vel: Vec2::ZERO,
        }
    }

    /// A 100x100 world, collisions inside 2.0, goal at the centre with a
    /// 10.0 arrival radius.
    fn params() -> SimParams {
        SimParams {
            world: w100(),
            collision_radius: 2.0,
            goal: Some(Vec2::new(50.0, 50.0)),
            goal_arrival_radius: 10.0,
            ..SimParams::default()
        }
    }

    /// The worked example every `frame_metrics` assertion below refers to.
    ///
    /// - `0` at the goal moving +x at speed 2, `1` one unit away at speed 4:
    ///   both arrived, and one collision between them.
    /// - `2` at the origin moving -y at speed 3 and `3` at `x=99` stationary:
    ///   1.0 apart **across the seam**, so a second collision, and neither has
    ///   arrived.
    fn worked_example() -> [Agent; 4] {
        [
            agent(0, Vec2::new(50.0, 50.0), Vec2::new(2.0, 0.0)),
            agent(1, Vec2::new(51.0, 50.0), Vec2::new(4.0, 0.0)),
            agent(2, Vec2::new(0.0, 0.0), Vec2::new(0.0, -3.0)),
            agent(3, Vec2::new(99.0, 0.0), Vec2::ZERO),
        ]
    }

    fn agent(id: u32, pos: Vec2, vel: Vec2) -> Agent {
        Agent { id, pos, vel }
    }

    fn moving(id: u32, vel: Vec2) -> Agent {
        agent(id, Vec2::ZERO, vel)
    }

    #[test]
    fn ac23_polarization_is_exactly_one_for_a_perfectly_aligned_flock() {
        // Different speeds, one heading: polarization measures direction only.
        let flock = [
            moving(0, Vec2::new(1.0, 0.0)),
            moving(1, Vec2::new(3.0, 0.0)),
            moving(2, Vec2::new(0.25, 0.0)),
            moving(3, Vec2::new(97.5, 0.0)),
        ];
        assert_eq!(
            polarization(&flock),
            1.0,
            "a perfectly aligned flock must be exactly 1.0"
        );
    }

    #[test]
    fn ac23_polarization_is_exactly_zero_for_four_agents_at_right_angles() {
        // 0deg / 90deg / 180deg / 270deg, again at differing speeds.
        let flock = [
            moving(0, Vec2::new(3.0, 0.0)),
            moving(1, Vec2::new(0.0, 7.0)),
            moving(2, Vec2::new(-1.0, 0.0)),
            moving(3, Vec2::new(0.0, -0.5)),
        ];
        let p = polarization(&flock);
        assert!(p.abs() < 1e-12, "opposed headings must cancel, got {p}");
        assert_eq!(p, 0.0, "the four unit headings sum exactly to zero");
    }

    #[test]
    fn ac23_zero_velocity_agents_have_no_heading_and_never_produce_nan() {
        // A stationary agent has no heading, so it is excluded from the mean
        // rather than being counted as a misaligned member.
        let flock = [
            moving(0, Vec2::new(2.0, 0.0)),
            moving(1, Vec2::ZERO),
            moving(2, Vec2::new(5.0, 0.0)),
        ];
        let p = polarization(&flock);
        assert!(!p.is_nan(), "a zero-velocity agent produced NaN");
        assert_eq!(p, 1.0, "the two moving agents are perfectly aligned");

        // Degenerate flocks still yield a number, not NaN.
        assert_eq!(polarization(&[]), 0.0);
        assert_eq!(polarization(&[moving(0, Vec2::ZERO)]), 0.0);
        assert_eq!(
            polarization(&[moving(0, Vec2::ZERO), moving(1, Vec2::ZERO)]),
            0.0
        );
    }

    #[test]
    fn ac23_polarization_is_always_within_the_unit_interval() {
        let mut r = crate::rng::Rng::seeded(0x9017);
        for n in [1usize, 2, 3, 7, 40] {
            for _ in 0..2_000 {
                let flock: Vec<Agent> = (0..n)
                    .map(|i| {
                        // Deliberately includes exact zeros now and then.
                        let vx = if r.next_f64() < 0.1 {
                            0.0
                        } else {
                            r.range(-50.0, 50.0)
                        };
                        let vy = if r.next_f64() < 0.1 {
                            0.0
                        } else {
                            r.range(-50.0, 50.0)
                        };
                        moving(i as u32, Vec2::new(vx, vy))
                    })
                    .collect();
                let p = polarization(&flock);
                assert!(
                    (0.0..=1.0).contains(&p),
                    "polarization {p} outside [0,1] for {flock:?}"
                );
            }
        }
    }

    #[test]
    fn ac24_mean_nearest_neighbor_distance_goes_the_short_way_round_the_seam() {
        // AC-24's named case: x=1 and x=99 in a width-100 world are 2 apart,
        // so the mean NND is 2.0 and emphatically not 98.0.
        let flock = [at(0, 1.0, 0.0), at(1, 99.0, 0.0)];
        let d = mean_nearest_neighbor_distance(&flock, &w100());
        assert!(
            (d - 2.0).abs() < EPS,
            "expected 2.0 across the seam, got {d}"
        );
    }

    #[test]
    fn ac24_mean_nearest_neighbor_distance_averages_per_agent_nearest_distances() {
        // Nearest neighbours: 0->1 is 3, 1->0 is 3, 2->1 is 7.
        // Mean = (3 + 3 + 7) / 3.
        let flock = [at(0, 10.0, 0.0), at(1, 13.0, 0.0), at(2, 20.0, 0.0)];
        let d = mean_nearest_neighbor_distance(&flock, &w100());
        assert!((d - 13.0 / 3.0).abs() < EPS, "expected 13/3, got {d}");
    }

    #[test]
    fn ac24_mean_nearest_neighbor_distance_of_fewer_than_two_agents_is_zero() {
        let w = w100();
        assert_eq!(mean_nearest_neighbor_distance(&[], &w), 0.0);
        assert_eq!(mean_nearest_neighbor_distance(&[at(0, 5.0, 5.0)], &w), 0.0);
    }

    #[test]
    fn ac24_mean_nearest_neighbor_distance_ignores_the_agent_itself() {
        // Two agents sharing a position would report 0 if an agent were
        // allowed to be its own nearest neighbour, which is also 0 — so use
        // distinct positions where self-matching gives a *different* answer.
        let flock = [at(0, 0.0, 0.0), at(1, 40.0, 0.0)];
        let d = mean_nearest_neighbor_distance(&flock, &w100());
        assert!(d > 0.0, "self-matching would collapse the mean NND to 0");
        assert!((d - 40.0).abs() < EPS, "expected 40.0, got {d}");
    }

    #[test]
    fn ac24_mean_nearest_neighbor_distance_never_exceeds_the_half_diagonal() {
        // A toroidal distance is bounded by the half-diagonal; a raw
        // coordinate subtraction is not, so this catches a non-toroidal
        // implementation over randomised flocks.
        let w = w100();
        let max = (50.0_f64 * 50.0 + 50.0 * 50.0).sqrt();
        let mut r = crate::rng::Rng::seeded(0x24_24);
        for _ in 0..2_000 {
            let n = 2 + (r.next_f64() * 30.0) as usize;
            let flock: Vec<Agent> = (0..n)
                .map(|i| at(i as u32, r.range(0.0, 100.0), r.range(0.0, 100.0)))
                .collect();
            let d = mean_nearest_neighbor_distance(&flock, &w);
            assert!(d >= 0.0 && d <= max + EPS, "mean NND {d} out of range");
        }
    }

    #[test]
    fn ac25_two_mutually_overlapping_agents_are_one_collision_not_two() {
        // The headline of AC-25: the pair is unordered. An implementation that
        // counts every ordered (i,j) pair reports 2 here.
        let flock = [at(0, 10.0, 10.0), at(1, 10.5, 10.0)];
        assert_eq!(collision_count(&flock, &w100(), 2.0), 1);
    }

    #[test]
    fn ac25_three_mutually_overlapping_agents_are_three_pairs() {
        // {0,1}, {0,2}, {1,2} — three unordered pairs, not six and not three
        // "colliding agents" that happen to coincide with the pair count by
        // accident, so a fourth agent is checked too: C(4,2) = 6.
        let three = [at(0, 10.0, 10.0), at(1, 10.4, 10.0), at(2, 10.0, 10.4)];
        assert_eq!(collision_count(&three, &w100(), 2.0), 3);

        let four = [
            at(0, 10.0, 10.0),
            at(1, 10.4, 10.0),
            at(2, 10.0, 10.4),
            at(3, 10.4, 10.4),
        ];
        assert_eq!(collision_count(&four, &w100(), 2.0), 6);
    }

    #[test]
    fn ac25_an_agent_is_never_counted_against_itself() {
        // One agent is at distance 0 from itself, so a missing i != j guard
        // would report a collision for a flock of one.
        let w = w100();
        assert_eq!(collision_count(&[at(0, 10.0, 10.0)], &w, 5.0), 0);
        assert_eq!(collision_count(&[], &w, 5.0), 0);

        // With N well-separated agents the answer is still 0, not N.
        let spread = [at(0, 0.0, 0.0), at(1, 30.0, 0.0), at(2, 0.0, 30.0)];
        assert_eq!(collision_count(&spread, &w, 5.0), 0);
    }

    #[test]
    fn ac25_collisions_are_counted_across_the_seam() {
        // x=0.5 and x=99.5 are 1.0 apart on the torus, so they collide.
        let flock = [at(0, 0.5, 50.0), at(1, 99.5, 50.0)];
        assert_eq!(collision_count(&flock, &w100(), 2.0), 1);
    }

    #[test]
    fn ac25_the_radius_is_a_strict_upper_bound() {
        let w = w100();
        // Exactly at the radius is not "closer than" the radius.
        let touching = [at(0, 10.0, 10.0), at(1, 12.0, 10.0)];
        assert_eq!(collision_count(&touching, &w, 2.0), 0);
        assert_eq!(collision_count(&touching, &w, 2.000_000_001), 1);

        // A non-positive radius admits nothing, not even coincident agents.
        let coincident = [at(0, 10.0, 10.0), at(1, 10.0, 10.0)];
        assert_eq!(collision_count(&coincident, &w, 0.0), 0);
        assert_eq!(collision_count(&coincident, &w, -1.0), 0);
        assert_eq!(collision_count(&coincident, &w, 0.5), 1);
    }

    #[test]
    fn ac25_collision_count_never_exceeds_the_number_of_pairs() {
        let w = w100();
        let mut r = crate::rng::Rng::seeded(0x2525);
        for _ in 0..2_000 {
            let n = (r.next_f64() * 12.0) as usize;
            // A small world crammed with agents so collisions actually happen.
            let flock: Vec<Agent> = (0..n)
                .map(|i| at(i as u32, r.range(0.0, 10.0), r.range(0.0, 10.0)))
                .collect();
            let c = collision_count(&flock, &w, 3.0);
            assert!(c <= n * n.saturating_sub(1) / 2, "{c} exceeds C({n},2)");
        }
    }

    #[test]
    fn frame_metrics_reports_every_field_of_the_worked_example() {
        let m = frame_metrics(&worked_example(), &params());

        // Headings (1,0), (1,0), (0,-1); agent 3 is stationary and excluded.
        // |(2,-1)/3| = sqrt(5)/3.
        let expected_pol = 5.0_f64.sqrt() / 3.0;
        assert!(
            (m.polarization - expected_pol).abs() < EPS,
            "polarization {} != {expected_pol}",
            m.polarization
        );

        // Every agent's nearest neighbour is exactly 1.0 away (0<->1 directly,
        // 2<->3 across the seam).
        assert!(
            (m.mean_nearest_neighbor_distance - 1.0).abs() < EPS,
            "mean NND {}",
            m.mean_nearest_neighbor_distance
        );

        // {0,1} and {2,3}, the latter only if the seam is handled.
        assert_eq!(m.collisions, 2);

        // (2 + 4 + 3 + 0) / 4.
        assert!(
            (m.mean_speed - 2.25).abs() < EPS,
            "mean speed {}",
            m.mean_speed
        );

        // Agents 0 and 1 are inside the 10.0 arrival radius; 2 and 3 are ~70
        // away.
        assert!(
            (m.fraction_arrived - 0.5).abs() < EPS,
            "fraction arrived {}",
            m.fraction_arrived
        );
    }

    #[test]
    fn frame_metrics_fraction_arrived_is_zero_without_a_goal() {
        let p = SimParams {
            goal: None,
            ..params()
        };
        let m = frame_metrics(&worked_example(), &p);
        assert_eq!(
            m.fraction_arrived, 0.0,
            "no goal means nothing can have arrived"
        );
        // The other metrics do not depend on the goal.
        assert_eq!(m.collisions, 2);
        assert!((m.mean_speed - 2.25).abs() < EPS);
    }

    #[test]
    fn frame_metrics_fraction_arrived_measures_arrival_across_the_seam() {
        // Goal at x=1; an agent at x=99 is 2.0 away on the torus, so with a
        // 3.0 arrival radius it has arrived. A non-toroidal check says 98.
        let p = SimParams {
            goal: Some(Vec2::new(1.0, 50.0)),
            goal_arrival_radius: 3.0,
            ..params()
        };
        let flock = [at(0, 99.0, 50.0), at(1, 50.0, 50.0)];
        let m = frame_metrics(&flock, &p);
        assert!(
            (m.fraction_arrived - 0.5).abs() < EPS,
            "expected 1 of 2 arrived, got {}",
            m.fraction_arrived
        );
    }

    #[test]
    fn frame_metrics_arrival_radius_is_inclusive() {
        let p = SimParams {
            goal: Some(Vec2::new(50.0, 50.0)),
            goal_arrival_radius: 10.0,
            ..params()
        };
        // Exactly on the radius counts as arrived: "within" includes the edge.
        let flock = [at(0, 60.0, 50.0)];
        assert_eq!(frame_metrics(&flock, &p).fraction_arrived, 1.0);
        let flock = [at(0, 60.001, 50.0)];
        assert_eq!(frame_metrics(&flock, &p).fraction_arrived, 0.0);
    }

    #[test]
    fn frame_metrics_of_an_empty_flock_is_all_zeros() {
        let m = frame_metrics(&[], &params());
        assert_eq!(
            m,
            FrameMetrics {
                polarization: 0.0,
                mean_nearest_neighbor_distance: 0.0,
                collisions: 0,
                mean_speed: 0.0,
                fraction_arrived: 0.0,
            },
            "an empty frame must be zeros, not NaNs"
        );
    }

    #[test]
    fn frame_metrics_are_always_finite() {
        // Adversarial flocks: coincident agents, zero velocities, positions
        // far outside the world.
        let p = params();
        let mut r = crate::rng::Rng::seeded(0xF4A3);
        for _ in 0..2_000 {
            let n = (r.next_f64() * 15.0) as usize;
            let flock: Vec<Agent> = (0..n)
                .map(|i| {
                    let coincident = r.next_f64() < 0.3;
                    let pos = if coincident {
                        Vec2::new(10.0, 10.0)
                    } else {
                        Vec2::new(r.range(-300.0, 300.0), r.range(-300.0, 300.0))
                    };
                    let vel = if r.next_f64() < 0.3 {
                        Vec2::ZERO
                    } else {
                        Vec2::new(r.range(-5.0, 5.0), r.range(-5.0, 5.0))
                    };
                    agent(i as u32, pos, vel)
                })
                .collect();
            let m = frame_metrics(&flock, &p);
            assert!(
                m.polarization.is_finite()
                    && m.mean_nearest_neighbor_distance.is_finite()
                    && m.mean_speed.is_finite()
                    && m.fraction_arrived.is_finite(),
                "non-finite metric: {m:?}"
            );
            assert!((0.0..=1.0).contains(&m.fraction_arrived));
            assert!((0.0..=1.0).contains(&m.polarization));
        }
    }

    #[test]
    fn frame_metrics_serialises_under_its_persisted_field_names() {
        // These names are the `frames.metrics` JSONB contract. Renaming a
        // field silently breaks every stored frame, so pin them here.
        let m = frame_metrics(&worked_example(), &params());
        let json = serde_json::to_string(&m).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        let object = value.as_object().expect("FrameMetrics is a JSON object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "collisions",
                "fraction_arrived",
                "mean_nearest_neighbor_distance",
                "mean_speed",
                "polarization",
            ]
        );
        let back: FrameMetrics = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back, "FrameMetrics must round-trip exactly");
    }

    /// A frame carrying nothing but a `fraction_arrived` value — the only
    /// field `time_to_goal` reads.
    fn arrived(fraction: f64) -> FrameMetrics {
        FrameMetrics {
            polarization: 0.0,
            mean_nearest_neighbor_distance: 0.0,
            collisions: 0,
            mean_speed: 0.0,
            fraction_arrived: fraction,
        }
    }

    #[test]
    fn ac26_time_to_goal_is_the_first_qualifying_tick() {
        let series: Vec<FrameMetrics> = [0.0, 0.1, 0.4, 0.75, 0.9, 1.0]
            .into_iter()
            .map(arrived)
            .collect();
        assert_eq!(time_to_goal(&series, 0.75), Some(3));
        assert_eq!(time_to_goal(&series, 0.4), Some(2));
        assert_eq!(time_to_goal(&series, 1.0), Some(5));
        // Index 0 qualifies immediately when the bar is already met.
        assert_eq!(time_to_goal(&series, 0.0), Some(0));
    }

    #[test]
    fn ac26_time_to_goal_is_none_when_the_goal_is_never_reached() {
        let series: Vec<FrameMetrics> = [0.0, 0.1, 0.2, 0.3].into_iter().map(arrived).collect();
        let result = time_to_goal(&series, 0.9);

        // Genuinely `None` — not a sentinel dressed up as a number.
        assert!(result.is_none(), "expected None, got {result:?}");
        assert_eq!(result, None);
        assert_ne!(result, Some(usize::MAX), "usize::MAX is not an answer");
        assert!(
            result.is_none_or(|t| t < series.len()),
            "any Some must index a real frame"
        );

        // An empty series has no frame to point at either.
        assert_eq!(time_to_goal(&[], 0.5), None);
    }

    #[test]
    fn ac26_time_to_goal_takes_the_first_crossing_not_a_later_one() {
        // The flock arrives, scatters, and arrives again. The answer is the
        // first crossing; a max/last-index implementation returns 4.
        let series: Vec<FrameMetrics> = [0.0, 0.9, 0.1, 0.2, 0.95]
            .into_iter()
            .map(arrived)
            .collect();
        assert_eq!(time_to_goal(&series, 0.8), Some(1));
    }

    #[test]
    fn ac26_time_to_goal_threshold_is_inclusive() {
        let series = [arrived(0.5)];
        assert_eq!(time_to_goal(&series, 0.5), Some(0), "exactly met counts");
        assert_eq!(time_to_goal(&series, 0.500_000_1), None);
    }

    #[test]
    fn ac26_time_to_goal_of_an_unreachable_threshold_is_none() {
        // `fraction_arrived` never exceeds 1.0, and a NaN threshold is
        // unsatisfiable; neither may panic or invent a tick.
        let series: Vec<FrameMetrics> = [0.0, 0.5, 1.0].into_iter().map(arrived).collect();
        assert_eq!(time_to_goal(&series, 1.5), None);
        assert_eq!(time_to_goal(&series, f64::NAN), None);
        assert_eq!(time_to_goal(&series, f64::INFINITY), None);
    }

    /// Threshold used throughout the stuck tests: a flock whose net progress
    /// is under 20% of the ground it covered is going nowhere.
    const STUCK_RATIO: f64 = 0.2;

    /// A centroid trapped in a pocket, sloshing back and forth by +-4 units
    /// about `x = 50` and never escaping. Path length is large, net
    /// displacement is ~0.
    fn oscillating_path(len: usize) -> Vec<Vec2> {
        (0..len)
            .map(|i| {
                let x = if i % 2 == 0 { 46.0 } else { 54.0 };
                Vec2::new(x, 50.0)
            })
            .collect()
    }

    /// A centroid cruising steadily at 1.5 units/tick along +x. Net
    /// displacement equals path length exactly.
    fn cruising_path(len: usize, world: &World) -> Vec<Vec2> {
        (0..len)
            .map(|i| world.wrap(Vec2::new(1.5 * i as f64, 20.0)))
            .collect()
    }

    #[test]
    fn ac27_is_stuck_fires_on_a_centroid_oscillating_in_place() {
        let path = oscillating_path(40);
        assert!(
            is_stuck(&path, &w100(), 20, STUCK_RATIO),
            "an oscillating centroid must be reported stuck"
        );
    }

    #[test]
    fn ac27_is_stuck_does_not_fire_on_a_flock_cruising_steadily() {
        // The negative assertion: a detector that always returns true fails
        // here. The path is long enough to lap the world more than once, so a
        // detector that compares raw endpoint positions also fails.
        let w = w100();
        let path = cruising_path(200, &w);
        assert!(
            !is_stuck(&path, &w, 20, STUCK_RATIO),
            "a steadily cruising flock must not be reported stuck"
        );
        // Even a demanding threshold must not flag perfectly straight motion.
        assert!(!is_stuck(&path, &w, 20, 0.95));
    }

    #[test]
    fn ac27_is_stuck_looks_only_at_the_most_recent_window() {
        // A flock that was cruising and has now become trapped must be caught
        // even though the early history is perfectly straight.
        let w = w100();
        let mut path = cruising_path(60, &w);
        path.extend(oscillating_path(40));
        assert!(
            is_stuck(&path, &w, 20, STUCK_RATIO),
            "recent trapping must outweigh old progress"
        );

        // And the reverse: a flock that was trapped and has broken free is no
        // longer stuck.
        let mut path = oscillating_path(40);
        path.extend(cruising_path(60, &w));
        assert!(
            !is_stuck(&path, &w, 20, STUCK_RATIO),
            "an escaped flock must stop being reported stuck"
        );
    }

    #[test]
    fn ac27_is_stuck_handles_a_path_shorter_than_the_window() {
        let w = w100();
        // No panic, and no claim of stuckness on evidence that does not exist.
        assert!(!is_stuck(&[], &w, 20, STUCK_RATIO));
        assert!(!is_stuck(&[Vec2::new(1.0, 1.0)], &w, 20, STUCK_RATIO));
        assert!(!is_stuck(&oscillating_path(5), &w, 20, STUCK_RATIO));
        // Exactly the window length is enough evidence.
        assert!(is_stuck(&oscillating_path(20), &w, 20, STUCK_RATIO));
        // A degenerate window is not enough evidence at any path length.
        assert!(!is_stuck(&oscillating_path(40), &w, 0, STUCK_RATIO));
        assert!(!is_stuck(&oscillating_path(40), &w, 1, STUCK_RATIO));
    }

    #[test]
    fn ac27_is_stuck_ignores_a_motionless_flock() {
        // A centroid that has not moved at all has no path to measure, so
        // tortuosity is undefined. That is a frozen flock, a different
        // diagnosis from a trapped one, and it must not produce a NaN.
        let w = w100();
        let frozen = vec![Vec2::new(30.0, 30.0); 40];
        assert!(!is_stuck(&frozen, &w, 20, STUCK_RATIO));
    }

    #[test]
    fn ac27_is_stuck_follows_a_path_across_the_seam() {
        // A flock cruising straight through the seam is making real progress;
        // measuring net displacement between raw endpoints would fold it back
        // and call this stuck.
        let w = w100();
        let path: Vec<Vec2> = (0..40)
            .map(|i| w.wrap(Vec2::new(90.0 + 1.0 * f64::from(i), 50.0)))
            .collect();
        assert!(
            !is_stuck(&path, &w, 20, STUCK_RATIO),
            "crossing the seam is progress, not stuckness"
        );
    }

    #[test]
    fn ac27_is_stuck_counts_a_full_lap_of_the_world_as_progress() {
        // The window spans 79 steps of 1.5 = 118.5 units, more than one lap of
        // a width-100 world. Net displacement measured as the minimum-image
        // distance between the window's *endpoints* folds that to 18.5 and
        // reports a straightness of 0.156, i.e. "stuck". Summing the
        // minimum-image *steps* unwraps the lap and gives 1.0.
        let w = w100();
        let path = cruising_path(200, &w);
        assert!(
            !is_stuck(&path, &w, 80, STUCK_RATIO),
            "lapping the world is progress; endpoint distance folds it away"
        );
    }

    #[test]
    fn ac27_is_stuck_is_monotone_in_the_threshold() {
        // A stricter threshold can only ever flag more paths, never fewer.
        // This kills a detector that ignores `ratio_threshold` entirely.
        let w = w100();
        let mut r = crate::rng::Rng::seeded(0x27_27);
        for _ in 0..500 {
            // A random walk with a drift, so straightness lands all over.
            let drift = r.range(-1.0, 1.0);
            let mut p = Vec2::new(50.0, 50.0);
            let path: Vec<Vec2> = (0..40)
                .map(|_| {
                    p = w.wrap(p.add(Vec2::new(drift + r.range(-1.0, 1.0), r.range(-1.0, 1.0))));
                    p
                })
                .collect();
            let lenient = is_stuck(&path, &w, 20, 0.1);
            let strict = is_stuck(&path, &w, 20, 0.9);
            assert!(
                !lenient || strict,
                "threshold 0.1 fired where 0.9 did not, on {path:?}"
            );
        }
    }

    /// Translate every agent by `offset` and wrap back into the world.
    fn translate(agents: &[Agent], world: &World, offset: Vec2) -> Vec<Agent> {
        agents
            .iter()
            .map(|a| Agent {
                pos: world.wrap(a.pos.add(offset)),
                ..*a
            })
            .collect()
    }

    #[test]
    fn ac28_every_metric_is_invariant_under_translation() {
        // The test that catches any non-toroidal distance calculation
        // anywhere in this module. Offsets include values far larger than the
        // world and negative ones, and the flocks are dense enough that
        // collisions and nearest-neighbour pairs actually occur.
        let p = params();
        let w = p.world;
        let mut r = crate::rng::Rng::seeded(0x28_28);

        for _ in 0..3_000 {
            let n = 2 + (r.next_f64() * 18.0) as usize;
            let flock: Vec<Agent> = (0..n)
                .map(|i| {
                    agent(
                        i as u32,
                        Vec2::new(r.range(0.0, 100.0), r.range(0.0, 100.0)),
                        Vec2::new(r.range(-4.0, 4.0), r.range(-4.0, 4.0)),
                    )
                })
                .collect();
            let before = frame_metrics(&flock, &p);

            for _ in 0..4 {
                // Deliberately includes offsets many world-widths out and
                // offsets that are exact multiples of the world size.
                let offset = match (r.next_f64() * 4.0) as u32 {
                    0 => Vec2::new(r.range(-5_000.0, 5_000.0), r.range(-5_000.0, 5_000.0)),
                    1 => Vec2::new(w.width, w.height),
                    2 => Vec2::new(-3.0 * w.width, 7.0 * w.height),
                    _ => Vec2::new(r.range(0.0, 100.0), r.range(0.0, 100.0)),
                };
                let shifted = translate(&flock, &w, offset);
                let after = frame_metrics(&shifted, &p);

                assert!(
                    (before.polarization - after.polarization).abs() < 1e-12,
                    "polarization moved under offset {offset:?}: {before:?} -> {after:?}"
                );
                assert!(
                    (before.mean_nearest_neighbor_distance - after.mean_nearest_neighbor_distance)
                        .abs()
                        < 1e-9,
                    "mean NND moved under offset {offset:?}: {before:?} -> {after:?}"
                );
                assert_eq!(
                    before.collisions, after.collisions,
                    "collision count moved under offset {offset:?}"
                );
                assert!(
                    (before.mean_speed - after.mean_speed).abs() < 1e-12,
                    "mean speed moved under offset {offset:?}"
                );
            }
        }
    }

    #[test]
    fn ac28_fraction_arrived_is_invariant_when_the_goal_moves_with_the_world() {
        // `fraction_arrived` is measured against the goal, so translating the
        // agents alone is *supposed* to change it. Translating the goal too
        // is the invariance that must hold.
        let base = params();
        let w = base.world;
        let goal = base.goal.expect("params() sets a goal");
        let mut r = crate::rng::Rng::seeded(0x28_29);

        for _ in 0..2_000 {
            let flock: Vec<Agent> = (0..12)
                .map(|i| {
                    agent(
                        i,
                        Vec2::new(r.range(0.0, 100.0), r.range(0.0, 100.0)),
                        Vec2::ZERO,
                    )
                })
                .collect();
            let before = frame_metrics(&flock, &base).fraction_arrived;

            let offset = Vec2::new(r.range(-800.0, 800.0), r.range(-800.0, 800.0));
            let shifted = SimParams {
                goal: Some(w.wrap(goal.add(offset))),
                ..base.clone()
            };
            let after = frame_metrics(&translate(&flock, &w, offset), &shifted).fraction_arrived;
            assert!(
                (before - after).abs() < 1e-12,
                "fraction arrived moved from {before} to {after} under {offset:?}"
            );
        }
    }

    #[test]
    fn ac28_stuck_detection_is_invariant_under_translation() {
        let w = w100();
        let mut r = crate::rng::Rng::seeded(0x28_2A);
        for _ in 0..500 {
            let drift = r.range(-1.5, 1.5);
            let mut p = Vec2::new(50.0, 50.0);
            let path: Vec<Vec2> = (0..40)
                .map(|_| {
                    p = w.wrap(p.add(Vec2::new(drift + r.range(-1.0, 1.0), r.range(-1.0, 1.0))));
                    p
                })
                .collect();
            let offset = Vec2::new(r.range(-900.0, 900.0), r.range(-900.0, 900.0));
            let shifted: Vec<Vec2> = path.iter().map(|q| w.wrap(q.add(offset))).collect();
            assert_eq!(
                is_stuck(&path, &w, 20, STUCK_RATIO),
                is_stuck(&shifted, &w, 20, STUCK_RATIO),
                "stuck verdict changed under offset {offset:?}"
            );
        }
    }

    #[test]
    fn toroidal_centroid_of_a_seam_straddling_pair_is_the_seam_not_the_middle() {
        // The headline case: a naive coordinate mean of x=1 and x=99 gives 50,
        // which is the point furthest from both agents.
        let w = w100();
        let flock = [at(0, 1.0, 50.0), at(1, 99.0, 50.0)];
        let c = toroidal_centroid(&flock, &w);
        let to_seam = c.x.min(w.width - c.x);
        assert!(
            to_seam < 1e-9,
            "centroid x should be 0 (or 100) across the seam, got {}",
            c.x
        );
        assert!(
            (c.x - 50.0).abs() > 1.0,
            "centroid landed at the naive coordinate mean: {c:?}"
        );
        assert!((c.y - 50.0).abs() < 1e-9, "y should be 50, got {}", c.y);
    }

    #[test]
    fn toroidal_centroid_of_a_clustered_flock_matches_the_plain_mean() {
        // Away from a seam the circular mean must agree with the obvious
        // answer, otherwise the metric would be useless in the common case.
        let w = w100();
        let flock = [at(0, 40.0, 20.0), at(1, 50.0, 30.0), at(2, 60.0, 40.0)];
        let c = toroidal_centroid(&flock, &w);
        assert!(
            w.distance(c, Vec2::new(50.0, 30.0)) < 1e-9,
            "expected (50,30), got {c:?}"
        );
    }

    #[test]
    fn toroidal_centroid_is_always_inside_the_world() {
        let w = World::new(100.0, 250.0);
        let mut r = crate::rng::Rng::seeded(0xCE47);
        for _ in 0..2_000 {
            let n = 1 + (r.next_f64() * 20.0) as usize;
            let flock: Vec<Agent> = (0..n)
                .map(|i| at(i as u32, r.range(-500.0, 500.0), r.range(-500.0, 500.0)))
                .collect();
            let c = toroidal_centroid(&flock, &w);
            assert!(c.is_finite(), "centroid went non-finite: {c:?}");
            assert!(
                c.x >= 0.0 && c.x < w.width && c.y >= 0.0 && c.y < w.height,
                "centroid {c:?} outside {w:?}"
            );
        }
    }

    #[test]
    fn toroidal_centroid_of_an_empty_flock_is_the_origin() {
        assert_eq!(toroidal_centroid(&[], &w100()), Vec2::ZERO);
    }

    #[test]
    fn toroidal_centroid_moves_with_the_flock() {
        // Translation equivariance: shifting every agent by an offset shifts
        // the centroid by the same offset. A naive mean fails this the moment
        // the shift pushes the cluster over a seam.
        let w = w100();
        let mut r = crate::rng::Rng::seeded(0xCE48);
        for _ in 0..2_000 {
            let flock: Vec<Agent> = (0..8)
                .map(|i| at(i, r.range(30.0, 45.0), r.range(30.0, 45.0)))
                .collect();
            let off = Vec2::new(r.range(-400.0, 400.0), r.range(-400.0, 400.0));
            let shifted: Vec<Agent> = flock
                .iter()
                .map(|a| Agent {
                    pos: w.wrap(a.pos.add(off)),
                    ..*a
                })
                .collect();
            let expected = w.wrap(toroidal_centroid(&flock, &w).add(off));
            let actual = toroidal_centroid(&shifted, &w);
            assert!(
                w.distance(expected, actual) < 1e-9,
                "expected {expected:?}, got {actual:?}"
            );
        }
    }
}
