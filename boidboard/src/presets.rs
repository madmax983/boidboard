//! Named starting points for a new run (AC-42).
//!
//! A blank parameter form is a dead end: fifteen coupled floats with no
//! indication of which combinations produce flocking rather than a static
//! cloud or an exploding one. Every preset here is a *known-interesting* point
//! in that space, chosen so the four behave visibly differently — a user
//! comparing two runs should be able to see the difference without reading the
//! numbers.
//!
//! This module is pure data. It has no database, HTTP or rendering dependency,
//! so it is unit-testable on its own and reusable by the CLI, the workflow and
//! the web form alike.

use boids_core::{NeighborBackend, Obstacle, SimParams, Vec2, World};

/// A named, described parameter set offered on the new-run form.
///
/// `slug` is the stable identifier submitted by the form and is what a run's
/// provenance can be traced back to; `name` and `description` are for humans
/// and may be reworded freely.
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    /// URL- and form-safe stable identifier.
    pub slug: &'static str,
    /// Human-readable name shown on the preset card.
    pub name: &'static str,
    /// One sentence on what the preset is *for* — what a user should expect to
    /// see, not a restatement of the parameters.
    pub description: &'static str,
    /// The parameter set itself.
    pub params: SimParams,
}

/// Every preset, in the order they are offered on the form.
///
/// The first entry is the default selection, so it is deliberately the most
/// legible one.
#[must_use]
pub fn all() -> Vec<Preset> {
    vec![classic_flock(), nervous_swarm(), highway(), scatter()]
}

/// Look a preset up by its stable slug.
///
/// Returns `None` for an unknown slug rather than falling back to a default:
/// silently substituting a different parameter set would make a run's
/// provenance a lie.
#[must_use]
pub fn by_slug(slug: &str) -> Option<Preset> {
    all().into_iter().find(|p| p.slug == slug)
}

/// Balanced Reynolds weights — the textbook flock.
fn classic_flock() -> Preset {
    Preset {
        slug: "classic-flock",
        name: "Classic Flock",
        description: "Balanced separation, alignment and cohesion: one coherent \
                      flock that turns as a body. The reference behaviour every \
                      other preset is a departure from.",
        params: SimParams {
            world: World::new(400.0, 300.0),
            agent_count: 120,
            neighbor_radius: 30.0,
            separation_radius: 9.0,
            max_speed: 2.0,
            max_force: 0.25,
            w_separation: 1.5,
            w_alignment: 1.0,
            w_cohesion: 1.0,
            w_goal: 0.0,
            w_avoidance: 2.0,
            goal: None,
            obstacles: Vec::new(),
            backend: NeighborBackend::SpatialHash,
            ..SimParams::default()
        },
    }
}

/// Separation-heavy with a short horizon — a jittery, boiling swarm.
fn nervous_swarm() -> Preset {
    Preset {
        slug: "nervous-swarm",
        name: "Nervous Swarm",
        description: "Strong separation against weak cohesion over a short \
                      neighbour horizon. The group stays together but never \
                      settles — polarization stays low and churns.",
        params: SimParams {
            world: World::new(320.0, 320.0),
            agent_count: 160,
            neighbor_radius: 18.0,
            separation_radius: 14.0,
            max_speed: 2.6,
            max_force: 0.55,
            w_separation: 3.2,
            w_alignment: 0.4,
            w_cohesion: 0.3,
            w_goal: 0.0,
            w_avoidance: 2.5,
            collision_radius: 2.5,
            goal: None,
            obstacles: Vec::new(),
            backend: NeighborBackend::SpatialHash,
            ..SimParams::default()
        },
    }
}

/// Goal-seeking and highly aligned, threaded between obstacles.
fn highway() -> Preset {
    Preset {
        slug: "highway",
        name: "Highway",
        description: "Strong goal-seeking plus strong alignment, with three \
                      obstacles between the flock and the waypoint. Watch the \
                      lane split and re-merge; `fraction_arrived` is the metric \
                      that matters here.",
        params: SimParams {
            world: World::new(500.0, 260.0),
            agent_count: 140,
            neighbor_radius: 28.0,
            separation_radius: 8.0,
            max_speed: 2.4,
            max_force: 0.35,
            w_separation: 1.2,
            w_alignment: 2.0,
            w_cohesion: 0.6,
            w_goal: 2.5,
            w_avoidance: 3.5,
            goal: Some(Vec2::new(470.0, 130.0)),
            obstacles: vec![
                Obstacle {
                    center: Vec2::new(200.0, 80.0),
                    radius: 28.0,
                },
                Obstacle {
                    center: Vec2::new(260.0, 190.0),
                    radius: 34.0,
                },
                Obstacle {
                    center: Vec2::new(360.0, 120.0),
                    radius: 22.0,
                },
            ],
            goal_arrival_radius: 25.0,
            backend: NeighborBackend::SpatialHash,
            ..SimParams::default()
        },
    }
}

/// Separation-dominant with cohesion near zero — the flock disperses.
fn scatter() -> Preset {
    Preset {
        slug: "scatter",
        name: "Scatter",
        description: "Separation dominant, cohesion effectively switched off. \
                      The flock spreads until it fills the torus — the \
                      mean-nearest-neighbour sparkline climbs and then flattens \
                      at the packing limit.",
        params: SimParams {
            world: World::new(360.0, 360.0),
            agent_count: 90,
            neighbor_radius: 45.0,
            separation_radius: 30.0,
            max_speed: 1.8,
            max_force: 0.2,
            w_separation: 2.6,
            w_alignment: 0.2,
            w_cohesion: 0.02,
            w_goal: 0.0,
            w_avoidance: 1.5,
            goal: None,
            obstacles: Vec::new(),
            backend: NeighborBackend::SpatialHash,
            ..SimParams::default()
        },
    }
}
