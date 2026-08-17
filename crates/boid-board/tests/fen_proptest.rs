//! AC1's 200 generated positions, and proptest over adversarial input.
//!
//! Two generators, doing two different jobs.
//!
//! The **deterministic corpus** in `support/positions.rs` is AC1's evidence: exactly 200
//! legal positions, a pure function of one hardcoded seed, so a failure is reproducible by
//! running the test again rather than by hoping. Its diversity is asserted rather than
//! assumed — a generator that quietly narrowed to 200 near-empty boards would satisfy AC1's
//! wording and prove nothing, and the seven fixture FENs record no en-passant square at all,
//! so without the corpus there is *zero* en-passant coverage in AC1.
//!
//! **proptest** does what it is genuinely better at: throwing adversarial strings at the
//! parser and shrinking whatever survives to a minimal reproduction. That is AC2's half.

use std::collections::HashSet;

use boid_board::board::Board;
use boid_board::fen::{FEN_MAX_LEN, FenLayout};
use boid_board::types::{CastlingRight, Colour, File, Piece, PieceKind, Rank, Square};
use proptest::prelude::*;

#[allow(dead_code)]
mod support;
use support::naive_attacks::{is_in_check, is_square_attacked};
use support::positions::{CORPUS_SIZE, corpus, corpus_attempts};

#[test]
fn two_hundred_generated_positions_round_trip() {
    // AC1's second half, counted rather than configured: the length is asserted, so a
    // generator that returned three positions could not satisfy this by claiming 200.
    let positions = corpus();
    assert_eq!(positions.len(), CORPUS_SIZE);
    assert_eq!(CORPUS_SIZE, 200, "AC1 says 200");

    for (index, board) in positions.iter().enumerate() {
        let fen = board.to_fen();
        let (reparsed, layout) = Board::from_fen_with_layout(&fen).unwrap_or_else(|e| {
            panic!("position {index}: emitted {fen:?} but it will not parse: {e}")
        });

        assert_eq!(layout, FenLayout::SixField);
        assert_eq!(
            reparsed.to_fen(),
            fen,
            "position {index} did not round-trip"
        );
        assert_eq!(reparsed, *board, "position {index}: the board changed");
        assert_eq!(
            reparsed.key(),
            board.key(),
            "position {index}: the key changed"
        );
        assert_eq!(reparsed.pawn_key(), board.pawn_key());
        assert_eq!(reparsed.consistency(), Ok(()));
        assert!(fen.is_ascii());
        assert!(fen.len() <= FEN_MAX_LEN);
    }
}

#[test]
fn every_generated_position_is_legal_by_the_stated_definition() {
    // "Legal" is a claim, so it is spelled out and checked rather than asserted in prose.
    // What is NOT claimed: retrograde legality. No attempt is made to show these positions
    // are reachable from the initial one.
    for (index, board) in corpus().iter().enumerate() {
        for colour in Colour::ALL {
            assert!(
                board.king_square(colour).is_some(),
                "position {index}: {colour:?} has no king"
            );
            assert_eq!(
                (board.pieces(PieceKind::King) & board.colours(colour)).count(),
                1,
                "position {index}: {colour:?} has more than one king"
            );
            assert!(
                (board.pieces(PieceKind::Pawn) & board.colours(colour)).count() <= 8,
                "position {index}: too many {colour:?} pawns"
            );
            assert!(
                board.colours(colour).count() <= 16,
                "position {index}: too many {colour:?} pieces"
            );
        }

        for square in board.pieces(PieceKind::Pawn) {
            assert!(
                square.rank() != Rank::R1 && square.rank() != Rank::R8,
                "position {index}: a pawn on {square}"
            );
        }

        let (white, black) = (
            board.king_square(Colour::White).expect("a white king"),
            board.king_square(Colour::Black).expect("a black king"),
        );
        assert!(
            white.file().index().abs_diff(black.file().index()) > 1
                || white.rank().index().abs_diff(black.rank().index()) > 1,
            "position {index}: the kings are adjacent"
        );

        assert!(
            !is_in_check(board, board.side_to_move().flip()),
            "position {index}: the side that just moved is still in check"
        );
        assert_eq!(board.consistency(), Ok(()), "position {index}");
    }
}

