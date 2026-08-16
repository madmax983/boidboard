//! Print the canonical state hash of a fixed scenario, one line per seed.
//!
//! This exists to be run as a **separate OS process** by
//! `tests/cross_process.rs`, which is the only way to actually demonstrate
//! AC-22: reproducibility that holds across processes, not merely within one.
//!
//! Usage:
//!
//! ```text
//! cargo run --quiet --example state_hash -p boids-core -- <ticks> <seed>...
//! ```
//!
//! The scenario is [`SimParams::default`] so that both sides read one shared
//! definition, and the ticks and seeds arrive as arguments so that the test
//! owns the experiment. Nothing is printed but the hashes: the test parses
//! stdout.

use boids_core::config::SimParams;
use boids_core::sim::{SimState, run_batch};

fn main() {
    let mut args = std::env::args().skip(1);
    let ticks: u32 = args
        .next()
        .and_then(|a| a.parse().ok())
        .expect("usage: state_hash <ticks> <seed>...");

    let params = SimParams::default();
    for arg in args {
        let seed: u64 = arg
            .parse()
            .unwrap_or_else(|_| panic!("`{arg}` is not a u64 seed"));
        let start = SimState::seeded(&params, seed);
        // No metrics: this process exists only to report the final hash.
        let (end, _) = run_batch(&start, &params, ticks, 0);
        println!("{}", end.state_hash_hex());
    }
}
