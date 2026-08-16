//! Classical control evaluation: material, piece-square tables, pawn structure, mobility.
//!
//! Phase 4, issue #10. This is the *control arm*. Its job is to be an honest, conventional
//! evaluator good enough to anchor the boids evaluator against an external Elo scale, so
//! that issue #13's decision gate measures the idea rather than the scaffolding.
//!
//! Fixed-point integers only: `clippy::float_arithmetic` is denied in this crate
//! (`docs/DECISIONS.md` D-0011).
#![forbid(unsafe_code)]
// Fixed-point only. A manifest [lints.clippy] table cannot coexist with
// `[lints] workspace = true`, so the extra denial lives here instead (D-0011).
#![deny(clippy::float_arithmetic)]
