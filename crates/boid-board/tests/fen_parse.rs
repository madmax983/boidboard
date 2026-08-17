//! Cycle 2: the FEN parser accepts the language D-0020 defines and rejects everything else
//! with a named reason.
//!
//! This is acceptance criterion 2. Its wording — "invalid FENs return `Err`, never panic" —
//! has a one-bit reading that a parser rejecting *everything* also satisfies, so no test
//! here asserts `.is_err()`. Each rejection names the [`FenError`] it expects, and each is
//! paired with a **minimally repaired** input that must be accepted, so a parser that grew
//! too strict fails just as loudly as one that grew too lax.

use boid_board::board::Board;
use boid_board::fen::{FenError, FenField};
use boid_board::{CastlingRights, Color, Piece, PieceKind, Square};

/// The baseline every negative case below mutates: a legal, canonical, six-field FEN.
const VALID: &str = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";

fn err(fen: &str) -> FenError {
    Board::from_fen(fen).expect_err("expected this FEN to be rejected")
}

fn ok(fen: &str) -> Board {
    Board::from_fen(fen).unwrap_or_else(|e| panic!("expected {fen:?} to be accepted: {e}"))
}

// ---------------------------------------------------------------------------------
// The corpus that already exists
// ---------------------------------------------------------------------------------

#[test]
fn parses_all_seven_fixture_rows() {
    let cases = boid_board::perft::oracle::parse(boid_board::perft::oracle::ORACLE_TEXT)
        .expect("the committed fixture parses");
    assert_eq!(cases.len(), 7);
    for case in cases {
        let board = ok(case.fen);
        board
            .check_invariants()
            .unwrap_or_else(|e| panic!("{}: {e}", case.id));
    }
}

#[test]
fn the_start_position_constant_parses() {
    let board = ok(Board::STARTPOS_FEN);
    assert_eq!(board.side_to_move(), Color::White);
    assert_eq!(board.castling(), CastlingRights::ALL);
    assert_eq!(board.ep_file(), None);
    assert_eq!(board.halfmove_clock(), 0);
    assert_eq!(board.fullmove_number(), 1);
    assert_eq!(board.occupied().count(), 32);
}

/// Pins individual squares rather than only aggregate counts.
///
/// A rank-order inversion (rank 8 written into rank 1) or a colour-case swap cancels
/// perfectly between a parser and its own emitter, so a round-trip test cannot see it.
/// These eight assertions can.
#[test]
fn named_squares_hold_the_pieces_they_should_in_the_start_position() {
    let board = Board::startpos();
    let at = |name: &str| board.piece_at(Square::from_uci(name).expect("a square"));
    assert_eq!(at("e1"), Some(Piece::WhiteKing));
    assert_eq!(at("d1"), Some(Piece::WhiteQueen));
    assert_eq!(at("a1"), Some(Piece::WhiteRook));
    assert_eq!(at("b1"), Some(Piece::WhiteKnight));
    assert_eq!(at("e2"), Some(Piece::WhitePawn));
    assert_eq!(at("e8"), Some(Piece::BlackKing));
    assert_eq!(at("d8"), Some(Piece::BlackQueen));
    assert_eq!(at("e7"), Some(Piece::BlackPawn));
    assert_eq!(at("e4"), None);
}

#[test]
fn the_four_field_kiwipete_form_is_accepted_and_defaults_its_clocks() {
    let four = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -";
    let board = ok(four);
    assert_eq!(board.halfmove_clock(), 0);
    assert_eq!(board.fullmove_number(), 1);
    // The same position written out in full must produce an identical board, including
    // both keys. This is what licenses D-0020's " 0 1" canonicalisation.
    assert_eq!(ok(&format!("{four} 0 1")), board);
}

// ---------------------------------------------------------------------------------
// Field count and encoding
// ---------------------------------------------------------------------------------

#[test]
fn rejects_an_empty_fen() {
    assert_eq!(err(""), FenError::Empty);
}

#[test]
fn rejects_wrong_field_counts() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3"),
        FenError::WrongFieldCount { found: 1 }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - 0"),
        FenError::WrongFieldCount { found: 5 }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - 0 1 extra"),
        FenError::WrongFieldCount { found: 7 }
    );
    ok(VALID);
}

