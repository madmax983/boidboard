//! Steering forces — the five behaviours that decide where an agent wants to go.
//!
//! Every function here is a **pure function of explicit inputs**: the agent
//! array, the world, the index of the agent being steered, and a
//! *pre-computed* neighbour index slice. Nothing in this module queries
//! `neighbors`, reads global state, or draws from an RNG, so a force can be
//! reproduced from a frame alone.
//!
//! Three invariants hold for every function in this module:
//!
//! * **Finite.** No input — coincident agents, zero-length headings, an agent
//!   sitting exactly on an obstacle's centre, a degenerate world — produces
//!   `NaN` or an infinity. A single `NaN` would spread through the flock in
//!   one tick and is unrecoverable, so totality is a hard requirement.
//! * **Deterministic.** Degenerate cases are broken by fixed, documented
//!   rules over integer agent identity, never by randomness and never by
//!   iteration order of an unordered collection. The `neighbors` slice is
//!   consumed in the order given, because float addition is not associative
//!   and the summation order is therefore part of the contract.
//! * **Toroidal.** Every position difference goes through
//!   [`World::displacement`]. Nothing here subtracts two positions directly.

use crate::config::{Obstacle, SimParams};
use crate::vec2::Vec2;
use crate::world::{Agent, World};

/// How far an obstacle's influence reaches, as a multiple of its radius.
///
/// Avoidance has to start *before* the surface or an agent only ever reacts
/// once it is already colliding; a multiple of the radius rather than a fixed
/// margin keeps that lead time proportional to the size of the thing being
/// dodged. Beyond this the obstacle contributes exactly nothing, which is
/// what makes a distant obstacle free rather than merely weak.
const OBSTACLE_INFLUENCE: f64 = 2.0;

/// Half of the square root of two: the diagonal component of a unit vector.
const DIAGONAL: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// Four evenly spaced unit axes (0°, 45°, 90°, 135°) used to break ties
/// between agents that occupy *exactly* the same point.
///
/// Four rather than eight because the tie-break also assigns a sign: a
/// coincident pair escapes along one axis in opposite senses, which covers
/// all eight compass directions while guaranteeing the pair separates.
const ESCAPE_AXES: [Vec2; 4] = [
    Vec2 { x: 1.0, y: 0.0 },
    Vec2 {
        x: DIAGONAL,
        y: DIAGONAL,
    },
    Vec2 { x: 0.0, y: 1.0 },
    Vec2 {
        x: -DIAGONAL,
        y: DIAGONAL,
    },
];

/// A coincident pair is treated as this fraction of the separation radius
/// apart. `1/d` has no finite value at `d == 0`, so the law needs a floor;
/// expressing it relative to the radius keeps the force scale-free.
const COINCIDENT_FRACTION: f64 = 1e-3;

