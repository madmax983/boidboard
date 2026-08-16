//! Boidboard simulation kernel.
//!
//! Pure, deterministic, and free of any framework, database, or IO dependency.
//! Everything here is a value transformation, which is what makes the whole
//! simulation test-drivable without infrastructure.

pub mod config;
pub mod forces;
pub mod hash;
pub mod metrics;
pub mod neighbors;
pub mod rng;
pub mod sim;
pub mod vec2;
pub mod world;

pub use config::{NeighborBackend, Obstacle, SimParams};
pub use rng::Rng;
pub use vec2::Vec2;
pub use world::{Agent, World};

/// Kernel version string, recorded in a run's provenance so a stored result
/// can be tied to the code that produced it.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
