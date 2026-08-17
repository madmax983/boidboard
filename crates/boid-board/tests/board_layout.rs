//! What the board is made of: its size, its encodings, and the two positions that need no
//! FEN parser to describe.
//!
//! AC3 asks for `size_of::<Board>() <= 256`. That inequality alone is satisfied by a struct
//! with no fields at all, so it is asserted here alongside the exact measured size and the
//! liveness assertions that make a hollow board impossible: the starting position's
//! bitboards, read against published literals, and the empty board's key.
//!
//! The encodings pinned here — LERF squares, `Piece`'s discriminant as its zobrist index,
//! `CastlingRights` in FEN `KQkq` bit order — are load-bearing for the zobrist keys and for
//! issue #5's move generation. Each is checked against an independently written expression
//! of the same rule rather than against the implementation's own constant.

use boid_board::bitboard::Bitboard;
use boid_board::board::Board;
use boid_board::types::{
    CastlingRight, CastlingRights, Colour, File, Piece, PieceKind, Rank, Square,
};

#[test]
fn board_is_one_hundred_and_fifty_two_bytes() {
    // Both halves matter. `<= 256` is the acceptance criterion; `== 152` is the claim
    // D-0019 makes, and D-0016's rule is that a numeric claim about this project is
    // asserted by a test or not made at all. The inequality on its own is true of a
    // `struct Board;` and of a board that stores nothing but a FEN string.
    assert_eq!(
        size_of::<Board>(),
        152,
        "D-0019 records the layout as 152 bytes; if this changed deliberately, the entry \
         needs a superseding one"
    );
    assert!(size_of::<Board>() <= 256, "AC3");
    assert_eq!(align_of::<Board>(), 8, "u64 keys and bitboards");
}

#[test]
fn option_piece_is_one_byte() {
    // The whole "[Option<Piece>; 64] is a narrowing of [u8; 64]" argument rests on this,
    // and it is a niche optimisation rather than a language guarantee, so it is checked
    // rather than assumed.
    assert_eq!(size_of::<Option<Piece>>(), 1);
    assert_eq!(size_of::<[Option<Piece>; 64]>(), 64);
    assert_eq!(size_of::<Piece>(), 1);
}

#[test]
fn board_is_copy() {
    // AC7. This is a compile-time assertion wearing a test's clothes: removing `Copy` from
    // Board breaks the build rather than reddening this test. The behavioural half — that a
    // copy is independent of its original — is in board_consistency.rs, where a primitive
    // exists to mutate one of them.
    fn assert_copy<T: Copy>() {}
    assert_copy::<Board>();
}

#[test]
fn square_new_accepts_exactly_the_sixty_four_indices() {
    for index in 0..=u8::MAX {
        assert_eq!(
            Square::new(index).is_some(),
            index < 64,
            "Square::new({index})"
        );
    }
    // And the index survives the round trip, so a constructor that accepted the right count
    // of wrong squares would still fail.
    for index in 0..64u8 {
        assert_eq!(Square::new(index).expect("below 64").index(), index);
    }
}

#[test]
fn square_corners_are_lerf() {
    // Little-Endian Rank-File: index = rank * 8 + file, a1 = 0, h8 = 63. The alternative
    // convention (a8 = 0) is equally common in published code and produces a board that is
    // mirrored top to bottom -- which round-trips through FEN perfectly and gives the wrong
    // answer for every pawn move.
    assert_eq!(Square::A1.index(), 0);
    assert_eq!(Square::H1.index(), 7);
    assert_eq!(Square::A8.index(), 56);
    assert_eq!(Square::H8.index(), 63);
    assert_eq!(Square::E1.index(), 4);
    assert_eq!(Square::E4.index(), 28);
    assert_eq!(Square::E3.index(), 20, "White's ep target after e2e4");
    assert_eq!(Square::E6.index(), 44, "Black's ep target after e7e5");

    assert_eq!(Square::from_file_rank(File::E, Rank::R4), Square::E4);
    assert_eq!(Square::E4.file(), File::E);
    assert_eq!(Square::E4.rank(), Rank::R4);
    assert_eq!(Square::E4.to_string(), "e4");
}

