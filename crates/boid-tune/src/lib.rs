//! Texel tuning, the ablation matrix, and the resumable SPRT gauntlet.
//!
//! Phase 7 and 9, issues #14 and #16.
//!
//! Deliberately outside the `clippy::float_arithmetic` denial that binds the two evaluator
//! crates: gradient descent needs floats. The *evaluation* is fixed-point; the *tuner* that
//! produces its constants is not (`docs/DECISIONS.md` D-0011).
#![forbid(unsafe_code)]

/// Re-export of the pinned workflow-orchestration engine. See [`boid_web::framework`] for
/// why this is a plain re-export.
///
/// [`boid_web::framework`]: https://github.com/madmax983/boidboard
#[cfg(feature = "orchestrate")]
pub use autumn_harvest as orchestrator;
