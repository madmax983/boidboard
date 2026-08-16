//! Analysis board with the live force overlay.
//!
//! Phase 8, issue #15. Renders a position together with the boids force vectors the
//! evaluator computed for it, so the evaluation can be *seen* rather than inferred from a
//! centipawn number.
//!
//! The web framework is behind the off-by-default `serve` feature (`docs/DECISIONS.md`
//! D-0012).
#![forbid(unsafe_code)]

/// Re-export of the pinned web framework, proving the dependency resolves and compiles.
///
/// A plain re-export rather than a macro invocation: lints fired inside a third-party
/// macro expansion are attributed to *this* crate and could fail `-D warnings` in a way we
/// could not fix.
#[cfg(feature = "serve")]
pub use autumn_web as framework;
