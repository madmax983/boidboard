//! Cycle 4, differential: every legal root move applied by this crate agrees with
//! Stockfish about the resulting position.
//!
//! The unit tests in `board_apply.rs` assert the rules this project chose to write down.
//! This asserts them against an implementation this project did not write, over every legal
//! move of several positions at once — so a rule nobody thought to test is still covered.
//!
//! It is the **only external oracle the halfmove clock will ever have**. Perft cannot see
//! the clock at any depth, and neither can any count-based check, so without this the clock
//! is verified only against assertions written by the same person who wrote the code.
//!
//! # The one field that is deliberately not compared
//!
//! Stockfish uses the **opposite en-passant convention**. It records the square only when
//! an en-passant capture is actually available, and drops it otherwise; issue #4 mandates
//! setting it after every double push regardless (D-0020). Measured here:
//!
//! ```text
//! position startpos moves e2e4   ->  Stockfish `d` prints "... b KQkq - 0 1"
//! this crate                     ->  "... b KQkq e3 0 1"
//! ```
//!
//! So five of the six FEN fields are compared, and the divergence is asserted *positively*
//! by [`the_en_passant_divergence_from_stockfish_is_the_declared_one`] rather than passed
//! over in silence. Perft counts are unaffected: the conventions differ only in whether a
//! legally unusable target is recorded, never in which moves exist.
//!
//! # Skipping
//!
//! Without Stockfish these tests skip loudly, so `cargo test --workspace` passes on a clean
//! checkout (D-0015). CI sets `BOIDBOARD_REQUIRE_STOCKFISH=1`, which turns the skip into a
//! failure.

use std::io::Write;
use std::process::{Command, Stdio};

use boid_board::board::Board;
use boid_board::moves::Move;
use boid_board::perft::engine::StockfishEngine;

