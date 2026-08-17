//! FEN parse and emit: does the string survive the trip, and does the position?
//!
//! AC1's inputs are not retyped here. They are read out of `tests/fixtures/perft_oracle.txt`
//! through `oracle::parse`, because that file is SHA-256-pinned and independently
//! transcribed (D-0007), while a literal in this file is something a developer edits when a
//! test goes red.
//!
//! What a round-trip proves, stated plainly because it is less than it looks: it shows parse
//! and emit are mutual inverses, not that either is *correct*. Any bijection applied
//! symmetrically — a rank mirror, a file mirror, a colour inversion, a castling bit-order
//! reversal — round-trips byte-identically forever. So orientation and content are pinned
//! separately here, against literal squares and literal clocks, and against Stockfish in
//! `fen_stockfish_agreement.rs`.

use boid_board::board::Board;
use boid_board::fen::{FEN_MAX_LEN, FenLayout};
use boid_board::perft::oracle::{self, ORACLE_TEXT};
use boid_board::types::{
    CastlingRight, CastlingRights, Colour, File, Piece, PieceKind, Rank, Square,
};

/// Every FEN in the committed oracle fixture, with the id it is filed under.
fn fixture_fens() -> Vec<(&'static str, &'static str)> {
    let cases = oracle::parse(ORACLE_TEXT).expect("the committed fixture must parse");
    assert_eq!(
        cases.len(),
        7,
        "the fixture holds the six standard positions plus the colour-mirror of position 4"
    );
    cases.into_iter().map(|case| (case.id, case.fen)).collect()
}

fn fixture_fen(id: &str) -> &'static str {
    fixture_fens()
        .into_iter()
        .find(|(case_id, _)| *case_id == id)
        .unwrap_or_else(|| panic!("{id} is not in the fixture"))
        .1
}

#[test]
fn all_seven_fixture_fens_round_trip_byte_identically() {
    // AC1's first half. Compared as whole strings, with no trimming, splitting or
    // normalising on either side — the criterion says byte-identical.
    for (id, fen) in fixture_fens() {
        let (board, layout) = Board::from_fen_with_layout(fen)
            .unwrap_or_else(|e| panic!("{id}: the fixture FEN must parse, got {e}"));
        assert_eq!(
            board.to_fen_with_layout(layout),
            fen,
            "{id} did not round-trip"
        );
        assert_eq!(board.consistency(), Ok(()), "{id}");
    }
}

#[test]
fn kiwipete_round_trips_in_its_published_four_field_form() {
    // Found by id rather than by filtering on field count: a filter that quietly excluded
    // the hard row would leave this test green and AC1 unproven for the one position that
    // makes it interesting.
    let fen = fixture_fen("kiwipete");
    assert_eq!(fen.split(' ').count(), 4, "as published, without counters");

    let (board, layout) = Board::from_fen_with_layout(fen).expect("four fields must parse");
    assert_eq!(layout, FenLayout::FourField);
    assert_eq!(board.to_fen_with_layout(layout), fen);

    // And the six-field form of the same position is the four-field form plus " 0 1".
    assert_eq!(board.to_fen(), format!("{fen} 0 1"));
}

#[test]
fn four_field_parse_defaults_the_clocks() {
    let (board, _) =
        Board::from_fen_with_layout(fixture_fen("kiwipete")).expect("four fields must parse");
    assert_eq!(board.halfmove_clock(), 0);
    assert_eq!(
        board.fullmove_number(),
        1,
        "a defaulted fullmove of 0 is a textbook off-by-one, and FEN numbering starts at 1"
    );
}

#[test]
fn four_field_emission_is_documented_lossy() {
    // Documented rather than discovered. Emitting four fields from a board whose clocks are
    // not the defaults throws them away; the alternative — making it fallible — would put a
    // Result in the path of reproducing a published FEN, which is the only reason the
    // four-field form exists here.
    let mut board = Board::startpos();
    board.set_halfmove_clock(7);
    board.set_fullmove_number(19);

    let four = board.to_fen_with_layout(FenLayout::FourField);
    let (reparsed, layout) = Board::from_fen_with_layout(&four).expect("valid FEN");
    assert_eq!(layout, FenLayout::FourField);
    assert_eq!(reparsed.halfmove_clock(), 0);
    assert_eq!(reparsed.fullmove_number(), 1);
    assert_eq!(
        reparsed.key(),
        board.key(),
        "the clocks are not hashed, so the key survives what the string loses"
    );
}

