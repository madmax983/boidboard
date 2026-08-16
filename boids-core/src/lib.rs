//! Boidboard simulation kernel.
//!
//! Pure, deterministic, and free of any framework, database, or IO dependency.
//! Everything here is a value transformation, which is what makes the whole
//! simulation test-drivable without infrastructure.

/// Placeholder for the walking-skeleton gate; replaced by the real kernel.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
