//! Simulation integration — the layer that ties the kernel together.
//!
//! [`neighbors`](crate::neighbors) decides who can see whom, [`forces`](crate::forces)
//! decides where each agent wants to go, and [`metrics`](crate::metrics)
//! describes what happened. This module is what turns those into a *run*: it
//! owns the state, the timestep, and the batch boundary, and it is where the
//! guarantees the rest of the product rests on are actually established.
//!
//! Four invariants hold here, and each has an acceptance criterion behind it:
//!
//! * **Double-buffered.** [`step`] computes every agent's next state from the
//!   old state of the whole flock, so permuting the agent array and
//!   un-permuting the result gives a **bit-identical** answer. Updating in
//!   place is the canonical boids bug (**AC-18**). This requires more than
//!   avoiding a mutable loop: the neighbour fold is ordered by agent
//!   *identity*, because `f64` addition is not associative and slice order is
//!   not identity.
//! * **Deterministic and reproducible.** The same `(params, seed)` yields the
//!   same state hash in every process, on every platform (**AC-22**). No
//!   ambient entropy, no trigonometry, no unordered-collection iteration
//!   anywhere in the pipeline.
//! * **Checkpoint-safe.** Running `n` ticks in one call and in any sequence of
//!   batches totalling `n` — serialising the state to JSON and back between
//!   each — produces the same final hash and the same metric series
//!   (**AC-20**). That equivalence is the entire justification for a durable
//!   workflow checkpointing at a batch boundary, and it is why [`SimState`]
//!   does not simply derive its JSON representation.
//! * **Stable.** Over long adversarial runs every position and velocity stays
//!   finite, every speed stays within `max_speed`, and every applied force
//!   within `max_force` (**AC-21**).
//!
//! `dt` is a **resolution** knob, not a strength knob: forces are scaled by
//! it, so refining the timestep converges on one trajectory rather than
//! changing the answer (**AC-19**).

use crate::config::{NeighborBackend, SimParams};
use crate::forces::blend;
use crate::hash;
use crate::metrics::{FrameMetrics, frame_metrics};
use crate::neighbors::{SpatialHash, neighbors_with};
use crate::rng::Rng;
use crate::vec2::Vec2;
use crate::world::Agent;
use serde::{Deserialize, Serialize};

/// The complete, serializable state of a simulation at one instant.
///
/// **Checkpoint-safe.** `SimState -> JSON -> SimState` is lossless: it is what
/// a durable workflow stores between batches, so a resumed run must reproduce
/// an identical state hash rather than a similar one.
///
/// # Why the floats are serialized as strings
///
/// Losslessness here is a property that had to be *built*, not assumed. A
/// bare `f64` field serialized as a JSON number does **not** survive
/// `serde_json` unchanged: with default features `serde_json` parses floats
/// with a fast best-effort algorithm that can land one ULP away from the
/// value that was written (its exact-rounding path is behind the non-default
/// `float_roundtrip` feature). One ULP is enough to change the canonical
/// state hash, which would make every resumed run look like a divergence.
///
/// So this type serializes each coordinate as its shortest round-tripping
/// **decimal string** and reads it back with `f64::from_str`, which is
/// correctly rounded. That is exact regardless of the JSON library's float
/// precision, keeps the checkpoint readable, and — unlike a JSON number —
/// can even represent a non-finite value rather than silently becoming
/// `null`. The alternative, raw bit patterns, would be equally exact and
/// entirely unreadable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(into = "WireState", from = "WireState")]
pub struct SimState {
    /// How many ticks have been simulated to reach this state.
    pub tick: u32,
    /// The flock. Slice order is arbitrary; identity lives in [`Agent::id`].
    pub agents: Vec<Agent>,
}

impl SimState {
    /// Deterministically seed a starting state from `params` and `seed`.
    ///
    /// The same `(params, seed)` always yields the same state, in every
    /// process and on every platform: positions and velocities come from the
    /// kernel's own [`Rng`] and pure `f64` arithmetic, with no trigonometry
    /// and no ambient entropy anywhere in the pipeline.
    ///
    /// Agents are given ids `0..agent_count`, positions uniform over the
    /// world, and velocities drawn from the square `[-max_speed, max_speed]²`
    /// then clamped to `max_speed`. Every position is wrapped, so an agent
    /// always starts inside the world bounds.
    #[must_use]
    pub fn seeded(params: &SimParams, seed: u64) -> SimState {
        let mut rng = Rng::seeded(seed);
        let world = &params.world;
        let agents = (0..params.agent_count)
            .map(|i| {
                // `Rng::range` is half-open in exact arithmetic, but the
                // scaling can still round up to the bound, so wrap rather
                // than trust it: `[0,size)` is a hard contract here.
                let pos = world.wrap(Vec2::new(
                    rng.range(0.0, world.width),
                    rng.range(0.0, world.height),
                ));
                let vel = Vec2::new(
                    rng.range(-params.max_speed, params.max_speed),
                    rng.range(-params.max_speed, params.max_speed),
                )
                .limit(params.max_speed);
                Agent {
                    id: i as u32,
                    pos,
                    vel,
                }
            })
            .collect();
        SimState { tick: 0, agents }
    }

    /// Canonical, cross-process hash of the flock configuration.
    ///
    /// Delegates to [`hash::state_hash`], so it is permutation-invariant and
    /// sensitive to every agent field. The **tick is deliberately excluded**:
    /// the hash names a configuration of the flock, which is what makes two
    /// runs that reach the same state comparable.
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        hash::state_hash(&self.agents)
    }

    /// Hex rendering of [`SimState::state_hash`]: 16 lowercase digits.
    #[must_use]
    pub fn state_hash_hex(&self) -> String {
        hash::state_hash_hex(&self.agents)
    }
}

/// Advance the simulation exactly one tick.
///
/// # Double-buffered
///
/// Every agent's new velocity and position are computed from the **old** state
/// of the whole flock. `step` reads `state` and returns a new one; it never
/// writes into the array it is reading. Updating in place — so that agent 0
/// steers by old neighbours while agent 89 steers by a mix of old and freshly
/// moved ones — is the canonical boids bug, and makes the result depend on
/// which slot an agent happens to occupy. **AC-18** exists to catch exactly
/// that, and it does: this function was first written in place, and the
/// order-independence test failed against it.
///
/// # Order-independent
///
/// Permuting `state.agents`, stepping, and un-permuting yields a
/// **bit-identical** flock. Two things are needed for that, not one:
///
/// * the double buffering above, and
/// * a neighbour fold ordered by **agent identity**. `f64` addition is not
///   associative, so summing the same neighbours in a different order changes
///   the last bits of the steering force. Neighbour queries return *slice
///   indices* in ascending order, and a permuted array turns that into a
///   different sequence of agents — same set, different sum. Sorting each
///   neighbour list by `id` before it reaches [`blend`] pins the summation
///   order to identity, which no permutation can disturb.
///
/// Identity ordering assumes ids are unique within a flock, which is
/// [`Agent::id`]'s documented contract; the fallback tie-break on slice index
/// keeps the sort total if they are not.
///
/// # Integration
///
/// Semi-implicit Euler, with **forces scaled by `dt`** so that refining the
/// timestep converges on one trajectory rather than changing the answer
/// (**AC-19**):
///
/// ```text
/// vel' = limit(vel + blend * dt, max_speed)
/// pos' = wrap(pos + vel' * dt)
/// ```
///
/// The new velocity — not the old one — carries the position, which is what
/// makes the scheme semi-implicit and keeps a flock from over-shooting into
/// obstacles it has already started to steer away from.
///
/// # Cost
///
/// One [`SpatialHash`] is built per tick and shared by every neighbour query,
/// never one per agent. With [`NeighborBackend::Naive`] no grid is built at
/// all and each query is a full scan.
#[must_use]
pub fn step(state: &SimState, params: &SimParams) -> SimState {
    let agents = &state.agents;
    let world = &params.world;

    // Built once for the whole tick, from the OLD positions — which is both
    // the fast thing and the correct thing, since a grid rebuilt mid-tick
    // would describe a flock that is half-moved.
    let grid = match params.backend {
        NeighborBackend::SpatialHash => {
            Some(SpatialHash::build(agents, world, params.neighbor_radius))
        }
        NeighborBackend::Naive => None,
    };

    let mut scratch = Vec::new();
    let next = agents
        .iter()
        .enumerate()
        .map(|(i, me)| {
            scratch.clear();
            scratch.extend(neighbors_with(
                params.backend,
                agents,
                world,
                grid.as_ref(),
                i,
                params.neighbor_radius,
            ));
            // Identity order, not slice order. See "Order-independent" above.
            scratch.sort_unstable_by_key(|&j| (agents[j].id, j));

            let acc = blend(params, agents, world, i, &scratch);
            let vel = me.vel.add(acc.scale(params.dt)).limit(params.max_speed);
            Agent {
                id: me.id,
                pos: world.wrap(me.pos.add(vel.scale(params.dt))),
                vel,
            }
        })
        .collect();

    SimState {
        // Saturating so a run that somehow reaches `u32::MAX` ticks stops
        // counting rather than wrapping round to zero and looking fresh.
        tick: state.tick.saturating_add(1),
        agents: next,
    }
}

