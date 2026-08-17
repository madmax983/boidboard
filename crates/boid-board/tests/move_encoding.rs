//! `Move`'s encoding, its UCI conversion, and the structural preconditions of applying one.
//!
//! Every test in this file was added after a multi-angle review, and most of them exist
//! because a mutation survived: the review deleted or weakened the guard each one now pins
//! and the suite stayed green. Where a test corresponds to a defect the review *found*
//! rather than a hole it *probed*, the repro that found it is quoted.

use boid_board::board::Board;
use boid_board::moves::{Move, MoveKind, MoveNotApplicable, MoveParseError};
use boid_board::{CastlingRights, Color, Piece, PieceKind, Square};

fn sq(name: &str) -> Square {
    Square::from_uci(name).unwrap_or_else(|| panic!("{name} is a square"))
}

fn board(fen: &str) -> Board {
    Board::from_fen(fen).unwrap_or_else(|e| panic!("{fen:?} must parse: {e}"))
}

// ---------------------------------------------------------------------------------
// The encoding
// ---------------------------------------------------------------------------------

#[test]
fn move_is_two_bytes() {
    assert_eq!(std::mem::size_of::<Move>(), 2);
    assert_eq!(std::mem::size_of::<Option<Move>>(), 4);
}

/// A `Move` can never encode a null move.
///
/// Issue #5's acceptance criterion 4 gives the null move its own entry point; a degenerate
/// `Move` with `from == to` would give it a second, unmarked one.
#[test]
fn from_equals_to_is_rejected_by_every_constructor() {
    assert_eq!(Move::new(Square::E1, Square::E1, MoveKind::Quiet), None);
    let bits = (u16::from(Square::E1.index() as u8) << 6) | Square::E1.index() as u16;
    assert_eq!(Move::from_bits(bits), None);
}

/// Flags 6 and 7 decode to no kind, so a raw-bits entry point must not admit them.
#[test]
fn unused_flag_values_are_rejected() {
    for flags in [6u16, 7] {
        let bits = (flags << 12) | (4 << 6) | 12;
        assert_eq!(
            Move::from_bits(bits),
            None,
            "flag {flags} decodes to no MoveKind and must not be constructible"
        );
    }
    // Every other flag value is accepted, so the rejection above is specific.
    for flags in (0u16..6).chain(8..16) {
        let bits = (flags << 12) | (4 << 6) | 12;
        assert!(Move::from_bits(bits).is_some(), "flag {flags}");
    }
}

/// `Move::new(f, t, k).kind() == k` for every kind a caller can name.
///
/// Found by review: `Promotion(Pawn)` and `Promotion(King)` were silently coerced to
/// `Promotion(Queen)`, so the round trip a caller is entitled to assume did not hold.
#[test]
fn every_constructible_kind_round_trips_and_the_impossible_ones_are_declined() {
    let kinds = [
        MoveKind::Quiet,
        MoveKind::DoublePawnPush,
        MoveKind::KingCastle,
        MoveKind::QueenCastle,
        MoveKind::Capture,
        MoveKind::EnPassant,
    ];
    for kind in kinds {
        let mv = Move::new(Square::E1, Square::H8, kind).expect("distinct squares");
        assert_eq!(mv.kind(), kind);
        assert_eq!(mv.from(), Square::E1);
        assert_eq!(mv.to(), Square::H8);
        assert_eq!(Move::from_bits(mv.as_u16()), Some(mv));
    }
    for promoted in PieceKind::PROMOTIONS {
        for kind in [
            MoveKind::Promotion(promoted),
            MoveKind::PromoCapture(promoted),
        ] {
            let mv = Move::new(sq("a7"), sq("a8"), kind).expect("distinct squares");
            assert_eq!(mv.kind(), kind, "{kind:?}");
        }
    }
    // A pawn promotes to exactly four pieces. The other two are declined rather than
    // rewritten into a queen.
    for impossible in [PieceKind::Pawn, PieceKind::King] {
        assert_eq!(
            Move::new(sq("a7"), sq("a8"), MoveKind::Promotion(impossible)),
            None,
            "Promotion({impossible:?}) must not be constructible"
        );
        assert_eq!(
            Move::new(sq("a7"), sq("a8"), MoveKind::PromoCapture(impossible)),
            None,
        );
    }
}