/// The highest-value guard in this file.
///
/// Six of the seven FENs this project transcribed are published with U+00A0 separators.
/// Rust's `split_whitespace` treats U+00A0 as whitespace and would parse this happily;
/// Stockfish's ASCII-only tokeniser reads it as a *different legal position* and reports
/// zero nodes. A lenient parser here would reopen the exact silent wrong-answer path
/// D-0008 exists to close.
#[test]
fn rejects_non_breaking_space_separators() {
    let nbsp = "4k3/8/8/8/8/8/8/4K3\u{a0}w\u{a0}-\u{a0}-\u{a0}0\u{a0}1";
    assert!(
        matches!(err(nbsp), FenError::NonAscii { .. }),
        "got {:?}",
        err(nbsp)
    );
    ok(VALID);
}

/// A trimming parser would break the round-trip law's universal quantification: the
/// trimmed and untrimmed forms would both parse, but only one of them can be emitted.
#[test]
fn rejects_leading_and_trailing_whitespace() {
    // A leading or trailing space produces a SEVENTH field rather than an empty one,
    // because the split is on ' ' rather than on runs of whitespace.
    assert_eq!(
        err(&format!(" {VALID}")),
        FenError::WrongFieldCount { found: 7 }
    );
    assert_eq!(
        err(&format!("{VALID} ")),
        FenError::WrongFieldCount { found: 7 }
    );
    // An empty field is reachable when a doubled space REPLACES a field rather than
    // padding one: six fields, the third of them empty.
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w  - 0 1"),
        FenError::EmptyField {
            field: FenField::Castling
        }
    );
    ok(VALID);
}

// ---------------------------------------------------------------------------------
// Piece placement
// ---------------------------------------------------------------------------------

#[test]
fn rejects_wrong_rank_counts() {
    assert_eq!(
        err("4k3/8/8/8/8/8/4K3 w - - 0 1"),
        FenError::WrongRankCount { found: 7 }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/8/4K3 w - - 0 1"),
        FenError::WrongRankCount { found: 9 }
    );
    ok(VALID);
}

#[test]
fn rejects_a_bad_piece_character() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4X3 w - - 0 1"),
        FenError::BadPieceChar {
            found: 'X',
            rank: 1
        }
    );
    // Minimally repaired: X becomes a legal piece letter.
    ok("4k3/8/8/8/8/8/8/4K2R w - - 0 1");
}

#[test]
fn rejects_bad_skip_digits() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K03 w - - 0 1"),
        FenError::BadSkipDigit {
            found: '0',
            rank: 1
        }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/9 w - - 0 1"),
        FenError::BadSkipDigit {
            found: '9',
            rank: 1
        }
    );
    ok(VALID);
}

/// `44` sums to eight files, so a rank-sum check alone waves it through — but no emitter
/// can produce it, so accepting it would break the round-trip law.
#[test]
fn rejects_consecutive_skip_digits() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/44 w - - 0 1"),
        FenError::ConsecutiveSkipDigits { rank: 1 }
    );
    // Minimally repaired: one skip digit rather than two.
    ok(VALID);
}

#[test]
fn rejects_wrong_file_counts() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K4 w - - 0 1"),
        FenError::WrongFileCount { rank: 1, found: 9 }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K2 w - - 0 1"),
        FenError::WrongFileCount { rank: 1, found: 7 }
    );
    ok(VALID);
}

#[test]
fn rejects_a_side_without_exactly_one_king() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/8 w - - 0 1"),
        FenError::WrongKingCount {
            color: Color::White,
            found: 0
        }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/3KK3 w - - 0 1"),
        FenError::WrongKingCount {
            color: Color::White,
            found: 2
        }
    );
    assert_eq!(
        err("3kk3/8/8/8/8/8/8/4K3 w - - 0 1"),
        FenError::WrongKingCount {
            color: Color::Black,
            found: 2
        }
    );
    ok(VALID);
}

#[test]
fn rejects_a_pawn_on_a_back_rank() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/P3K3 w - - 0 1"),
        FenError::PawnOnBackRank { square: Square::A1 }
    );
    assert_eq!(
        err("p3k3/8/8/8/8/8/8/4K3 w - - 0 1"),
        FenError::PawnOnBackRank { square: Square::A8 }
    );
    // Minimally repaired: the same pawn one rank up.
    ok("4k3/8/8/8/8/8/P7/4K3 w - - 0 1");
}

// ---------------------------------------------------------------------------------
// Side to move
// ---------------------------------------------------------------------------------

#[test]
fn rejects_a_bad_side_to_move() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 x - - 0 1"),
        FenError::BadSideToMove
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 W - - 0 1"),
        FenError::BadSideToMove
    );
    ok("4k3/8/8/8/8/8/8/4K3 b - - 0 1");
}