/// Advance `ticks` ticks, returning the final state and the metric series
/// gathered along the way.
///
/// # The checkpointing contract
///
/// **Splitting a run into batches changes nothing.** Running `n` ticks in one
/// call, or in any sequence of calls totalling `n` — with the state written to
/// JSON and read back between them — yields the same final state hash and the
/// same concatenated metric series. That equivalence is what makes a durable
/// workflow's checkpoint boundary legitimate rather than merely convenient,
/// and **AC-20** asserts it directly.
///
/// Two properties make it hold, and both are easy to get wrong:
///
/// * Metrics are sampled on the **absolute** tick number, never on an offset
///   within the batch. Sampling `i % metrics_every` over a batch-local counter
///   would put the samples in different places depending on where the batches
///   happened to be cut.
/// * A batch reports the states it **produces**, never the state it was
///   handed. So consecutive batches concatenate into one series with no
///   duplicated tick at the seam, and the input state — which the previous
///   batch already reported — is not counted twice.
///
/// # Subsampling
///
/// `metrics_every` is a stride over ticks: `1` records every tick, `10` every
/// tenth, and **`0` records none at all**, which is what a caller that only
/// wants the final state should pass rather than paying for metrics it will
/// discard. Metrics are computed from a frame alone, so a skipped tick can
/// always be recomputed later from stored state.
#[must_use]
pub fn run_batch(
    state: &SimState,
    params: &SimParams,
    ticks: u32,
    metrics_every: u32,
) -> (SimState, Vec<(u32, FrameMetrics)>) {
    let mut current = state.clone();
    let mut series = Vec::new();
    for _ in 0..ticks {
        current = step(&current, params);
        // Absolute tick, not batch offset: see "The checkpointing contract".
        if metrics_every > 0 && current.tick.is_multiple_of(metrics_every) {
            series.push((current.tick, frame_metrics(&current.agents, params)));
        }
    }
    (current, series)
}

