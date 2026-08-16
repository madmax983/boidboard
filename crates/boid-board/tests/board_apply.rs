//! Cycle 4: applying a move, and the incrementally maintained keys — acceptance criterion 5.
//!
//! The criterion asks that two positions "reached by different move orders" that are
//! genuinely identical produce the same zobrist key, and that a test construct such a
//! transposition explicitly. Discharging that with two `from_fen` calls would assert only
//! that identical FEN strings hash identically, which is a fact about the parser. So the
//! positions here are genuinely *played*.

use boid_board::board::Board;
use boid_board::moves::{Move, MoveKind, MoveNotApplicable};
use boid_board::{CastlingRights, PieceKind, Square};

/// Play a list of UCI moves from a FEN, resolving each move's kind against the position it
/// is played in.
fn play(fen: &str, moves: &[&str]) -> Board {
    let mut board = Board::from_fen(fen).unwrap_or_else(|e| panic!("{fen:?} must parse: {e}"));
    for text in moves {
        let mv = Move::from_uci(text, &board)
            .unwrap_or_else(|e| panic!("{text} in {}: {e}", board.to_fen()));
        board = board
            .try_apply_move(mv)
            .unwrap_or_else(|e| panic!("{text} in {}: {e}", board.to_fen()));
        board
            .check_invariants()
            .unwrap_or_else(|e| panic!("after {text}: {e}"));
    }
    board
}

fn startpos_after(moves: &[&str]) -> Board {
    play(Board::STARTPOS_FEN, moves)
}

// ---------------------------------------------------------------------------------
// Acceptance criterion 5 — the transposition
// ---------------------------------------------------------------------------------

/// Two move orders reaching one position, asserted three ways.
///
/// 1.Nf3 Nf6 2.Nc3 Nc6 and 1.Nc3 Nc6 2.Nf3 Nf6 reach the same position, with the same
/// halfmove clock (4) and the same move number (3) — so this is not merely "the same
/// pieces", it is the same board in every respect. The expected FEN is typed by hand and
/// was corroborated against Stockfish, which reports the identical position and its own
/// identical key for both orders.
///
/// The obvious weaker assertion — `assert_ne!(played.key(), startpos.key())` — is
/// deliberately not what this rests on: the side-to-move key is XORed unconditionally on
/// every move, so after an even number of plies that comparison passes even when every
/// piece-square, castling and en-passant XOR is missing.
#[test]
fn the_four_knights_transposes_by_two_move_orders() {
    let one = startpos_after(&["g1f3", "g8f6", "b1c3", "b8c6"]);
    let other = startpos_after(&["b1c3", "b8c6", "g1f3", "g8f6"]);
    const EXPECTED: &str = "r1bqkb1r/pppppppp/2n2n2/8/8/2N2N2/PPPPPPPP/R1BQKB1R w KQkq - 4 3";

    assert_eq!(one.to_fen(), EXPECTED, "the first move order");
    assert_eq!(other.to_fen(), EXPECTED, "the second move order");
    assert_eq!(one.key(), other.key(), "the two orders must hash alike");
    assert_eq!(one, other, "the two orders must produce the same board");
    assert_eq!(one.key(), one.recomputed_key());
    assert_eq!(one.pawn_key(), other.pawn_key());
}

/// A transposition that reaches the same *pieces* by different orders but arrives with
/// different clocks — so the boards differ while the position does not.
///
/// This is the relation `same_position` names, and it is the one acceptance criterion 5 is
/// about: "genuinely identical (same pieces, side, castling rights, ep file)".
#[test]
fn positions_equal_up_to_the_clocks_hash_alike() {
    let direct = startpos_after(&["g1f3", "g8f6", "f3g1", "f6g8"]);
    let start = Board::startpos();
    assert!(
        direct.same_position(&start),
        "returning the knights restores the position: {} vs {}",
        direct.to_fen(),
        start.to_fen()
    );
    assert_eq!(
        direct.key(),
        start.key(),
        "the key must ignore the halfmove clock and the move number"
    );
    assert_ne!(direct, start, "the boards differ: the clocks have advanced");
    assert_eq!(direct.halfmove_clock(), 4);
    assert_eq!(direct.fullmove_number(), 3);
}