/// Positions chosen to exercise the rules that have no other external check: castling
/// rights on both sides, en passant available, promotions on the next move, and a
/// non-trivial halfmove clock.
const POSITIONS: [(&str, &str); 6] = [
    ("startpos", Board::STARTPOS_FEN),
    (
        "kiwipete",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    ),
    ("position3", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"),
    (
        "position4",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    ),
    (
        "position5",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    ),
    // A high halfmove clock, so the increment rule is checked away from zero.
    (
        "clock",
        "r3k2r/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R3K2R w KQkq - 17 30",
    ),
];

/// Returns the engine, or `None` if it is absent and absence is tolerated here.
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

/// Run one Stockfish process over a script, returning its stdout.
fn ask(engine: &StockfishEngine, script: &str) -> String {
    let mut child = Command::new(engine.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawning Stockfish");
    {
        let mut stdin = child.stdin.take().expect("Stockfish stdin");
        stdin
            .write_all(script.as_bytes())
            .expect("writing to Stockfish");
    }
    let output = child.wait_with_output().expect("waiting for Stockfish");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Stockfish's legal root moves for `fen`, from `go perft 1`.
fn legal_root_moves(engine: &StockfishEngine, fen: &str) -> Vec<String> {
    let out = ask(engine, &format!("position fen {fen}\ngo perft 1\nquit\n"));
    out.lines()
        .filter_map(|line| {
            let (mv, count) = line.trim().split_once(": ")?;
            // `go perft 1` prints "<move>: 1" per root move, then "Nodes searched: N".
            if mv == "Nodes searched" || count.trim() != "1" {
                return None;
            }
            let ok = (4..=5).contains(&mv.len()) && mv.bytes().all(|b| b.is_ascii_alphanumeric());
            ok.then(|| mv.to_owned())
        })
        .collect()
}

/// All but the en-passant field, which the two implementations deliberately disagree on.
fn fields_except_en_passant(fen: &str) -> Vec<&str> {
    let fields: Vec<&str> = fen.split(' ').collect();
    assert_eq!(fields.len(), 6, "expected a six-field FEN, got {fen:?}");
    vec![fields[0], fields[1], fields[2], fields[4], fields[5]]
}

/// Applying every legal root move agrees with Stockfish on five of the six FEN fields.
///
/// This covers, end to end and against an outside implementation: piece placement after
/// every move kind, castling-right revocation by move and by capture, promotion piece
/// placement, the side-to-move flip, the halfmove clock's reset and increment rules, and
/// the fullmove increment.
#[test]
fn applying_every_legal_root_move_agrees_with_stockfish() {
    let Some(engine) = engine_or_skip("applying_every_legal_root_move_agrees_with_stockfish")
    else {
        return;
    };

    let mut compared = 0usize;
    for (id, fen) in POSITIONS {
        let board = Board::from_fen(fen).unwrap_or_else(|e| panic!("{id}: {e}"));
        let moves = legal_root_moves(&engine, fen);
        assert!(
            moves.len() >= 5,
            "{id}: Stockfish reported only {} root moves",
            moves.len()
        );

        // One process for the whole position rather than one per move: 20-plus process
        // spawns per position would dominate the runtime entirely.
        let mut script = String::new();
        for mv in &moves {
            script.push_str(&format!("position fen {fen} moves {mv}\nd\n"));
        }
        script.push_str("quit\n");
        let theirs: Vec<String> = ask(&engine, &script)
            .lines()
            .filter_map(|line| line.trim().strip_prefix("Fen: ").map(str::to_owned))
            .collect();
        assert_eq!(
            theirs.len(),
            moves.len(),
            "{id}: Stockfish printed {} positions for {} moves",
            theirs.len(),
            moves.len()
        );

        for (text, theirs) in moves.iter().zip(theirs) {
            let mv = Move::from_uci(text, &board).unwrap_or_else(|e| {
                panic!("{id}: Stockfish's legal move {text} did not parse: {e}")
            });
            let ours = board
                .try_apply_move(mv)
                .unwrap_or_else(|e| panic!("{id}: Stockfish's legal move {text} was rejected: {e}"))
                .to_fen();

            assert_eq!(
                fields_except_en_passant(&ours),
                fields_except_en_passant(&theirs),
                "{id}: after {text}\n  ours:   {ours}\n  theirs: {theirs}"
            );
            compared += 1;
        }
    }

    // Exact, not a floor: D-0028's register quotes this number, so it has to be pinned
    // rather than bounded. A change here means the fixture or the move rules moved.
    assert_eq!(
        compared, 179,
        "the six fixture positions have 179 legal root moves between them"
    );
    eprintln!("compared {compared} root moves against Stockfish");
}

/// Every move Stockfish calls legal is structurally applicable here.
///
/// The converse of the test above, and a real constraint on issue #5: a `try_apply_move`
/// that rejected legal moves would make the move generator look broken.
#[test]
fn stockfish_legal_moves_are_all_structurally_applicable() {
    let Some(engine) = engine_or_skip("stockfish_legal_moves_are_all_structurally_applicable")
    else {
        return;
    };

    for (id, fen) in POSITIONS {
        let board = Board::from_fen(fen).unwrap_or_else(|e| panic!("{id}: {e}"));
        for text in legal_root_moves(&engine, fen) {
            let mv = Move::from_uci(&text, &board)
                .unwrap_or_else(|e| panic!("{id}: {text} did not parse: {e}"));
            assert!(
                board.try_apply_move(mv).is_ok(),
                "{id}: {text} is legal for Stockfish but not applicable here: {:?}",
                board.try_apply_move(mv)
            );
            // And the UCI spelling round-trips, which is what issue #6's divide comparison
            // will rest on.
            assert_eq!(mv.to_uci(), text, "{id}: UCI round trip");
        }
    }
}

/// The declared divergence, asserted rather than assumed.
///
/// If a future Stockfish adopted this project's convention, this test would fail and the
/// exclusion above would become dead weight that nobody would notice. Asserting the
/// divergence positively is what keeps the exclusion honest.
#[test]
fn the_en_passant_divergence_from_stockfish_is_the_declared_one() {
    let Some(engine) =
        engine_or_skip("the_en_passant_divergence_from_stockfish_is_the_declared_one")
    else {
        return;
    };

    // A double push with no enemy pawn able to capture: we record the file, they do not.
    let board = Board::from_fen(Board::STARTPOS_FEN).expect("the start position parses");
    let mv = Move::from_uci("e2e4", &board).expect("e2e4 is a move");
    let ours = board
        .try_apply_move(mv)
        .expect("e2e4 is applicable")
        .to_fen();
    let theirs = ask(&engine, "position startpos moves e2e4\nd\nquit\n")
        .lines()
        .find_map(|line| line.trim().strip_prefix("Fen: ").map(str::to_owned))
        .expect("Stockfish prints a FEN");

    assert!(
        ours.contains(" e3 "),
        "this project records the en-passant file after every double push (D-0020): {ours}"
    );
    assert!(
        theirs.contains(" - "),
        "Stockfish drops an en-passant square no capture can use; if this fails, the \
         conventions have converged and the exclusion in this file is no longer needed: \
         {theirs}"
    );
    assert_eq!(
        fields_except_en_passant(&ours),
        fields_except_en_passant(&theirs),
        "the two conventions must differ in the en-passant field and nowhere else"
    );

    // And where a capture IS available, the two agree exactly, ep field included.
    let fen = "rnbqkbnr/ppp1pppp/8/8/3p4/8/PPPPPPPP/RNBQKBNR w KQkq - 0 3";
    let board = Board::from_fen(fen).expect("parses");
    let mv = Move::from_uci("e2e4", &board).expect("e2e4 is a move");
    let ours = board.try_apply_move(mv).expect("applicable").to_fen();
    let theirs = ask(
        &engine,
        &format!("position fen {fen} moves e2e4\nd\nquit\n"),
    )
    .lines()
    .find_map(|line| line.trim().strip_prefix("Fen: ").map(str::to_owned))
    .expect("Stockfish prints a FEN");
    assert_eq!(
        ours, theirs,
        "with an en-passant capture available the conventions coincide exactly"
    );
}
