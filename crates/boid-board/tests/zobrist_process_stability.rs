//! AC4, read literally: the keys are byte-identical across two separate process runs.
//!
//! This test re-executes its own binary and compares what the child prints with what the
//! parent computes. It is the weakest of the four mechanisms this project uses for AC4, and
//! it is here because it is the one the criterion's words actually describe. The others,
//! strongest first (D-0020):
//!
//! 1. **Per-position key literals** in `zobrist_incremental.rs`. The only pins that can see
//!    the index formula — the digest cannot, because reindexing the same 781 keys leaves the
//!    digest untouched.
//! 2. **The pinned table digest** in `zobrist_tables.rs`. The only mechanism that also kills
//!    a per-*build* entropy source, which every runtime comparison in a single build would
//!    agree with itself about.
//! 3. **Const evaluation.** `src/zobrist.rs` forces the table through the const interpreter,
//!    which has no clock, no I/O, no entropy and no FFI. Per-process variation there is not
//!    *detected*; it is impossible.
//! 4. This test.
//!
//! Worth being explicit about the trap: `assert_eq!(build(), build())` inside one process
//! would pass under every threat model AC4 is about, including seeding from `/dev/urandom`
//! once at startup. It is named and rejected in D-0020 rather than written.

use std::process::Command;

use boid_board::board::Board;
use boid_board::zobrist::{ZOBRIST, Zobrist};

#[allow(dead_code)]
mod support;
use support::sha256::sha256_hex;

/// Set in the child, so it prints and returns instead of spawning another child.
const CHILD_ENV: &str = "BOIDBOARD_ZOBRIST_CHILD";

/// The same digest `zobrist_tables.rs` pins, restated here rather than shared: this file
/// must be able to fail on its own, in a second process, without depending on a helper the
/// same mutation could change.
const ZOBRIST_SHA256: &str = "79dbfe5ac62eb22d3e1961835c668ca10c79b467fa36b9c63391e78f5ea985a2";

/// What each process prints. Three position keys as well as the table digest, because a
/// table can be stable while the *selection* from it is not — a `recomputed_key` that walked
/// a `HashSet` would produce a fixed table and a wandering key.
fn report() -> String {
    let digest = sha256_hex(
        &(0..Zobrist::LEN)
            .flat_map(|i| ZOBRIST.flat(i).to_le_bytes())
            .collect::<Vec<u8>>(),
    );
    let kiwipete = Board::from_fen_with_layout(
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -",
    )
    .expect("the published Kiwipete FEN parses")
    .0;

    format!(
        "DIGEST={digest} startpos={:016X} kiwipete={:016X} pawn={:016X}",
        Board::startpos().key(),
        kiwipete.key(),
        Board::startpos().pawn_key(),
    )
}

#[test]
fn zobrist_keys_are_identical_across_two_processes() {
    let mine = report();

    if std::env::var(CHILD_ENV).is_ok() {
        // The child branch is not a no-op: it asserts before it prints, so if this variable
        // is ever set in the ambient environment and the parent branch never runs, what is
        // left still tests something real.
        assert!(
            mine.contains(ZOBRIST_SHA256),
            "the child computed a different table digest: {mine}"
        );
        println!("{mine}");
        return;
    }

    assert!(
        mine.contains(ZOBRIST_SHA256),
        "the parent computed a different table digest: {mine}"
    );

    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let output = Command::new(exe)
        .args([
            "--exact",
            "--nocapture",
            "zobrist_keys_are_identical_across_two_processes",
        ])
        .env(CHILD_ENV, "1")
        .output()
        .expect("re-running this test binary must succeed");

    assert!(
        output.status.success(),
        "the child process failed: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let reports: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("DIGEST="))
        .collect();

    // Exactly one, and `expect` rather than a default: a child that silently ran zero tests
    // would otherwise be indistinguishable from one that agreed.
    assert_eq!(
        reports.len(),
        1,
        "expected exactly one report from the child, got {}:\n{stdout}",
        reports.len()
    );
    let theirs = *reports.first().expect("exactly one report");

    assert_eq!(
        theirs, mine,
        "two processes of the same binary produced different zobrist keys"
    );
}
