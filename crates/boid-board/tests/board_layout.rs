//! Cycle 2, layout: what `Board` costs, what it derives, and what it deliberately lacks.
//!
//! These are acceptance criteria 3 and 7. Both are one-line claims in the issue and both
//! have a vacuous reading, so each is asserted twice: once in the issue's own words, and
//! once in a form that fails for the right reason.

use std::mem::{align_of, size_of};

use boid_board::board::{Board, POSITION_MASK};
use boid_board::{Bitboard, CastlingRights, Color, File, Piece, PieceKind, Square};

// ---------------------------------------------------------------------------------
// Acceptance criterion 3 — size
// ---------------------------------------------------------------------------------

/// The issue's literal wording.
///
/// Kept exactly as written even though it is the weaker of the two assertions: a criterion
/// is evidenced by testing what it says, not by testing something adjacent and claiming it
/// covers the case.
#[test]
fn size_of_board_is_within_the_issues_ceiling() {
    assert!(
        size_of::<Board>() <= 256,
        "size_of::<Board>() is {}, which exceeds the issue's 256-byte ceiling",
        size_of::<Board>()
    );
}

/// The exact size, because `<= 256` is satisfied by 120 as well as by 152.
///
/// A `Board` that had silently lost its 64-byte mailbox would be 88 bytes and would pass
/// the criterion above while being a different type from the one the issue describes.
#[test]
fn board_is_exactly_one_hundred_and_fifty_two_bytes() {
    assert_eq!(
        size_of::<Board>(),
        152,
        "6 piece bitboards (48) + 2 colour bitboards (16) + mailbox (64) + key (8) + \
         pawn key (8) + packed state (8)"
    );
}

/// The same claim again at compile time, where the failure names the new size.
///
/// An array-length mismatch is reported as `expected an array with a fixed size of 152
/// elements, found one with N elements`, so a layout change says what it changed to
/// without anyone running a test.
const _BOARD_IS_152_BYTES: [(); 152] = [(); size_of::<Board>()];

#[test]
fn board_alignment_is_eight_with_no_padding() {
    assert_eq!(align_of::<Board>(), 8);
    // 152 is a multiple of 8, so the fields pack with nothing wasted between them.
    assert_eq!(size_of::<Board>() % align_of::<Board>(), 0);
}

/// The mailbox is `[Option<Piece>; 64]`, not the issue's literal `[u8; 64]`.
///
/// It is the same 64 bytes because `Piece` has twelve variants and the compiler uses one
/// of the four unused discriminants as `None`'s niche. Asserting it converts a compiler
/// optimisation this project is relying on into a loud failure if a future toolchain stops
/// doing it, rather than a silent 64-byte growth.
#[test]
fn the_option_piece_niche_costs_nothing() {
    assert_eq!(size_of::<Piece>(), 1);
    assert_eq!(size_of::<Option<Piece>>(), 1);
    assert_eq!(size_of::<[Option<Piece>; 64]>(), 64);
}

// ---------------------------------------------------------------------------------
// Acceptance criterion 7 — Copy, and no unmake_move
// ---------------------------------------------------------------------------------

/// `Board: Copy`, as a bound rather than as a runtime check.
///
/// If someone later adds a `Vec<u64>` repetition history to `Board` — which is exactly the
/// thing the issue warns against, since the history belongs in the search stack — this
/// stops compiling.
#[test]
fn board_is_copy() {
    fn requires_copy<T: Copy>() {}
    requires_copy::<Board>();
}

/// There is no `unmake_move`, as a *method-resolution* fact rather than a grep.
///
/// An inherent method shadows a blanket-trait one. So if `Board` ever grew an inherent
/// `unmake_move`, the call below would resolve to it instead, and the assertion would fail
/// (or the file would stop compiling, if its signature differed). The same probe covers
/// the obvious synonyms, because the criterion is about the *capability*, not the spelling.
#[test]
fn there_is_no_unmake_move_in_the_public_api() {
    trait AbsentUndoApi {
        fn unmake_move(&self) -> &'static str {
            "absent"
        }
        fn undo_move(&self) -> &'static str {
            "absent"
        }
        fn unapply_move(&self) -> &'static str {
            "absent"
        }
        fn take_back(&self) -> &'static str {
            "absent"
        }
    }
    impl<T> AbsentUndoApi for T {}

    let board = Board::startpos();
    assert_eq!(board.unmake_move(), "absent");
    assert_eq!(board.undo_move(), "absent");
    assert_eq!(board.unapply_move(), "absent");
    assert_eq!(board.take_back(), "absent");
}