/// `is_capture` and `is_promotion` are single mask tests, and they agree with the kind for
/// every one of the sixteen flag values.
#[test]
fn is_capture_and_is_promotion_agree_with_the_kind() {
    for flags in (0u16..6).chain(8..16) {
        let mv = Move::from_bits((flags << 12) | (4 << 6) | 12).expect("a valid encoding");
        let expected_capture = matches!(
            mv.kind(),
            MoveKind::Capture | MoveKind::EnPassant | MoveKind::PromoCapture(_)
        );
        let expected_promotion = matches!(
            mv.kind(),
            MoveKind::Promotion(_) | MoveKind::PromoCapture(_)
        );
        assert_eq!(mv.is_capture(), expected_capture, "{:?}", mv.kind());
        assert_eq!(mv.is_promotion(), expected_promotion, "{:?}", mv.kind());
    }
    // En passant must report as a capture even though its victim is not on the destination.
    let ep = Move::new(sq("e5"), sq("d6"), MoveKind::EnPassant).expect("distinct");
    assert!(ep.is_capture());
    assert!(!ep.is_promotion());
}

/// `as_u16` carries `from` and `to`, not only the flags — issue #6 sorts on it.
#[test]
fn the_packed_representation_carries_every_field() {
    let a = Move::new(sq("e2"), sq("e4"), MoveKind::DoublePawnPush).expect("distinct");
    let b = Move::new(sq("d2"), sq("d4"), MoveKind::DoublePawnPush).expect("distinct");
    assert_ne!(a.as_u16(), b.as_u16(), "different squares, same kind");
    let c = Move::new(sq("e2"), sq("e4"), MoveKind::Quiet).expect("distinct");
    assert_ne!(a.as_u16(), c.as_u16(), "same squares, different kind");
    // Sorting by the packed value is a total order over distinct moves.
    let mut moves = vec![a, b, c];
    moves.sort();
    moves.dedup();
    assert_eq!(moves.len(), 3);
}

// ---------------------------------------------------------------------------------
// UCI
// ---------------------------------------------------------------------------------

/// Found by review: `from_uci` sliced by byte offset without checking for ASCII, so a
/// four-byte string with a multibyte character straddling index 2 **panicked** out of a
/// `Result`-returning function.
///
/// > `Move::from_uci("aé1b", &startpos)` panicked at `moves.rs:266`:
/// > byte index 2 is not a char boundary
#[test]
fn from_uci_returns_an_error_rather_than_panicking_on_non_ascii() {
    let board = Board::startpos();
    for text in ["aé1b", ">\u{c7}Yp", "e2é4", "♞e4", "e2e4\u{a0}"] {
        assert_eq!(
            Move::from_uci(text, &board),
            Err(MoveParseError::NotAscii),
            "{text:?} must be rejected, not panic"
        );
    }
}

/// A sweep, because "never panics" over a handful of hand-picked strings is not a claim.
#[test]
fn from_uci_never_panics_on_arbitrary_short_strings() {
    let board = Board::startpos();
    let mut examined = 0usize;
    let alphabet = [
        "",
        "a",
        "1",
        "e2",
        "e2e",
        "e2e4",
        "e2e4q",
        "e2e4qq",
        "zzzz",
        "a0a0",
        "e2e2",
        "é",
        "e2é4",
        "\u{0}\u{0}\u{0}\u{0}",
    ];
    for text in alphabet {
        let _ = Move::from_uci(text, &board);
        examined += 1;
    }
    // Every 4-character string over a small alphabet, which is where the slicing lives.
    for a in "ae1z".chars() {
        for b in "18z\u{e9}".chars() {
            for c in "ah".chars() {
                for d in "14".chars() {
                    let text: String = [a, b, c, d].into_iter().collect();
                    let _ = Move::from_uci(&text, &board);
                    examined += 1;
                }
            }
        }
    }
    assert!(examined > 60, "only {examined} inputs examined");
}