#[test]
fn piece_discriminants_are_the_zobrist_index() {
    // D-0020 fixes the piece index as `colour * 6 + kind` and D-0019 makes `Piece`'s
    // discriminant that same number, so there is one map rather than two. If this ever
    // drifts, every pinned position key in zobrist_incremental.rs is wrong -- and the table
    // digest cannot see it, because the table does not change.
    for colour in Colour::ALL {
        for kind in PieceKind::ALL {
            let piece = Piece::new(colour, kind);
            assert_eq!(
                piece.index(),
                colour.index() * 6 + kind.index(),
                "{piece:?} index"
            );
            assert_eq!(piece.colour(), colour, "{piece:?} colour");
            assert_eq!(piece.kind(), kind, "{piece:?} kind");
        }
    }
    assert_eq!(Piece::WhitePawn.index(), 0);
    assert_eq!(Piece::BlackPawn.index(), 6);
    assert_eq!(Piece::BlackKing.index(), 11);
    assert_eq!(Piece::ALL.len(), 12);
}

#[test]
fn piece_chars_round_trip_and_are_case_split_by_colour() {
    for piece in Piece::ALL {
        assert_eq!(Piece::from_char(piece.to_char()), Some(piece));
        assert_eq!(
            piece.to_char().is_ascii_uppercase(),
            piece.colour() == Colour::White,
            "{piece:?} case"
        );
    }
    assert_eq!(Piece::from_char('x'), None);
    assert_eq!(Piece::from_char('1'), None);
}

#[test]
fn castling_bits_are_fen_order() {
    // K=1, Q=2, k=4, q=8, matching the order the four zobrist castling keys are laid out
    // in. A transposed bit order hashes and round-trips self-consistently and breaks issue
    // #5's castling generation, which is a long way from here.
    assert_eq!(CastlingRights::NONE.bits(), 0);
    assert_eq!(CastlingRights::ALL.bits(), 0b1111);
    for (expected_bit, right) in CastlingRight::ALL.iter().enumerate() {
        assert_eq!(right.index(), expected_bit, "{right:?} index");
        assert_eq!(
            CastlingRights::NONE.with(*right).bits(),
            1 << expected_bit,
            "{right:?} bit"
        );
        assert!(CastlingRights::ALL.has(*right));
        assert!(!CastlingRights::ALL.without(*right).has(*right));
    }
    assert_eq!(
        CastlingRight::ALL.map(CastlingRight::to_char),
        ['K', 'Q', 'k', 'q']
    );
}

#[test]
fn castling_rights_imply_home_squares() {
    // Issue #5 consumes these. A swapped king/rook mapping is invisible to everything in
    // issue #4 and produces illegal castling moves in issue #6.
    assert_eq!(CastlingRight::WhiteKingside.king_from(), Square::E1);
    assert_eq!(CastlingRight::WhiteKingside.rook_from(), Square::H1);
    assert_eq!(CastlingRight::WhiteKingside.king_to(), Square::G1);
    assert_eq!(CastlingRight::WhiteKingside.rook_to(), Square::F1);

    assert_eq!(CastlingRight::WhiteQueenside.rook_from(), Square::A1);
    assert_eq!(CastlingRight::WhiteQueenside.king_to(), Square::C1);
    assert_eq!(CastlingRight::WhiteQueenside.rook_to(), Square::D1);

    assert_eq!(CastlingRight::BlackKingside.king_from(), Square::E8);
    assert_eq!(CastlingRight::BlackKingside.rook_from(), Square::H8);
    assert_eq!(CastlingRight::BlackKingside.king_to(), Square::G8);

    assert_eq!(CastlingRight::BlackQueenside.rook_from(), Square::A8);
    assert_eq!(CastlingRight::BlackQueenside.king_to(), Square::C8);
    assert_eq!(CastlingRight::BlackQueenside.rook_to(), Square::D8);
}

#[test]
fn castling_rights_from_bits_rejects_out_of_range() {
    assert!(CastlingRights::from_bits(0b1111).is_some());
    assert!(CastlingRights::from_bits(0b1_0000).is_none());
    assert!(CastlingRights::from_bits(u8::MAX).is_none());
}

#[test]
fn empty_board_is_empty_and_consistent() {
    let board = Board::empty();

    assert!(board.occupied().is_empty(), "no pieces");
    for kind in PieceKind::ALL {
        assert!(board.pieces(kind).is_empty(), "{kind:?}");
    }
    for colour in Colour::ALL {
        assert!(board.colours(colour).is_empty(), "{colour:?}");
        assert_eq!(board.king_square(colour), None, "{colour:?} king");
    }
    for index in 0..64u8 {
        let square = Square::new(index).expect("below 64");
        assert_eq!(board.piece_at(square), None, "{square}");
    }

    // A zobrist key is the XOR of the contributions a position makes, and this one makes
    // none: no pieces, White to move, no rights, no ep file.
    assert_eq!(board.key(), 0);
    assert_eq!(board.pawn_key(), 0);
    assert_eq!(board.side_to_move(), Colour::White);
    assert_eq!(board.castling(), CastlingRights::NONE);
    assert_eq!(board.en_passant_file(), None);
    assert_eq!(board.en_passant_target(), None);
    assert_eq!(board.halfmove_clock(), 0);
    assert_eq!(
        board.fullmove_number(),
        1,
        "FEN fullmove numbers start at 1"
    );
    assert_eq!(board.consistency(), Ok(()));
}

