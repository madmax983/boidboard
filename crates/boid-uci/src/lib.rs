//! UCI protocol handling, time management, and the `boidboard` binary.
//!
//! Phase 2, issue #7.
//!
//! This crate is the **composition root**: it is the one place that knows about both
//! evaluators, and it selects between them at runtime via a UCI option. Nothing below it
//! in the dependency graph may name a concrete evaluator (`docs/DECISIONS.md` D-0010).
#![forbid(unsafe_code)]