/// The same criterion from the other side: nothing in the crate's sources declares one.
///
/// The probe above cannot see a free function or a trait method; this can. Reads the source
/// tree the way `gitignore_anchoring.rs` reads the ignore file — the crate directory is
/// known at compile time.
#[test]
fn no_source_file_declares_an_unmake_function() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the crate's src directory is readable") {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a readable source file");
            for (number, line) in text.lines().enumerate() {
                let squashed = line.replace(' ', "");
                if squashed.contains("fnunmake")
                    || squashed.contains("fnundo")
                    || squashed.contains("fnunapply")
                    || squashed.contains("fntake_back")
                {
                    offenders.push(format!(
                        "{}:{}: {}",
                        path.display(),
                        number + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "acceptance criterion 7 requires no unmake_move anywhere in the crate:\n{}",
        offenders.join("\n")
    );
}

// ---------------------------------------------------------------------------------
// The packed word
// ---------------------------------------------------------------------------------

/// The nine bits the zobrist key reads: side to move (1) + castling (4) + ep file (4).
#[test]
fn the_position_mask_is_the_low_nine_bits() {
    assert_eq!(POSITION_MASK, 0x1FF);
    assert_eq!(POSITION_MASK.count_ones(), 9);
    assert_eq!(POSITION_MASK.trailing_ones(), 9);
}

/// Every state word a real position produces leaves the reserved region clear.
#[test]
fn reserved_state_bits_are_clear_for_the_fixture_rows() {
    for fen in fixture_fens() {
        let board = Board::from_fen(&fen).unwrap_or_else(|e| panic!("{fen:?} must parse: {e}"));
        assert_eq!(
            board.state_word() >> 41,
            0,
            "{fen:?} sets a reserved bit: {:#018x}",
            board.state_word()
        );
        board
            .check_invariants()
            .unwrap_or_else(|e| panic!("{fen:?} violates an invariant: {e}"));
    }
}

// ---------------------------------------------------------------------------------
// The primitives the rest of the crate is typed in
// ---------------------------------------------------------------------------------

/// LERF, asserted at the corners and at the two king home squares.
#[test]
fn squares_are_lerf_numbered() {
    assert_eq!(Square::A1.index(), 0);
    assert_eq!(Square::H1.index(), 7);
    assert_eq!(Square::A8.index(), 56);
    assert_eq!(Square::H8.index(), 63);
    assert_eq!(Square::E1.index(), 4);
    assert_eq!(Square::E8.index(), 60);
    assert_eq!(Square::from_uci("e4").expect("e4 is a square").index(), 28);
    assert_eq!(Square::A1.to_string(), "a1");
    assert_eq!(Square::H8.to_string(), "h8");
}

/// The piece numbering D-0019 froze, checked at both ends and in the middle.
#[test]
fn pieces_are_numbered_kind_times_two_plus_colour() {
    for kind in PieceKind::ALL {
        for color in Color::ALL {
            let piece = Piece::new(color, kind);
            assert_eq!(
                piece.index(),
                kind.index() * 2 + color.index(),
                "{piece:?} is not at kind * 2 + colour"
            );
            assert_eq!(piece.kind(), kind);
            assert_eq!(piece.color(), color);
        }
    }
    assert_eq!(Piece::WhitePawn.index(), 0);
    assert_eq!(Piece::BlackPawn.index(), 1);
    assert_eq!(Piece::BlackKing.index(), 11);
    assert_eq!(Piece::COUNT * Square::COUNT, 768);
}

/// `is_pawn` is the `< 2` comparison the ordering exists to buy, and it agrees with the
/// general form for every piece.
#[test]
fn is_pawn_agrees_with_the_kind_for_every_piece() {
    for piece in Piece::ALL {
        assert_eq!(
            piece.is_pawn(),
            piece.kind() == PieceKind::Pawn,
            "{piece:?}"
        );
    }
}

#[test]
fn fen_letters_round_trip_for_every_piece() {
    for piece in Piece::ALL {
        assert_eq!(Piece::from_fen_char(piece.to_fen_char()), Some(piece));
    }
    assert_eq!(Piece::from_fen_char('x'), None);
    assert_eq!(Piece::from_fen_char('1'), None);
}

#[test]
fn castling_rights_render_in_canonical_order() {
    assert_eq!(CastlingRights::ALL.to_string(), "KQkq");
    assert_eq!(CastlingRights::NONE.to_string(), "-");
    assert_eq!(
        CastlingRights::NONE
            .with(CastlingRights::BLACK_QUEEN)
            .with(CastlingRights::WHITE_QUEEN)
            .to_string(),
        "Qq"
    );
    assert_eq!(CastlingRights::EACH.len(), 4);
}

#[test]
fn files_round_trip_through_their_letters() {
    for i in 0..File::COUNT {
        let file = File::new(i as u8).expect("index is below 8");
        assert_eq!(File::from_char(file.to_char()), Some(file));
    }
    assert_eq!(File::from_char('i'), None);
    assert_eq!(File::new(8), None);
}

/// Guards on the primitives that a reviewer removed with the suite still green.
#[test]
fn the_primitive_range_checks_actually_reject() {
    // offset_rank must decline both ends, not only the bottom.
    assert_eq!(Square::A1.offset_rank(-1), None);
    assert_eq!(Square::H8.offset_rank(1), None);
    assert_eq!(
        Square::A1.offset_rank(7).map(|s| s.to_string()),
        Some("a8".into())
    );
    assert_eq!(Square::A1.offset_rank(8), None);

    // from_uci is length-checked, so a longer coordinate is not silently truncated.
    assert_eq!(Square::from_uci("e"), None);
    assert_eq!(Square::from_uci("e44"), None);
    assert_eq!(Square::from_uci(""), None);
    assert_eq!(Square::from_uci("e9"), None);
    assert_eq!(Square::from_uci("i4"), None);

    // contains is a SUBSET test, not "any bit in common". A reviewer replaced it with the
    // latter and nothing disagreed.
    let both = CastlingRights::WHITE_KING.with(CastlingRights::BLACK_QUEEN);
    assert!(both.contains(CastlingRights::WHITE_KING));
    assert!(both.contains(both));
    assert!(!both.contains(CastlingRights::ALL));
    assert!(
        !CastlingRights::WHITE_KING.contains(both),
        "a single right does not contain a pair that merely overlaps it"
    );
    assert!(CastlingRights::ALL.contains(CastlingRights::NONE));
}

/// `Bitboard::squares` is an exact-size, cloneable iterator — not an opaque one.
///
/// The `ExactSizeIterator` impl existed but was unreachable: `squares()` returned
/// `impl Iterator`, which leaks only auto traits, so `.len()` did not compile for any
/// caller. Issue #5 counts bitboard populations constantly.
#[test]
fn bitboard_squares_is_an_exact_size_iterator() {
    let board = Board::startpos();
    let pawns = board.pieces(PieceKind::Pawn);
    assert_eq!(pawns.squares().len(), 16);
    assert_eq!(pawns.squares().len() as u32, pawns.count());

    // And it is cloneable, so an attack set can be walked twice without recomputing.
    let iter = pawns.squares();
    let first: Vec<Square> = iter.clone().collect();
    let second: Vec<Square> = iter.collect();
    assert_eq!(first, second);
    assert_eq!(first.len(), 16);

    // Ascending LERF order, and the empty set yields nothing.
    assert!(first.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(Bitboard::EMPTY.squares().len(), 0);
    assert_eq!(Bitboard::EMPTY.squares().next(), None);
}

/// The seven fixture rows, read from the committed oracle rather than retyped.
fn fixture_fens() -> Vec<String> {
    boid_board::perft::oracle::parse(boid_board::perft::oracle::ORACLE_TEXT)
        .expect("the committed fixture parses")
        .into_iter()
        .map(|case| case.fen.to_owned())
        .collect()
}