// ---------------------------------------------------------------------------------
// Castling
// ---------------------------------------------------------------------------------

#[test]
fn rejects_a_bad_castling_character() {
    assert_eq!(
        err("r3k2r/8/8/8/8/8/8/R3K2R w KQkx - 0 1"),
        FenError::BadCastlingChar { found: 'x' }
    );
    ok("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
}

/// Shredder/X-FEN castling is rejected *by name*, so a wider EPD suite in issue #6 gets a
/// useful diagnosis rather than a confusing one. Chess960 is out of scope (issue #5).
#[test]
fn rejects_shredder_castling_notation() {
    assert_eq!(
        err("r3k2r/8/8/8/8/8/8/R3K2R w HAha - 0 1"),
        FenError::BadCastlingChar { found: 'H' }
    );
}

#[test]
fn rejects_a_repeated_or_misordered_castling_field() {
    assert_eq!(
        err("r3k2r/8/8/8/8/8/8/R3K2R w KK - 0 1"),
        FenError::RepeatedCastlingRight { found: 'K' }
    );
    assert_eq!(
        err("r3k2r/8/8/8/8/8/8/R3K2R w qkQK - 0 1"),
        FenError::NonCanonicalCastlingOrder
    );
    assert_eq!(
        err("r3k2r/8/8/8/8/8/8/R3K2R w  - 0 1"),
        FenError::EmptyField {
            field: FenField::Castling
        }
    );
    ok("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
}

/// A castling right with no rook behind it is the commonest transcription damage there is,
/// and it silently changes move generation in issue #5.
#[test]
fn rejects_a_castling_right_without_its_king_or_rook() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w K - 0 1"),
        FenError::CastlingRightWithoutPieces {
            right: CastlingRights::WHITE_KING
        }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/R3K3 w K - 0 1"),
        FenError::CastlingRightWithoutPieces {
            right: CastlingRights::WHITE_KING
        }
    );
    // Minimally repaired: put the h1 rook there.
    ok("4k3/8/8/8/8/8/8/4K2R w K - 0 1");
}

// ---------------------------------------------------------------------------------
// En passant
// ---------------------------------------------------------------------------------

#[test]
fn rejects_a_malformed_en_passant_field() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - e9 0 1"),
        FenError::BadEnPassantSquare
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - zz 0 1"),
        FenError::BadEnPassantSquare
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - e 0 1"),
        FenError::BadEnPassantSquare
    );
}

/// The rank follows from the side to move. After White double pushes, the target is on
/// rank 3 and it is *Black's* turn — the reverse pairing is decidable without a board.
#[test]
fn rejects_an_en_passant_rank_that_contradicts_the_side_to_move() {
    assert_eq!(
        err("4k3/8/8/8/4P3/8/8/4K3 w - e3 0 1"),
        FenError::EnPassantRankContradictsSideToMove {
            square: Square::from_uci("e3").expect("a square")
        }
    );
    // Minimally repaired: the same position with Black to move.
    ok("4k3/8/8/8/4P3/8/8/4K3 b - e3 0 1");
}

/// An en-passant square with no pawn that could have produced it. Note the repaired FEN
/// puts the black pawn on e5, not on e6: a pawn *on* the target square is still an
/// impossible en passant, which is the mistake the obvious repair makes.
#[test]
fn rejects_an_en_passant_square_that_no_double_push_could_have_produced() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 b - e3 0 1"),
        FenError::EnPassantNotReachable {
            square: Square::from_uci("e3").expect("a square")
        }
    );
    // A black pawn sitting on the target square itself is still not reachable.
    assert_eq!(
        err("4k3/8/4p3/8/8/8/8/4K3 w - e6 0 1"),
        FenError::EnPassantNotReachable {
            square: Square::from_uci("e6").expect("a square")
        }
    );
    // Repaired properly: the pawn that just double pushed stands on e5.
    let board = ok("4k3/8/8/4p3/8/8/8/4K3 w - e6 0 1");
    assert_eq!(board.ep_square(), Square::from_uci("e6"));
    assert_eq!(board.ep_file(), Square::from_uci("e6").map(Square::file));
}