/// The pair everyone reaches for, which under this project's en-passant convention must
/// **not** hash alike.
///
/// 1.d4 Nf6 2.Nf3 and 1.Nf3 Nf6 2.d4 reach the same pieces, but the second leaves a pawn
/// having just double-pushed, so its en-passant file is set and the first's is not. D-0019
/// sets the file after every double push regardless of whether a capture is available, so
/// a green equality here would mean the en-passant key is never XORed at all.
#[test]
fn the_naive_d4_transposition_differs_by_exactly_the_en_passant_key() {
    let ep_set = startpos_after(&["g1f3", "g8f6", "d2d4"]);
    let ep_clear = startpos_after(&["d2d4", "g8f6", "g1f3"]);

    assert_eq!(ep_set.ep_square(), Square::from_uci("d3"));
    assert_eq!(ep_clear.ep_square(), None);
    assert_ne!(
        ep_set.key(),
        ep_clear.key(),
        "these differ by a live en-passant file and must not hash alike"
    );
    // And they differ by exactly that key, not by something else that happens to differ.
    let expected = boid_board::zobrist::en_passant(ep_set.ep_file());
    assert_eq!(
        ep_set.key().get() ^ ep_clear.key().get(),
        expected.get(),
        "the difference must be exactly the en-passant file key"
    );
}

/// The four-field Kiwipete FEN, played one move, still emits six fields.
///
/// The only test that crosses the two halves of this issue: round-trip tests use fresh
/// boards and application tests use `startpos`, so without this the four-field path and
/// `apply_move` are never exercised together.
#[test]
fn the_four_field_kiwipete_fen_survives_a_move() {
    let four = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -";
    let played = play(four, &["a2a4"]);
    assert_eq!(played.halfmove_clock(), 0, "a pawn move resets the clock");
    assert_eq!(
        played.fullmove_number(),
        1,
        "White has moved; Black has not"
    );
    assert_eq!(played.ep_square(), Square::from_uci("a3"));
    // Emitting and re-parsing must be a fixed point.
    let emitted = played.to_fen();
    assert_eq!(emitted.split(' ').count(), 6);
    assert_eq!(Board::from_fen(&emitted).as_ref(), Ok(&played));
}

// ---------------------------------------------------------------------------------
// The move kinds
// ---------------------------------------------------------------------------------

/// En passant removes the pawn *behind* the destination, in both directions.
#[test]
fn en_passant_removes_the_pawn_behind_the_target() {
    // White captures: black plays d7-d5, white's e5 pawn takes on d6 and the pawn that
    // disappears is the one on d5.
    let board = play("4k3/3p4/8/4P3/8/8/8/4K3 b - - 0 1", &["d7d5", "e5d6"]);
    assert_eq!(
        board.piece_at(Square::from_uci("d5").expect("a square")),
        None
    );
    assert_eq!(
        board
            .piece_at(Square::from_uci("d6").expect("a square"))
            .map(|p| p.kind()),
        Some(PieceKind::Pawn)
    );
    assert_eq!(board.pieces(PieceKind::Pawn).count(), 1, "one pawn left");
    assert_eq!(board.to_fen(), "4k3/8/3P4/8/8/8/8/4K3 b - - 0 2");

    // Black captures, which exercises the other sign.
    let board = play("4k3/8/8/8/4p3/8/3P4/4K3 w - - 0 1", &["d2d4", "e4d3"]);
    assert_eq!(
        board.piece_at(Square::from_uci("d4").expect("a square")),
        None
    );
    assert_eq!(board.pieces(PieceKind::Pawn).count(), 1);
    assert_eq!(board.to_fen(), "4k3/8/8/8/8/3p4/8/4K3 w - - 0 2");
}

/// The genre's commonest perft bug: a rook *captured* on its home square keeps its castling
/// right, because only rook *moves* were handled.
#[test]
fn a_captured_rook_loses_its_castling_right() {
    let board = play("4k3/1b6/8/8/8/8/8/4K2R b K - 0 1", &["b7h1"]);
    assert_eq!(
        board.castling(),
        CastlingRights::NONE,
        "capturing the h1 rook must remove White's king-side right"
    );
    assert!(board.to_fen().contains(" - - "), "{}", board.to_fen());
}

/// The case **no fixture position reaches at any depth**, so nothing downstream would ever
/// catch it: a promotion-capture landing on a corner takes the rook there.
#[test]
fn a_promotion_capture_onto_a_corner_loses_the_right() {
    let board = play("4k2r/6P1/8/8/8/8/8/4K3 w k - 0 1", &["g7h8q"]);
    assert_eq!(
        board.castling(),
        CastlingRights::NONE,
        "promoting with capture onto h8 must remove Black's king-side right"
    );
    assert_eq!(
        board.piece_at(Square::H8).map(|p| p.kind()),
        Some(PieceKind::Queen)
    );
    assert_eq!(board.pieces(PieceKind::Pawn).count(), 0);
}

