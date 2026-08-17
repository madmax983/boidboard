//! Helpers shared by the integration tests.
//!
//! Not a cargo test target: only `tests/*.rs` and `tests/*/main.rs` are compiled as test
//! binaries, so this directory is included by the files that need it with
//! `#[allow(dead_code)] mod support;` — the OUTER form, on the `mod` item. The inner
//! crate-level form is banned by `scripts/anti-theatre.sh`, and rightly: it would disable
//! the lint for a whole test binary rather than for one module a given consumer only
//! partly uses.

pub mod naive_attacks;
pub mod positions;
pub mod sha256;
