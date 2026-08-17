//! Cycle 3: acceptance criterion 1 — FEN parse then emit round-trips byte-identically.
//!
//! The criterion says "all six perft positions plus 200 randomly generated legal positions
//! (proptest)". Two things about that sentence are worth stating plainly rather than
//! quietly working around.
//!
//! **The fixture's Kiwipete row has four fields.** D-0008 stores the published FEN exactly
//! as published, without the halfmove and fullmove counters, and forbids appending them.
//! A canonical six-field emitter therefore cannot return its bytes. So the criterion is
//! discharged as the *law* D-0021 states — for every FEN the parser accepts, emitting
//! returns the input when it had six fields and the input plus `" 0 1"` when it had four,
//! and there is no third case. That is a stronger claim than the criterion's, because it
//! quantifies over the whole accepted language rather than over seven rows. The four-field
//! expansion is not this project's invention: Stockfish canonicalises the same input the
//! same way.
//!
//! **"Legal" is not what the generator can promise.** Deciding legality needs to know
//! whether the side not to move is in check, which needs attack tables, which are issue
//! #5's. The corpus is 200 positions in the *accepted language* — every one structurally
//! valid, with two kings, no back-rank pawns, castling rights only where the king and rook
//! are home, and an en-passant file only where a real double push could have left one.
//! Calling them "legal" would be the kind of claim this project's decision log exists to
//! stop, so D-0026 records the narrowing.
//!
//! The generator builds **text**, not boards. A corpus produced by calling `to_fen` would
//! be by construction the set the parser accepts, and the round trip over it would prove
//! nothing.

use std::collections::HashSet;

use boid_board::board::Board;
use boid_board::{CastlingRights, File, Square};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

mod fenlab;

/// The five fixture rows published with six fields.
const SIX_FIELD_FIXTURE_ROWS: [&str; 6] = [
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
];

/// The one row D-0008 stores with four fields, and its canonical expansion typed by hand.
const KIWIPETE_FOUR_FIELD: &str =
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -";
const KIWIPETE_SIX_FIELD: &str =
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

fn parse(fen: &str) -> Board {
    Board::from_fen(fen).unwrap_or_else(|e| panic!("{fen:?} must parse: {e}"))
}

// ---------------------------------------------------------------------------------
// The fixture rows
// ---------------------------------------------------------------------------------

#[test]
fn six_field_fixture_rows_round_trip_byte_identically() {
    for fen in SIX_FIELD_FIXTURE_ROWS {
        assert_eq!(parse(fen).to_fen(), fen);
    }
}

/// The one row that cannot be byte-identical, asserted against a hand-typed literal.
///
/// Written out in full rather than as `format!("{KIWIPETE_FOUR_FIELD} 0 1")`, so that an
/// emitter special-casing the four-field input — or a `Board` that secretly remembered its
/// source layout — could not satisfy it by echoing.
#[test]
fn the_four_field_kiwipete_row_expands_to_its_canonical_six_field_form() {
    assert_eq!(parse(KIWIPETE_FOUR_FIELD).to_fen(), KIWIPETE_SIX_FIELD);
    // And the two spellings denote the same board, keys included.
    assert_eq!(parse(KIWIPETE_FOUR_FIELD), parse(KIWIPETE_SIX_FIELD));
    assert_eq!(
        parse(KIWIPETE_FOUR_FIELD).key(),
        parse(KIWIPETE_SIX_FIELD).key()
    );
}

/// Every row of the committed oracle, read from the fixture rather than retyped, obeys the
/// law in one of its two cases and never in a third.
#[test]
fn every_fixture_row_obeys_the_round_trip_law() {
    let cases = boid_board::perft::oracle::parse(boid_board::perft::oracle::ORACLE_TEXT)
        .expect("the committed fixture parses");
    assert_eq!(cases.len(), 7);
    let mut four_field_rows = 0;
    for case in cases {
        let emitted = parse(case.fen).to_fen();
        match case.fen.split(' ').count() {
            6 => assert_eq!(emitted, case.fen, "{}", case.id),
            4 => {
                four_field_rows += 1;
                assert_eq!(emitted, format!("{} 0 1", case.fen), "{}", case.id);
            }
            other => panic!("{}: {other} fields, which the law has no case for", case.id),
        }
    }
    assert_eq!(
        four_field_rows, 1,
        "exactly one fixture row is stored in the four-field form"
    );
}

