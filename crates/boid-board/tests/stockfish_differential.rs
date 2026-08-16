//! Cycle 3: replay the oracle against a real engine.
//!
//! This is the third and strongest layer of defence for the fixture. The pinned hash proves
//! the file has not changed; the second transcription proves two humans-plus-a-program read
//! the same page the same way; this proves the numbers are *true*.
//!
//! It is also issue #6's differential harness, arriving a phase early with our own engine
//! side still absent. When `boid-board` can generate moves, the same comparison runs with
//! two implementations of [`PerftEngine`] instead of one.
//!
//! # Budget
//!
//! Only counts flagged `Verified` and no larger than `BOID_PERFT_MAX_NODES` (default
//! 5,000,000) are replayed. Budgeting by node count rather than by depth is what keeps the
//! policy correct as positions are added: `position3` depth 5 is 674,624 nodes while
//! `kiwipete` depth 4 is 4,085,603.
//!
//! # Skipping
//!
//! Without Stockfish these tests skip loudly, so that `cargo test --workspace` passes on a
//! clean checkout as issue #3's AC1 requires. CI sets `BOIDBOARD_REQUIRE_STOCKFISH=1`,
//! which turns the skip into a failure so the coverage cannot silently evaporate.

use boid_board::perft::engine::{EngineError, PerftEngine, StockfishEngine};
use boid_board::perft::oracle::{ORACLE_TEXT, parse};

/// Default replay ceiling: about 16 million nodes across the whole fixture, well under a
/// second against Stockfish.
const DEFAULT_MAX_NODES: u64 = 5_000_000;

fn max_nodes() -> u64 {
    std::env::var("BOID_PERFT_MAX_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_NODES)
}

/// Returns the engine, or `None` if it is absent and absence is tolerated here.
///
/// Panics instead of returning `None` when `BOIDBOARD_REQUIRE_STOCKFISH=1`.
fn engine_or_skip(test: &str) -> Option<StockfishEngine> {
    let engine = StockfishEngine::from_env();
    if engine.is_available() {
        return Some(engine);
    }
    assert!(
        std::env::var("BOIDBOARD_REQUIRE_STOCKFISH").as_deref() != Ok("1"),
        "BOIDBOARD_REQUIRE_STOCKFISH=1 but no Stockfish binary was found at {:?}. \
         Run scripts/setup-stockfish.sh.",
        engine.path()
    );
    eprintln!(
        "SKIPPED {test}: no Stockfish at {:?}. Run scripts/setup-stockfish.sh to enable \
         the differential tests.",
        engine.path()
    );
    None
}

#[test]
fn every_verified_row_within_budget_matches_the_engine() {
    let Some(engine) = engine_or_skip("every_verified_row_within_budget_matches_the_engine") else {
        return;
    };
    let budget = max_nodes();
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");

    let mut checked = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    for case in &cases {
        for count in case.verified_counts() {
            if count.nodes > budget || count.depth < 1 {
                continue;
            }
            let actual = engine
                .perft(case.fen, count.depth)
                .unwrap_or_else(|e| panic!("{} depth {}: {e}", case.id, count.depth));
            if actual != count.nodes {
                mismatches.push(format!(
                    "{} depth {}: fixture says {}, engine says {actual}",
                    case.id, count.depth, count.nodes
                ));
            }
            checked += 1;
        }
    }

    assert!(
        mismatches.is_empty(),
        "the oracle disagrees with Stockfish:\n  {}",
        mismatches.join("\n  ")
    );
    assert!(
        checked >= 25,
        "expected to replay at least 25 rows within the {budget}-node budget, replayed {checked}"
    );
    eprintln!("replayed {checked} verified rows within a {budget}-node budget");
}

#[test]
fn engine_rejects_depth_zero() {
    let Some(engine) = engine_or_skip("engine_rejects_depth_zero") else {
        return;
    };
    // `go perft 0` prints no "Nodes searched:" line at all -- it falls through to a real
    // search. A lenient driver would return None here and the row would be silently
    // "skipped" forever.
    let e = engine
        .perft(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            0,
        )
        .expect_err("depth 0 must be rejected before the engine is spawned");
    assert!(
        matches!(e, EngineError::DepthTooLow(0)),
        "expected DepthTooLow, got {e:?}"
    );
}

#[test]
fn engine_rejects_non_ascii_fen() {
    let Some(engine) = engine_or_skip("engine_rejects_non_ascii_fen") else {
        return;
    };
    // The exact contamination D-0008 guards the fixture against: Stockfish would parse
    // this into a different legal position and report 0 nodes rather than failing.
    let nbsp_fen =
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R\u{a0}w\u{a0}KQkq\u{a0}-";
    let e = engine
        .perft(nbsp_fen, 3)
        .expect_err("a non-ASCII FEN must be rejected, not silently mis-parsed");
    assert!(
        matches!(e, EngineError::NonAsciiFen(_)),
        "expected NonAsciiFen, got {e:?}"
    );
}

#[test]
fn engine_is_not_a_stub() {
    let Some(engine) = engine_or_skip("engine_is_not_a_stub") else {
        return;
    };
    // A value a hardcoded stub could not know, at a depth that takes measurable time.
    // If this passes instantly with the right answer, the driver is not running Stockfish.
    let kiwipete = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -";
    assert_eq!(
        engine.perft(kiwipete, 5).expect("kiwipete depth 5"),
        193_690_690
    );
}

#[test]
fn ac5_command_reports_8902() {
    let Some(engine) = engine_or_skip("ac5_command_reports_8902") else {
        return;
    };
    // Issue #3 AC5, as a test rather than a transcript.
    assert_eq!(
        engine
            .perft(
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                3
            )
            .expect("startpos depth 3"),
        8902
    );
}