/// The en-passant reachability rule has three clauses, and two of them were unpinned: a
/// reviewer deleted the "target must be empty" and "origin must be empty" checks
/// independently and the whole suite stayed green.
#[test]
fn each_en_passant_reachability_clause_is_enforced_separately() {
    let ep = Square::from_uci("e6").expect("a square");
    // The pusher is present but the TARGET square is occupied -- impossible, because the
    // pawn passed through it.
    assert_eq!(
        err("4k3/8/4r3/4p3/8/8/8/4K3 w - e6 0 1"),
        FenError::EnPassantNotReachable { square: ep }
    );
    // The pusher is present and the target empty, but the square it left is occupied.
    assert_eq!(
        err("4k3/4r3/8/4p3/8/8/8/4K3 w - e6 0 1"),
        FenError::EnPassantNotReachable { square: ep }
    );
    // The target and origin are empty but there is no pawn that could have pushed.
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - e6 0 1"),
        FenError::EnPassantNotReachable { square: ep }
    );
    // All three satisfied.
    ok("4k3/8/8/4p3/8/8/8/4K3 w - e6 0 1");
}

/// The en-passant target must be on rank 3 or 6 -- the shape check, which is distinct from
/// the side-to-move check that follows it.
#[test]
fn an_en_passant_target_off_ranks_three_and_six_is_rejected_on_its_shape() {
    for square in ["e4", "e5", "e1", "e8", "e2", "e7"] {
        assert_eq!(
            err(&format!("4k3/8/8/4p3/8/8/8/4K3 w - {square} 0 1")),
            FenError::BadEnPassantSquare,
            "{square} is not on rank 3 or 6"
        );
    }
}

/// `NonAscii`'s offset is a BYTE offset naming the first offending byte, not a char index.
#[test]
fn the_non_ascii_offset_is_the_byte_index_of_the_first_offending_byte() {
    // U+00A0 sits immediately after the 19-byte placement field.
    let fen = "4k3/8/8/8/8/8/8/4K3\u{a0}w - - 0 1";
    assert_eq!(err(fen), FenError::NonAscii { offset: 19 });
    // Once a multibyte character appears, the byte offset and the char index diverge, and
    // this is the byte offset.
    let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 \u{e9}1";
    assert_eq!(err(fen), FenError::NonAscii { offset: 28 });
    assert_eq!(fen.chars().count(), 30);
    assert_eq!(fen.len(), 31, "one character occupies two bytes");
}

/// `EmptyField` is reachable for the two clock fields, not only for the first four.
#[test]
fn an_empty_clock_field_is_reported_as_such() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - -  1"),
        FenError::EmptyField {
            field: FenField::HalfmoveClock
        }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - 0 "),
        FenError::EmptyField {
            field: FenField::FullmoveNumber
        }
    );
}

// ---------------------------------------------------------------------------------
// Clocks
// ---------------------------------------------------------------------------------

#[test]
fn rejects_non_numeric_clocks() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - x 1"),
        FenError::BadNumber {
            field: FenField::HalfmoveClock
        }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - 0 x"),
        FenError::BadNumber {
            field: FenField::FullmoveNumber
        }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - -1 1"),
        FenError::BadNumber {
            field: FenField::HalfmoveClock
        }
    );
    ok(VALID);
}

#[test]
fn rejects_out_of_range_clocks() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - 65536 1"),
        FenError::NumberOutOfRange {
            field: FenField::HalfmoveClock
        }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - 0 65536"),
        FenError::NumberOutOfRange {
            field: FenField::FullmoveNumber
        }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - 0 0"),
        FenError::FullmoveNumberIsZero
    );
    // Minimally repaired: the largest values that do fit.
    let board = ok("4k3/8/8/8/8/8/8/4K3 w - - 65535 65535");
    assert_eq!(board.halfmove_clock(), 65535);
    assert_eq!(board.fullmove_number(), 65535);
}

/// Canonical FEN has no leading zeros, and accepting them would break the round-trip law:
/// `"01"` parses to 1 and emits as `"1"`.
#[test]
fn rejects_leading_zeros_in_the_clocks() {
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - 00 1"),
        FenError::LeadingZero {
            field: FenField::HalfmoveClock
        }
    );
    assert_eq!(
        err("4k3/8/8/8/8/8/8/4K3 w - - 0 01"),
        FenError::LeadingZero {
            field: FenField::FullmoveNumber
        }
    );
    ok(VALID);
}

// ---------------------------------------------------------------------------------
// Never panics
// ---------------------------------------------------------------------------------

