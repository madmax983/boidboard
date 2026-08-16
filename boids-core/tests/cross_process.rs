//! AC-22 — the simulation is reproducible **across OS processes**.
//!
//! Determinism within one process is easy to achieve by accident: a cached
//! value, a lazily initialised table, or a `HashMap` iteration order that
//! happens to be stable for the life of a process will all pass an in-process
//! test and fail the moment the run is resumed somewhere else. That is not a
//! theoretical worry here — a durable workflow resumes runs in whatever
//! process picks the work up, and the reproducibility hash is stored and shown
//! to users as a claim that a run can be re-created.
//!
//! So this test does the only thing that actually proves it: it runs a
//! scenario in **this** process, spawns a **separate** process to run the same
//! scenario, and compares the hashes.
//!
//! The child is `examples/state_hash.rs`, run via `cargo run --example`. The
//! scenario is defined *here* and passed on the command line, so the two sides
//! cannot drift apart into testing different things.

use boids_core::config::SimParams;
use boids_core::sim::{SimState, run_batch};
use std::process::Command;

/// How many ticks the compared scenario runs for.
///
/// Long enough that any per-process nondeterminism has had hundreds of
/// opportunities to compound into a visible difference, rather than being
/// rounded away in the first few frames.
const TICKS: u32 = 250;

/// Seeds compared in a single child invocation.
///
/// More than one, because a single seed could agree by luck if a defect only
/// affected part of the state space; batched into one child so the test costs
/// one process spawn rather than four.
const SEEDS: [u64; 4] = [0, 1, 0xB01D_B0A2_D000_1234, u64::MAX];

/// The scenario under test: the shared default parameters, so both processes
/// read the same definition from the library rather than each spelling one
/// out.
fn params() -> SimParams {
    SimParams::default()
}

/// The hash this process computes for `seed`.
fn local_hash(seed: u64) -> String {
    let p = params();
    let start = SimState::seeded(&p, seed);
    let (end, _) = run_batch(&start, &p, TICKS, 0);
    end.state_hash_hex()
}

#[test]
fn the_same_scenario_hashes_identically_in_a_separate_process() {
    // Skip rather than fail where there is no toolchain to spawn — a
    // packaged binary running its own test suite is a legitimate environment,
    // and a test that cannot run is not a test that failed. It must, however,
    // really run in development and CI, so the skip is loud.
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not available to spawn a child process");
        return;
    }

    let expected: Vec<String> = SEEDS.iter().map(|&s| local_hash(s)).collect();

    let mut command = Command::new("cargo");
    command
        .args([
            "run",
            "--quiet",
            "--example",
            "state_hash",
            "-p",
            "boids-core",
        ])
        .arg("--")
        .arg(TICKS.to_string());
    for seed in SEEDS {
        command.arg(seed.to_string());
    }
    let output = command.output().expect("failed to spawn the child process");
    assert!(
        output.status.success(),
        "child process failed: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("child printed non-UTF-8");
    let actual: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    assert_eq!(
        actual.len(),
        SEEDS.len(),
        "expected one hash per seed, got {actual:?}"
    );
    for ((seed, mine), theirs) in SEEDS.iter().zip(&expected).zip(&actual) {
        assert_eq!(
            mine, theirs,
            "seed {seed} hashed as {mine} in this process and {theirs} in a separate one, \
             after {TICKS} ticks"
        );
    }

    // Guard: if the scenario were degenerate — say every run collapsed to an
    // empty flock — every seed would agree trivially and this test would
    // prove nothing about determinism.
    let distinct: std::collections::BTreeSet<&String> = expected.iter().collect();
    assert_eq!(
        distinct.len(),
        SEEDS.len(),
        "different seeds produced the same hash, so the comparison is vacuous: {expected:?}"
    );
}
