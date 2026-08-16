//! Search: principal variation search, the move-ordering stack, and late move reductions.
//!
//! Phase 2–3, issues #7, #8, #9.
//!
//! This crate is generic over an `Evaluator` trait defined in [`boid_board`] and must
//! **never** depend on a concrete evaluator. Evaluator selection is a runtime UCI option,
//! not a cargo feature, so that issue #13's decision gate and issue #16's ablation
//! campaign are a loop over UCI options rather than a loop over cargo builds.
//!
//! See `docs/DECISIONS.md` D-0010.
#![forbid(unsafe_code)]