#[test]
fn startpos_would_not_round_trip_under_clock_sniffing() {
    // A tempting way to resolve the four-field problem is to omit the counters whenever
    // they are 0 and 1. The starting position IS 0 and 1, so that rule would emit four
    // fields for the most-quoted FEN in chess and fail its own round-trip (D-0024).
    let emitted = Board::startpos().to_fen();
    assert_eq!(emitted.split(' ').count(), 6);
    assert!(emitted.ends_with(" 0 1"), "got {emitted}");
    assert_eq!(
        emitted,
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
    assert_eq!(emitted, fixture_fen("startpos"));
}

#[test]
fn position5_clocks_are_one_and_eight() {
    // The only fixture row whose clocks differ from each other and are neither 0 nor 1. A
    // halfmove/fullmove swap applied symmetrically in the parser and the emitter round-trips
    // byte-identically on every other row and on every generated position; this is where it
    // dies.
    let board = Board::from_fen(fixture_fen("position5")).expect("position5 must parse");
    assert_eq!(board.halfmove_clock(), 1);
    assert_eq!(board.fullmove_number(), 8);
}

#[test]
fn position4_mirror_is_black_to_move_at_fullmove_one() {
    let board = Board::from_fen(fixture_fen("position4-mirror")).expect("must parse");
    assert_eq!(board.side_to_move(), Colour::Black);
    assert_eq!(board.fullmove_number(), 1);
    assert_eq!(
        board.castling(),
        CastlingRights::NONE
            .with(CastlingRight::WhiteKingside)
            .with(CastlingRight::WhiteQueenside),
        "the mirror keeps White's rights, which is what a colour mirror does to KQ"
    );

    let original = Board::from_fen(fixture_fen("position4")).expect("must parse");
    assert_eq!(original.side_to_move(), Colour::White);
    assert_ne!(
        original.key(),
        board.key(),
        "the colour-mirror is a different position, however identical its perft counts are"
    );
}

#[test]
fn startpos_and_kiwipete_squares_are_concrete() {
    // Content, not shape. Kiwipete is asymmetric in every direction, so a rank mirror, a
    // file mirror, a colour inversion and a case swap each move at least one of these.
    let start = Board::from_fen(fixture_fen("startpos")).expect("must parse");
    assert_eq!(start, Board::startpos(), "the two constructions agree");

    let (kiwipete, _) = Board::from_fen_with_layout(fixture_fen("kiwipete")).expect("must parse");
    assert_eq!(kiwipete.piece_at(Square::D5), Some(Piece::WhitePawn));
    assert_eq!(kiwipete.piece_at(Square::E5), Some(Piece::WhiteKnight));
    assert_eq!(kiwipete.piece_at(Square::A6), Some(Piece::BlackBishop));
    assert_eq!(kiwipete.piece_at(Square::H3), Some(Piece::BlackPawn));
    assert_eq!(kiwipete.piece_at(Square::E1), Some(Piece::WhiteKing));
    assert_eq!(kiwipete.piece_at(Square::E8), Some(Piece::BlackKing));
    assert_eq!(kiwipete.castling(), CastlingRights::ALL);
    assert_eq!(kiwipete.side_to_move(), Colour::White);
}

#[test]
fn en_passant_targets_for_both_colours() {
    // Sixteen rows: eight files by two sides to move. An en-passant rank rule inverted in
    // BOTH directions -- rank 6 when Black is to move, rank 3 when White is -- round-trips
    // perfectly, and the key uses the file only, so no key test can see it either.
    for file in File::ALL {
        for stm in Colour::ALL {
            let (target_rank, pusher_rank, placement) = match stm {
                // Black to move: White has just pushed, target on rank 3, pawn on rank 4.
                Colour::Black => (Rank::R3, Rank::R4, "4k3/8/8/8/{}/8/8/4K3"),
                // White to move: Black has just pushed, target on rank 6, pawn on rank 5.
                Colour::White => (Rank::R6, Rank::R5, "4k3/8/8/{}/8/8/8/4K3"),
            };
            let pawn = match stm {
                Colour::Black => 'P',
                Colour::White => 'p',
            };
            let mut rank_field = String::new();
            let index = file.index();
            if index > 0 {
                rank_field.push_str(&index.to_string());
            }
            rank_field.push(pawn);
            if index < 7 {
                rank_field.push_str(&(7 - index).to_string());
            }

            let target = Square::from_file_rank(file, target_rank);
            let fen = format!(
                "{} {} - {} 0 1",
                placement.replace("{}", &rank_field),
                stm.to_char(),
                target
            );

            let board = Board::from_fen(&fen).unwrap_or_else(|e| panic!("{fen}: {e}"));
            assert_eq!(board.en_passant_file(), Some(file), "{fen}");
            assert_eq!(board.en_passant_target(), Some(target), "{fen}");
            assert_eq!(
                board.piece_at(Square::from_file_rank(file, pusher_rank)),
                Some(Piece::new(stm.flip(), PieceKind::Pawn)),
                "{fen}: the pawn that pushed"
            );
            assert_eq!(board.to_fen(), fen, "{fen} did not round-trip");
        }
    }
}

#[test]
fn en_passant_pin_position_parses() {
    // position3 after e2e4. The en-passant capture f4xe3 is illegal -- the black f4 pawn is
    // pinned along the fourth rank by the b4 rook against the h4 king -- and Stockfish
    // generates 16 moves here, none of them f4e3. The FEN is still valid and this parser
    // must accept it: requiring an ep capture to be LEGAL needs move generation, which is
    // issue #5's, and it would make from_fen unable to load a position handed to it
    // mid-analysis (D-0021).
    let fen = "8/2p5/3p4/KP5r/1R2Pp1k/8/6P1/8 b - e3 0 1";
    let board = Board::from_fen(fen).expect("a pinned en-passant capture is still a FEN");
    assert_eq!(board.en_passant_target(), Some(Square::E3));
    assert_eq!(board.piece_at(Square::E4), Some(Piece::WhitePawn));
    assert_eq!(board.to_fen(), fen);
}

#[test]
fn parse_does_not_normalise_castling_or_en_passant() {
    // Stockfish drops an en-passant square whose capture is unavailable, and rewrites
    // castling rights it considers unexercisable. Either behaviour here would break AC1 on
    // the very inputs it is most likely to matter for.
    let unusable_ep = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq c6 0 2";
    let board = Board::from_fen(unusable_ep).expect("must parse");
    assert_eq!(board.en_passant_file(), Some(File::C));
    assert_eq!(board.to_fen(), unusable_ep, "the ep square must survive");

    let rights = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
    let board = Board::from_fen(rights).expect("must parse");
    assert_eq!(board.castling(), CastlingRights::ALL);
    assert_eq!(board.to_fen(), rights);
}

#[test]
fn en_passant_file_alone_decides_the_key() {
    // AC6, through the FEN layer. Three FENs identical but for the en-passant field, and
    // three distinct keys -- the same triple Stockfish itself distinguishes, which is what
    // makes the choice of convention observable rather than merely asserted.
    let base = "rnbqkbnr/pp2p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq";
    let d6 = Board::from_fen(&format!("{base} d6 0 4")).expect("must parse");
    let f6 = Board::from_fen(&format!("{base} f6 0 4")).expect("must parse");
    let none = Board::from_fen(&format!("{base} - 0 4")).expect("must parse");

    assert_ne!(d6.key(), f6.key(), "different en-passant files");
    assert_ne!(d6.key(), none.key());
    assert_ne!(f6.key(), none.key());
    assert_eq!(d6.pawn_key(), f6.pawn_key(), "the pawns are the same");

    // Clause 2, in its sharpest form: two boards whose FENs are byte-identical hash
    // identically, because nothing outside the emitted string reaches the key.
    for board in [d6, f6, none] {
        let reparsed = Board::from_fen(&board.to_fen()).expect("emitted FEN must re-parse");
        assert_eq!(reparsed.key(), board.key());
        assert_eq!(reparsed, board);
    }
}

#[test]
fn from_fen_returns_a_fresh_board() {
    // A parser that built onto whatever was in the destination would pass every test that
    // parses into a new variable, and fail the first time issue #7 reuses a Board.
    let mut board = Board::from_fen(fixture_fen("position6")).expect("must parse");
    assert_eq!(
        board.occupied().count(),
        32,
        "position6 is a full-material middlegame"
    );
    board = Board::from_fen(fixture_fen("position3")).expect("must parse");
    assert_eq!(
        board,
        Board::from_fen(fixture_fen("position3")).expect("must parse")
    );
    assert_eq!(
        board.occupied().count(),
        10,
        "position3 is a ten-piece endgame"
    );
}

#[test]
fn fen_max_len_is_tight() {
    // The constant is what `to_fen` reserves. If it were generously large the "never
    // reallocates" property it exists for would be vacuous, so it is asserted to be exactly
    // reachable -- and to be reached only by the worst case.
    assert_eq!(FEN_MAX_LEN, 91);

    let mut crowded = Board::empty();
    for index in 0..64u8 {
        let square = Square::new(index).expect("below 64");
        crowded.place(square, Piece::WhiteQueen);
    }
    crowded.set_halfmove_clock(u8::MAX);
    crowded.set_fullmove_number(u16::MAX);
    crowded.set_castling(CastlingRights::ALL);
    // An en-passant square costs two characters where "-" costs one. No legal position has
    // both a full board and an en-passant target — but the editing primitives can build
    // one, and the buffer has to cover what the emitter can be handed, not what a legal
    // game can reach.
    crowded.set_en_passant(Some(File::E));
    let longest = crowded.to_fen();
    assert_eq!(longest.len(), FEN_MAX_LEN, "{longest}");
    assert_eq!(
        crowded.to_fen_with_layout(FenLayout::FourField).len(),
        FEN_MAX_LEN - " 255 65535".len()
    );

    for (id, fen) in fixture_fens() {
        assert!(fen.len() <= FEN_MAX_LEN, "{id} is longer than the maximum");
    }
}

#[test]
fn emitted_fens_are_ascii() {
    // D-0008's hazard, from our side of the wire: a FEN this crate emits is a FEN Stockfish
    // will be asked to parse in issue #6, and a non-ASCII separator there is a silent
    // wrong-answer path rather than an error.
    for (id, fen) in fixture_fens() {
        let (board, layout) = Board::from_fen_with_layout(fen).expect("must parse");
        assert!(board.to_fen().is_ascii(), "{id}");
        assert!(board.to_fen_with_layout(layout).is_ascii(), "{id}");
    }
}

#[test]
fn write_fen_agrees_with_to_fen() {
    // `to_fen` is a thin wrapper over `write_fen`; pinned so it cannot become a second
    // implementation that drifts.
    for (id, fen) in fixture_fens() {
        let (board, layout) = Board::from_fen_with_layout(fen).expect("must parse");
        let mut buffer = String::new();
        board
            .write_fen(&mut buffer, layout)
            .expect("writing into a String is infallible");
        assert_eq!(buffer, board.to_fen_with_layout(layout), "{id}");
    }
}

#[test]
fn castling_rights_survive_in_fen_order() {
    // All sixteen masks, on a board where every right is backed by its king and rook.
    for bits in 0..16u8 {
        let rights = CastlingRights::from_bits(bits).expect("four bits");
        let mut field = String::new();
        for right in CastlingRight::ALL {
            if rights.has(right) {
                field.push(right.to_char());
            }
        }
        if field.is_empty() {
            field.push('-');
        }

        let fen = format!("r3k2r/8/8/8/8/8/8/R3K2R w {field} - 0 1");
        let board = Board::from_fen(&fen).unwrap_or_else(|e| panic!("{fen}: {e}"));
        assert_eq!(board.castling(), rights, "{fen}");
        assert_eq!(board.to_fen(), fen, "{fen} did not round-trip");
    }
}