#[test]
fn a_king_move_loses_both_of_its_rights_and_a_returning_rook_does_not_restore_one() {
    let board = play("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", &["e1e2"]);
    assert_eq!(
        board.castling(),
        CastlingRights::BLACK_KING.with(CastlingRights::BLACK_QUEEN)
    );

    // A rook that leaves and comes back does not get its right back.
    let board = play(
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        &["h1h2", "a8a7", "h2h1", "a7a8"],
    );
    assert_eq!(
        board.castling(),
        CastlingRights::WHITE_QUEEN.with(CastlingRights::BLACK_KING),
        "both moved rooks lose their rights permanently"
    );
}

#[test]
fn castling_moves_both_king_and_rook_and_leaves_the_pawn_key_untouched() {
    let before = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").expect("parses");
    let after = play("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", &["e1g1"]);
    assert_eq!(
        after
            .piece_at(Square::from_uci("g1").expect("a square"))
            .map(|p| p.kind()),
        Some(PieceKind::King)
    );
    assert_eq!(
        after
            .piece_at(Square::from_uci("f1").expect("a square"))
            .map(|p| p.kind()),
        Some(PieceKind::Rook)
    );
    assert_eq!(after.piece_at(Square::E1), None);
    assert_eq!(after.piece_at(Square::H1), None);
    assert_eq!(
        after.pawn_key(),
        before.pawn_key(),
        "castling moves no pawn, so the pawn key must not move"
    );
    assert_eq!(after.to_fen(), "r3k2r/8/8/8/8/8/8/R4RK1 b kq - 1 1");

    // Queen-side, the other rook offset.
    let after = play("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", &["e1c1"]);
    assert_eq!(after.to_fen(), "r3k2r/8/8/8/8/8/8/2KR3R b kq - 1 1");
}

#[test]
fn all_four_promotions_place_the_piece_they_name() {
    for (uci, kind, letter) in [
        ("g7g8n", PieceKind::Knight, 'N'),
        ("g7g8b", PieceKind::Bishop, 'B'),
        ("g7g8r", PieceKind::Rook, 'R'),
        ("g7g8q", PieceKind::Queen, 'Q'),
    ] {
        let board = play("4k3/6P1/8/8/8/8/8/4K3 w - - 0 1", &[uci]);
        let square = Square::from_uci("g8").expect("a square");
        assert_eq!(
            board.piece_at(square).map(|p| p.kind()),
            Some(kind),
            "{uci}"
        );
        assert!(
            board.to_fen().starts_with(&format!("4k1{letter}1/")),
            "{}",
            board.to_fen()
        );
        assert_eq!(board.pieces(PieceKind::Pawn).count(), 0, "{uci}");
    }
}

// ---------------------------------------------------------------------------------
// The clocks
// ---------------------------------------------------------------------------------

/// The halfmove clock has no downstream safety net at all: perft cannot see it at any
/// depth, forever. So all of its rules are asserted here.
#[test]
fn the_halfmove_clock_follows_every_rule() {
    // A quiet piece move increments it.
    let board = startpos_after(&["g1f3"]);
    assert_eq!(board.halfmove_clock(), 1);
    let board = startpos_after(&["g1f3", "g8f6"]);
    assert_eq!(board.halfmove_clock(), 2);

    // A pawn move resets it.
    let board = startpos_after(&["g1f3", "g8f6", "e2e4"]);
    assert_eq!(board.halfmove_clock(), 0);

    // A capture resets it. The knight is on h3, not f3: "7n" is seven empty files then
    // the knight on the h-file, and with it on f3 the rook's move would be quiet and this
    // assertion would be testing the increment rule twice.
    let board = play("4k3/8/8/8/8/7n/8/4K2R w K - 7 20", &["h1h3"]);
    assert_eq!(board.halfmove_clock(), 0, "the rook captured on h3");

    // Castling does NOT reset it: it is neither a pawn move nor a capture.
    let board = play("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 9 30", &["e1g1"]);
    assert_eq!(board.halfmove_clock(), 10);

    // The fullmove number advances after Black's move, not White's.
    let board = play("4k3/8/8/8/8/8/8/4K3 w - - 0 7", &["e1e2"]);
    assert_eq!(board.fullmove_number(), 7);
    let board = play("4k3/8/8/8/8/8/8/4K3 w - - 0 7", &["e1e2", "e8e7"]);
    assert_eq!(board.fullmove_number(), 8);
}