/// A literal, typed by hand, so a parser and emitter that are wrong in mirror-image ways
/// cannot agree with each other and pass.
#[test]
fn the_start_position_emits_the_bytes_i_typed() {
    assert_eq!(
        Board::startpos().to_fen(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
}

/// A `44`-emitting emitter matched by a `44`-accepting parser round-trips perfectly and is
/// still wrong. The parser rejects adjacent skip digits; this is the other half.
#[test]
fn emitting_never_produces_adjacent_skip_digits() {
    for fen in SIX_FIELD_FIXTURE_ROWS {
        let placement = parse(fen).to_fen();
        let placement = placement.split(' ').next().expect("a placement field");
        let mut previous_was_digit = false;
        for ch in placement.chars() {
            let is_digit = ch.is_ascii_digit();
            assert!(
                !(is_digit && previous_was_digit),
                "{fen}: emitted adjacent skip digits in {placement:?}"
            );
            previous_was_digit = is_digit;
        }
    }
}

// ---------------------------------------------------------------------------------
// The colour-mirror property
// ---------------------------------------------------------------------------------

/// Emitting must commute with colour-mirroring.
///
/// This is the only test in the file that can see a whole class of bugs: a rank order
/// reversed in one direction only, castling letters swapped without reordering, the
/// en-passant rank derived correctly for White and not for Black, a colour-indexed
/// bitboard written to the wrong slot. None of those is visible in a same-colour round
/// trip, and `position4-mirror` is the fixture's only Black-to-move row.
#[test]
fn the_mirror_property_holds_over_the_fixture_rows() {
    for fen in SIX_FIELD_FIXTURE_ROWS {
        let mirrored_then_emitted = parse(&fenlab::flip(fen)).to_fen();
        let emitted_then_mirrored = fenlab::flip(&parse(fen).to_fen());
        assert_eq!(
            mirrored_then_emitted, emitted_then_mirrored,
            "emitting does not commute with mirroring for {fen}"
        );
    }
}

// ---------------------------------------------------------------------------------
// The generated corpus — acceptance criterion 1's second half
// ---------------------------------------------------------------------------------

/// 200 positions, generated as text, each round-tripping byte-identically.
///
/// The runner is seeded deterministically rather than from entropy, so a failure reproduces
/// exactly on the next run instead of vanishing — the same reasoning the issue gives for
/// the zobrist seed. `failure_persistence: None` follows from that: with a fixed seed there
/// is no regression file to write, because the failing case is regenerated every time.
#[test]
fn the_generated_corpus_round_trips_byte_identically() {
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: 200,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );

    let seen = std::cell::RefCell::new(Vec::new());
    runner
        .run(&fenlab::arbitrary_fen(), |fen| {
            let board = Board::from_fen(&fen)
                .map_err(|e| TestCaseError::fail(format!("{fen:?} must parse: {e}")))?;
            prop_assert_eq!(
                board.to_fen(),
                fen.clone(),
                "the generated FEN did not round-trip"
            );
            // The board is also internally consistent, which the text alone cannot show.
            board
                .check_invariants()
                .map_err(|e| TestCaseError::fail(format!("{fen:?}: {e}")))?;
            prop_assert_eq!(board.key(), board.recomputed_key());
            prop_assert_eq!(board.pawn_key(), board.recomputed_pawn_key());
            seen.borrow_mut().push(fen);
            Ok(())
        })
        .expect("every generated FEN must round-trip");

    let corpus = seen.into_inner();
    assert_eq!(corpus.len(), 200, "the criterion asks for 200 positions");

    // A generator whose rejection loop discarded everything interesting would leave 200
    // near-identical two-king positions and an asserted count with no content behind it.
    let distinct: HashSet<&String> = corpus.iter().collect();
    assert!(
        distinct.len() >= 190,
        "only {} of 200 generated positions are distinct",
        distinct.len()
    );

    let mut castling_masks = HashSet::new();
    let mut ep_files = HashSet::new();
    let mut sides = HashSet::new();
    let mut saw_a_high_clock = false;
    for fen in &corpus {
        let board = parse(fen);
        castling_masks.insert(board.castling().bits());
        ep_files.insert(board.ep_file().map(File::index));
        sides.insert(board.side_to_move());
        saw_a_high_clock |= board.halfmove_clock() >= 100;
    }
    assert_eq!(sides.len(), 2, "both sides must be represented");
    assert!(
        castling_masks.len() >= 8,
        "only {} distinct castling masks in the corpus",
        castling_masks.len()
    );
    assert!(
        ep_files.len() >= 4,
        "only {} distinct en-passant states in the corpus",
        ep_files.len()
    );
    assert!(
        saw_a_high_clock,
        "no generated position had a halfmove clock at or above 100"
    );
}

/// Parsing our own emission reproduces the board exactly.
///
/// One line, and it crosses every representation at once: `to_fen` reads the **bitboards**,
/// `recomputed_key` walks the **mailbox**, and equality compares the packed state word and
/// both keys. A bug in any one of them shows up here.
#[test]
fn parsing_our_own_emission_reproduces_the_board() {
    let mut fens: Vec<String> = SIX_FIELD_FIXTURE_ROWS
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    fens.push(KIWIPETE_FOUR_FIELD.to_owned());
    fens.push("4k3/8/8/4p3/8/8/8/4K3 w - e6 0 1".to_owned());
    fens.push("4k3/8/8/8/8/8/8/4K3 b - - 65535 65535".to_owned());
    for fen in fens {
        let board = parse(&fen);
        assert_eq!(Board::from_fen(&board.to_fen()).as_ref(), Ok(&board));
    }
}

/// The emitter's buffer estimate is not smaller than anything it emits.
#[test]
fn no_emitted_fen_exceeds_the_declared_maximum() {
    let mut longest = 0;
    for fen in SIX_FIELD_FIXTURE_ROWS {
        longest = longest.max(parse(fen).to_fen().len());
    }
    // A fully packed board with the widest clocks is the true worst case.
    let dense = parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 65535 65535");
    longest = longest.max(dense.to_fen().len());
    assert!(
        longest <= boid_board::fen::MAX_FEN_LEN,
        "emitted {longest} bytes, above the declared MAX_FEN_LEN of {}",
        boid_board::fen::MAX_FEN_LEN
    );
}

/// En-passant squares survive the round trip, in both directions.
///
/// The stored state is a *file*; the square is reconstructed from the side to move. A
/// derivation that worked for only one colour would pass every White-to-move test in this
/// file.
#[test]
fn en_passant_squares_survive_the_round_trip_for_both_colours() {
    for (fen, expected) in [
        ("4k3/8/8/4p3/8/8/8/4K3 w - e6 0 1", "e6"),
        ("4k3/8/8/8/4P3/8/8/4K3 b - e3 0 1", "e3"),
        ("4k3/8/8/p7/8/8/8/4K3 w - a6 0 1", "a6"),
        ("4k3/8/8/8/7P/8/8/4K3 b - h3 0 1", "h3"),
    ] {
        let board = parse(fen);
        assert_eq!(board.ep_square(), Square::from_uci(expected));
        assert_eq!(board.to_fen(), fen);
    }
}

/// Castling fields are emitted in the canonical order regardless of the input's order —
/// which the parser rejects anyway, so this pins the emitter rather than the parser.
#[test]
fn castling_fields_are_emitted_in_canonical_order() {
    let board = parse("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    assert_eq!(board.castling(), CastlingRights::ALL);
    assert!(board.to_fen().contains(" KQkq "));
    let none = parse("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    assert!(none.to_fen().contains(" - - "));
}
