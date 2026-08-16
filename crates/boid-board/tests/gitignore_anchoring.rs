//! Cycle 4: build-artifact ignore patterns must be directory-anchored.
//!
//! Issue #3 AC7 asks that "the stale `target/` directory from the prior attempt is removed
//! and gitignored". No such directory ever existed — not in the working tree, and not at
//! any ref in this repository's history, including the archived attempt's 79-file tree. So
//! the "removed" half is vacuous, and is reported as satisfied by invariant rather than as
//! work performed (`docs/DECISIONS.md` D-0005).
//!
//! The "gitignored" half contains a real defect, though. The inherited patterns were the
//! slashless `target` and `debug`. An unanchored gitignore pattern matches **files as well
//! as directories, at any depth** — so it would also have silently ignored a future
//! `crates/boid-board/src/target/mod.rs` or `docs/debug/`. In a chess engine, `target` and
//! `debug` are entirely plausible source-tree names: a search has a target square, and a
//! debug module is a debug module.
//!
//! That is the latent form of the bug AC7 gestures at, and it is what this test pins.
//!
//! Deliberately a pure file read rather than a `git` subprocess: a git-dependent test
//! either fails under a shallow CI clone or, worse, passes vacuously behind a "no git
//! metadata, return early" guard. The *behavioural* half of this check — that
//! `git check-ignore` agrees — lives in the `repo-invariants` CI job, where a full clone
//! is guaranteed.

/// The repository's `.gitignore`, embedded at compile time so that this test cannot pass
/// by failing to find the file.
const GITIGNORE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.gitignore"));

/// Build-artifact directory names that must be anchored to the repository root.
const ARTIFACT_DIRS: [&str; 2] = ["target", "debug"];

#[test]
fn build_artifact_patterns_are_root_anchored() {
    let lines: Vec<&str> = GITIGNORE
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    for dir in ARTIFACT_DIRS {
        // Measured with `git check-ignore` against a scratch repository:
        //   "target"   ignores any file OR directory named target, at any depth
        //   "target/"  ignores any DIRECTORY named target, at any depth -- still shadows
        //              crates/boid-board/src/target/mod.rs
        //   "/target/" ignores only the root build directory
        // A trailing slash alone is not sufficient; the leading slash is what does the work.
        for insufficient in [dir.to_owned(), format!("{dir}/")] {
            assert!(
                !lines.contains(&insufficient.as_str()),
                "`.gitignore` contains {insufficient:?}, which still matches \
                 `crates/boid-board/src/{dir}/mod.rs` at any depth. Use \"/{dir}/\"."
            );
        }
        assert!(
            lines.contains(&format!("/{dir}/").as_str()),
            "`.gitignore` should ignore the root build directory with \"/{dir}/\""
        );
    }
}

#[test]
fn gitignore_does_not_ignore_the_perft_oracle() {
    // The fixture lives under `tests/`, a directory name that appears in plenty of
    // stock ignore templates. Losing the oracle to a stray pattern would be quiet and
    // catastrophic.
    let lines: Vec<&str> = GITIGNORE
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    for forbidden in ["tests", "tests/", "fixtures", "fixtures/", "*.txt"] {
        assert!(
            !lines.contains(&forbidden),
            "`.gitignore` contains {forbidden:?}, which would exclude the perft oracle"
        );
    }
}