/// Reject nonsense parameters before a run starts.
///
/// # Every problem, not the first one
///
/// The error is a `Vec<String>` containing **all** the faults found. A
/// validator that stops at the first one turns fixing a scenario into a
/// guessing game played one submission at a time, which is a worse experience
/// than no validation at all. Each message names the field, quotes the
/// offending value, and states the rule, so it can be shown to a user
/// unmodified.
///
/// # What is rejected, and what deliberately is not
///
/// Rejected: a world without positive finite area; a non-positive or
/// non-finite `dt`; any negative or non-finite radius, speed or force cap; a
/// non-finite weight; an `agent_count` of zero; a `separation_radius` wider
/// than `neighbor_radius`; an obstacle with a non-positive or non-finite
/// radius or a non-finite centre; and a non-finite goal.
///
/// **Not** rejected, because each is a legitimate experiment rather than a
/// mistake:
///
/// * A **negative weight** inverts a behaviour — agents that flee the flock
///   instead of joining it — which is a scenario worth running.
/// * A **zero radius, speed or force** switches a behaviour off.
/// * `separation_radius == neighbor_radius`: the whole neighbourhood repels.
/// * **No goal at all** (`None`) is a scenario without one, not an omission.
///
/// # Relationship to the kernel's totality rule
///
/// Nothing here is required for safety. Every kernel function is total, so an
/// invalid parameter set produces a finite, deterministic, meaningless run
/// rather than a panic or a `NaN`. This exists to tell a *user* that their
/// scenario does not say what they think it says — which is why the messages
/// matter as much as the checks.
pub fn validate(params: &SimParams) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();

    for (name, size) in [
        ("world.width", params.world.width),
        ("world.height", params.world.height),
    ] {
        if !size.is_finite() || size <= 0.0 {
            problems.push(format!(
                "{name} is {size}, but must be a positive, finite number — \
                 a world with no area has no geometry to simulate"
            ));
        }
    }

    if !params.dt.is_finite() || params.dt <= 0.0 {
        problems.push(format!(
            "dt is {}, but must be a positive, finite number — time has to \
             move forward for a tick to mean anything",
            params.dt
        ));
    }

    if params.agent_count == 0 {
        problems.push(
            "agent_count is 0, but must be at least 1 — an empty flock has \
             nothing to simulate"
                .to_string(),
        );
    }

    for (name, value) in [
        ("neighbor_radius", params.neighbor_radius),
        ("separation_radius", params.separation_radius),
        ("collision_radius", params.collision_radius),
        ("goal_arrival_radius", params.goal_arrival_radius),
        ("max_speed", params.max_speed),
        ("max_force", params.max_force),
    ] {
        if !value.is_finite() || value < 0.0 {
            problems.push(format!(
                "{name} is {value}, but must be a finite number of zero or \
                 more — zero switches the behaviour off, negative means nothing"
            ));
        }
    }

    for (name, weight) in [
        ("w_separation", params.w_separation),
        ("w_alignment", params.w_alignment),
        ("w_cohesion", params.w_cohesion),
        ("w_goal", params.w_goal),
        ("w_avoidance", params.w_avoidance),
    ] {
        if !weight.is_finite() {
            problems.push(format!(
                "{name} is {weight}, but must be finite — a negative weight is \
                 a legitimate inverted behaviour, an infinite one is not a \
                 weight at all"
            ));
        }
    }

    // A contradiction between two individually reasonable values, so it is
    // checked only when both are usable on their own.
    if params.separation_radius.is_finite()
        && params.neighbor_radius.is_finite()
        && params.separation_radius > params.neighbor_radius
    {
        problems.push(format!(
            "separation_radius ({}) is larger than neighbor_radius ({}), but \
             must not exceed it — an agent cannot be repelled by a neighbour \
             it cannot see",
            params.separation_radius, params.neighbor_radius
        ));
    }

    if let Some(goal) = params.goal
        && !goal.is_finite()
    {
        problems.push(format!(
            "goal is {goal:?}, but must be a finite point — use `None` for a \
             scenario with no goal"
        ));
    }

    for (i, obstacle) in params.obstacles.iter().enumerate() {
        // Indexed, because "an obstacle is invalid" is not actionable in a
        // scenario that has forty of them.
        if !obstacle.radius.is_finite() || obstacle.radius <= 0.0 {
            problems.push(format!(
                "obstacle {i} has radius {}, but must have a positive, finite \
                 radius — remove it rather than giving it no size",
                obstacle.radius
            ));
        }
        if !obstacle.center.is_finite() {
            problems.push(format!(
                "obstacle {i} is centred at {:?}, but must have a finite centre",
                obstacle.center
            ));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// The on-the-wire form of [`SimState`]: identical shape, but every `f64`
/// carried as an exact decimal string. See [`SimState`] for why.
#[derive(Serialize, Deserialize)]
struct WireState {
    tick: u32,
    agents: Vec<WireAgent>,
}

/// The on-the-wire form of an [`Agent`]. Flat rather than nested so a
/// checkpoint reads as one line per agent.
#[derive(Serialize, Deserialize)]
struct WireAgent {
    id: u32,
    #[serde(with = "exact_f64")]
    px: f64,
    #[serde(with = "exact_f64")]
    py: f64,
    #[serde(with = "exact_f64")]
    vx: f64,
    #[serde(with = "exact_f64")]
    vy: f64,
}

impl From<SimState> for WireState {
    fn from(s: SimState) -> WireState {
        WireState {
            tick: s.tick,
            agents: s
                .agents
                .iter()
                .map(|a| WireAgent {
                    id: a.id,
                    px: a.pos.x,
                    py: a.pos.y,
                    vx: a.vel.x,
                    vy: a.vel.y,
                })
                .collect(),
        }
    }
}

impl From<WireState> for SimState {
    fn from(w: WireState) -> SimState {
        SimState {
            tick: w.tick,
            agents: w
                .agents
                .iter()
                .map(|a| Agent {
                    id: a.id,
                    pos: Vec2::new(a.px, a.py),
                    vel: Vec2::new(a.vx, a.vy),
                })
                .collect(),
        }
    }
}

/// Serialize an `f64` as an exact decimal string and read it back exactly.
///
/// `{:?}` on an `f64` emits the shortest decimal that uniquely identifies the
/// value, and `f64::from_str` is correctly rounded, so the pair is an exact
/// identity — a guarantee the JSON number path does not offer.
mod exact_f64 {
    use serde::Serializer;
    use serde::de::{Deserialize, Deserializer, Error};

    /// Write the float as its shortest round-tripping decimal string.
    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        // `collect_str` on a `format_args!` writes straight into the
        // serializer, so no intermediate `String` is allocated per field.
        s.collect_str(&format_args!("{v:?}"))
    }

    /// Read a float back from its decimal string, exactly.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(|_| {
            D::Error::custom(format!("`{text}` is not a decimal floating-point number"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Obstacle, SimParams};
    use crate::world::World;

    #[test]
    fn seeded_is_reproducible_for_the_same_seed() {
        let params = SimParams::default();
        let a = SimState::seeded(&params, 0xB01D_5EED);
        let b = SimState::seeded(&params, 0xB01D_5EED);
        assert_eq!(a, b, "the same (params, seed) must give the same state");
        assert_eq!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn seeded_differs_for_different_seeds() {
        let params = SimParams::default();
        let a = SimState::seeded(&params, 1);
        let b = SimState::seeded(&params, 2);
        assert_ne!(a.state_hash(), b.state_hash(), "seeds must not alias");
    }

    #[test]
    fn seeded_starts_every_agent_inside_the_world() {
        let params = SimParams {
            world: World::new(37.5, 211.0),
            agent_count: 500,
            ..SimParams::default()
        };
        let state = SimState::seeded(&params, 0xC0FFEE);
        assert_eq!(state.tick, 0, "a seeded state starts at tick zero");
        assert_eq!(state.agents.len(), params.agent_count);
        for a in &state.agents {
            assert!(a.pos.is_finite() && a.vel.is_finite(), "non-finite {a:?}");
            assert!(
                a.pos.x >= 0.0 && a.pos.x < params.world.width,
                "x out of bounds: {a:?}"
            );
            assert!(
                a.pos.y >= 0.0 && a.pos.y < params.world.height,
                "y out of bounds: {a:?}"
            );
            assert!(
                a.vel.length() <= params.max_speed + 1e-9,
                "spawned over max_speed: {a:?}"
            );
        }
    }

    #[test]
    fn seeded_gives_every_agent_a_distinct_id() {
        // Identity is what makes the canonical hash order-independent, so a
        // duplicated id would quietly break every downstream invariant.
        let params = SimParams {
            agent_count: 64,
            ..SimParams::default()
        };
        let state = SimState::seeded(&params, 5);
        let mut ids: Vec<u32> = state.agents.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 64, "ids must be unique");
    }

    #[test]
    fn seeded_state_survives_a_json_round_trip() {
        // The checkpoint contract in miniature: a state written to JSON and
        // read back must be the *same* state, not merely a similar one.
        let params = SimParams::default();
        let state = SimState::seeded(&params, 99);
        let json = serde_json::to_string(&state).expect("serialize");
        let back: SimState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, back);
        assert_eq!(state.state_hash(), back.state_hash());
        assert_eq!(state.state_hash_hex(), back.state_hash_hex());
    }

    #[test]
    fn json_round_trip_is_exact_for_awkward_float_values() {
        // The round trip has to be exact for *every* bit pattern a run can
        // reach, not just for the tidy ones. `serde_json`'s default float
        // parser is off by an ULP on values like these, which is exactly the
        // defect this type's wire format exists to prevent.
        let mut r = Rng::seeded(0xF10A7);
        let mut agents = vec![
            Agent {
                id: 0,
                pos: Vec2::new(0.909_968_551_659_149_1, 1e-300),
                vel: Vec2::new(-0.0, f64::MIN_POSITIVE),
            },
            Agent {
                id: 1,
                pos: Vec2::new(f64::MAX, -f64::MAX),
                vel: Vec2::new(0.1 + 0.2, 1.0 / 3.0),
            },
        ];
        for id in 2..600u32 {
            // Random *bit patterns*, not random magnitudes: this reaches the
            // subnormals and the long-mantissa values a decimal literal never
            // would.
            let bits = |r: &mut Rng| f64::from_bits(r.next_u64() & 0x7fef_ffff_ffff_ffff);
            agents.push(Agent {
                id,
                pos: Vec2::new(bits(&mut r), bits(&mut r)),
                vel: Vec2::new(r.range(-2.0, 2.0), bits(&mut r)),
            });
        }
        let state = SimState { tick: 7, agents };
        let json = serde_json::to_string(&state).expect("serialize");
        let back: SimState = serde_json::from_str(&json).expect("deserialize");
        for (a, b) in state.agents.iter().zip(&back.agents) {
            // Bit patterns, not `==`: this is the level the state hash reads.
            assert_eq!(a.pos.x.to_bits(), b.pos.x.to_bits(), "{a:?} vs {b:?}");
            assert_eq!(a.pos.y.to_bits(), b.pos.y.to_bits(), "{a:?} vs {b:?}");
            assert_eq!(a.vel.x.to_bits(), b.vel.x.to_bits(), "{a:?} vs {b:?}");
            assert_eq!(a.vel.y.to_bits(), b.vel.y.to_bits(), "{a:?} vs {b:?}");
        }
        assert_eq!(state, back);
        assert_eq!(state.tick, back.tick, "the tick is part of the checkpoint");
    }

    // ---------------------------------------------------------------- AC-18

    /// Fisher-Yates over the agent array using the kernel's own RNG, so the
    /// permutation is itself reproducible and a failure can be re-run.
    fn shuffled(state: &SimState, seed: u64) -> SimState {
        let mut out = state.clone();
        let mut r = Rng::seeded(seed);
        for i in (1..out.agents.len()).rev() {
            let j = (r.next_u64() % (i as u64 + 1)) as usize;
            out.agents.swap(i, j);
        }
        out
    }

    /// The agents in ascending id order: the un-permutation.
    fn by_id(state: &SimState) -> Vec<Agent> {
        let mut agents = state.agents.clone();
        agents.sort_by_key(|a| a.id);
        agents
    }

    /// Assert two flocks are equal **in their float bit patterns**, which is
    /// stricter than `==` (that would accept `-0.0` for `0.0`) and stricter
    /// than any epsilon.
    fn assert_bit_identical(left: &[Agent], right: &[Agent], context: &str) {
        assert_eq!(left.len(), right.len(), "{context}: different flock sizes");
        for (a, b) in left.iter().zip(right) {
            assert_eq!(a.id, b.id, "{context}: un-permutation misaligned");
            let fields = [
                ("pos.x", a.pos.x, b.pos.x),
                ("pos.y", a.pos.y, b.pos.y),
                ("vel.x", a.vel.x, b.vel.x),
                ("vel.y", a.vel.y, b.vel.y),
            ];
            for (name, l, r) in fields {
                assert_eq!(
                    l.to_bits(),
                    r.to_bits(),
                    "{context}: agent {} {name} differs: {l:?} vs {r:?} \
                     (a difference of {} ULP)",
                    a.id,
                    (l.to_bits() as i64 - r.to_bits() as i64).abs(),
                );
            }
        }
    }

    /// A densely interacting scenario: every agent has many neighbours, so an
    /// update that leaked new positions into the same tick could not possibly
    /// go unnoticed.
    fn interacting() -> SimParams {
        SimParams {
            world: World::new(120.0, 120.0),
            agent_count: 90,
            neighbor_radius: 30.0,
            separation_radius: 9.0,
            goal: Some(Vec2::new(30.0, 90.0)),
            w_goal: 0.4,
            obstacles: vec![
                Obstacle {
                    center: Vec2::new(60.0, 60.0),
                    radius: 12.0,
                },
                Obstacle {
                    center: Vec2::new(10.0, 110.0),
                    radius: 6.0,
                },
            ],
            ..SimParams::default()
        }
    }

    #[test]
    fn stepping_is_independent_of_agent_array_order() {
        // AC-18, and the reason `step` returns a new state instead of mutating
        // one. If agent 0 were updated in place, agent 1 would steer by agent
        // 0's NEW position while agent 89 steered by everyone's old one — so
        // the answer would depend on where in the array an agent happened to
        // sit. Permuting the array and un-permuting the result is the exact
        // experiment that detects it.
        let params = interacting();
        let state = SimState::seeded(&params, 0x000D_0B1E);
        let reference = step(&state, &params);

        let ids = |s: &SimState| -> Vec<u32> { s.agents.iter().map(|a| a.id).collect() };
        for seed in 0..16u64 {
            let mixed = shuffled(&state, seed);
            assert_ne!(
                ids(&mixed),
                ids(&state),
                "seed {seed} produced the identity permutation; \
                 the test would prove nothing"
            );
            let stepped = step(&mixed, &params);

            // Exact equality, no tolerance: the hash is the persisted
            // reproducibility token.
            assert_eq!(
                stepped.state_hash(),
                reference.state_hash(),
                "permutation {seed} changed the state hash"
            );
            assert_eq!(stepped.tick, reference.tick);
            assert_bit_identical(
                &by_id(&stepped),
                &by_id(&reference),
                &format!("permutation seed {seed}"),
            );
        }
    }

    #[test]
    fn order_independence_survives_a_long_run() {
        // One tick can hide a leak that only compounds. Run both orderings
        // for many ticks and require them to stay bit-identical throughout.
        let params = interacting();
        let mut plain = SimState::seeded(&params, 0x5A1AD);
        let mut mixed = shuffled(&plain, 0xBEEF);
        for tick in 1..=120u32 {
            plain = step(&plain, &params);
            mixed = step(&mixed, &params);
            assert_eq!(plain.tick, tick);
            assert_eq!(mixed.tick, tick);
            assert_eq!(
                plain.state_hash(),
                mixed.state_hash(),
                "orderings diverged at tick {tick}"
            );
        }
        assert_bit_identical(&by_id(&plain), &by_id(&mixed), "after 120 ticks");
    }

    #[test]
    fn step_leaves_the_input_state_untouched() {
        // The structural half of double buffering: `step` reads a state and
        // returns a new one, so a caller holding a checkpoint still holds it
        // afterwards.
        let params = interacting();
        let before = SimState::seeded(&params, 4242);
        let snapshot = before.clone();
        let after = step(&before, &params);
        assert_eq!(before, snapshot, "step mutated its input");
        assert_ne!(after.state_hash(), before.state_hash(), "step did nothing");
    }

    #[test]
    fn step_advances_the_tick_by_one() {
        let params = SimParams::default();
        let state = SimState::seeded(&params, 1);
        assert_eq!(step(&state, &params).tick, 1);
        assert_eq!(step(&step(&state, &params), &params).tick, 2);
    }

    #[test]
    fn step_preserves_identity_and_slice_order() {
        // The output is the same agents in the same slots — only their
        // position and velocity move. Anything else would make a stored frame
        // impossible to line up against its predecessor.
        let params = interacting();
        let state = SimState::seeded(&params, 77);
        let next = step(&state, &params);
        let before: Vec<u32> = state.agents.iter().map(|a| a.id).collect();
        let after: Vec<u32> = next.agents.iter().map(|a| a.id).collect();
        assert_eq!(before, after);
    }

    // ---------------------------------------------------------------- AC-19

    /// A deliberately *smooth* scenario, so the test measures integration
    /// error and nothing else.
    ///
    /// Every discontinuity the kernel contains is switched off: the neighbour
    /// radius exceeds the world (so the neighbour set never changes), the
    /// speed and force caps are far out of reach (so `limit` never engages),
    /// separation is disabled (its `1/d` law has a step at its radius), and
    /// there are no obstacles (whose influence band has one too). What is
    /// left — alignment, cohesion, goal seeking — is a smooth vector field,
    /// which is the only setting where a convergence *order* means anything.
    fn smooth_params(dt: f64) -> SimParams {
        SimParams {
            world: World::new(200.0, 200.0),
            agent_count: 8,
            neighbor_radius: 500.0,
            separation_radius: 0.0,
            max_speed: 1e9,
            max_force: 1e9,
            dt,
            w_separation: 0.0,
            w_alignment: 0.5,
            w_cohesion: 0.05,
            w_goal: 0.02,
            w_avoidance: 0.0,
            goal: Some(Vec2::new(140.0, 120.0)),
            obstacles: Vec::new(),
            backend: NeighborBackend::Naive,
            ..SimParams::default()
        }
    }

    /// A small cluster near the middle of the world, written out by hand so
    /// the initial condition is independent of `max_speed`.
    fn smooth_state() -> SimState {
        let agents = (0..8u32)
            .map(|i| {
                let f = f64::from(i);
                Agent {
                    id: i,
                    pos: Vec2::new(95.0 + 2.5 * f, 100.0 + 1.5 * f64::from(i % 3)),
                    vel: Vec2::new(0.3 - 0.1 * f, 0.2 + 0.05 * f),
                }
            })
            .collect();
        SimState { tick: 0, agents }
    }

    /// The largest distance any agent ends up from where the other run put
    /// it, matched by id.
    fn largest_disagreement(a: &SimState, b: &SimState, world: &World) -> f64 {
        let (a, b) = (by_id(a), by_id(b));
        a.iter()
            .zip(&b)
            .map(|(x, y)| {
                assert_eq!(x.id, y.id);
                world.distance(x.pos, y.pos)
            })
            .fold(0.0, f64::max)
    }

    #[test]
    fn halving_dt_converges_rather_than_changing_the_answer() {
        // AC-19. Forces are scaled by `dt`, so `dt` is a *resolution* knob,
        // not a parameter of the model: refining it must home in on one
        // trajectory. The assertion is therefore on the convergence TREND,
        // not on equality — a first-order scheme is never exactly equal at
        // two step sizes, and demanding that would be demanding the wrong
        // thing. What must be true is that successive refinements disagree
        // less and less.
        const SIMULATED_TIME: f64 = 4.0;
        let world = World::new(200.0, 200.0);

        // Exact binary fractions, so every run covers *precisely* the same
        // simulated time and the comparison is not smeared by dt rounding.
        let finals: Vec<SimState> = [0.5, 0.25, 0.125, 0.0625]
            .iter()
            .map(|&dt| {
                let params = smooth_params(dt);
                let ticks = (SIMULATED_TIME / dt) as u32;
                assert_eq!(f64::from(ticks) * dt, SIMULATED_TIME, "dt {dt} misaligned");
                run_batch(&smooth_state(), &params, ticks, 0).0
            })
            .collect();

        let gaps: Vec<f64> = finals
            .windows(2)
            .map(|w| largest_disagreement(&w[0], &w[1], &world))
            .collect();

        assert!(
            gaps[0] > 1e-6,
            "the coarsest pair already agrees ({:e}); the scenario is too \
             inert to demonstrate anything",
            gaps[0]
        );
        for (i, pair) in gaps.windows(2).enumerate() {
            let (coarse, fine) = (pair[0], pair[1]);
            assert!(
                fine < coarse,
                "refinement {i} did not converge: gap grew from {coarse:e} to {fine:e} \
                 (all gaps: {gaps:?})"
            );
            // A first-order scheme roughly halves the gap each time. Requiring
            // a clear majority of that rules out a scheme that merely drifts
            // slower, without asserting a precise order.
            assert!(
                fine < coarse * 0.75,
                "refinement {i} converged too weakly: {coarse:e} -> {fine:e}, \
                 ratio {:.3} (all gaps: {gaps:?})",
                fine / coarse
            );
        }
    }

    #[test]
    fn dt_scales_the_force_so_a_finer_step_is_not_a_slower_simulation() {
        // The failure mode `dt` scaling exists to prevent: if the force were
        // added raw, halving `dt` would halve how far the flock travelled in
        // the same simulated time, and `dt` would silently be a strength
        // knob. Both runs must reach roughly the same place.
        let world = World::new(200.0, 200.0);
        let coarse = run_batch(&smooth_state(), &smooth_params(0.5), 8, 0).0;
        let fine = run_batch(&smooth_state(), &smooth_params(0.0625), 64, 0).0;
        let travelled = |s: &SimState| largest_disagreement(&smooth_state(), s, &world);
        let (slow, quick) = (travelled(&coarse), travelled(&fine));
        assert!(slow > 1.0, "the flock barely moved: {slow}");
        // Stated as a relative difference in *distance covered*, because that
        // is the quantity the bug would wreck: with the force added raw
        // instead of scaled, the 64-tick run accelerates eight times as often
        // as the 8-tick one and ends up multiples further along. The gap here
        // is ordinary first-order truncation error, which is a few percent.
        let relative = (quick - slow).abs() / slow;
        assert!(
            relative < 0.15,
            "the two step sizes describe different journeys, not the same one \
             at different resolutions: coarse travelled {slow:.6}, fine \
             travelled {quick:.6}, a relative difference of {relative:.3}"
        );
    }

    // ---------------------------------------------------------------- AC-20

    /// Run `batches` batches of `per_batch` ticks, forcing the state through
    /// **JSON and back between every batch** — which is precisely what a
    /// durable workflow does when it checkpoints, and the only reason it is
    /// allowed to.
    fn checkpointed(
        state: &SimState,
        params: &SimParams,
        batches: u32,
        per_batch: u32,
        metrics_every: u32,
    ) -> (SimState, Vec<(u32, FrameMetrics)>) {
        let mut current = state.clone();
        let mut series = Vec::new();
        for _ in 0..batches {
            let (next, batch_series) = run_batch(&current, params, per_batch, metrics_every);
            series.extend(batch_series);
            // The checkpoint round trip. If this is lossy by so much as one
            // ULP, the final hashes below diverge.
            let json = serde_json::to_string(&next).expect("checkpoint write");
            current = serde_json::from_str(&json).expect("checkpoint read");
        }
        (current, series)
    }

    #[test]
    fn one_long_batch_equals_ten_checkpointed_batches() {
        // AC-20. THIS IS THE TEST THAT PERMITS CHECKPOINTING AT ALL.
        //
        // The durable workflow does not carry agent state in its history; it
        // carries a cursor, and reloads the flock from storage at the start of
        // every batch. That design is only sound if cutting a run into batches
        // — with a serialize/deserialize at each seam — is *unobservable*. So:
        // 1000 ticks in one go, versus 10 batches of 100 with the state
        // written to JSON and read back between each, must agree on the final
        // state hash EXACTLY and on every metric of every sampled tick.
        //
        // If this test ever fails, resuming a run is no longer equivalent to
        // never having interrupted it, and the product's central claim — that
        // a resumed run is the same run — is false.
        let params = interacting();
        let start = SimState::seeded(&params, 0xC4EC_4901);

        let (whole, whole_series) = run_batch(&start, &params, 1000, 1);
        let (pieces, piece_series) = checkpointed(&start, &params, 10, 100, 1);

        assert_eq!(whole.tick, 1000);
        assert_eq!(pieces.tick, 1000, "the tick must survive checkpointing too");
        assert_eq!(
            whole.state_hash(),
            pieces.state_hash(),
            "1x1000 and 10x100-with-checkpoints reached different states"
        );
        assert_bit_identical(&by_id(&whole), &by_id(&pieces), "after 1000 ticks");

        // The metric series must match tick for tick, not merely in its
        // endpoints: a chart drawn from a resumed run has to be the same
        // chart.
        assert_eq!(whole_series.len(), 1000, "expected one sample per tick");
        assert_eq!(piece_series.len(), whole_series.len());
        for (a, b) in whole_series.iter().zip(&piece_series) {
            assert_eq!(a.0, b.0, "metric series ticks misaligned");
            assert_eq!(a.1, b.1, "metrics differ at tick {}", a.0);
        }
    }

    #[test]
    fn batching_is_equivalent_for_every_way_of_cutting_a_run() {
        // Not just the 10x100 split: any partition of 240 ticks must land in
        // the same place, including uneven ones. A workflow's batch size is a
        // tuning decision, so it must not be a modelling one.
        let params = interacting();
        let start = SimState::seeded(&params, 0x5B17_7EED);
        let (reference, reference_series) = run_batch(&start, &params, 240, 10);

        for &per_batch in &[1u32, 2, 3, 7, 16, 60, 120, 240] {
            let mut current = start.clone();
            let mut series = Vec::new();
            let mut done = 0;
            while done < 240 {
                let take = per_batch.min(240 - done);
                let (next, batch) = run_batch(&current, &params, take, 10);
                series.extend(batch);
                let json = serde_json::to_string(&next).expect("checkpoint write");
                current = serde_json::from_str(&json).expect("checkpoint read");
                done += take;
            }
            assert_eq!(
                current.state_hash(),
                reference.state_hash(),
                "batches of {per_batch} diverged from one run of 240"
            );
            assert_eq!(
                series, reference_series,
                "batches of {per_batch} produced a different metric series"
            );
        }
    }

    #[test]
    fn metrics_are_sampled_on_the_absolute_tick_not_the_batch_offset() {
        // The subtlety that makes the batching contract work. If sampling
        // counted from the start of each batch, a run cut at a different
        // point would sample different ticks — and two runs of the same
        // scenario would produce different charts.
        let params = interacting();
        let start = SimState::seeded(&params, 31337);
        let (_, series) = run_batch(&start, &params, 100, 25);
        let ticks: Vec<u32> = series.iter().map(|s| s.0).collect();
        assert_eq!(ticks, vec![25, 50, 75, 100]);

        // Resume from tick 100 and the stride stays on the same grid.
        let (mid, _) = run_batch(&start, &params, 100, 0);
        let (_, resumed) = run_batch(&mid, &params, 100, 25);
        let resumed_ticks: Vec<u32> = resumed.iter().map(|s| s.0).collect();
        assert_eq!(resumed_ticks, vec![125, 150, 175, 200]);
    }

    #[test]
    fn a_batch_never_reports_the_state_it_was_handed() {
        // No duplicated tick at a batch seam: the previous batch already
        // reported that state, and a repeated row would break the
        // `UNIQUE (run_id, tick)` contract the persistence layer relies on.
        let params = interacting();
        let start = SimState::seeded(&params, 8);
        let (_, series) = run_batch(&start, &params, 5, 1);
        assert_eq!(
            series.iter().map(|s| s.0).collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5],
            "a batch reports the ticks it produced, starting after its input"
        );
    }

    #[test]
    fn zero_ticks_is_a_no_op_and_zero_stride_collects_nothing() {
        let params = interacting();
        let start = SimState::seeded(&params, 12);
        let (same, series) = run_batch(&start, &params, 0, 1);
        assert_eq!(same, start, "zero ticks must not move the flock");
        assert!(series.is_empty());

        let (moved, none) = run_batch(&start, &params, 10, 0);
        assert_eq!(moved.tick, 10);
        assert!(none.is_empty(), "a stride of zero collects no metrics");
    }

    #[test]
    fn run_batch_agrees_with_stepping_by_hand() {
        // `run_batch` must be exactly a fold of `step`, with no extra
        // rounding, reordering or bookkeeping of its own.
        let params = interacting();
        let start = SimState::seeded(&params, 606);
        let mut manual = start.clone();
        for _ in 0..50 {
            manual = step(&manual, &params);
        }
        let (batched, _) = run_batch(&start, &params, 50, 0);
        assert_eq!(manual.state_hash(), batched.state_hash());
        assert_bit_identical(&by_id(&manual), &by_id(&batched), "50 ticks");
    }

    // ---------------------------------------------------------------- AC-21

    /// Parameters chosen to break the simulation if it can be broken: a
    /// crowded world, weights two orders of magnitude above anything sane, a
    /// separation radius nearly as wide as the neighbourhood, obstacles
    /// sitting where agents already are, and a timestep coarse enough that a
    /// naive integrator would overshoot every frame.
    fn adversarial() -> SimParams {
        SimParams {
            world: World::new(40.0, 40.0),
            agent_count: 120,
            neighbor_radius: 30.0,
            separation_radius: 25.0,
            // `max_force * dt` is comfortably above `max_speed`, so a single
            // tick of almost any force saturates the speed clamp. That is the
            // point: the invariant is only tested where it binds.
            max_speed: 1.0,
            max_force: 3.0,
            dt: 0.5,
            w_separation: 90.0,
            w_alignment: 20.0,
            w_cohesion: 75.0,
            w_goal: 10.0,
            w_avoidance: 120.0,
            goal: Some(Vec2::new(20.0, 20.0)),
            obstacles: vec![
                Obstacle {
                    center: Vec2::new(20.0, 20.0),
                    radius: 9.0,
                },
                Obstacle {
                    center: Vec2::new(0.0, 0.0),
                    radius: 7.0,
                },
                Obstacle {
                    center: Vec2::new(39.0, 5.0),
                    radius: 5.0,
                },
            ],
            collision_radius: 1.5,
            goal_arrival_radius: 6.0,
            backend: NeighborBackend::SpatialHash,
        }
    }

    /// A flock with no goal to fall into, whose alignment weight makes it
    /// pick a heading and cruise. It laps the world repeatedly, which is the
    /// only way the position wrap gets exercised — the crowded scenario
    /// collapses onto its goal and never reaches a seam.
    fn cruising() -> SimParams {
        SimParams {
            goal: None,
            w_goal: 0.0,
            w_separation: 40.0,
            w_alignment: 200.0,
            w_cohesion: 20.0,
            ..adversarial()
        }
    }

    /// Every adversarial scenario the invariants are checked against.
    fn adversarial_scenarios() -> [(&'static str, SimParams); 2] {
        [("crowded", adversarial()), ("cruising", cruising())]
    }

    #[test]
    fn a_long_adversarial_run_never_breaks_its_invariants() {
        // AC-21. Checked on EVERY tick, for EVERY agent — not just at the end,
        // because a run that goes non-finite at tick 3 and is only inspected
        // at tick 200 looks identical to one that never ran at all.
        const TICKS: u32 = 200;
        let mut wraps = 0usize;

        for (name, params) in adversarial_scenarios() {
            let mut state = SimState::seeded(&params, 0x0BAD_1DEA);
            assert!(
                params.agent_count * TICKS as usize >= 20_000,
                "{name}: not enough agent-ticks to be called a long run"
            );

            for tick in 1..=TICKS {
                let previous = state;
                state = step(&previous, &params);

                // A coordinate that jumped more than half the world in one tick
                // can only have crossed a seam; `max_speed * dt` is far smaller.
                wraps += state
                    .agents
                    .iter()
                    .zip(&previous.agents)
                    .filter(|(now, was)| {
                        (now.pos.x - was.pos.x).abs() > params.world.width / 2.0
                            || (now.pos.y - was.pos.y).abs() > params.world.height / 2.0
                    })
                    .count();

                let grid = SpatialHash::build(&state.agents, &params.world, params.neighbor_radius);
                for (i, a) in state.agents.iter().enumerate() {
                    assert!(
                        a.pos.is_finite(),
                        "{name} tick {tick}: agent {} has a non-finite position {:?}",
                        a.id,
                        a.pos
                    );
                    assert!(
                        a.vel.is_finite(),
                        "{name} tick {tick}: agent {} has a non-finite velocity {:?}",
                        a.id,
                        a.vel
                    );
                    assert!(
                        a.vel.length() <= params.max_speed + 1e-9,
                        "{name} tick {tick}: agent {} exceeded max_speed: {} > {}",
                        a.id,
                        a.vel.length(),
                        params.max_speed
                    );
                    // Positions stay inside the world: `step` wraps, and a wrap
                    // that let an agent escape would corrupt every bucket index.
                    assert!(
                        a.pos.x >= 0.0
                            && a.pos.x < params.world.width
                            && a.pos.y >= 0.0
                            && a.pos.y < params.world.height,
                        "{name} tick {tick}: agent {} left the world at {:?}",
                        a.id,
                        a.pos
                    );

                    // The force the NEXT tick will apply, recomputed here so the
                    // clamp is asserted rather than assumed.
                    let neighbors =
                        grid.neighbors(&state.agents, &params.world, i, params.neighbor_radius);
                    let force = blend(&params, &state.agents, &params.world, i, &neighbors);
                    assert!(
                        force.is_finite(),
                        "{name} tick {tick}: agent {} has a non-finite steering force {force:?}",
                        a.id
                    );
                    assert!(
                        force.length() <= params.max_force + 1e-9,
                        "{name} tick {tick}: agent {} exceeded max_force: {} > {}",
                        a.id,
                        force.length(),
                        params.max_force
                    );
                }
            }
        }

        // Coverage guard, not an invariant. Without it the in-bounds
        // assertion above is vacuous: deleting the `world.wrap` from `step`
        // used to leave this test passing, because the crowded scenario
        // settles in the middle of the world and never reaches an edge.
        assert!(
            wraps > 100,
            "only {wraps} seam crossings across both scenarios; the position \
             wrap is not being exercised"
        );
    }

    #[test]
    fn the_adversarial_scenario_actually_stresses_the_clamps() {
        // A stability test on a placid scenario proves nothing. Pin that this
        // one really does saturate both limits and really does pile agents on
        // top of each other, so the invariants above are being defended
        // rather than merely observed.
        let params = adversarial();
        let mut state = SimState::seeded(&params, 0x0BAD_1DEA);
        let (mut speed_clamped, mut force_clamped, mut crowded) = (0usize, 0usize, 0usize);

        // Counted across the whole run, not at the end: this scenario settles
        // into a pile, so its final frame is the calmest one in it.
        for _ in 0..200 {
            state = step(&state, &params);
            let grid = SpatialHash::build(&state.agents, &params.world, params.neighbor_radius);
            for (i, a) in state.agents.iter().enumerate() {
                if (a.vel.length() - params.max_speed).abs() < 1e-9 {
                    speed_clamped += 1;
                }
                let n = grid.neighbors(&state.agents, &params.world, i, params.neighbor_radius);
                let f = blend(&params, &state.agents, &params.world, i, &n);
                if (f.length() - params.max_force).abs() < 1e-9 {
                    force_clamped += 1;
                }
            }
            crowded += frame_metrics(&state.agents, &params).collisions;
        }

        assert!(
            speed_clamped > 1_000,
            "only {speed_clamped} speed-clamped agent-ticks; the scenario is tame \
             and the max_speed invariant is untested"
        );
        assert!(
            force_clamped > 1_000,
            "only {force_clamped} force-clamped agent-ticks; the scenario is tame \
             and the max_force invariant is untested"
        );
        // ~1900 in practice: roughly ten overlapping pairs on every tick of
        // the run, so the coincident-agent tie-breaks are continuously live
        // rather than hit once by luck.
        assert!(
            crowded > 1_000,
            "only {crowded} colliding pairs over the run; the degenerate \
             coincident-agent paths are never exercised"
        );
    }

    #[test]
    fn an_absurdly_coarse_timestep_still_produces_a_finite_world() {
        // The nastiest input a user can supply through a valid config: a `dt`
        // so large that an agent crosses the world several times per tick.
        // The result may be physically meaningless, but it must never be
        // `NaN`, and every agent must still be somewhere in the world.
        let params = SimParams {
            dt: 250.0,
            ..adversarial()
        };
        let (state, _) = run_batch(&SimState::seeded(&params, 5), &params, 60, 0);
        for a in &state.agents {
            assert!(a.pos.is_finite() && a.vel.is_finite(), "blew up: {a:?}");
            assert!(
                a.vel.length() <= params.max_speed + 1e-9,
                "over speed: {a:?}"
            );
            assert!(
                a.pos.x >= 0.0 && a.pos.x < params.world.width,
                "escaped the world: {a:?}"
            );
        }
    }

    #[test]
    fn a_flock_stacked_on_a_single_point_stays_finite() {
        // Every agent coincident with every other, inside an obstacle, on the
        // goal: all three of the kernel's degenerate tie-breaks firing at once,
        // every tick.
        let params = SimParams {
            agent_count: 24,
            ..adversarial()
        };
        let agents = (0..24u32)
            .map(|id| Agent {
                id,
                pos: Vec2::new(20.0, 20.0),
                vel: Vec2::ZERO,
            })
            .collect();
        let (state, _) = run_batch(&SimState { tick: 0, agents }, &params, 100, 0);
        for a in &state.agents {
            assert!(a.pos.is_finite() && a.vel.is_finite(), "blew up: {a:?}");
            assert!(
                a.vel.length() <= params.max_speed + 1e-9,
                "over speed: {a:?}"
            );
        }
        // And it must have pushed them apart rather than leaving the pile.
        assert!(
            state.agents.iter().any(|a| a.pos != Vec2::new(20.0, 20.0)),
            "the pile never dispersed"
        );
    }

    // ---------------------------------------------------------------- AC-10

    #[test]
    fn both_neighbour_backends_produce_the_same_run() {
        // AC-10. `neighbors.rs` already proves the two backends return equal
        // sets for a single query. That is necessary but not sufficient: a
        // simulation compounds, so a disagreement in one query on one tick
        // would fan out through the whole flock. This asserts the stronger and
        // more useful thing — that an entire RUN is identical — which is what
        // makes the spatial hash an optimisation rather than a second model.
        //
        // Exact hash equality and exact metric equality. No tolerance: "close"
        // would mean the optimisation changes results, and a user switching
        // backends for speed would silently get a different experiment.
        let naive = SimParams {
            backend: NeighborBackend::Naive,
            ..interacting()
        };
        let hashed = SimParams {
            backend: NeighborBackend::SpatialHash,
            ..interacting()
        };
        let start = SimState::seeded(&naive, 0x0B71_4152);

        let (naive_end, naive_series) = run_batch(&start, &naive, 400, 1);
        let (hashed_end, hashed_series) = run_batch(&start, &hashed, 400, 1);

        assert_eq!(
            naive_end.state_hash(),
            hashed_end.state_hash(),
            "the two backends produced different runs"
        );
        assert_bit_identical(
            &by_id(&naive_end),
            &by_id(&hashed_end),
            "400 ticks, naive vs spatial hash",
        );
        assert_eq!(naive_series.len(), 400);
        assert_eq!(naive_series, hashed_series, "metric series diverged");
    }

    #[test]
    fn the_backends_agree_across_a_range_of_scenario_shapes() {
        // One scenario can hide a disagreement that only appears at a
        // particular density or radius — and the grid's cell count is derived
        // from exactly those. So: worlds far wider than tall, radii larger
        // than the world (which collapse the grid to a single cell), radii
        // small enough to make it fine, and a seeded flock in each.
        let shapes: [(f64, f64, f64, usize); 5] = [
            (200.0, 200.0, 25.0, 80), // the default shape
            (300.0, 12.0, 40.0, 60),  // a long thin world
            (30.0, 30.0, 60.0, 40),   // radius wider than the world
            (150.0, 90.0, 4.0, 150),  // a fine grid, dense flock
            (10.0, 10.0, 0.5, 25),    // a tiny world and a tiny radius
        ];
        for (w, h, radius, count) in shapes {
            let base = SimParams {
                world: World::new(w, h),
                agent_count: count,
                neighbor_radius: radius,
                separation_radius: (radius * 0.4).min(radius),
                ..interacting()
            };
            let naive = SimParams {
                backend: NeighborBackend::Naive,
                ..base.clone()
            };
            let hashed = SimParams {
                backend: NeighborBackend::SpatialHash,
                ..base.clone()
            };
            let start = SimState::seeded(&base, 0xE0F_1234);
            let (a, a_series) = run_batch(&start, &naive, 120, 5);
            let (b, b_series) = run_batch(&start, &hashed, 120, 5);
            assert_eq!(
                a.state_hash(),
                b.state_hash(),
                "backends diverged in a {w}x{h} world, radius {radius}, {count} agents"
            );
            assert_eq!(a_series, b_series, "metrics diverged in a {w}x{h} world");
        }
    }

    #[test]
    fn switching_backend_is_the_only_difference_between_the_two_runs() {
        // AC-49's half of this: the call site does not change, only the enum
        // value does. Guard that the test above is not accidentally comparing
        // two identically-configured runs.
        let naive = SimParams {
            backend: NeighborBackend::Naive,
            ..interacting()
        };
        let hashed = SimParams {
            backend: NeighborBackend::SpatialHash,
            ..interacting()
        };
        assert_ne!(naive.backend, hashed.backend);
        assert_eq!(
            SimParams {
                backend: hashed.backend,
                ..naive.clone()
            },
            hashed,
            "the two parameter sets differ in more than the backend"
        );
    }

    // ------------------------------------------------------------- validate

    /// The problems reported for `params`, or a panic if it was accepted.
    fn problems(params: &SimParams) -> Vec<String> {
        match validate(params) {
            Ok(()) => panic!("expected rejection, got acceptance"),
            Err(problems) => problems,
        }
    }

    /// Assert some reported problem mentions every one of `needles`.
    fn assert_mentions(params: &SimParams, needles: &[&str]) {
        let found = problems(params);
        for needle in needles {
            assert!(
                found.iter().any(|p| p.contains(needle)),
                "no problem mentioned `{needle}`; got {found:#?}"
            );
        }
    }

    #[test]
    fn validate_accepts_the_default_parameters() {
        // The defaults are what a new run starts from and what every preset
        // is built on, so a default that does not validate would be a trap.
        assert_eq!(validate(&SimParams::default()), Ok(()));
    }

    #[test]
    fn validate_accepts_every_scenario_this_module_simulates() {
        // A guard against `validate` drifting away from what the kernel can
        // actually run: every scenario used in the tests above must pass it.
        for (name, params) in [
            ("interacting", interacting()),
            ("adversarial", adversarial()),
            ("cruising", cruising()),
            ("smooth", smooth_params(0.5)),
        ] {
            assert_eq!(validate(&params), Ok(()), "{name} was rejected");
        }
    }

    #[test]
    fn validate_rejects_a_world_with_no_area() {
        for world in [
            World::new(0.0, 100.0),
            World::new(100.0, 0.0),
            World::new(-5.0, 100.0),
            World::new(f64::NAN, 100.0),
            World::new(f64::INFINITY, 100.0),
        ] {
            assert_mentions(
                &SimParams {
                    world,
                    ..SimParams::default()
                },
                &["world"],
            );
        }
    }

    #[test]
    fn validate_rejects_a_non_positive_timestep() {
        for dt in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_mentions(
                &SimParams {
                    dt,
                    ..SimParams::default()
                },
                &["dt"],
            );
        }
    }

    #[test]
    fn validate_rejects_an_empty_flock() {
        assert_mentions(
            &SimParams {
                agent_count: 0,
                ..SimParams::default()
            },
            &["agent_count"],
        );
    }

    #[test]
    fn validate_rejects_negative_or_non_finite_distances_and_limits() {
        // Each field named individually, so a validator that checks four of
        // the seven cannot pass by accident.
        type Set = fn(&mut SimParams, f64);
        let fields: [(&str, Set); 6] = [
            ("neighbor_radius", |p, v| p.neighbor_radius = v),
            ("separation_radius", |p, v| p.separation_radius = v),
            ("collision_radius", |p, v| p.collision_radius = v),
            ("goal_arrival_radius", |p, v| p.goal_arrival_radius = v),
            ("max_speed", |p, v| p.max_speed = v),
            ("max_force", |p, v| p.max_force = v),
        ];
        for (name, set) in fields {
            for bad in [-1.0, f64::NAN, f64::INFINITY] {
                let mut params = SimParams::default();
                set(&mut params, bad);
                // Keep the separation/neighbour ordering rule out of it, so
                // the reported problem is unambiguously about this field.
                if name == "neighbor_radius" {
                    params.separation_radius = 0.0;
                }
                assert_mentions(&params, &[name]);
            }
        }
    }

    #[test]
    fn validate_rejects_non_finite_weights() {
        // Negative weights are legal — they invert a behaviour, which is a
        // legitimate experiment — but a non-finite one poisons the whole
        // flock in a single tick.
        type Set = fn(&mut SimParams, f64);
        let weights: [(&str, Set); 5] = [
            ("w_separation", |p, v| p.w_separation = v),
            ("w_alignment", |p, v| p.w_alignment = v),
            ("w_cohesion", |p, v| p.w_cohesion = v),
            ("w_goal", |p, v| p.w_goal = v),
            ("w_avoidance", |p, v| p.w_avoidance = v),
        ];
        for (name, set) in weights {
            for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut params = SimParams::default();
                set(&mut params, bad);
                assert_mentions(&params, &[name]);
            }
            let mut negative = SimParams::default();
            set(&mut negative, -2.0);
            assert_eq!(
                validate(&negative),
                Ok(()),
                "{name} of -2.0 is a legitimate inverted behaviour"
            );
        }
    }

    #[test]
    fn validate_rejects_a_separation_radius_wider_than_the_neighbourhood() {
        // Not a range error on either field alone — both values are perfectly
        // reasonable — but a contradiction between them: an agent cannot be
        // repelled by a neighbour it is unable to see.
        assert_mentions(
            &SimParams {
                neighbor_radius: 10.0,
                separation_radius: 25.0,
                ..SimParams::default()
            },
            &["separation_radius", "neighbor_radius"],
        );
        // Equal is fine: the whole neighbourhood repels.
        assert_eq!(
            validate(&SimParams {
                neighbor_radius: 10.0,
                separation_radius: 10.0,
                ..SimParams::default()
            }),
            Ok(())
        );
    }

    #[test]
    fn validate_rejects_degenerate_obstacles_and_names_which_one() {
        let params = SimParams {
            obstacles: vec![
                Obstacle {
                    center: Vec2::new(10.0, 10.0),
                    radius: 5.0,
                },
                Obstacle {
                    center: Vec2::new(20.0, 20.0),
                    radius: 0.0,
                },
                Obstacle {
                    center: Vec2::new(30.0, 30.0),
                    radius: -3.0,
                },
                Obstacle {
                    center: Vec2::new(f64::NAN, 40.0),
                    radius: 4.0,
                },
            ],
            ..SimParams::default()
        };
        let found = problems(&params);
        // The index matters: "an obstacle is invalid" is not actionable when
        // the scenario has forty of them.
        assert!(
            found.iter().any(|p| p.contains("obstacle 1")),
            "the zero-radius obstacle was not named: {found:#?}"
        );
        assert!(found.iter().any(|p| p.contains("obstacle 2")));
        assert!(found.iter().any(|p| p.contains("obstacle 3")));
        assert!(
            !found.iter().any(|p| p.contains("obstacle 0")),
            "the valid obstacle was reported: {found:#?}"
        );
    }

    #[test]
    fn validate_rejects_a_non_finite_goal() {
        assert_mentions(
            &SimParams {
                goal: Some(Vec2::new(f64::NAN, 10.0)),
                ..SimParams::default()
            },
            &["goal"],
        );
        // No goal at all is not an error; it is a scenario without one.
        assert_eq!(
            validate(&SimParams {
                goal: None,
                ..SimParams::default()
            }),
            Ok(())
        );
    }

    #[test]
    fn validate_reports_every_problem_at_once() {
        // The point of returning a Vec. A user fixing a form one field per
        // submission because the validator stops at the first fault is a
        // validator that has failed at its job.
        let params = SimParams {
            world: World::new(0.0, -1.0),
            agent_count: 0,
            dt: -0.5,
            neighbor_radius: -3.0,
            separation_radius: f64::NAN,
            max_speed: f64::NAN,
            max_force: -1.0,
            w_cohesion: f64::INFINITY,
            goal: Some(Vec2::new(f64::INFINITY, 0.0)),
            obstacles: vec![Obstacle {
                center: Vec2::ZERO,
                radius: 0.0,
            }],
            ..SimParams::default()
        };
        let found = problems(&params);
        for needle in [
            "world",
            "agent_count",
            "dt",
            "neighbor_radius",
            "separation_radius",
            "max_speed",
            "max_force",
            "w_cohesion",
            "goal",
            "obstacle 0",
        ] {
            assert!(
                found.iter().any(|p| p.contains(needle)),
                "`{needle}` was not reported; got {found:#?}"
            );
        }
        assert!(
            found.len() >= 10,
            "expected every fault, got only {}: {found:#?}",
            found.len()
        );
    }

    #[test]
    fn validation_messages_say_what_is_wrong_and_what_is_required() {
        // An actionable message names the field, shows the offending value,
        // and states the rule. "invalid parameter" is none of those.
        let params = SimParams {
            dt: -0.5,
            max_speed: -2.0,
            ..SimParams::default()
        };
        let found = problems(&params);
        for problem in &found {
            assert!(
                problem.len() > 20,
                "message is too terse to act on: {problem:?}"
            );
            assert!(
                !problem.ends_with('.') || problem.contains("must"),
                "message does not state a requirement: {problem:?}"
            );
        }
        let dt = found
            .iter()
            .find(|p| p.contains("dt"))
            .expect("dt problem missing");
        assert!(dt.contains("-0.5"), "message omits the value: {dt:?}");
        assert!(dt.contains("must"), "message omits the rule: {dt:?}");
    }

    #[test]
    fn a_validated_scenario_actually_runs() {
        // Validation is only worth anything if acceptance means something.
        // Anything `validate` accepts must survive a real run with the
        // stability invariants intact.
        for params in [
            SimParams::default(),
            interacting(),
            adversarial(),
            cruising(),
        ] {
            assert_eq!(validate(&params), Ok(()));
            let (state, _) = run_batch(&SimState::seeded(&params, 909), &params, 60, 0);
            for a in &state.agents {
                assert!(a.pos.is_finite() && a.vel.is_finite(), "blew up: {a:?}");
                assert!(a.vel.length() <= params.max_speed + 1e-9);
            }
        }
    }
}