/// Acceptance criterion 2's second half, over inputs nobody chose.
///
/// Every prefix, every suffix, and every single-byte deletion and substitution of a valid
/// FEN — a few thousand mutations, all of which must return rather than unwind. A parser
/// that indexes a byte slice without checking its length dies here and nowhere else.
#[test]
fn never_panics_on_mutations_of_a_valid_fen() {
    let seeds = [
        VALID,
        Board::STARTPOS_FEN,
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    ];
    let mut examined = 0usize;
    for seed in seeds {
        for cut in 0..=seed.len() {
            if seed.is_char_boundary(cut) {
                let _ = Board::from_fen(&seed[..cut]);
                let _ = Board::from_fen(&seed[cut..]);
                examined += 2;
            }
        }
        for index in 0..seed.len() {
            if !seed.is_char_boundary(index) {
                continue;
            }
            let mut deleted = seed.to_owned();
            deleted.remove(index);
            let _ = Board::from_fen(&deleted);
            examined += 1;
            for byte in [b'0', b'9', b'/', b' ', b'k', b'X', b'-'] {
                let mut swapped = seed.as_bytes().to_vec();
                swapped[index] = byte;
                if let Ok(text) = std::str::from_utf8(&swapped) {
                    let _ = Board::from_fen(text);
                    examined += 1;
                }
            }
        }
    }
    assert!(
        examined > 1500,
        "the mutation sweep examined only {examined} inputs, which is too few to mean \
         anything"
    );
}

/// A pathological input is rejected without materialising a field per separator.
///
/// `split(' ').collect::<Vec<_>>()` on a megabyte of spaces builds a million-entry `Vec`
/// before anyone reads its length, and an allocation failure ABORTS the process rather than
/// unwinding — which no caller can contain. Counting first is O(1) in space. This test
/// cannot observe the allocation directly; it pins the behaviour and the reason is in the
/// parser's comment.
#[test]
fn a_pathological_separator_run_is_rejected_by_field_count() {
    for len in [1_000usize, 100_000, 1_000_000] {
        let spaces = " ".repeat(len);
        assert_eq!(
            Board::from_fen(&spaces),
            Err(FenError::WrongFieldCount { found: len + 1 }),
            "a run of {len} separators must be rejected on its field count"
        );
    }
}

/// The other half: inputs that are not mutations of anything valid.
#[test]
fn never_panics_on_adversarial_inputs() {
    let inputs = [
        "",
        " ",
        "/////// w - - 0 1",
        "8/8/8/8/8/8/8/8 w - - 0 1",
        "k7/8/8/8/8/8/8/7K w KQkq - 0 1",
        "99999999/8/8/8/8/8/8/8 w - - 0 1",
        "4k3/8/8/8/8/8/8/4K3 w - - 99999999999999999999 1",
        "4k3/8/8/8/8/8/8/4K3 w - - 0 99999999999999999999",
        "\u{1F600} w - - 0 1",
        "4k3/8/8/8/8/8/8/4K3 w KQkq e3 0 1",
    ];
    // The contract under test is "returns rather than unwinds", so the absence of a panic
    // IS the assertion. But every one of these is in fact rejected, and saying so keeps the
    // list honest -- an earlier comment claimed some were legal, and a review measured that
    // none of the ten were.
    for input in inputs {
        assert!(
            Board::from_fen(input).is_err(),
            "{input:?} was expected to be rejected"
        );
    }
}

/// A parser is only useful if its `Ok` values are right, so every accepted FEN above is
/// also structurally consistent.
#[test]
fn every_accepted_fen_satisfies_the_board_invariants() {
    for fen in [
        VALID,
        Board::STARTPOS_FEN,
        "4k3/8/8/4p3/8/8/8/4K3 w - e6 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        "4k3/8/8/8/8/8/8/4K3 w - - 65535 65535",
    ] {
        let board = ok(fen);
        board
            .check_invariants()
            .unwrap_or_else(|e| panic!("{fen:?}: {e}"));
        assert_eq!(board.key(), board.recomputed_key());
        assert_eq!(board.pawn_key(), board.recomputed_pawn_key());
    }
}

/// The pawn key is a *different* value from the position key, and it is zero exactly when
/// there are no pawns.
#[test]
fn the_pawn_key_is_pawns_only() {
    let start = Board::startpos();
    assert_ne!(
        start.pawn_key().get(),
        start.key().get(),
        "the pawn key must not be a copy of the position key"
    );
    let pawnless = ok(VALID);
    assert_eq!(
        pawnless.pawn_key().get(),
        0,
        "a position with no pawns has the empty pawn key, so a pawn-hash cache must not \
         use 0 as its empty sentinel"
    );
    assert_eq!(
        start.pieces(PieceKind::Pawn).count(),
        16,
        "the start position has sixteen pawns"
    );
}
