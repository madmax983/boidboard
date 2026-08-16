//! Cycle 1, guards: the key table is pinned, independently derived, and demonstrably not
//! produced at runtime.
//!
//! This file is what discharges the issue's "zobrist keys are byte-identical across two
//! separate process runs". It does so four ways, because that sentence has a weak reading
//! and a strong one and only the strong one is worth having:
//!
//! 1. **A pinned digest.** [`ZOBRIST_SHA256`] is held here, in the checker, not in the
//!    module under test — the same idiom `oracle_transcription.rs` uses for the fixture
//!    (D-0007). Any drift in any of the 781 keys, on any machine, forever, fails here.
//! 2. **An independent derivation.** `scripts/zobrist-reference.py` was written from the
//!    decision log rather than from the Rust, and its digest was computed *first*; the
//!    constant below is the Python's output and the Rust was then made to agree. A
//!    `repo-invariants` CI step re-runs the diff.
//! 3. **A second process.** [`the_digest_is_identical_in_a_second_process`] re-executes
//!    this test binary and compares. That is AC4's sentence taken literally.
//! 4. **The bytes are in the executable image.** A table built at runtime from an entropy
//!    source can pass 1 and 3 — it need only be stable *within* a run and seeded the same
//!    way twice. It cannot pass 4.
//!
//! Layer 4 is the one that actually excludes entropy at runtime, and the `const` item in
//! `src/zobrist.rs` excludes it at compile time. The digest is the pin; the process re-run
//! is the AC's literal wording.

mod support;

use std::process::Command;

use boid_board::zobrist::{self, KEY_COUNT};
use support::sha256_hex;

/// SHA-256 of the 781 keys, each big-endian, concatenated in table-index order.
///
/// Produced by `scripts/zobrist-reference.py` **before** it was pinned here. If this test
/// fails, the key table changed: that is not automatically wrong, but it must be
/// deliberate, and D-0018 requires a superseding decision entry to change any of the four
/// orderings that feed it.
const ZOBRIST_SHA256: &str = "31e98d78da31b5d7439ed0601a698f7ab6dc54499d3e0be77d3be645dee2a314";

/// Environment variable that turns [`the_key_digest_is_pinned`] into a reporter.
const CHILD_VAR: &str = "BOID_ZOBRIST_DIGEST_CHILD";

/// Prefix the child prints the digest behind. Must not appear in libtest's own output.
const MARKER: &str = "ZOBRIST-DIGEST=";

/// The digest, serialised exactly as `scripts/zobrist-reference.py` serialises it.
fn digest() -> String {
    let mut body = Vec::with_capacity(KEY_COUNT * 8);
    for key in zobrist::table() {
        body.extend_from_slice(&key.get().to_be_bytes());
    }
    sha256_hex(&body)
}

/// The pin, and — when the child variable is set — the reporter the parent reads.
///
/// It asserts in both roles, so the child process cannot be a no-op that merely prints.
#[test]
fn the_key_digest_is_pinned() {
    let got = digest();
    assert_eq!(
        got, ZOBRIST_SHA256,
        "the zobrist key table changed; scripts/zobrist-reference.py derives the expected \
         value independently"
    );
    if std::env::var_os(CHILD_VAR).is_some() {
        println!("{MARKER}{got}");
    }
}

/// AC4, taken literally: the same digest in a *different operating-system process*.
///
/// Re-executes this very test binary rather than invoking cargo, so the check does not
/// depend on a build system being present or on a target directory layout.
///
/// The two ways this test could pass without meaning anything are both closed explicitly.
/// A mistyped filter makes libtest print `running 0 tests` and exit **0**, which is
/// a skipped test reconstructed out of exit codes — so the parent requires the child to
/// report exactly one passing test. And a child that printed nothing would leave the
/// comparison with nothing to compare, so the marker's absence is a panic rather than a
/// skip.
#[test]
fn the_digest_is_identical_in_a_second_process() {
    let exe = std::env::current_exe().expect("a test binary knows its own path");
    let output = Command::new(&exe)
        .args([
            "--exact",
            "the_key_digest_is_pinned",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_VAR, "1")
        .output()
        .expect("re-running this test binary as a child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the child process failed: {}\n{stdout}",
        output.status
    );
    assert!(
        stdout.contains("1 passed"),
        "the child must run exactly the one test asked for; a filter that matches nothing \
         prints 'running 0 tests' and still exits 0. stdout was:\n{stdout}"
    );

    // Searched anywhere in the line, not as a line prefix: under `--nocapture` libtest
    // writes its own "test <name> ... " progress text and the test's stdout onto the same
    // line, so the marker is mid-line rather than at its start.
    let reported = stdout
        .lines()
        .find_map(|line| line.split(MARKER).nth(1))
        .map(str::trim)
        .unwrap_or_else(|| {
            panic!("the child printed no {MARKER} line; stdout was:\n{stdout}");
        });

    assert_eq!(
        reported,
        digest(),
        "the key table differs between two processes of the same binary"
    );
}

/// The strongest of the four layers.
///
/// Searches the executable image for the serialised key table. A table built at runtime —
/// a `LazyLock` or `OnceLock` calling `getrandom`, say — is stable within a process and
/// would satisfy both the pin and the second-process check if it were seeded identically;
/// it cannot put its 6,248 bytes into `.rodata` at link time.
///
/// Little-endian here, not big-endian: this asserts the *in-memory* representation on the
/// host, which is the thing a compile-time table has and a runtime one does not. The
/// big-endian form is the digest's serialisation and is asserted separately.
#[test]
fn the_keys_are_baked_into_the_executable_image() {
    let exe = std::env::current_exe().expect("a test binary knows its own path");
    let image = std::fs::read(&exe).expect("reading this test binary");

    let mut needle = Vec::with_capacity(KEY_COUNT * 8);
    for key in zobrist::table() {
        needle.extend_from_slice(&key.get().to_ne_bytes());
    }

    let found = image
        .windows(needle.len())
        .any(|window| window == needle.as_slice());
    assert!(
        found,
        "the 781-key table is not present verbatim in {}; a table assembled at runtime \
         cannot be in the image, which is the point of this assertion",
        exe.display()
    );
}

/// Pins the digest's serialisation, not just its value.
///
/// Every runner this project has is x86-64, so a native-endian digest would agree with a
/// big-endian one nowhere and disagree with it visibly nowhere either. This asserts the two
/// serialisations genuinely differ, so the choice recorded in D-0018 is a choice and not a
/// coincidence.
#[test]
fn the_digest_serialisation_is_big_endian_and_that_matters() {
    let mut native = Vec::with_capacity(KEY_COUNT * 8);
    for key in zobrist::table() {
        native.extend_from_slice(&key.get().to_ne_bytes());
    }
    let native_digest = sha256_hex(&native);
    if cfg!(target_endian = "little") {
        assert_ne!(
            native_digest, ZOBRIST_SHA256,
            "on a little-endian host the native-endian digest must differ from the pinned \
             big-endian one, or the serialisation choice is untested"
        );
    }
    assert_eq!(digest(), ZOBRIST_SHA256);
}