#[test]
fn every_move_parse_error_variant_is_reachable() {
    let start = Board::startpos();
    assert_eq!(Move::from_uci("é", &start), Err(MoveParseError::NotAscii));
    assert_eq!(
        Move::from_uci("e2e", &start),
        Err(MoveParseError::BadLength)
    );
    assert_eq!(
        Move::from_uci("e2e4qq", &start),
        Err(MoveParseError::BadLength)
    );
    assert_eq!(
        Move::from_uci("z9y8", &start),
        Err(MoveParseError::BadSquare)
    );
    assert_eq!(
        Move::from_uci("a7a8k", &board("4k3/P7/8/8/8/8/8/4K3 w - - 0 1")),
        Err(MoveParseError::BadPromotionPiece)
    );
    assert_eq!(
        Move::from_uci("e4e5", &start),
        Err(MoveParseError::NoPieceOnFrom)
    );
    assert_eq!(
        Move::from_uci("e7e5", &start),
        Err(MoveParseError::NotSideToMove)
    );
    // A promotion piece named for a move that does not reach the last rank, and omitted for
    // one that does.
    assert_eq!(
        Move::from_uci("e2e4q", &start),
        Err(MoveParseError::PromotionMismatch)
    );
    assert_eq!(
        Move::from_uci("a7a8", &board("4k3/P7/8/8/8/8/8/4K3 w - - 0 1")),
        Err(MoveParseError::PromotionMismatch)
    );
}

/// Promotion letters are lowercase for both colours — what UCI specifies and what
/// Stockfish prints (`b2a1q`, never `b2a1Q`).
#[test]
fn all_eight_promotion_spellings_round_trip_through_uci() {
    let white = board("4k2r/6P1/8/8/8/8/8/4K3 w k - 0 1");
    let black = board("4k3/8/8/8/8/8/6p1/4K2R b K - 0 1");
    for (position, quiet, capture) in [(&white, "g7g8", "g7h8"), (&black, "g2g1", "g2h1")] {
        for letter in ["n", "b", "r", "q"] {
            for stem in [quiet, capture] {
                let text = format!("{stem}{letter}");
                let mv = Move::from_uci(&text, position)
                    .unwrap_or_else(|e| panic!("{text} in {}: {e}", position.to_fen()));
                assert_eq!(mv.to_uci(), text);
                assert!(mv.is_promotion(), "{text}");
                assert_eq!(mv.is_capture(), stem == capture, "{text}");
            }
        }
    }
}

/// Castling is spelled king-from / king-to, never the Chess960 rook convention — which
/// issue #6's `divide` comparison depends on.
#[test]
fn castling_is_spelled_king_from_king_to() {
    let position = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    for (text, kind) in [
        ("e1g1", MoveKind::KingCastle),
        ("e1c1", MoveKind::QueenCastle),
    ] {
        let mv = Move::from_uci(text, &position).expect("a castling move");
        assert_eq!(mv.kind(), kind);
        assert_eq!(mv.to_uci(), text);
    }
    // The rook convention is NOT read as castling.
    let mv = Move::from_uci("e1h1", &position);
    assert_ne!(
        mv.map(|m| m.kind()),
        Ok(MoveKind::KingCastle),
        "e1h1 is the Chess960 spelling and must not be read as castling"
    );
}

/// Found by review. A king two files from anywhere was read as a castle, so
/// `from_uci("e1g7")` manufactured a `KingCastle` out of plain UCI text.
#[test]
fn only_the_four_real_castling_square_pairs_are_read_as_castling() {
    let position = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    // A king moving two files but not to a castling destination.
    for text in ["e1g2", "e1c2"] {
        let mv = Move::from_uci(text, &position);
        if let Ok(mv) = mv {
            assert!(
                !matches!(mv.kind(), MoveKind::KingCastle | MoveKind::QueenCastle),
                "{text} was read as {:?}",
                mv.kind()
            );
        }
    }
    // A king not on its home square.
    let off_home = board("r3k2r/8/8/8/8/8/8/R2K3R w kq - 0 1");
    let mv = Move::from_uci("d1f1", &off_home).expect("a king move");
    assert_eq!(mv.kind(), MoveKind::Quiet);
}

// ---------------------------------------------------------------------------------
// try_apply_move's structural preconditions
// ---------------------------------------------------------------------------------

