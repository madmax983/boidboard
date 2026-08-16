//! Boids force-field evaluation: the attack-set neighbourhood and the five forces.
//!
//! Phase 5–6, issues #11, #12, #13. This is the idea the project exists to test.
//!
//! The geometry is **discrete and attack-set-based**: a piece's neighbourhood is the set of
//! squares it attacks and the set of pieces attacking it, on 64 squares, in fixed-point
//! integers. It is emphatically *not* the toroidal, continuous, floating-point geometry of
//! the archived simulator (`archive/boids-sim-attempt`), none of whose code may be reused.
//!
//! `clippy::float_arithmetic` is denied here so that continuous-space code fails the build
//! rather than merely violating a documented rule.
//!
//! See `docs/DECISIONS.md` D-0001 and D-0011.
#![forbid(unsafe_code)]
// The mechanical enforcement of D-0001's discrete-geometry rule: continuous-space
// code fails the build on contact rather than merely violating a documented rule.
#![deny(clippy::float_arithmetic)]