#[test]
fn the_corpus_is_diverse_enough_to_mean_something() {
    // Without this, AC1 could pass on 200 near-identical boards. The floors are measured,
    // then pinned: if the generator narrows, a number moves and somebody has to look.
    let positions = corpus();

    let mut ep_files: HashSet<(File, Colour)> = HashSet::new();
    let mut castling_masks: HashSet<u8> = HashSet::new();
    let mut occupied_squares: HashSet<Square> = HashSet::new();
    let mut piece_squares: HashSet<(Piece, Square)> = HashSet::new();
    let (mut white_to_move, mut black_to_move) = (0usize, 0usize);
    let mut with_ep = 0usize;

    for board in &positions {
        if let Some(file) = board.en_passant_file() {
            ep_files.insert((file, board.side_to_move()));
            with_ep += 1;
        }
        castling_masks.insert(board.castling().bits());
        match board.side_to_move() {
            Colour::White => white_to_move += 1,
            Colour::Black => black_to_move += 1,
        }
        for square in board.occupied() {
            occupied_squares.insert(square);
            if let Some(piece) = board.piece_at(square) {
                piece_squares.insert((piece, square));
            }
        }
    }

    assert!(white_to_move >= 60, "white to move in {white_to_move}");
    assert!(black_to_move >= 60, "black to move in {black_to_move}");
    assert!(
        with_ep >= 10,
        "only {with_ep} positions record an ep square"
    );
    assert!(
        ep_files.len() >= 8,
        "en-passant coverage is {} (file, side) pairs",
        ep_files.len()
    );
    assert!(
        castling_masks.len() >= 8,
        "only {} distinct castling masks",
        castling_masks.len()
    );
    assert_eq!(
        occupied_squares.len(),
        64,
        "every square is used by something"
    );
    assert!(
        piece_squares.len() >= 600,
        "only {} of the 768 piece-square combinations appear",
        piece_squares.len()
    );
}

#[test]
fn the_legality_filter_actually_rejects_positions() {
    // A filter that never fires is a filter that is not running. The counts are pinned
    // because they move whenever the generator or the attack routine changes, and that is
    // exactly when somebody should be made to look.
    let (accepted, rejected) = corpus_attempts();
    assert_eq!(accepted, CORPUS_SIZE);
    assert!(
        rejected > 0,
        "the generator rejected nothing; either every attempt is legal by luck or the \
         filter is not running"
    );
    assert!(
        rejected < CORPUS_SIZE * 20,
        "the generator rejected {rejected} attempts for {accepted} positions, which is a \
         sign the constructor and the filter disagree about what a position is"
    );
}

#[test]
fn naive_attacks_matches_the_hand_table() {
    // Written before the routine it checks, and by hand, because the generator's definition
    // of "legal" rests on it and a file-wrap bug there would silently widen or narrow AC1's
    // corpus. Every attacker kind, both pawn directions, a blocked ray, and the board edges.
    let cases: [(&str, &str, Colour, bool); 14] = [
        // A white pawn on e4 attacks d5 and f5, and nothing behind it.
        ("4k3/8/8/8/4P3/8/8/4K3", "d5", Colour::White, true),
        ("4k3/8/8/8/4P3/8/8/4K3", "f5", Colour::White, true),
        ("4k3/8/8/8/4P3/8/8/4K3", "e5", Colour::White, false),
        ("4k3/8/8/8/4P3/8/8/4K3", "d3", Colour::White, false),
        // A black pawn attacks downward.
        ("4k3/8/8/4p3/8/8/8/4K3", "d4", Colour::Black, true),
        ("4k3/8/8/4p3/8/8/8/4K3", "d6", Colour::Black, false),
        // Knight, including a corner where a naive index offset would wrap.
        ("4k3/8/8/8/8/8/8/N3K3", "b3", Colour::White, true),
        ("4k3/8/8/8/8/8/8/N3K3", "c2", Colour::White, true),
        ("4k3/8/8/8/8/8/8/N3K3", "h2", Colour::White, false),
        // Rook along a file, and the same ray blocked.
        ("4k3/8/8/8/8/8/8/R3K3", "a8", Colour::White, true),
        ("4k3/8/8/8/p7/8/8/R3K3", "a8", Colour::White, false),
        // Bishop on a diagonal, and a king's neighbourhood.
        ("4k3/8/8/8/8/8/8/B3K3", "h8", Colour::White, true),
        ("4k3/8/8/8/8/8/8/4K3", "d2", Colour::White, true),
        // The a1/h8 edge: a rook on h1 does not attack a2 by wrapping around the rank.
        ("4k3/8/8/8/8/8/8/4K2R", "a2", Colour::White, false),
    ];

    for (placement, square_name, by, expected) in cases {
        let fen = format!("{placement} w - - 0 1");
        let board = Board::from_fen(&fen).unwrap_or_else(|e| panic!("{fen}: {e}"));
        let square = square_named(square_name);
        assert_eq!(
            is_square_attacked(&board, square, by),
            expected,
            "{fen}: is {square_name} attacked by {by:?}?"
        );
    }
}

fn square_named(name: &str) -> Square {
    let mut chars = name.chars();
    let file = File::from_char(chars.next().expect("a file")).expect("a file");
    let rank = Rank::from_char(chars.next().expect("a rank")).expect("a rank");
    Square::from_file_rank(file, rank)
}

