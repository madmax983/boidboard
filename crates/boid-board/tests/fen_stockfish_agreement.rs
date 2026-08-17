//! Does a third party agree that our FEN describes the position we think it does?
//!
//! Every other test of AC1 in this suite is self-referential in one specific way: a
//! **bijection applied symmetrically to the parser and the emitter round-trips byte for
//! byte, forever**. Mirror the ranks in both, and every fixture FEN still round-trips. Invert
//! the colours in both, swap the files, reverse the castling bit order — all of them survive
//! every string comparison this project can make on its own.
//!
//! Literal square assertions catch some of that, and they are in `fen_roundtrip.rs`. This
//! catches the class, by asking a different engine to count.
//!
//! The method: take a FEN out of the SHA-pinned oracle fixture, parse it with our code, emit
//! it with our code, hand *the emitted string* to Stockfish, and compare its `perft 1` with
//! the fixture's published depth-1 count. Any of the mirrors above changes at least one of
//! those seven numbers. It is D-0006's argument for committing `position4-mirror` — that no
//! count-based test can distinguish a mirrored board — arriving one phase early, and it is
//! also a rehearsal of issue #6's differential harness with the move generator still absent.
//!
//! Depth 1 only. Deeper is issue #6's job and would be paying for move generation we have
//! not written; depth 1 is the cheapest question whose answer depends on the whole board.

use boid_board::board::Board;
use boid_board::perft::engine::{PerftEngine, StockfishEngine};
use boid_board::perft::oracle::{self, ORACLE_TEXT};

/// Returns the engine, or `None` if it is absent and absence is tolerated here.
///
/// The same posture `stockfish_differential.rs` takes, for the same reason (D-0015):
/// `cargo test --workspace` must pass on a clean checkout, while CI sets
/// `BOIDBOARD_REQUIRE_STOCKFISH=1` so the coverage cannot silently evaporate on the runner.
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
    eprintln!("SKIPPED {test}: no Stockfish at {:?}.", engine.path());
    None
}

#[test]
fn stockfish_agrees_with_our_emitted_fen_at_depth_one() {
    let Some(engine) = engine_or_skip("stockfish_agrees_with_our_emitted_fen_at_depth_one") else {
        return;
    };

    let cases = oracle::parse(ORACLE_TEXT).expect("the committed fixture must parse");
    let mut checked = 0usize;

    for case in &cases {
        let Some(published) = case.nodes_at(1) else {
            continue;
        };

        // Round-trip through our own code first. What Stockfish is asked about is the FEN
        // WE produced, not the one the fixture holds — otherwise the emitter is not on
        // trial at all.
        let (board, layout) =
            Board::from_fen_with_layout(case.fen).unwrap_or_else(|e| panic!("{}: {e}", case.id));
        let ours = board.to_fen_with_layout(layout);
        assert_eq!(ours, case.fen, "{}: round-trip", case.id);

        let counted = engine
            .perft(&ours, 1)
            .unwrap_or_else(|e| panic!("{}: {e}", case.id));
        assert_eq!(
            counted, published,
            "{}: Stockfish counts {counted} legal moves in the position our FEN describes, \
             but the fixture publishes {published}. The string round-trips, so this is not \
             a FEN bug -- it is the board being mirrored, inverted or transposed in the \
             parser and the emitter together.",
            case.id
        );
        checked += 1;

        // And the six-field form of the same position must describe the same position:
        // Stockfish accepts both spellings, so this catches a counters-only divergence.
        let six = board.to_fen();
        assert_eq!(
            engine
                .perft(&six, 1)
                .unwrap_or_else(|e| panic!("{}: {e}", case.id)),
            published,
            "{}: the four- and six-field spellings disagree",
            case.id
        );
    }

    assert_eq!(
        checked, 7,
        "every fixture row publishes a depth-1 count, so all seven must have been checked"
    );
    eprintln!("checked {checked} positions against Stockfish at depth 1");
}

#[test]
fn stockfish_and_this_crate_disagree_about_dead_en_passant_squares() {
    // Not a defect on either side, and it is written down here because issue #6 will trip
    // over it. Issue #4 mandates that the en-passant square is recorded after every double
    // push, whether or not a capture is available; Stockfish records it only when an enemy
    // pawn attacks the target, and NORMALISES AN UNCAPTURABLE ONE AWAY ON INPUT (D-0021).
    //
    // The divergence is invisible to perft -- neither convention adds or removes a move --
    // and visible to every FEN and key comparison. A differential harness that compared
    // emitted FENs with Stockfish's `d` output would fail here for no good reason.
    let Some(engine) =
        engine_or_skip("stockfish_and_this_crate_disagree_about_dead_en_passant_squares")
    else {
        return;
    };

    // After 1.e4 c5 2.e5 d5 the target d6 IS capturable by the e5 pawn.
    let capturable = "rnbqkbnr/pp2pppp/8/2ppP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3";
    // After 1.e4 c5 the target c6 is not: no white pawn is adjacent to it.
    let dead = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq c6 0 2";

    for fen in [capturable, dead] {
        let board = Board::from_fen(fen).expect("both are valid FENs here");
        assert_eq!(board.to_fen(), fen, "we keep the ep square in both cases");
        // Stockfish still counts the same tree, which is why perft cannot see the
        // difference and why issue #6's harness stays valid.
        assert!(
            engine.perft(fen, 1).is_ok(),
            "Stockfish accepts both spellings"
        );
    }

    let with_dead_ep = Board::from_fen(dead).expect("valid");
    let without = Board::from_fen("rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2")
        .expect("valid");
    assert_ne!(
        with_dead_ep.key(),
        without.key(),
        "our convention distinguishes them; Stockfish's would not"
    );
}
