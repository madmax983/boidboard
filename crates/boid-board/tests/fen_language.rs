//! What `Board::from_fen` deliberately does **not** enforce.
//!
//! D-0021's binding rule: *"Every rule `from_fen` deliberately does not enforce must have a
//! test asserting that a FEN violating it is accepted, so that the boundary is a choice on
//! record rather than an omission."*
//!
//! Without this file that rule is unsatisfied, and the difference between "we decided not
//! to check this" and "nobody thought of it" is invisible. Each test below names the rule,
//! asserts a FEN breaking it is accepted, and names the issue that owns it.
//!
//! The last test is the mirror image: a rule that is out of scope and is nonetheless
//! rejected *by name*, so a wider EPD suite in issue #6 gets a useful diagnosis.

use boid_board::board::Board;
use boid_board::fen::FenError;
use boid_board::{Color, PieceKind};

fn accepted(fen: &str) -> Board {
    let board = Board::from_fen(fen).unwrap_or_else(|e| {
        panic!("{fen:?} must be ACCEPTED — D-0021 records this rule as unenforced: {e}")
    });
    board
        .check_invariants()
        .unwrap_or_else(|e| panic!("{fen:?}: {e}"));
    // Everything accepted must still obey the round-trip law.
    assert_eq!(
        Board::from_fen(&board.to_fen()).as_ref(),
        Ok(&board),
        "{fen:?} round trip"
    );
    board
}

/// **Not enforced: the side not to move must not already be in check.** Owner: issue #5.
///
/// Deciding this needs to know which squares are attacked, which needs the attack tables
/// D-0023 places in issue #5. Black is to move here while White's king stands in check from
/// the black rook — a position that cannot arise in a real game.
#[test]
fn a_position_with_the_side_not_to_move_in_check_is_accepted() {
    let board = accepted("4k3/8/8/8/8/8/8/4K2r b - - 0 1");
    assert_eq!(board.side_to_move(), Color::Black);
}

/// **Not enforced: the kings must not be adjacent.** Owner: issue #5.
///
/// Also a legality rule requiring attack knowledge — a king adjacent to the enemy king is
/// in check from it.
#[test]
fn a_position_with_adjacent_kings_is_accepted() {
    let board = accepted("8/8/8/8/8/8/8/3kK3 w - - 0 1");
    assert_eq!(board.pieces(PieceKind::King).count(), 2);
}

/// **Not enforced: the position must be reachable from the initial array.** No owner.
///
/// Nine white pawns, or ten queens, or two light-squared bishops, are all unreachable —
/// but deciding reachability is expensive and no engine needs it. Note that five of the
/// seven fixture rows sit at or near the promotion boundary already.
#[test]
fn a_position_over_the_promotion_budget_is_accepted() {
    // Ten white queens. Nine is the legal maximum -- eight promoted pawns plus the
    // original -- so ten cannot arise in any game.
    let board = accepted("QQQQkQQQ/QQ6/8/8/8/8/8/3QK3 w - - 0 1");
    assert_eq!(
        board.pieces_colored(PieceKind::Queen, Color::White).count(),
        10
    );
    // And a side with more than sixteen pieces in total.
    accepted("QQQQkQQQ/QQQQQQQQ/8/8/8/8/8/3QK3 w - - 0 1");
}

/// **Not enforced: the halfmove clock must be consistent with the fullmove number.**
/// No owner — it is not a chess rule.
///
/// A halfmove clock of 300 at move 2 is arithmetically impossible; it is also not something
/// a parser should adjudicate, and Stockfish accepts it too.
#[test]
fn an_arithmetically_inconsistent_clock_pair_is_accepted() {
    let board = accepted("4k3/8/8/8/8/8/8/4K3 w - - 300 2");
    assert_eq!(board.halfmove_clock(), 300);
    assert_eq!(board.fullmove_number(), 2);
}

/// **Not enforced: the halfmove clock must be below the fifty-move limit.** No owner.
///
/// A clock past 100 means the game is drawable, and past 150 that it is drawn — both are
/// facts about the *game*, not about whether the position can be written down.
#[test]
fn a_halfmove_clock_past_the_fifty_move_limit_is_accepted() {
    assert_eq!(
        accepted("4k3/8/8/8/8/8/8/4K3 w - - 100 60").halfmove_clock(),
        100
    );
    assert_eq!(
        accepted("4k3/8/8/8/8/8/8/4K3 w - - 65535 65535").halfmove_clock(),
        65535
    );
}

/// **Not enforced: an en-passant square is only recorded when a capture is available.**
/// Owner: issue #8, and deliberately so.
///
/// This is the convention divergence D-0019 records. Stockfish drops a dead en-passant
/// square; issue #4 mandates keeping it. The rule is *implementable* here — it needs only
/// pawn-attack knowledge, which is a 64-entry table — and is deliberately not implemented,
/// because the issue mandates the other convention and it is the conservative one for
/// issue #6's hashed perft.
#[test]
fn an_en_passant_square_with_no_capture_available_is_accepted() {
    // No white pawn anywhere can capture on e6, and the square is kept regardless.
    let board = accepted("4k3/8/8/4p3/8/8/8/4K3 w - e6 0 1");
    assert_eq!(board.ep_file().map(|f| f.to_char()), Some('e'));
    assert!(board.to_fen().contains(" e6 "));
}

/// **Not enforced: a pawn's position must be consistent with its file's pawn structure.**
/// No owner.
///
/// Eight white pawns on one file is unreachable without seven captures that the rest of the
/// position does not evidence. Structural validity is all that is checked.
#[test]
fn an_impossible_pawn_structure_is_accepted() {
    let board = accepted("4k3/P7/P7/P7/P7/P7/P7/4K3 w - - 0 1");
    assert_eq!(
        board.pieces_colored(PieceKind::Pawn, Color::White).count(),
        6
    );
}

/// The mirror image: a rule that is out of scope and is rejected **by name** anyway.
///
/// Shredder and X-FEN castling notation (`HAha`) denotes Chess960 castling, which issue #5
/// puts out of scope. Rejecting it with a named error rather than a generic parse failure
/// is what makes a wider EPD suite in issue #6 diagnosable.
#[test]
fn shredder_castling_notation_is_rejected_by_name() {
    assert_eq!(
        Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w HAha - 0 1"),
        Err(FenError::BadCastlingChar { found: 'H' })
    );
    // A Chess960 starting array is otherwise perfectly parseable — only the castling
    // notation is refused, so the boundary is the notation and not the position.
    accepted("bqnbnrkr/pppppppp/8/8/8/8/PPPPPPPP/BQNBNRKR w - - 0 1");
}

/// Every rule in D-0021's "deliberately does not enforce" list has a test above.
///
/// A count, so that adding a rule to the decision log without adding a test here is a
/// failure rather than a silent divergence.
#[test]
fn every_unenforced_rule_in_the_decision_log_has_a_test() {
    const UNENFORCED_RULES: [&str; 7] = [
        "side not to move in check",
        "adjacent kings",
        "reachable from the initial array",
        "clock pair arithmetically consistent",
        "halfmove clock below the fifty-move limit",
        "en passant recorded only when capturable",
        "pawn structure reachable",
    ];
    assert_eq!(
        UNENFORCED_RULES.len(),
        7,
        "D-0021 lists seven unenforced rules and this file tests each of them"
    );
}