/// **The most serious defect this branch's review found.**
///
/// `try_apply_move`'s castling arm checked only that the right existed and the path was
/// empty. It never checked that the mover was a king, or that the squares were a castling
/// pair. So a rook flagged `KingCastle` was accepted, and `apply_move` then moved it *and*
/// unconditionally toggled a rook on h1/f1 — **minting a rook that never existed**, with
/// `check_invariants()` returning `Ok` because the XOR is symmetric and every
/// representation agreed on the fiction.
///
/// The reviewer's repro: `h1h2` flagged `KingCastle` on the position below turned six
/// pieces into eight.
#[test]
fn a_castle_flagged_move_must_be_a_king_on_its_castling_pair() {
    let position = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    let before = position.occupied().count();

    // A rook, flagged as castling.
    let mv = Move::new(Square::H1, sq("h2"), MoveKind::KingCastle).expect("distinct");
    assert_eq!(
        position.try_apply_move(mv),
        Err(MoveNotApplicable::NotAKingMove)
    );

    // The king, but not on a castling square pair.
    let mv = Move::new(Square::E1, sq("g2"), MoveKind::KingCastle).expect("distinct");
    assert_eq!(
        position.try_apply_move(mv),
        Err(MoveNotApplicable::NotACastlingSquarePair)
    );

    // A king that is not on its home square cannot castle from wherever it stands.
    let off_home = board("r3k2r/8/8/8/8/8/8/R2K3R w kq - 0 1");
    let mv = Move::new(sq("d1"), sq("f1"), MoveKind::KingCastle).expect("distinct");
    assert_eq!(
        off_home.try_apply_move(mv),
        Err(MoveNotApplicable::NotACastlingSquarePair)
    );

    // And the real thing still works, conserving material.
    let mv = Move::new(Square::E1, sq("g1"), MoveKind::KingCastle).expect("distinct");
    let after = position.try_apply_move(mv).expect("a legal castle");
    assert_eq!(
        after.occupied().count(),
        before,
        "castling must not change the number of pieces on the board"
    );
}

/// Found by review: capturing a king was accepted, producing a board `from_fen` rejects —
/// which is D-0027's rule.
#[test]
fn capturing_a_king_is_rejected() {
    let position = board("4k3/8/8/8/8/8/8/4K2R w K - 0 1");
    let mv = Move::new(Square::H1, sq("e8"), MoveKind::Capture).expect("distinct");
    assert_eq!(
        position.try_apply_move(mv),
        Err(MoveNotApplicable::CapturesAKing)
    );
    // A capture of anything else from the same square is fine.
    let with_target = board("4k3/8/8/8/8/8/8/4K2r w - - 0 1");
    let mv = Move::new(Square::E1, sq("f1"), MoveKind::Quiet).expect("distinct");
    assert!(with_target.try_apply_move(mv).is_ok());
}

/// Found by review: a mislabelled en passant made `apply_move` **create a piece from
/// nothing**, because the victim square was toggled rather than removed and toggling an
/// empty square adds.
///
/// > `d4d3` flagged `EnPassant` on `4k3/8/8/3r4/3K4/8/8/8 w` produced a board with a black
/// > pawn on d2 that never existed, and `check_invariants()` returned `Ok`.
#[test]
fn a_mislabelled_en_passant_cannot_conjure_a_pawn() {
    let position = board("4k3/8/8/3r4/3K4/8/8/8 w - - 0 1");
    let before = position.occupied().count();
    let mv = Move::new(sq("d4"), sq("d3"), MoveKind::EnPassant).expect("distinct");
    let result = position.try_apply_move(mv);
    assert!(
        result.is_err(),
        "a king flagged EnPassant must be rejected, got {}",
        result.map(|b| b.to_fen()).unwrap_or_default()
    );
    // Nothing was created: the rejection happens before any mutation.
    assert_eq!(position.occupied().count(), before);
}

/// `apply_move` is public and unchecked, so its `debug_assert` is the last line of defence.
///
/// `try_apply_move` now rejects every mislabelled move that used to reach the mutation
/// path, which means the guards inside `place`/`remove` are unreachable through the checked
/// entry point. They are still reachable through the unchecked one, and this is what pins
/// them: without the asymmetric primitives, `toggle` on an empty victim square ADDS a pawn
/// and every invariant agrees afterwards, because XOR is its own inverse.
#[test]
#[should_panic(expected = "but that piece is not there")]
fn apply_move_refuses_to_remove_a_piece_that_is_not_there() {
    // A king flagged as an en-passant capture. The "victim" square behind the destination
    // is empty, so a symmetric toggle would conjure a black pawn onto it.
    let position = board("4k3/8/8/3r4/3K4/8/8/8 w - - 0 1");
    let mv = Move::new(sq("d4"), sq("d3"), MoveKind::EnPassant).expect("distinct");
    let _ = position.apply_move(mv);
}