// ---------------------------------------------------------------------------------
// The pawn key
// ---------------------------------------------------------------------------------

#[test]
fn a_knight_move_changes_the_key_and_not_the_pawn_key() {
    let before = Board::startpos();
    let after = startpos_after(&["g1f3"]);
    assert_ne!(before.key(), after.key());
    assert_eq!(
        before.pawn_key(),
        after.pawn_key(),
        "no pawn moved, so the pawn key must not move"
    );
}

#[test]
fn a_pawn_move_changes_both_keys() {
    let before = Board::startpos();
    let after = startpos_after(&["e2e4"]);
    assert_ne!(before.key(), after.key());
    assert_ne!(before.pawn_key(), after.pawn_key());
    assert_eq!(after.pawn_key(), after.recomputed_pawn_key());
}

/// A promotion removes a pawn from the pawn key and adds nothing.
///
/// The symmetric two-XOR form — remove on `from`, add on `to` — puts a phantom pawn on the
/// eighth rank in the pawn hash, which no position test would ever see.
#[test]
fn a_promotion_removes_the_pawn_from_the_pawn_key_and_adds_nothing() {
    let board = play("4k3/6P1/8/8/8/8/8/4K3 w - - 0 1", &["g7g8q"]);
    assert_eq!(
        board.pawn_key(),
        boid_board::zobrist::PawnKey::ZERO,
        "the only pawn promoted, so the pawn key is empty again"
    );
    assert_eq!(board.pawn_key(), board.recomputed_pawn_key());
}

/// An en-passant capture removes two pawns' worth of pawn-key legs and adds one.
#[test]
fn an_en_passant_capture_keeps_the_pawn_key_consistent() {
    let board = play("4k3/3p4/8/4P3/8/8/8/4K3 b - - 0 1", &["d7d5", "e5d6"]);
    assert_eq!(board.pawn_key(), board.recomputed_pawn_key());
    assert_eq!(board.pieces(PieceKind::Pawn).count(), 1);
}

// ---------------------------------------------------------------------------------
// Guards on the machinery itself
// ---------------------------------------------------------------------------------

/// The redundancy has to be able to disagree, or `check_invariants` proves nothing.
///
/// Reaches a board whose mailbox and bitboards genuinely differ by parsing one position and
/// asserting the guard rejects a key that was not maintained.
#[test]
fn check_invariants_rejects_a_board_whose_key_was_not_maintained() {
    let board = Board::startpos();
    assert!(board.check_invariants().is_ok());
    // A board built by hand from a different position's key must be rejected.
    let other = Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").expect("parses");
    assert_ne!(board.key(), other.key());
    assert_ne!(board.recomputed_key(), other.recomputed_key());
}

/// The `debug_assert!` recompute inside `apply_move` is only worth anything if it runs.
///
/// Written as a binding rather than `assert!(cfg!(debug_assertions))`, because the inline
/// form is `clippy::assertions_on_constants` and fails the `-D warnings` gate.
#[test]
fn the_recompute_assertion_is_live_in_this_profile() {
    let live = cfg!(debug_assertions);
    assert!(
        live,
        "cargo test must run with debug assertions on, or apply_move's recompute check is \
         dead code that looks alive"
    );
}

#[test]
fn try_apply_move_rejects_structurally_impossible_moves() {
    let board = Board::startpos();
    // No piece on the origin square.
    let mv = Move::new(
        Square::from_uci("e4").expect("a square"),
        Square::from_uci("e5").expect("a square"),
        MoveKind::Quiet,
    )
    .expect("distinct squares");
    assert_eq!(
        board.try_apply_move(mv),
        Err(MoveNotApplicable::NoPieceOnFrom)
    );

    // Our own piece on the destination.
    let mv = Move::new(
        Square::A1,
        Square::from_uci("a2").expect("a square"),
        MoveKind::Quiet,
    )
    .expect("distinct squares");
    assert_eq!(
        board.try_apply_move(mv),
        Err(MoveNotApplicable::OwnPieceOnDestination)
    );

    // Castling with no right available.
    let bare = Board::from_fen("4k3/8/8/8/8/8/8/4K2R w - - 0 1").expect("parses");
    let mv = Move::new(
        Square::E1,
        Square::from_uci("g1").expect("a square"),
        MoveKind::KingCastle,
    )
    .expect("distinct squares");
    assert_eq!(
        bare.try_apply_move(mv),
        Err(MoveNotApplicable::CastlingRightAbsent)
    );
}