/// SplitMix64's finalising avalanche, used to spread coincident pairs across
/// [`ESCAPE_AXES`] instead of collapsing every pile onto the x axis.
///
/// Pure wrapping integer arithmetic: no float, no trigonometry, and therefore
/// bit-identical on every platform, which a `sin`/`cos`-based angle would not
/// be. Duplicated from `rng` rather than shared because this is a fixed
/// property of the tie-break, not a generator that may ever be re-tuned.
fn mix64(key: u64) -> u64 {
    let mut z = key;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The direction agent `me` escapes when it is exactly coincident with
/// `other`. Each argument is that agent's `(id, slot index)`.
///
/// **Deterministic and antisymmetric.** The *axis* is chosen by hashing the
/// unordered pair of ids, so it depends on identity alone and is unaffected
/// by the order agents happen to sit in the array (which is what keeps the
/// double-buffered step permutation-invariant). The *sign* is decided by
/// which agent sorts lower, so `escape_direction(a, b) == -escape_direction(b, a)`
/// exactly, and a coincident pair is pushed apart rather than shoved along
/// together. The slot index only breaks a tie between duplicate ids, which
/// the identity contract already forbids.
fn escape_direction(me: (u32, usize), other: (u32, usize)) -> Vec2 {
    let (low, high, sign) = if me <= other {
        (me.0, other.0, 1.0)
    } else {
        (other.0, me.0, -1.0)
    };
    let key = (u64::from(low) << u32::BITS) | u64::from(high);
    let axis = mix64(key) as usize % ESCAPE_AXES.len();
    ESCAPE_AXES[axis].scale(sign)
}

/// The `1/d` weight to use when no distance is resolvable, i.e. the pair is
/// coincident or closer than `f64` can divide by.
///
/// Falls back to a unit push if even that overflows (a subnormal separation
/// radius), because a finite force is worth more than a faithful one.
fn coincident_weight(separation_radius: f64) -> f64 {
    let weight = 1.0 / (separation_radius * COINCIDENT_FRACTION);
    if weight.is_finite() { weight } else { 1.0 }
}

/// Steer away from neighbours closer than `separation_radius`.
///
/// Each neighbour strictly inside the radius contributes a unit vector
/// pointing from that neighbour to this agent, weighted by `1/distance`, so
/// closer neighbours push harder. Neighbours at or beyond the radius
/// contribute exactly nothing.
///
/// Toroidal: the escape direction comes from [`World::displacement`], so a
/// neighbour across a seam pushes the short way round.
///
/// # The radius is exclusive — and the neighbour query's is not
///
/// `d < separation_radius`, so a neighbour at **exactly** the radius pushes
/// with exactly [`Vec2::ZERO`]. The neighbour query is the other way round:
/// [`crate::neighbors`] uses `d <= neighbor_radius`, so an agent at exactly
/// the neighbourhood boundary **is** a flockmate. The asymmetry is deliberate,
/// because the two radii answer different kinds of question:
///
/// * `neighbor_radius` decides **membership** of a set. The closed ball is the
///   right answer there: an agent exactly on the boundary is as visible as one
///   a millionth of a unit inside, and excluding it would make a run's
///   neighbour sets depend on the last bit of a distance computation.
/// * `separation_radius` marks **where a behaviour has switched off**. The
///   value at the boundary is the point where the repulsion has just faded
///   out, so the boundary belongs to the "off" side.
///
/// `sim::validate` permits `separation_radius == neighbor_radius`, and this is
/// what that case means: the whole neighbourhood repels *except* its outermost
/// rim. The same strict `<` is used by
/// [`metrics::collision_count`](crate::metrics::collision_count), so "just
/// touching" is consistently not-yet-interacting across the kernel.
#[must_use]
pub fn separation(
    agents: &[Agent],
    world: &World,
    index: usize,
    neighbors: &[usize],
    separation_radius: f64,
) -> Vec2 {
    let Some(me) = agents.get(index) else {
        return Vec2::ZERO;
    };
    let (force, _) = neighbor_fold(agents, index, neighbors, |j, other| {
        let to_other = world.displacement(me.pos, other.pos);
        let distance = to_other.length();
        // Written as a positive test so a `NaN` distance (a degenerate world)
        // falls outside the radius rather than inside it.
        if distance < separation_radius {
            let weight = 1.0 / distance;
            if weight.is_finite() {
                to_other.normalize().scale(-weight)
            } else {
                // Coincident, or too close for `f64` to give a direction:
                // there is no vector between the two points to normalise, so
                // fall back to the deterministic tie-break.
                escape_direction((me.id, index), (other.id, j))
                    .scale(coincident_weight(separation_radius))
            }
        } else {
            Vec2::ZERO
        }
    });
    force
}

/// Steer toward the mean heading of `neighbors`.
///
/// The result is the *correction* — the mean neighbour velocity minus this
/// agent's own — so it vanishes at consensus. Returning the raw mean instead
/// would keep pushing a perfectly aligned flock in the direction it is
/// already travelling, which is an accelerator, not an alignment behaviour.
///
/// Position-free, hence world-free: velocities need no minimum-image
/// treatment because a velocity is already a displacement per unit time.
///
/// Returns exactly [`Vec2::ZERO`] when the slice names no usable neighbour —
/// an agent alone has nothing to align to, and must not be told to brake.
/// Exactly antiparallel neighbours cancel to `ZERO` rather than to `NaN`,
/// because nothing here divides by the length of the mean.
#[must_use]
pub fn alignment(agents: &[Agent], index: usize, neighbors: &[usize]) -> Vec2 {
    let Some(me) = agents.get(index) else {
        return Vec2::ZERO;
    };
    let (sum, count) = neighbor_fold(agents, index, neighbors, |_, other| other.vel);
    if count == 0 {
        return Vec2::ZERO;
    }
    sum.scale(1.0 / count as f64).sub(me.vel)
}

/// Steer toward the centroid of `neighbors`.
///
/// **Toroidal, and this is the whole point of the function.** The centroid is
/// the mean of the *displacement vectors* from this agent to each neighbour,
/// never the mean of their raw coordinates. In a 100-wide world an agent at
/// `x=1` with a neighbour at `x=99` is pulled two units in `-x` across the
/// seam; a coordinate mean would compute a centroid at `x=99` and haul it 98
/// units the wrong way, through the middle of the world. There is no
/// meaningful "mean coordinate" on a torus — the average of two points on a
/// circle is not a point on that circle — so the displacement form is the
/// only correct one, not merely the more careful one.
///
/// Returns exactly [`Vec2::ZERO`] when the slice names no usable neighbour.
#[must_use]
pub fn cohesion(agents: &[Agent], world: &World, index: usize, neighbors: &[usize]) -> Vec2 {
    let Some(me) = agents.get(index) else {
        return Vec2::ZERO;
    };
    let (sum, count) = neighbor_fold(agents, index, neighbors, |_, other| {
        world.displacement(me.pos, other.pos)
    });
    if count == 0 {
        return Vec2::ZERO;
    }
    sum.scale(1.0 / count as f64)
}

/// Steer toward `goal`, or nowhere at all when there is no goal.
///
/// Toroidal: the pull is [`World::displacement`] from the agent to the goal,
/// so a goal just across a seam is chased the short way round rather than all
/// the way back through the world.
///
/// The magnitude is the toroidal distance, which makes this an *arrival*
/// behaviour: the pull fades as the agent closes in and is exactly
/// [`Vec2::ZERO`] on the goal, so an arrived agent is not made to jitter
/// around it. It is also bounded by half the world diagonal and therefore
/// always finite.
///
/// `None` is a genuine "no goal" rather than a goal at the origin, and
/// returns exactly `ZERO`. That is what lets `w_goal = 0` be provably
/// irrelevant: with no goal *or* no weight, this behaviour cannot influence
/// the blend at all.
#[must_use]
pub fn goal_seek(agents: &[Agent], world: &World, index: usize, goal: Option<Vec2>) -> Vec2 {
    let (Some(me), Some(target)) = (agents.get(index), goal) else {
        return Vec2::ZERO;
    };
    world.displacement(me.pos, target)
}

/// Steer away from every obstacle within reach, summed in slice order.
///
/// The influence of an obstacle reaches [`OBSTACLE_INFLUENCE`] times its
/// radius. Across that band the push grows linearly from nothing at the outer
/// edge, to one at the surface, and on past one for an agent that is already
/// **inside** — which is pushed radially outward rather than abandoned, so a
/// bad spawn or a fast tick that tunnels into a rock recovers instead of
/// sticking. The linear law is bounded, so the force stays finite however
/// deep the agent is.
///
/// Toroidal: the radial direction comes from [`World::displacement`], so an
/// obstacle just across a seam is dodged rather than ignored.
///
/// Obstacles with a non-positive or non-finite radius contribute nothing, and
/// so do obstacles so large that their influence band
/// ([`obstacle_influence_radius`]) overflows to infinity.
#[must_use]
pub fn obstacle_avoidance(
    agents: &[Agent],
    world: &World,
    index: usize,
    obstacles: &[Obstacle],
) -> Vec2 {
    let Some(me) = agents.get(index) else {
        return Vec2::ZERO;
    };
    let mut force = Vec2::ZERO;
    for obstacle in obstacles {
        force = force.add(obstacle_push(me, world, obstacle));
    }
    force
}

/// How far an obstacle of this radius reaches: [`OBSTACLE_INFLUENCE`] times it.
///
/// Public because `sim::validate` reports an obstacle whose band is unusable,
/// and the validator must ask the force law rather than re-deriving the
/// multiple — two copies of the same constant is two chances to disagree about
/// which obstacles a run can actually use.
///
/// The result is **not** guaranteed finite: a radius above roughly `8.99e307`
/// is itself finite (so [`crate::world::World`] geometry and the plain radius
/// checks accept it) while its band overflows to `+inf`. Callers must handle
/// that; see [`obstacle_push`].
#[must_use]
pub fn obstacle_influence_radius(radius: f64) -> f64 {
    radius * OBSTACLE_INFLUENCE
}

/// The repulsion a single obstacle applies to one agent.
fn obstacle_push(me: &Agent, world: &World, obstacle: &Obstacle) -> Vec2 {
    if obstacle.radius <= 0.0 || !obstacle.radius.is_finite() {
        return Vec2::ZERO;
    }
    let influence = obstacle_influence_radius(obstacle.radius);
    // A finite radius does not imply a finite band: `radius * 2` overflows
    // above ~8.99e307, and the strength below would then be `inf/inf` = `NaN`.
    // `blend` finishes with `Vec2::limit`, which maps a `NaN` total to `ZERO`,
    // so that `NaN` would not surface as a broken run — it would silently
    // delete every steering behaviour from every agent for the whole run.
    // The radius check above cannot catch it, because the overflow is a
    // property of the band, not of the radius.
    if !influence.is_finite() {
        return Vec2::ZERO;
    }
    let to_center = world.displacement(me.pos, obstacle.center);
    let distance = to_center.length();
    // Positive comparison so a `NaN` distance falls outside the band.
    if distance < influence {
        // Denominator is `radius * (OBSTACLE_INFLUENCE - 1)`, a strictly
        // positive finite number, so the strength cannot blow up.
        let strength = (influence - distance) / (influence - obstacle.radius);
        let outward = to_center.normalize().scale(-1.0);
        if outward.length_squared() > 0.0 {
            outward.scale(strength)
        } else {
            // Dead centre: there is no radial direction to push along.
            centre_escape(me).scale(strength)
        }
    } else {
        Vec2::ZERO
    }
}

/// The direction an agent escapes when it sits exactly on an obstacle's
/// centre — the perfectly head-on case, where "away from the centre" names no
/// vector at all.
///
/// **Deterministic**, by a two-step cascade:
///
/// 1. A moving agent is steered along the **left normal of its own heading**,
///    i.e. sideways. Reversing it along its heading instead would only push
///    it back down the axis it is already stuck on; picking a side is the
///    whole point of a head-on tie-break. Left rather than right is
///    arbitrary, but it is *fixed*, which is what the contract asks for.
/// 2. A motionless agent has no heading to turn from, so the axis comes from
///    the same integer hash of its id used for coincident neighbours.
///
/// Neither step consults an RNG, a clock, or the array order, so two calls on
/// the same frame return bit-identical vectors.
fn centre_escape(me: &Agent) -> Vec2 {
    let heading = me.vel.normalize();
    if heading.length_squared() > 0.0 {
        Vec2::new(-heading.y, heading.x)
    } else {
        ESCAPE_AXES[mix64(u64::from(me.id)) as usize % ESCAPE_AXES.len()]
    }
}

/// The total steering force on one agent: the five behaviours, weighted,
/// summed, and clamped to `params.max_force`.
///
/// ```text
/// F = w_separation * sep
///   + w_alignment  * align
///   + w_cohesion   * coh
///   + w_goal       * goal
///   + w_avoidance  * avoid
/// ```
///
/// **The summation order above is part of the contract**, not an
/// implementation detail: float addition is not associative, so reordering
/// these five terms changes the last bits of the result and with them the
/// canonical state hash of the whole run.
///
/// The clamp is [`Vec2::limit`], which returns a force already within the cap
/// **unchanged** — bit-identical, not merely close — so clamping cannot
/// perturb an under-force agent. `limit` is also total: were any weight
/// `NaN`, the result would be [`Vec2::ZERO`] rather than a poisoned flock.
///
/// `world` is taken separately from `params.world` so the whole module reads
/// the topology from one argument; callers pass `&params.world`.
///
/// **This is acceleration, not velocity.** The caller integrates it — scaling
/// by `dt`, adding to velocity, and applying `max_speed` — none of which
/// happens here.
#[must_use]
pub fn blend(
    params: &SimParams,
    agents: &[Agent],
    world: &World,
    index: usize,
    neighbors: &[usize],
) -> Vec2 {
    let sep = separation(agents, world, index, neighbors, params.separation_radius);
    let align = alignment(agents, index, neighbors);
    let coh = cohesion(agents, world, index, neighbors);
    let goal = goal_seek(agents, world, index, params.goal);
    let avoid = obstacle_avoidance(agents, world, index, &params.obstacles);

    sep.scale(params.w_separation)
        .add(align.scale(params.w_alignment))
        .add(coh.scale(params.w_cohesion))
        .add(goal.scale(params.w_goal))
        .add(avoid.scale(params.w_avoidance))
        .limit(params.max_force)
}

/// Sum `contribution` over every usable entry of `neighbors`, returning the
/// total and how many entries contributed.
///
/// "Usable" excludes the steered agent itself and any index outside `agents`,
/// so a caller's stale or self-including neighbour list degrades to a smaller
/// flock rather than a panic or a self-interaction.
///
/// **Order matters.** The slice is folded left to right exactly as given:
/// float addition is not associative, so the summation order is part of the
/// reproducibility contract, not an implementation detail.
fn neighbor_fold(
    agents: &[Agent],
    index: usize,
    neighbors: &[usize],
    contribution: impl Fn(usize, &Agent) -> Vec2,
) -> (Vec2, usize) {
    let mut sum = Vec2::ZERO;
    let mut count = 0usize;
    for &j in neighbors {
        if j == index {
            continue;
        }
        let Some(other) = agents.get(j) else {
            continue;
        };
        sum = sum.add(contribution(j, other));
        count += 1;
    }
    (sum, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Obstacle, SimParams};
    use crate::rng::Rng;
    use crate::vec2::Vec2;
    use crate::world::{Agent, World};

    fn w100() -> World {
        World::new(100.0, 100.0)
    }

    fn at(id: u32, pos: Vec2) -> Agent {
        Agent {
            id,
            pos,
            vel: Vec2::ZERO,
        }
    }

    fn moving(id: u32, vel: Vec2) -> Agent {
        Agent {
            id,
            pos: Vec2::ZERO,
            vel,
        }
    }

    // ---------------------------------------------------------------- AC-11

    #[test]
    fn separation_pushes_away_from_a_close_neighbour() {
        // Neighbour sits two units to the +x side, so the escape is -x.
        let agents = [at(0, Vec2::new(10.0, 10.0)), at(1, Vec2::new(12.0, 10.0))];
        let f = separation(&agents, &w100(), 0, &[1], 5.0);
        assert!(f.x < 0.0, "must push away along -x, got {f:?}");
        assert_eq!(f.y, 0.0, "a purely horizontal pair must not push in y");
    }

    #[test]
    fn separation_ignores_a_neighbour_outside_the_radius() {
        let agents = [at(0, Vec2::new(10.0, 10.0)), at(1, Vec2::new(20.0, 10.0))];
        let f = separation(&agents, &w100(), 0, &[1], 5.0);
        assert_eq!(
            f,
            Vec2::ZERO,
            "a neighbour beyond the radius contributes nothing"
        );
    }

    #[test]
    fn separation_treats_its_radius_as_a_strict_threshold() {
        // Boundary semantics, pinned. `separation` uses `d < radius` while the
        // neighbour query uses `d <= radius`, and until now nothing
        // distinguished the two: turning `<` into `<=` here left the whole
        // suite green, because every other separation test sits comfortably
        // inside or outside the radius rather than exactly on it.
        let w = w100();
        let exactly = [at(0, Vec2::new(10.0, 10.0)), at(1, Vec2::new(15.0, 10.0))];
        assert_eq!(
            w.distance(exactly[0].pos, exactly[1].pos),
            5.0,
            "precondition: the pair is exactly the radius apart"
        );
        assert_eq!(
            separation(&exactly, &w, 0, &[1], 5.0),
            Vec2::ZERO,
            "a neighbour at exactly the separation radius must not repel"
        );

        // One ULP inside, and the repulsion is on — so the assertion above is
        // about the boundary and not about an inert scenario.
        let inside = f64::from_bits(15.0_f64.to_bits() - 1);
        let just_inside = [at(0, Vec2::new(10.0, 10.0)), at(1, Vec2::new(inside, 10.0))];
        let f = separation(&just_inside, &w, 0, &[1], 5.0);
        assert!(
            f.length() > 0.0,
            "one ULP inside the radius must repel: {f:?}"
        );
        assert!(f.x < 0.0, "and must still push away: {f:?}");

        // The other half of the documented asymmetry: at that same distance,
        // the neighbour query counts the agent IN.
        assert_eq!(
            crate::neighbors::neighbors_naive(&exactly, &w, 0, 5.0),
            vec![1],
            "the neighbour radius is inclusive where the separation radius is not"
        );
    }

    #[test]
    fn separation_pushes_harder_the_closer_the_neighbour() {
        // The 1/distance weighting is what makes separation a *pressure*
        // rather than a uniform shove.
        let near = [at(0, Vec2::new(10.0, 10.0)), at(1, Vec2::new(11.0, 10.0))];
        let far = [at(0, Vec2::new(10.0, 10.0)), at(1, Vec2::new(14.0, 10.0))];
        let near_f = separation(&near, &w100(), 0, &[1], 5.0).length();
        let far_f = separation(&far, &w100(), 0, &[1], 5.0).length();
        assert!(near_f > far_f, "near {near_f} should exceed far {far_f}");
    }

    #[test]
    fn separation_of_coincident_agents_is_finite() {
        // The classic divide-by-zero: two agents at the same point have no
        // direction between them, and `1/0` is an infinity.
        let agents = [at(0, Vec2::new(10.0, 10.0)), at(1, Vec2::new(10.0, 10.0))];
        let f = separation(&agents, &w100(), 0, &[1], 5.0);
        assert!(!f.x.is_nan() && !f.y.is_nan(), "produced NaN: {f:?}");
        assert!(f.is_finite(), "produced a non-finite force: {f:?}");
        assert!(
            f.length() > 0.0,
            "coincident agents must be pushed apart, not left stacked: {f:?}"
        );
    }

    #[test]
    fn separation_of_coincident_agents_is_deterministic() {
        // Calling twice must give a bit-identical answer: the tie-break may
        // be arbitrary but it may not be random.
        let agents = [at(0, Vec2::new(10.0, 10.0)), at(1, Vec2::new(10.0, 10.0))];
        let first = separation(&agents, &w100(), 0, &[1], 5.0);
        let second = separation(&agents, &w100(), 0, &[1], 5.0);
        assert_eq!(first, second, "tie-break is not deterministic");
    }

    #[test]
    fn coincident_agents_are_pushed_in_opposite_directions() {
        // Antisymmetry is what makes the pair separate. A tie-break keyed on
        // the agent alone would shove both the same way for ever.
        let agents = [at(0, Vec2::new(10.0, 10.0)), at(1, Vec2::new(10.0, 10.0))];
        let a = separation(&agents, &w100(), 0, &[1], 5.0);
        let b = separation(&agents, &w100(), 1, &[0], 5.0);
        assert_eq!(a, b.scale(-1.0), "coincident pair must escape back-to-back");
    }

    // ---------------------------------------------------------------- AC-12

    #[test]
    fn alignment_steers_toward_the_mean_neighbour_heading() {
        // Two neighbours heading +x at different speeds; the agent is at
        // rest, so the whole of the mean heading is the correction.
        let agents = [
            moving(0, Vec2::ZERO),
            moving(1, Vec2::new(2.0, 0.0)),
            moving(2, Vec2::new(4.0, 2.0)),
        ];
        let f = alignment(&agents, 0, &[1, 2]);
        assert_eq!(
            f,
            Vec2::new(3.0, 1.0),
            "must be the mean neighbour velocity"
        );
    }

    #[test]
    fn alignment_vanishes_once_the_agent_matches_the_flock() {
        // The property that makes alignment converge instead of accelerating
        // the flock for ever: it is a *correction*, so it is zero at
        // consensus.
        let agents = [
            moving(0, Vec2::new(2.0, -1.0)),
            moving(1, Vec2::new(2.0, -1.0)),
            moving(2, Vec2::new(2.0, -1.0)),
        ];
        assert_eq!(alignment(&agents, 0, &[1, 2]), Vec2::ZERO);
    }

    #[test]
    fn alignment_of_antiparallel_neighbours_is_zero_not_nan() {
        // The mean of (1,0) and (-1,0) is the zero vector, which has no
        // heading to normalise.
        let agents = [
            moving(0, Vec2::ZERO),
            moving(1, Vec2::new(1.0, 0.0)),
            moving(2, Vec2::new(-1.0, 0.0)),
        ];
        let f = alignment(&agents, 0, &[1, 2]);
        assert!(!f.x.is_nan() && !f.y.is_nan(), "produced NaN: {f:?}");
        assert!(f.is_finite(), "produced a non-finite force: {f:?}");
        assert_eq!(f, Vec2::ZERO, "antiparallel neighbours must cancel exactly");
    }

    #[test]
    fn alignment_with_no_neighbours_is_zero() {
        let agents = [moving(0, Vec2::new(3.0, 4.0))];
        assert_eq!(
            alignment(&agents, 0, &[]),
            Vec2::ZERO,
            "an agent with no flockmates has nothing to align to, and must \
             not be told to brake"
        );
    }

    // ---------------------------------------------------------------- AC-13

    #[test]
    fn cohesion_pulls_across_the_seam_not_toward_the_middle() {
        // THE toroidal test. World is 100 wide; the agent sits at x=1 and its
        // only neighbour at x=99, which is two units to its LEFT across the
        // seam. A naive mean of coordinates would put the centroid at x=99
        // and pull the agent 98 units to the right, straight through the
        // middle of the world.
        let agents = [at(0, Vec2::new(1.0, 50.0)), at(1, Vec2::new(99.0, 50.0))];
        let f = cohesion(&agents, &w100(), 0, &[1]);
        assert!(
            f.x < 0.0,
            "cohesion must pull across the seam (-x), got {f:?}"
        );
        assert_eq!(f, Vec2::new(-2.0, 0.0), "must be the toroidal displacement");
    }

    #[test]
    fn cohesion_pulls_toward_the_neighbour_centroid() {
        let agents = [
            at(0, Vec2::new(10.0, 10.0)),
            at(1, Vec2::new(14.0, 10.0)),
            at(2, Vec2::new(12.0, 16.0)),
        ];
        // Centroid of (14,10) and (12,16) is (13,13), i.e. (3,3) away.
        let f = cohesion(&agents, &w100(), 0, &[1, 2]);
        assert_eq!(f, Vec2::new(3.0, 3.0));
    }

    #[test]
    fn cohesion_across_the_seam_stays_small() {
        // A stronger statement of the same bug: the pull toward a neighbour
        // two units away must be a two-unit pull, not a 98-unit one.
        let agents = [at(0, Vec2::new(1.0, 1.0)), at(1, Vec2::new(99.0, 99.0))];
        let f = cohesion(&agents, &w100(), 0, &[1]);
        assert!(
            f.length() < 5.0,
            "a nearby neighbour produced a huge pull ({}), so the centroid \
             was averaged in raw coordinates: {f:?}",
            f.length()
        );
    }

    #[test]
    fn cohesion_with_no_neighbours_is_zero() {
        let agents = [at(0, Vec2::new(10.0, 10.0))];
        assert_eq!(cohesion(&agents, &w100(), 0, &[]), Vec2::ZERO);
    }

    // ---------------------------------------------------------------- AC-14

    #[test]
    fn goal_seek_steers_toward_the_goal() {
        let agents = [at(0, Vec2::new(10.0, 10.0))];
        let f = goal_seek(&agents, &w100(), 0, Some(Vec2::new(13.0, 14.0)));
        assert_eq!(f, Vec2::new(3.0, 4.0));
    }

    #[test]
    fn goal_seek_takes_the_short_way_round_the_seam() {
        // Agent at x=1, goal at x=99: two units left, not 98 right.
        let agents = [at(0, Vec2::new(1.0, 50.0))];
        let f = goal_seek(&agents, &w100(), 0, Some(Vec2::new(99.0, 50.0)));
        assert!(f.x < 0.0, "must cross the seam, got {f:?}");
        assert_eq!(f, Vec2::new(-2.0, 0.0));
    }

    #[test]
    fn goal_seek_without_a_goal_is_zero() {
        let agents = [at(0, Vec2::new(10.0, 10.0))];
        assert_eq!(goal_seek(&agents, &w100(), 0, None), Vec2::ZERO);
    }

    #[test]
    fn goal_seek_weakens_on_arrival() {
        // The pull is the displacement itself, so it fades to nothing at the
        // goal instead of overshooting and orbiting.
        let far = [at(0, Vec2::new(10.0, 10.0))];
        let near = [at(0, Vec2::new(19.0, 20.0))];
        let goal = Some(Vec2::new(20.0, 20.0));
        assert!(
            goal_seek(&far, &w100(), 0, goal).length()
                > goal_seek(&near, &w100(), 0, goal).length()
        );
        let arrived = [at(0, Vec2::new(20.0, 20.0))];
        assert_eq!(goal_seek(&arrived, &w100(), 0, goal), Vec2::ZERO);
    }

    // ---------------------------------------------------------------- AC-15

    fn rock(center: Vec2, radius: f64) -> Obstacle {
        Obstacle { center, radius }
    }

    #[test]
    fn obstacle_avoidance_pushes_an_approaching_agent_away() {
        // Agent is outside the rock but within its influence band.
        let agents = [at(0, Vec2::new(57.0, 50.0))];
        let rocks = [rock(Vec2::new(50.0, 50.0), 5.0)];
        let f = obstacle_avoidance(&agents, &w100(), 0, &rocks);
        assert!(f.x > 0.0, "must be pushed away along +x, got {f:?}");
        assert_eq!(f.y, 0.0, "a head-on approach must not gain a y component");
    }

    #[test]
    fn obstacle_avoidance_pushes_an_agent_inside_radially_outward() {
        let agents = [at(0, Vec2::new(52.0, 50.0))];
        let rocks = [rock(Vec2::new(50.0, 50.0), 5.0)];
        let f = obstacle_avoidance(&agents, &w100(), 0, &rocks);
        assert!(f.is_finite(), "produced a non-finite force: {f:?}");
        assert!(f.x > 0.0, "must be pushed radially outward, got {f:?}");
        assert_eq!(f.y, 0.0);
    }

    #[test]
    fn obstacle_avoidance_pushes_harder_from_deeper_inside() {
        let rocks = [rock(Vec2::new(50.0, 50.0), 5.0)];
        let deep = [at(0, Vec2::new(51.0, 50.0))];
        let shallow = [at(0, Vec2::new(54.0, 50.0))];
        let deep_f = obstacle_avoidance(&deep, &w100(), 0, &rocks).length();
        let shallow_f = obstacle_avoidance(&shallow, &w100(), 0, &rocks).length();
        assert!(
            deep_f > shallow_f,
            "deep {deep_f} should exceed shallow {shallow_f}"
        );
    }

    #[test]
    fn obstacle_avoidance_ignores_a_distant_obstacle() {
        let agents = [at(0, Vec2::new(70.0, 50.0))];
        let rocks = [rock(Vec2::new(50.0, 50.0), 5.0)];
        assert_eq!(
            obstacle_avoidance(&agents, &w100(), 0, &rocks),
            Vec2::ZERO,
            "an obstacle beyond its influence band must contribute nothing"
        );
    }

    #[test]
    fn obstacle_avoidance_crosses_the_seam() {
        // Rock at x=99, agent at x=1: the rock is two units to the LEFT, so
        // the escape is +x.
        let agents = [at(0, Vec2::new(1.0, 50.0))];
        let rocks = [rock(Vec2::new(99.0, 50.0), 5.0)];
        let f = obstacle_avoidance(&agents, &w100(), 0, &rocks);
        assert!(f.x > 0.0, "seam-crossing escape must be +x, got {f:?}");
    }

    #[test]
    fn obstacle_avoidance_with_no_obstacles_is_zero() {
        let agents = [at(0, Vec2::new(10.0, 10.0))];
        assert_eq!(obstacle_avoidance(&agents, &w100(), 0, &[]), Vec2::ZERO);
    }

    #[test]
    fn obstacle_avoidance_at_the_exact_centre_still_pushes() {
        // The degenerate head-on case: dead centre, there is no radial
        // direction at all. Doing nothing here would strand the agent in the
        // middle of the rock for ever.
        let agents = [Agent {
            id: 0,
            pos: Vec2::new(50.0, 50.0),
            vel: Vec2::new(1.0, 0.0),
        }];
        let rocks = [rock(Vec2::new(50.0, 50.0), 5.0)];
        let f = obstacle_avoidance(&agents, &w100(), 0, &rocks);
        assert!(!f.x.is_nan() && !f.y.is_nan(), "produced NaN: {f:?}");
        assert!(f.is_finite(), "produced a non-finite force: {f:?}");
        assert!(
            f.length() > 0.0,
            "an agent dead centre must be pushed out: {f:?}"
        );
    }

    #[test]
    fn obstacle_avoidance_at_the_exact_centre_is_deterministic() {
        let agents = [Agent {
            id: 0,
            pos: Vec2::new(50.0, 50.0),
            vel: Vec2::new(1.0, 0.0),
        }];
        let rocks = [rock(Vec2::new(50.0, 50.0), 5.0)];
        let first = obstacle_avoidance(&agents, &w100(), 0, &rocks);
        let second = obstacle_avoidance(&agents, &w100(), 0, &rocks);
        assert_eq!(first, second, "head-on tie-break is not deterministic");
    }

    #[test]
    fn a_head_on_agent_at_the_centre_is_pushed_sideways() {
        // Pushing it straight back down its own heading would only stall it
        // on the axis it is already stuck on; the tie-break steers it aside.
        let agents = [Agent {
            id: 0,
            pos: Vec2::new(50.0, 50.0),
            vel: Vec2::new(3.0, 0.0),
        }];
        let rocks = [rock(Vec2::new(50.0, 50.0), 5.0)];
        let f = obstacle_avoidance(&agents, &w100(), 0, &rocks);
        assert!(
            f.dot(agents[0].vel).abs() < 1e-9,
            "escape must be perpendicular to the heading, got {f:?}"
        );
    }

    #[test]
    fn a_motionless_agent_at_the_centre_is_still_pushed_out() {
        // No heading to steer aside from, so the tie-break falls back to
        // identity.
        let agents = [at(7, Vec2::new(50.0, 50.0))];
        let rocks = [rock(Vec2::new(50.0, 50.0), 5.0)];
        let f = obstacle_avoidance(&agents, &w100(), 0, &rocks);
        assert!(f.is_finite(), "produced a non-finite force: {f:?}");
        assert!(
            f.length() > 0.0,
            "a stationary agent must not be stranded: {f:?}"
        );
        assert_eq!(f, obstacle_avoidance(&agents, &w100(), 0, &rocks));
    }

    #[test]
    fn an_obstacle_whose_influence_band_overflows_contributes_nothing() {
        // `validate` rejects a non-finite obstacle *radius*, but a radius of
        // 1e308 is finite and accepted — and the influence band is
        // `radius * OBSTACLE_INFLUENCE`, which overflows to `+inf` for any
        // radius above about 8.99e307. The strength is then
        // `(inf - distance) / (inf - radius)` = `inf / inf` = `NaN`, which is
        // the exact failure the module's totality rule exists to forbid.
        let agents = [at(0, Vec2::new(10.0, 10.0))];
        for radius in [1e308, f64::MAX, 9e307] {
            let rocks = [rock(Vec2::new(50.0, 50.0), radius)];
            let f = obstacle_avoidance(&agents, &w100(), 0, &rocks);
            assert!(
                f.is_finite(),
                "radius {radius:e} produced a non-finite force: {f:?}"
            );
        }
    }

    #[test]
    fn an_obstacle_with_an_overflowing_influence_band_does_not_disable_the_flock() {
        // Why the NaN above is worse than it looks. `blend` finishes with
        // `Vec2::limit`, which is total and maps a `NaN` total to `ZERO`. So a
        // single absurd obstacle does not produce a visibly broken run — it
        // silently deletes **every** steering behaviour from **every** agent,
        // for the whole run, with nothing logged and no invariant breached.
        let (mut params, agents, neighbors) = all_five_active();
        let before = blend(&params, &agents, &params.world, 0, &neighbors);
        assert!(before.length() > 0.0, "the base scenario must steer");

        params.obstacles.push(rock(Vec2::new(90.0, 90.0), 1e308));
        let after = blend(&params, &agents, &params.world, 0, &neighbors);
        assert!(after.is_finite(), "blend went non-finite: {after:?}");
        assert!(
            after.length() > 0.0,
            "one absurd obstacle silently switched off all five behaviours: \
             {before:?} became {after:?}"
        );
    }

    #[test]
    fn the_influence_radius_is_the_force_laws_own_band() {
        // `sim::validate` reports an obstacle whose influence band overflows,
        // and it has to ask this module rather than re-deriving the multiple,
        // or the two can drift apart into disagreeing about which obstacles
        // are usable.
        assert_eq!(obstacle_influence_radius(5.0), 10.0);
        assert!(!obstacle_influence_radius(1e308).is_finite());
        assert!(obstacle_influence_radius(8.9e307).is_finite());
    }

    // ---------------------------------------------------------------- AC-16

    /// A scenario in which all five behaviours are simultaneously active, so
    /// that a weight change cannot be confused with a component that happens
    /// to be zero.
    fn all_five_active() -> (SimParams, Vec<Agent>, Vec<usize>) {
        let params = SimParams {
            world: World::new(100.0, 100.0),
            neighbor_radius: 25.0,
            separation_radius: 5.0,
            max_force: 1e9, // deliberately un-clamped
            w_separation: 1.0,
            w_alignment: 1.0,
            w_cohesion: 1.0,
            w_goal: 1.0,
            w_avoidance: 1.0,
            goal: Some(Vec2::new(40.0, 40.0)),
            obstacles: vec![rock(Vec2::new(16.0, 10.0), 5.0)],
            ..SimParams::default()
        };
        let agents = vec![
            Agent {
                id: 0,
                pos: Vec2::new(10.0, 10.0),
                vel: Vec2::new(1.0, 0.0),
            },
            Agent {
                id: 1,
                pos: Vec2::new(12.0, 11.0),
                vel: Vec2::new(0.0, 2.0),
            },
        ];
        (params, agents, vec![1])
    }

    /// The five components of the scenario, in blend order.
    fn components(p: &SimParams, agents: &[Agent], neighbors: &[usize]) -> [Vec2; 5] {
        [
            separation(agents, &p.world, 0, neighbors, p.separation_radius),
            alignment(agents, 0, neighbors),
            cohesion(agents, &p.world, 0, neighbors),
            goal_seek(agents, &p.world, 0, p.goal),
            obstacle_avoidance(agents, &p.world, 0, &p.obstacles),
        ]
    }

    #[test]
    fn blend_is_the_weighted_sum_of_its_components() {
        let (mut params, agents, neighbors) = all_five_active();
        params.w_separation = 1.5;
        params.w_alignment = 0.75;
        params.w_cohesion = 0.25;
        params.w_goal = 2.0;
        params.w_avoidance = 3.0;
        let [sep, al, coh, gl, av] = components(&params, &agents, &neighbors);
        for c in [sep, al, coh, gl, av] {
            assert!(c.length() > 0.0, "scenario must exercise every component");
        }
        // Exact equality, and in this order: float addition is not
        // associative, so the summation order is part of the contract.
        let expected = sep
            .scale(params.w_separation)
            .add(al.scale(params.w_alignment))
            .add(coh.scale(params.w_cohesion))
            .add(gl.scale(params.w_goal))
            .add(av.scale(params.w_avoidance));
        assert_eq!(
            blend(&params, &agents, &params.world, 0, &neighbors),
            expected
        );
    }

    #[test]
    fn blend_is_clamped_to_max_force() {
        let (mut params, agents, neighbors) = all_five_active();
        params.max_force = 0.25;
        let f = blend(&params, &agents, &params.world, 0, &neighbors);
        assert!(
            f.length() <= 0.25 + 1e-12,
            "clamp breached: {f:?} has length {}",
            f.length()
        );
        assert!((f.length() - 0.25).abs() < 1e-12, "this case should clamp");
    }

    #[test]
    fn each_weight_moves_the_blend_along_its_own_component() {
        let (base, agents, neighbors) = all_five_active();
        let parts = components(&base, &agents, &neighbors);
        /// Sets one weight on a copy of the parameters.
        type SetWeight = fn(&mut SimParams, f64);
        let setters: [(&str, SetWeight); 5] = [
            ("w_separation", |p, w| p.w_separation = w),
            ("w_alignment", |p, w| p.w_alignment = w),
            ("w_cohesion", |p, w| p.w_cohesion = w),
            ("w_goal", |p, w| p.w_goal = w),
            ("w_avoidance", |p, w| p.w_avoidance = w),
        ];
        for (i, (name, set)) in setters.iter().enumerate() {
            let mut low = base.clone();
            set(&mut low, 1.0);
            let mut high = base.clone();
            set(&mut high, 2.0);
            let moved = blend(&high, &agents, &high.world, 0, &neighbors)
                .sub(blend(&low, &agents, &low.world, 0, &neighbors));
            let component = parts[i];
            assert!(
                moved.dot(component) > 0.0,
                "raising {name} moved the blend {moved:?}, not along {component:?}"
            );
            let cross = moved.x * component.y - moved.y * component.x;
            assert!(
                cross.abs() < 1e-9,
                "raising {name} moved the blend off its component axis: \
                 {moved:?} vs {component:?}"
            );
        }
    }

    #[test]
    fn blend_never_exceeds_max_force() {
        let mut r = Rng::seeded(0x5EED_F00D);
        for _ in 0..5_000 {
            let world = World::new(r.range(10.0, 200.0), r.range(10.0, 200.0));
            let agents: Vec<Agent> = (0..6)
                .map(|id| Agent {
                    id,
                    pos: Vec2::new(r.range(0.0, world.width), r.range(0.0, world.height)),
                    vel: Vec2::new(r.range(-3.0, 3.0), r.range(-3.0, 3.0)),
                })
                .collect();
            let params = SimParams {
                world,
                separation_radius: r.range(0.0, 20.0),
                max_force: r.range(0.0, 5.0),
                w_separation: r.range(-5.0, 5.0),
                w_alignment: r.range(-5.0, 5.0),
                w_cohesion: r.range(-5.0, 5.0),
                w_goal: r.range(-5.0, 5.0),
                w_avoidance: r.range(-5.0, 5.0),
                goal: Some(Vec2::new(
                    r.range(0.0, world.width),
                    r.range(0.0, world.height),
                )),
                obstacles: vec![rock(
                    Vec2::new(r.range(0.0, world.width), r.range(0.0, world.height)),
                    r.range(0.0, 15.0),
                )],
                ..SimParams::default()
            };
            let neighbors: Vec<usize> = (1..agents.len()).collect();
            let f = blend(&params, &agents, &params.world, 0, &neighbors);
            assert!(f.is_finite(), "blend went non-finite: {f:?}");
            assert!(
                f.length() <= params.max_force + 1e-9,
                "blend {f:?} of length {} exceeds max_force {}",
                f.length(),
                params.max_force
            );
        }
    }

    // ---------------------------------------------------------------- AC-17

    #[test]
    fn with_no_neighbours_the_flocking_forces_are_exactly_zero() {
        // Exactly zero, not approximately: a lone agent must contribute
        // nothing at all to the accumulator, or it drifts.
        let agents = [Agent {
            id: 0,
            pos: Vec2::new(10.0, 10.0),
            vel: Vec2::new(1.0, 2.0),
        }];
        assert_eq!(separation(&agents, &w100(), 0, &[], 5.0), Vec2::ZERO);
        assert_eq!(alignment(&agents, 0, &[]), Vec2::ZERO);
        assert_eq!(cohesion(&agents, &w100(), 0, &[]), Vec2::ZERO);
    }

    #[test]
    fn with_no_neighbours_goal_and_avoidance_still_act() {
        // The distinction worth documenting: the three *flocking* forces need
        // flockmates, but the two task forces are properties of the agent and
        // the world, and keep working for a lone agent.
        let agents = [at(0, Vec2::new(52.0, 50.0))];
        let params = SimParams {
            world: World::new(100.0, 100.0),
            max_force: 1e9,
            w_separation: 1.0,
            w_alignment: 1.0,
            w_cohesion: 1.0,
            w_goal: 1.0,
            w_avoidance: 1.0,
            goal: Some(Vec2::new(80.0, 50.0)),
            obstacles: vec![rock(Vec2::new(50.0, 50.0), 5.0)],
            ..SimParams::default()
        };
        let goal = goal_seek(&agents, &params.world, 0, params.goal);
        let avoid = obstacle_avoidance(&agents, &params.world, 0, &params.obstacles);
        assert!(goal.length() > 0.0, "a lone agent still seeks its goal");
        assert!(avoid.length() > 0.0, "a lone agent still avoids obstacles");
        assert_eq!(
            blend(&params, &agents, &params.world, 0, &[]),
            goal.add(avoid),
            "with no neighbours the blend is exactly the two task forces"
        );
    }

    #[test]
    fn a_lone_agent_with_nothing_to_react_to_gets_no_force() {
        // No flockmates, no goal, no obstacles: the agent must coast, not
        // wander.
        let agents = [Agent {
            id: 0,
            pos: Vec2::new(10.0, 10.0),
            vel: Vec2::new(1.0, 2.0),
        }];
        let params = SimParams {
            world: World::new(100.0, 100.0),
            goal: None,
            obstacles: Vec::new(),
            ..SimParams::default()
        };
        assert_eq!(
            blend(&params, &agents, &params.world, 0, &[]),
            Vec2::ZERO,
            "an unstimulated agent must not be pushed anywhere"
        );
    }

    #[test]
    fn a_neighbour_list_naming_the_agent_itself_is_ignored() {
        // A self-reference would otherwise register as a coincident pair and
        // fire the separation tie-break against the agent itself.
        let agents = [Agent {
            id: 0,
            pos: Vec2::new(10.0, 10.0),
            vel: Vec2::new(1.0, 2.0),
        }];
        assert_eq!(separation(&agents, &w100(), 0, &[0, 0], 5.0), Vec2::ZERO);
        assert_eq!(alignment(&agents, 0, &[0]), Vec2::ZERO);
        assert_eq!(cohesion(&agents, &w100(), 0, &[0]), Vec2::ZERO);
    }

    // ------------------------------------------- general robustness property

    /// Draw a flock in which coincident agents, agents inside obstacles, and
    /// agents exactly on a goal are all *likely* rather than merely possible:
    /// positions are snapped to a coarse lattice, so collisions happen many
    /// times per run instead of never.
    fn degenerate_flock(r: &mut Rng, world: &World) -> Vec<Agent> {
        let lattice = 5.0;
        (0..8)
            .map(|id| {
                let snap = |v: f64| (v / lattice).floor() * lattice;
                Agent {
                    id,
                    pos: Vec2::new(
                        snap(r.range(0.0, world.width)),
                        snap(r.range(0.0, world.height)),
                    ),
                    vel: if r.next_f64() < 0.25 {
                        Vec2::ZERO
                    } else {
                        Vec2::new(r.range(-4.0, 4.0), r.range(-4.0, 4.0))
                    },
                }
            })
            .collect()
    }

    #[test]
    fn every_force_is_finite_for_every_random_configuration() {
        let mut r = Rng::seeded(0xF005_CE55);
        for _ in 0..20_000 {
            let world = World::new(r.range(5.0, 300.0), r.range(5.0, 300.0));
            let agents = degenerate_flock(&mut r, &world);
            let obstacles = vec![
                rock(
                    Vec2::new(r.range(0.0, world.width), r.range(0.0, world.height)),
                    r.range(0.0, 20.0),
                ),
                rock(agents[0].pos, r.range(0.0, 20.0)),
            ];
            let goal = if r.next_f64() < 0.5 {
                None
            } else {
                Some(agents[r.range(0.0, 8.0) as usize % 8].pos)
            };
            let params = SimParams {
                world,
                separation_radius: r.range(0.0, 30.0),
                max_force: r.range(0.0, 5.0),
                w_separation: r.range(-5.0, 5.0),
                w_alignment: r.range(-5.0, 5.0),
                w_cohesion: r.range(-5.0, 5.0),
                w_goal: r.range(-5.0, 5.0),
                w_avoidance: r.range(-5.0, 5.0),
                goal,
                obstacles,
                ..SimParams::default()
            };
            let neighbors: Vec<usize> = (0..agents.len()).collect();
            for index in 0..agents.len() {
                let forces = [
                    (
                        "separation",
                        separation(&agents, &world, index, &neighbors, params.separation_radius),
                    ),
                    ("alignment", alignment(&agents, index, &neighbors)),
                    ("cohesion", cohesion(&agents, &world, index, &neighbors)),
                    ("goal_seek", goal_seek(&agents, &world, index, params.goal)),
                    (
                        "obstacle_avoidance",
                        obstacle_avoidance(&agents, &world, index, &params.obstacles),
                    ),
                    ("blend", blend(&params, &agents, &world, index, &neighbors)),
                ];
                for (name, f) in forces {
                    assert!(
                        f.is_finite(),
                        "{name} went non-finite: {f:?} for agent {index} of {agents:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_force_is_reproducible_for_every_random_configuration() {
        // Determinism as a property, not just at the three named degenerate
        // points: the same frame must give bit-identical forces every time.
        let mut r = Rng::seeded(0xD37E_4E17);
        for _ in 0..2_000 {
            let world = World::new(r.range(5.0, 300.0), r.range(5.0, 300.0));
            let agents = degenerate_flock(&mut r, &world);
            let params = SimParams {
                world,
                separation_radius: r.range(0.0, 30.0),
                obstacles: vec![rock(agents[0].pos, r.range(0.0, 20.0))],
                goal: Some(agents[1].pos),
                ..SimParams::default()
            };
            let neighbors: Vec<usize> = (0..agents.len()).collect();
            for index in 0..agents.len() {
                assert_eq!(
                    blend(&params, &agents, &world, index, &neighbors),
                    blend(&params, &agents, &world, index, &neighbors),
                    "blend is not reproducible for agent {index}"
                );
            }
        }
    }

    #[test]
    fn a_pile_of_coincident_agents_is_finite_and_deterministic() {
        // The worst case the tie-break has to survive: five agents stacked on
        // one point, each a neighbour of all the others.
        let agents: Vec<Agent> = (0..5)
            .map(|id| Agent {
                id,
                pos: Vec2::new(50.0, 50.0),
                vel: Vec2::ZERO,
            })
            .collect();
        let neighbors: Vec<usize> = (0..5).collect();
        for index in 0..5 {
            let f = separation(&agents, &w100(), index, &neighbors, 5.0);
            assert!(f.is_finite(), "agent {index} got a non-finite force: {f:?}");
            assert_eq!(
                f,
                separation(&agents, &w100(), index, &neighbors, 5.0),
                "agent {index} is not reproducible"
            );
        }
    }

    #[test]
    fn neighbour_order_changes_nothing_but_rounding() {
        // The fold runs in the order given, so two orderings of the same set
        // may differ in their last bits — but each ordering must be exactly
        // stable against itself, which is the half the reproducibility
        // contract actually depends on.
        let mut r = Rng::seeded(0x0A11_0CED);
        let agents: Vec<Agent> = (0..6)
            .map(|id| Agent {
                id,
                pos: Vec2::new(r.range(0.0, 100.0), r.range(0.0, 100.0)),
                vel: Vec2::new(r.range(-2.0, 2.0), r.range(-2.0, 2.0)),
            })
            .collect();
        let forward: Vec<usize> = (1..6).collect();
        let backward: Vec<usize> = (1..6).rev().collect();
        // Same set, same maths, and the same answer only if the sums happen
        // to round identically — so assert on what is guaranteed: each
        // ordering is stable against itself.
        assert_eq!(
            cohesion(&agents, &w100(), 0, &forward),
            cohesion(&agents, &w100(), 0, &forward)
        );
        assert_eq!(
            cohesion(&agents, &w100(), 0, &backward),
            cohesion(&agents, &w100(), 0, &backward)
        );
        let difference = cohesion(&agents, &w100(), 0, &forward)
            .sub(cohesion(&agents, &w100(), 0, &backward))
            .length();
        assert!(
            difference < 1e-12,
            "orderings disagree by more than rounding"
        );
    }

    #[test]
    fn separation_crosses_the_seam() {
        // Toroidal: the neighbour at x=99 is two units to the *left* of the
        // agent at x=1, so the escape is +x, not -x.
        let agents = [at(0, Vec2::new(1.0, 0.0)), at(1, Vec2::new(99.0, 0.0))];
        let f = separation(&agents, &w100(), 0, &[1], 5.0);
        assert!(f.x > 0.0, "seam-crossing escape must be +x, got {f:?}");
    }
}