#[test]
fn startpos_bitboards_are_the_published_layout() {
    let board = Board::startpos();

    // Literals rather than expressions built from the board itself: this is the one place
    // orientation is pinned, and an expression derived from the same code being tested
    // would agree with it whichever way up it was.
    assert_eq!(
        board.colours(Colour::White),
        Bitboard::from_bits(0x0000_0000_0000_FFFF),
        "White occupies ranks 1 and 2"
    );
    assert_eq!(
        board.colours(Colour::Black),
        Bitboard::from_bits(0xFFFF_0000_0000_0000),
        "Black occupies ranks 7 and 8"
    );
    assert_eq!(
        board.pieces(PieceKind::Pawn) & board.colours(Colour::White),
        Bitboard::from_bits(0x0000_0000_0000_FF00),
        "White pawns on rank 2"
    );
    assert_eq!(
        board.pieces(PieceKind::Rook) & board.colours(Colour::White),
        Bitboard::from_bits(0x81),
        "White rooks on a1 and h1"
    );
    assert_eq!(
        board.pieces(PieceKind::King),
        Bitboard::from_bits(1 << 4 | 1 << 60),
        "kings on e1 and e8"
    );
    assert_eq!(board.occupied().count(), 32);
    assert_eq!(board.pieces(PieceKind::Pawn).count(), 16);

    assert_eq!(board.king_square(Colour::White), Some(Square::E1));
    assert_eq!(board.king_square(Colour::Black), Some(Square::E8));

    assert_eq!(board.side_to_move(), Colour::White);
    assert_eq!(board.castling(), CastlingRights::ALL);
    assert_eq!(board.en_passant_file(), None);
    assert_eq!(board.halfmove_clock(), 0);
    assert_eq!(board.fullmove_number(), 1);
    assert_eq!(board.consistency(), Ok(()));
}

#[test]
fn startpos_king_and_queen_are_not_mirrored() {
    // The starting position is symmetric under a file mirror in everything except this one
    // pair, so this is the only assertion about it that a left-right flip fails. Queens
    // start on their own colour: the white queen on d1, a dark square.
    let board = Board::startpos();
    assert_eq!(board.piece_at(Square::D1), Some(Piece::WhiteQueen));
    assert_eq!(board.piece_at(Square::E1), Some(Piece::WhiteKing));
    assert_eq!(board.piece_at(Square::D8), Some(Piece::BlackQueen));
    assert_eq!(board.piece_at(Square::E8), Some(Piece::BlackKing));

    // And the colour split, which a rank mirror would break.
    assert_eq!(board.piece_at(Square::A1), Some(Piece::WhiteRook));
    assert_eq!(board.piece_at(Square::A8), Some(Piece::BlackRook));
    assert_eq!(board.piece_at(Square::E2), Some(Piece::WhitePawn));
    assert_eq!(board.piece_at(Square::E7), Some(Piece::BlackPawn));
    assert_eq!(board.piece_at(Square::E4), None);
}

#[test]
fn occupied_agrees_with_a_mailbox_walk() {
    // Pinned against an independent computation rather than against a re-spelling of the
    // implementation: `occupied()` reads the colour bitboards, this walks the mailbox.
    for board in [Board::empty(), Board::startpos()] {
        let mut walked = Bitboard::EMPTY;
        for index in 0..64u8 {
            let square = Square::new(index).expect("below 64");
            if board.piece_at(square).is_some() {
                walked = walked.with(square);
            }
        }
        assert_eq!(board.occupied(), walked, "{board:?}");
    }
}

#[test]
fn board_debug_shows_the_placement_and_the_keys() {
    // Assertion messages are evidence in this repository, and a derived Debug on a board
    // with a 64-entry mailbox makes every failed `assert_eq!` between two boards unreadable.
    let rendered = format!("{:?}", Board::startpos());
    assert!(
        rendered.contains("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR"),
        "expected the placement, got {rendered}"
    );
    assert!(rendered.contains("key"), "expected the key, got {rendered}");
    assert!(
        !rendered.contains("None, None"),
        "expected no raw mailbox dump, got {rendered}"
    );
}