/// A FEN-shaped string: mostly valid, occasionally not. Generating pure noise would spend
/// every case in the first two characters of the parser; this reaches the deep checks.
fn fen_like() -> impl Strategy<Value = String> {
    let piece_run = prop::collection::vec(
        prop_oneof![
            Just("p".to_owned()),
            Just("P".to_owned()),
            Just("k".to_owned()),
            Just("K".to_owned()),
            Just("q".to_owned()),
            Just("R".to_owned()),
            Just("n".to_owned()),
            Just("1".to_owned()),
            Just("8".to_owned()),
            Just("0".to_owned()),
            Just("9".to_owned()),
            Just("x".to_owned()),
        ],
        0..10,
    )
    .prop_map(|parts| parts.join(""));

    (
        prop::collection::vec(piece_run, 0..10),
        prop_oneof![Just("w"), Just("b"), Just("W"), Just(""), Just("wb")],
        prop_oneof![
            Just("-"),
            Just("KQkq"),
            Just("kqKQ"),
            Just("KK"),
            Just("HAha"),
            Just("")
        ],
        prop_oneof![
            Just("-"),
            Just("e3"),
            Just("e6"),
            Just("E6"),
            Just("e9"),
            Just("i3"),
            Just("")
        ],
        prop_oneof![Just("0"), Just("01"), Just("255"), Just("256"), Just("x")],
        prop_oneof![Just("1"), Just("0"), Just("65535"), Just("65536"), Just("")],
    )
        .prop_map(|(ranks, stm, castling, ep, halfmove, fullmove)| {
            format!(
                "{} {stm} {castling} {ep} {halfmove} {fullmove}",
                ranks.join("/")
            )
        })
}

proptest! {
    #![proptest_config(ProptestConfig {
        // Written out rather than `..ProptestConfig::default()` on the fields that matter:
        // the default reads PROPTEST_* from the environment, so a stray CI variable could
        // silently change what ran.
        cases: 512,
        max_shrink_iters: 4096,
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::SourceParallel(
            "proptest-regressions",
        ))),
        ..ProptestConfig::default()
    })]

    /// AC2, with shrinking: whatever the parser is handed, it answers rather than dies.
    #[test]
    fn arbitrary_strings_never_panic(text in ".*") {
        // A panic in either call fails the test — that is the whole assertion, and it is a
        // stronger statement than a `catch_unwind` wrapper, which would also "pass" under a
        // panic-abort profile by taking the process down.
        let parsed = Board::from_fen(&text);
        let with_layout = Board::from_fen_with_layout(&text);
        prop_assert_eq!(parsed.is_ok(), with_layout.map(|(_, l)| l == FenLayout::SixField).unwrap_or(false));
    }

    /// The same, on strings shaped like FENs, so the deep rules are reached.
    #[test]
    fn fen_shaped_strings_never_panic(text in fen_like()) {
        if let Ok(board) = Board::from_fen(&text) {
            // Anything accepted must be canonical, or AC1 is false for it.
            prop_assert_eq!(board.to_fen(), text);
            prop_assert_eq!(board.consistency(), Ok(()));
        }
    }

    /// Emit then parse is the identity on boards, over the corpus's own generator.
    #[test]
    fn emit_then_parse_is_identity_on_boards(index in 0..CORPUS_SIZE) {
        let positions = corpus();
        let board = positions.get(index).copied().expect("index is in range");
        let parsed = Board::from_fen(&board.to_fen()).expect("a generated position must parse");
        prop_assert_eq!(parsed, board);
        prop_assert_eq!(parsed.key(), board.key());
    }

    /// A FEN's first four fields decide its key, and nothing else does — AC6 clause 2 as a
    /// property rather than as three examples.
    #[test]
    fn the_key_survives_a_round_trip(index in 0..CORPUS_SIZE, halfmove in 0..=u8::MAX, fullmove in 1..=u16::MAX) {
        let positions = corpus();
        let mut board = positions.get(index).copied().expect("index is in range");
        board.set_halfmove_clock(halfmove);
        board.set_fullmove_number(fullmove);

        let parsed = Board::from_fen(&board.to_fen()).expect("must parse");
        prop_assert_eq!(parsed.key(), board.key());
        prop_assert_eq!(parsed.pawn_key(), board.pawn_key());
        prop_assert_eq!(parsed.key(), parsed.recomputed_key());
    }
}

#[test]
fn board_empty_round_trips_to_an_error() {
    // Documented rather than discovered: `Board::empty()` is a representation, not a
    // position, and the FEN it emits is correctly rejected for having no kings. The
    // generator never produces one, which is why the corpus round-trip is not in tension
    // with this.
    let fen = Board::empty().to_fen();
    assert_eq!(fen, "8/8/8/8/8/8/8/8 w - - 0 1");
    assert!(Board::from_fen(&fen).is_err());
}

#[test]
fn castling_rights_in_the_corpus_are_backed_by_their_pieces() {
    // The generator derives rights from the board rather than picking a mask. If it ever
    // stopped doing so, `from_fen` would reject its own output and
    // two_hundred_generated_positions_round_trip would fail with a confusing message; this
    // says what actually went wrong.
    for (index, board) in corpus().iter().enumerate() {
        for right in CastlingRight::ALL {
            if !board.castling().has(right) {
                continue;
            }
            let colour = right.colour();
            assert_eq!(
                board.piece_at(right.king_from()),
                Some(Piece::new(colour, PieceKind::King)),
                "position {index}: {right:?} without its king"
            );
            assert_eq!(
                board.piece_at(right.rook_from()),
                Some(Piece::new(colour, PieceKind::Rook)),
                "position {index}: {right:?} without its rook"
            );
        }
    }
}