/// The other half of the pair: placing onto an occupied square.
#[test]
#[should_panic(expected = "onto an occupied square")]
fn apply_move_refuses_to_place_a_piece_onto_an_occupied_square() {
    // A quiet move onto an enemy piece: the victim is never removed, so the arrival is a
    // placement onto a square that is still occupied.
    let position = board("4k3/8/8/8/8/8/4r3/4K3 w - - 0 1");
    let mv = Move::new(Square::E1, sq("e2"), MoveKind::Quiet).expect("distinct");
    let _ = position.apply_move(mv);
}

/// An en-passant capture must be a pawn's diagonal step onto the board's ep square.
#[test]
fn en_passant_requires_a_pawn_beside_the_victim() {
    // A live en-passant position: Black has just played d7-d5.
    let position = board("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 3");
    let good = Move::new(sq("e5"), sq("d6"), MoveKind::EnPassant).expect("distinct");
    assert!(position.try_apply_move(good).is_ok());

    // A pawn nowhere near the target, flagged en passant. (Not on a8: a pawn on a back
    // rank is not a representable position at all.)
    let distant = board("4k3/8/8/3p4/8/8/P7/4K3 w - d6 0 3");
    let mv = Move::new(sq("a2"), sq("d6"), MoveKind::EnPassant).expect("distinct");
    assert!(distant.try_apply_move(mv).is_err());

    // The right pawn, but the wrong destination: not the board's ep square.
    let mv = Move::new(sq("e5"), sq("f6"), MoveKind::EnPassant).expect("distinct");
    assert_eq!(
        position.try_apply_move(mv),
        Err(MoveNotApplicable::NotTheEnPassantSquare)
    );
}

/// Promotions are pawn steps: one rank forward, or one rank diagonally when capturing.
#[test]
fn a_promotion_must_be_a_pawn_step() {
    let position = board("4k2r/6P1/8/8/8/8/8/4K3 w k - 0 1");
    // The real thing.
    let good =
        Move::new(sq("g7"), sq("h8"), MoveKind::PromoCapture(PieceKind::Queen)).expect("distinct");
    assert!(position.try_apply_move(good).is_ok());

    // A pawn teleporting across the board onto the last rank.
    let far = board("4k3/6P1/8/8/8/8/8/4K3 w - - 0 1");
    let mv =
        Move::new(sq("g7"), sq("a8"), MoveKind::Promotion(PieceKind::Queen)).expect("distinct");
    assert_eq!(far.try_apply_move(mv), Err(MoveNotApplicable::NotAPawnStep));

    // A non-pawn flagged as promoting.
    let rook = board("4k3/7R/8/8/8/8/8/4K3 w - - 0 1");
    let mv =
        Move::new(sq("h7"), sq("h8"), MoveKind::Promotion(PieceKind::Queen)).expect("distinct");
    assert_eq!(
        rook.try_apply_move(mv),
        Err(MoveNotApplicable::NotAPromotion)
    );
}

