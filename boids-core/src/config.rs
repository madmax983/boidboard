//! Simulation parameters.
//!
//! This module is deliberately pure data — the shared contract every other
//! kernel module is written against. Behaviour lives in `forces`, `neighbors`,
//! `metrics`, and `sim`; validation lives in `sim`.

use crate::vec2::Vec2;
use crate::world::World;
use serde::{Deserialize, Serialize};

/// A circular no-go region agents steer away from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Obstacle {
    pub center: Vec2,
    pub radius: f64,
}

/// Which neighbour-query implementation a run uses.
///
/// The two backends must be observationally identical; that equivalence is
/// asserted by a property test rather than assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NeighborBackend {
    /// Brute-force O(N^2). The reference implementation.
    Naive,
    /// Spatial hash grid. The optimisation under test.
    #[default]
    SpatialHash,
}

/// The full parameter set for a simulation run.
///
/// The five weights correspond to the classic Reynolds steering decomposition
/// plus the two task-oriented behaviours:
/// `F = w_sep*sep + w_align*align + w_coh*coh + w_goal*goal + w_avoid*avoid`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimParams {
    pub world: World,
    pub agent_count: usize,

    /// Radius within which another agent counts as a flockmate.
    pub neighbor_radius: f64,
    /// Radius within which separation pushes agents apart. Typically smaller
    /// than `neighbor_radius`.
    pub separation_radius: f64,

    pub max_speed: f64,
    pub max_force: f64,
    /// Integration timestep. Forces are scaled by this so results do not
    /// depend on how finely the run is ticked.
    pub dt: f64,

    pub w_separation: f64,
    pub w_alignment: f64,
    pub w_cohesion: f64,
    pub w_goal: f64,
    pub w_avoidance: f64,

    /// Optional goal waypoint for goal-seeking.
    pub goal: Option<Vec2>,
    pub obstacles: Vec<Obstacle>,

    /// Two agents closer than this are counted as colliding.
    pub collision_radius: f64,
    /// An agent within this distance of the goal has arrived.
    pub goal_arrival_radius: f64,

    pub backend: NeighborBackend,
}

impl Default for SimParams {
    fn default() -> Self {
        Self {
            world: World::new(200.0, 200.0),
            agent_count: 80,
            neighbor_radius: 25.0,
            separation_radius: 8.0,
            max_speed: 2.0,
            max_force: 0.25,
            dt: 1.0,
            w_separation: 1.5,
            w_alignment: 1.0,
            w_cohesion: 1.0,
            w_goal: 0.0,
            w_avoidance: 2.0,
            goal: None,
            obstacles: Vec::new(),
            collision_radius: 2.0,
            goal_arrival_radius: 10.0,
            backend: NeighborBackend::SpatialHash,
        }
    }
}