/// The queen-side castling path includes b1 and b8, which the king never crosses.
///
/// This is issue #5's acceptance criterion 3 verbatim, and the review found that removing
/// the b-file square from either path left the whole suite green.
#[test]
fn queenside_castling_requires_b1_and_b8_to_be_empty() {
    for (fen, uci, colour) in [
        (
            "r3k2r/8/8/8/8/8/8/RN2K2R w KQkq - 0 1",
            "e1c1",
            Color::White,
        ),
        (
            "rn2k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1",
            "e8c8",
            Color::Black,
        ),
    ] {
        let position = board(fen);
        let mv = Move::from_uci(uci, &position).expect("a castling move");
        assert_eq!(
            position.try_apply_move(mv),
            Err(MoveNotApplicable::CastlingPathOccupied),
            "{colour:?}: the knight on the b-file blocks queen-side castling"
        );
    }
    // Minimally repaired: move the knight off the b-file and it is legal again.
    for (fen, uci) in [
        ("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "e1c1"),
        ("r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1", "e8c8"),
    ] {
        let position = board(fen);
        let mv = Move::from_uci(uci, &position).expect("a castling move");
        assert!(position.try_apply_move(mv).is_ok(), "{fen}");
    }
}

/// Black castles too, and its rook moves.
///
/// The review deleted both Black castling arms from `castling_rook_squares` and the suite
/// stayed green: every castling assertion was White's.
#[test]
fn black_castles_on_both_wings_and_moves_the_right_rook() {
    let position = board("r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1");
    let king_side = position
        .try_apply_move(Move::from_uci("e8g8", &position).expect("a move"))
        .expect("black may castle king-side");
    assert_eq!(king_side.to_fen(), "r4rk1/8/8/8/8/8/8/R3K2R w KQ - 1 2");
    assert_eq!(king_side.piece_at(sq("f8")), Some(Piece::BlackRook));

    let queen_side = position
        .try_apply_move(Move::from_uci("e8c8", &position).expect("a move"))
        .expect("black may castle queen-side");
    assert_eq!(queen_side.to_fen(), "2kr3r/8/8/8/8/8/8/R3K2R w KQ - 1 2");
    assert_eq!(queen_side.piece_at(sq("d8")), Some(Piece::BlackRook));
}

/// Every `MoveNotApplicable` variant is reachable, and named by a test.
#[test]
fn every_move_not_applicable_variant_is_reachable() {
    let start = Board::startpos();
    let castling = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");

    let cases: Vec<(MoveNotApplicable, Board, Move)> = vec![
        (
            MoveNotApplicable::NoPieceOnFrom,
            start,
            Move::new(sq("e4"), sq("e5"), MoveKind::Quiet).expect("distinct"),
        ),
        (
            MoveNotApplicable::NotSideToMove,
            start,
            Move::new(sq("e7"), sq("e6"), MoveKind::Quiet).expect("distinct"),
        ),
        (
            MoveNotApplicable::OwnPieceOnDestination,
            start,
            Move::new(Square::A1, sq("a2"), MoveKind::Quiet).expect("distinct"),
        ),
        (
            MoveNotApplicable::CapturesAKing,
            board("4k3/8/8/8/8/8/8/4K2R w K - 0 1"),
            Move::new(Square::H1, sq("e8"), MoveKind::Capture).expect("distinct"),
        ),
        (
            MoveNotApplicable::KindDisagreesWithBoard {
                claimed: MoveKind::Capture,
            },
            start,
            Move::new(sq("e2"), sq("e3"), MoveKind::Capture).expect("distinct"),
        ),
        (
            MoveNotApplicable::NotTheEnPassantSquare,
            board("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 3"),
            Move::new(sq("e5"), sq("f6"), MoveKind::EnPassant).expect("distinct"),
        ),
        (
            MoveNotApplicable::NotAPawnCaptureStep,
            board("4k3/8/8/3p4/8/8/P7/4K3 w - d6 0 3"),
            Move::new(sq("a2"), sq("d6"), MoveKind::EnPassant).expect("distinct"),
        ),
        (
            MoveNotApplicable::CastlingRightAbsent,
            board("4k3/8/8/8/8/8/8/4K2R w - - 0 1"),
            Move::new(Square::E1, sq("g1"), MoveKind::KingCastle).expect("distinct"),
        ),
        (
            MoveNotApplicable::CastlingPathOccupied,
            board("r3k2r/8/8/8/8/8/8/R3KB1R w KQkq - 0 1"),
            Move::new(Square::E1, sq("g1"), MoveKind::KingCastle).expect("distinct"),
        ),
        (
            MoveNotApplicable::NotAKingMove,
            castling,
            Move::new(Square::H1, sq("h2"), MoveKind::KingCastle).expect("distinct"),
        ),
        (
            MoveNotApplicable::NotACastlingSquarePair,
            castling,
            Move::new(Square::E1, sq("g2"), MoveKind::KingCastle).expect("distinct"),
        ),
        (
            MoveNotApplicable::NotAPromotion,
            board("4k3/7R/8/8/8/8/8/4K3 w - - 0 1"),
            Move::new(sq("h7"), sq("h8"), MoveKind::Promotion(PieceKind::Queen)).expect("distinct"),
        ),
        (
            MoveNotApplicable::NotAPawnStep,
            board("4k3/6P1/8/8/8/8/8/4K3 w - - 0 1"),
            Move::new(sq("g7"), sq("a8"), MoveKind::Promotion(PieceKind::Queen)).expect("distinct"),
        ),
        (
            MoveNotApplicable::NotADoublePush,
            board("4k3/8/8/8/8/4P3/8/4K3 w - - 0 1"),
            Move::new(sq("e3"), sq("e5"), MoveKind::DoublePawnPush).expect("distinct"),
        ),
        (
            MoveNotApplicable::PawnWouldNotPromote,
            board("4k3/8/8/8/8/8/6P1/4K3 w - - 0 1"),
            Move::new(sq("g2"), sq("g8"), MoveKind::Quiet).expect("distinct"),
        ),
    ];

    for (expected, position, mv) in &cases {
        assert_eq!(
            position.try_apply_move(*mv),
            Err(*expected),
            "{mv} on {}",
            position.to_fen()
        );
    }
    assert_eq!(cases.len(), 15, "every variant must have a case");
}

/// A double push is blocked by a piece on the square it skips over.
#[test]
fn a_double_push_over_an_occupied_square_is_rejected() {
    let blocked = board("4k3/8/8/8/8/4n3/4P3/4K3 w - - 0 1");
    let mv = Move::new(sq("e2"), sq("e4"), MoveKind::DoublePawnPush).expect("distinct");
    assert_eq!(
        blocked.try_apply_move(mv),
        Err(MoveNotApplicable::NotADoublePush)
    );
    // And from anywhere but the home rank.
    let wrong_rank = board("4k3/8/8/8/8/4P3/8/4K3 w - - 0 1");
    let mv = Move::new(sq("e3"), sq("e5"), MoveKind::DoublePawnPush).expect("distinct");
    assert_eq!(
        wrong_rank.try_apply_move(mv),
        Err(MoveNotApplicable::NotADoublePush)
    );
}

/// `same_position` must be able to say *no*.
///
/// The review replaced its body with `true` and the whole suite stayed green — and
/// acceptance criterion 5 is stated in terms of this relation.
#[test]
fn same_position_distinguishes_positions_that_differ() {
    let start = Board::startpos();
    assert!(start.same_position(&start));

    // Different pieces.
    assert!(!start.same_position(&board("4k3/8/8/8/8/8/8/4K3 w - - 0 1")));
    // Different side to move.
    let after = start
        .try_apply_move(Move::from_uci("e2e3", &start).expect("a move"))
        .expect("applicable");
    assert!(!after.same_position(&start));
    // Different castling rights, same pieces.
    let all = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    let some = board("r3k2r/8/8/8/8/8/8/R3K2R w Kq - 0 1");
    assert!(!all.same_position(&some));
    assert_ne!(all.key(), some.key());
    // Different en-passant file, same pieces and rights.
    let ep_d = board("4k3/8/8/3p4/8/8/8/4K3 w - d6 0 3");
    let ep_none = board("4k3/8/8/3p4/8/8/8/4K3 w - - 0 3");
    assert!(!ep_d.same_position(&ep_none));
    // But the clocks are ignored, which is what the relation is for.
    let early = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    let late = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 99 40");
    assert!(early.same_position(&late));
    assert_eq!(early.key(), late.key());
    assert_ne!(early, late);
}

/// `RIGHTS_LOST` has exactly six meaningful squares, checked through the public behaviour
/// rather than through the private table.
#[test]
fn exactly_six_squares_revoke_a_castling_right() {
    let position = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    let all = CastlingRights::ALL;
    let mut revoking = Vec::new();
    for index in 0..64u8 {
        let from = Square::from_index(index).expect("in range");
        let Some(piece) = position.piece_at(from) else {
            continue;
        };
        if piece.color() != Color::White {
            continue;
        }
        // Move the piece somewhere harmless and see whether rights changed.
        for target in ["d4", "e4", "f4", "d5"] {
            let to = sq(target);
            let Some(mv) = Move::new(from, to, MoveKind::Quiet) else {
                continue;
            };
            if let Ok(after) = position.try_apply_move(mv)
                && after.castling() != all
            {
                revoking.push(from);
                break;
            }
        }
    }
    revoking.sort();
    revoking.dedup();
    assert_eq!(
        revoking,
        vec![Square::A1, Square::E1, Square::H1],
        "on White's side exactly a1, e1 and h1 revoke a right when they move"
    );
}
