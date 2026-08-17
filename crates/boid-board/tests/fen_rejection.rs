//! AC2: an invalid FEN returns `Err`, and never panics.
//!
//! Every row below asserts a **specific variant**, never merely `is_err()`. That matters
//! because of the obvious cheat: a `from_fen` that returns `Err` unconditionally passes
//! every test in this file. It is killed by `fen_roundtrip.rs`, which needs real positions
//! out of the same function — and by [`every_declared_variant_is_reached_by_the_corpus`],
//! which fails if the corpus turns out to exercise three code paths and a wildcard.
//!
//! "Never panics" is not provable by any finite corpus. The honest claim is three-layered:
//! the module denies `clippy::indexing_slicing`, `unwrap_used`, `expect_used`, `panic` and
//! `arithmetic_side_effects`, so the usual routes in are machine-checked out; the parser
//! rejects non-ASCII first, after which no byte index can split a code point; and the
//! corpora here — every prefix of every fixture FEN, and every single-byte substitution
//! across them — are bounded evidence on top of that.

use boid_board::board::Board;
use boid_board::fen::{FenError, FenField, FenLayout, FenTier};
use boid_board::perft::oracle::{self, ORACLE_TEXT};
use boid_board::types::Colour;

/// The name of a variant, by exhaustive match. Adding a variant without adding it here is a
/// compile error, which is what keeps [`every_declared_variant_is_reached_by_the_corpus`]
/// honest as the taxonomy grows.
fn variant_name(error: FenError) -> &'static str {
    match error {
        FenError::NonAscii { .. } => "NonAscii",
        FenError::FieldCount { .. } => "FieldCount",
        FenError::EmptyField { .. } => "EmptyField",
        FenError::RankCount { .. } => "RankCount",
        FenError::RankWidth { .. } => "RankWidth",
        FenError::PieceChar { .. } => "PieceChar",
        FenError::DigitOutOfRange { .. } => "DigitOutOfRange",
        FenError::ConsecutiveDigits { .. } => "ConsecutiveDigits",
        FenError::SideToMove { .. } => "SideToMove",
        FenError::CastlingChar { .. } => "CastlingChar",
        FenError::CastlingDuplicate { .. } => "CastlingDuplicate",
        FenError::CastlingOrder { .. } => "CastlingOrder",
        FenError::CastlingShredderNotation { .. } => "CastlingShredderNotation",
        FenError::EnPassantSyntax { .. } => "EnPassantSyntax",
        FenError::EnPassantSquare { .. } => "EnPassantSquare",
        FenError::EnPassantRankContradictsSideToMove { .. } => "EnPassantRankContradictsSideToMove",
        FenError::ClockNotANumber { .. } => "ClockNotANumber",
        FenError::ClockLeadingZero { .. } => "ClockLeadingZero",
        FenError::ClockOutOfRange { .. } => "ClockOutOfRange",
        FenError::FullmoveNumberZero => "FullmoveNumberZero",
        FenError::KingCount { .. } => "KingCount",
        FenError::PawnOnBackRank { .. } => "PawnOnBackRank",
        FenError::CastlingWithoutKing { .. } => "CastlingWithoutKing",
        FenError::CastlingWithoutRook { .. } => "CastlingWithoutRook",
        FenError::EnPassantTargetOccupied { .. } => "EnPassantTargetOccupied",
        FenError::EnPassantOriginOccupied { .. } => "EnPassantOriginOccupied",
        FenError::EnPassantNoDoublePushedPawn { .. } => "EnPassantNoDoublePushedPawn",
    }
}

/// Every variant this crate declares. A hand-written list, on purpose: derived from the enum
/// it would agree with any subset of itself.
const DECLARED_VARIANTS: [&str; 27] = [
    "NonAscii",
    "FieldCount",
    "EmptyField",
    "RankCount",
    "RankWidth",
    "PieceChar",
    "DigitOutOfRange",
    "ConsecutiveDigits",
    "SideToMove",
    "CastlingChar",
    "CastlingDuplicate",
    "CastlingOrder",
    "CastlingShredderNotation",
    "EnPassantSyntax",
    "EnPassantSquare",
    "EnPassantRankContradictsSideToMove",
    "ClockNotANumber",
    "ClockLeadingZero",
    "ClockOutOfRange",
    "FullmoveNumberZero",
    "KingCount",
    "PawnOnBackRank",
    "CastlingWithoutKing",
    "CastlingWithoutRook",
    "EnPassantTargetOccupied",
    "EnPassantOriginOccupied",
    "EnPassantNoDoublePushedPawn",
];

/// The adversarial corpus: `(fen, expected variant)`.
///
/// Ordered by the field each attacks, so a gap is visible by reading.
fn corpus() -> Vec<(&'static str, &'static str)> {
    vec![
        // --- whole string ---
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR\u{a0}w KQkq - 0 1",
            "NonAscii",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq \u{2013} 0 1",
            "NonAscii",
        ),
        ("", "FieldCount"),
        ("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR", "FieldCount"),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
            "FieldCount",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0",
            "FieldCount",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 extra",
            "FieldCount",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR  w KQkq - 0 1",
            "EmptyField",
        ),
        (
            " rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "EmptyField",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 ",
            "EmptyField",
        ),
        // --- placement ---
        ("8/8/8/8/8/8/8 w - - 0 1", "RankCount"),
        ("4k3/8/8/8/8/8/8/4K3/8 w - - 0 1", "RankCount"),
        ("4k3/8/8/8/8/8/8/4K4 w - - 0 1", "RankWidth"),
        ("4k3/8/8/8/8/8/8/4K2 w - - 0 1", "RankWidth"),
        ("4k3/8/8/8/8/8/8/4K2X w - - 0 1", "PieceChar"),
        ("4k3/8/8/8/8/8/8/4K2! w - - 0 1", "PieceChar"),
        ("4k3/8/8/8/8/8/8/4K30 w - - 0 1", "DigitOutOfRange"),
        ("9/4k3/8/8/8/8/8/4K3 w - - 0 1", "DigitOutOfRange"),
        ("4k3/8/8/8/8/8/8/1111K111 w - - 0 1", "ConsecutiveDigits"),
        ("44/4k3/8/8/8/8/8/4K3 w - - 0 1", "ConsecutiveDigits"),
        // --- side to move ---
        ("4k3/8/8/8/8/8/8/4K3 x - - 0 1", "SideToMove"),
        ("4k3/8/8/8/8/8/8/4K3 W - - 0 1", "SideToMove"),
        ("4k3/8/8/8/8/8/8/4K3 white - - 0 1", "SideToMove"),
        // --- castling ---
        ("r3k2r/8/8/8/8/8/8/R3K2R w KQkqX - 0 1", "CastlingChar"),
        ("r3k2r/8/8/8/8/8/8/R3K2R w K- - 0 1", "CastlingChar"),
        ("r3k2r/8/8/8/8/8/8/R3K2R w KK - 0 1", "CastlingDuplicate"),
        ("r3k2r/8/8/8/8/8/8/R3K2R w KQkqq - 0 1", "CastlingDuplicate"),
        ("r3k2r/8/8/8/8/8/8/R3K2R w kqKQ - 0 1", "CastlingOrder"),
        ("r3k2r/8/8/8/8/8/8/R3K2R w QK - 0 1", "CastlingOrder"),
        (
            "r3k2r/8/8/8/8/8/8/R3K2R w HAha - 0 1",
            "CastlingShredderNotation",
        ),
        (
            "r3k2r/8/8/8/8/8/8/R3K2R w Hh - 0 1",
            "CastlingShredderNotation",
        ),
        // --- en passant ---
        ("4k3/8/8/8/8/8/8/4K3 w - e 0 1", "EnPassantSyntax"),
        ("4k3/8/8/8/8/8/8/4K3 w - e63 0 1", "EnPassantSyntax"),
        ("4k3/8/8/8/8/8/8/4K3 w - -- 0 1", "EnPassantSyntax"),
        ("4k3/8/8/8/8/8/8/4K3 w - e9 0 1", "EnPassantSquare"),
        ("4k3/8/8/8/8/8/8/4K3 w - i6 0 1", "EnPassantSquare"),
        ("4k3/8/8/8/8/8/8/4K3 w - E6 0 1", "EnPassantSquare"),
        ("4k3/8/8/8/8/8/8/4K3 w - a0 0 1", "EnPassantSquare"),
        ("4k3/8/8/8/8/8/8/4K3 w - e4 0 1", "EnPassantSquare"),
        (
            "4k3/8/8/8/8/8/8/4K3 w - e3 0 1",
            "EnPassantRankContradictsSideToMove",
        ),
        (
            "4k3/8/8/8/8/8/8/4K3 b - e6 0 1",
            "EnPassantRankContradictsSideToMove",
        ),
        // --- clocks ---
        ("4k3/8/8/8/8/8/8/4K3 w - - x 1", "ClockNotANumber"),
        ("4k3/8/8/8/8/8/8/4K3 w - - 0 y", "ClockNotANumber"),
        ("4k3/8/8/8/8/8/8/4K3 w - - -1 1", "ClockNotANumber"),
        ("4k3/8/8/8/8/8/8/4K3 w - - +5 1", "ClockNotANumber"),
        ("4k3/8/8/8/8/8/8/4K3 w - - 1.5 1", "ClockNotANumber"),
        ("4k3/8/8/8/8/8/8/4K3 w - - 01 1", "ClockLeadingZero"),
        ("4k3/8/8/8/8/8/8/4K3 w - - 0 01", "ClockLeadingZero"),
        ("4k3/8/8/8/8/8/8/4K3 w - - 256 1", "ClockOutOfRange"),
        ("4k3/8/8/8/8/8/8/4K3 w - - 999999 1", "ClockOutOfRange"),
        ("4k3/8/8/8/8/8/8/4K3 w - - 0 65536", "ClockOutOfRange"),
        (
            "4k3/8/8/8/8/8/8/4K3 w - - 0 99999999999999999999",
            "ClockOutOfRange",
        ),
        ("4k3/8/8/8/8/8/8/4K3 w - - 0 0", "FullmoveNumberZero"),
        // --- board legality ---
        ("8/8/8/8/8/8/8/4K3 w - - 0 1", "KingCount"),
        ("4k3/8/8/8/8/8/8/8 w - - 0 1", "KingCount"),
        ("4k3/8/8/8/8/8/8/3KK3 w - - 0 1", "KingCount"),
        ("4kk2/8/8/8/8/8/8/4K3 w - - 0 1", "KingCount"),
        ("4k3/8/8/8/8/8/8/P3K3 w - - 0 1", "PawnOnBackRank"),
        ("p3k3/8/8/8/8/8/8/4K3 w - - 0 1", "PawnOnBackRank"),
        ("4k3/8/8/8/8/8/8/R3K3 w K - 0 1", "CastlingWithoutRook"),
        ("r3k3/8/8/8/8/8/8/4K2R w Kq - 0 1", "CastlingWithoutRook"),
        ("4k3/8/8/8/8/8/8/R3K3 w Q - 0 1", "CastlingWithoutKing"),
        ("r3k2r/8/8/8/8/8/8/R6R w KQ - 0 1", "CastlingWithoutKing"),
        // The en-passant triple: target empty, origin empty, pusher present.
        (
            "4k3/8/8/8/4P3/4P3/8/4K3 b - e3 0 1",
            "EnPassantTargetOccupied",
        ),
        (
            "4k3/8/8/8/4P3/8/4P3/4K3 b - e3 0 1",
            "EnPassantOriginOccupied",
        ),
        (
            "4k3/8/8/8/8/8/8/4K3 b - e3 0 1",
            "EnPassantNoDoublePushedPawn",
        ),
        (
            "4k3/8/8/8/4p3/8/8/4K3 b - e3 0 1",
            "EnPassantNoDoublePushedPawn",
        ),
        (
            "4k3/8/8/4P3/8/8/8/4K3 w - e6 0 1",
            "EnPassantNoDoublePushedPawn",
        ),
    ]
}

#[test]
fn adversarial_inputs_map_to_specific_variants() {
    // The corpus is checked against BOTH entry points. `from_fen` is six-field-strict, so
    // the field-count rows differ; everything else must agree, or the two parsers have
    // drifted apart.
    for (fen, expected) in corpus() {
        let error = Board::from_fen(fen)
            .err()
            .unwrap_or_else(|| panic!("{fen:?} should have been rejected"));
        assert_eq!(
            variant_name(error),
            expected,
            "{fen:?} produced {error} ({})",
            variant_name(error)
        );

        if expected != "FieldCount" {
            let with_layout = Board::from_fen_with_layout(fen)
                .err()
                .unwrap_or_else(|| panic!("{fen:?} should have been rejected"));
            assert_eq!(
                variant_name(with_layout),
                expected,
                "{fen:?}: the two entry points disagree"
            );
        }
    }
}

#[test]
fn every_declared_variant_is_reached_by_the_corpus() {
    // Without this, a corpus in which every row exits at the first check would be dozens of
    // green assertions about one code path.
    let mut reached: Vec<&'static str> = corpus()
        .into_iter()
        .filter_map(|(fen, _)| Board::from_fen(fen).err().map(variant_name))
        .collect();
    reached.sort_unstable();
    reached.dedup();

    let unreached: Vec<&&str> = DECLARED_VARIANTS
        .iter()
        .filter(|name| !reached.contains(name))
        .collect();
    assert!(
        unreached.is_empty(),
        "declared but never produced by the corpus: {unreached:?}"
    );
    assert_eq!(
        DECLARED_VARIANTS.len(),
        27,
        "the taxonomy grew or shrank; the list above is hand-maintained on purpose"
    );
}

#[test]
fn both_tiers_are_exercised() {
    // The board-legality tier runs only after a FEN has been assembled into a position. If
    // some earlier check swallowed those inputs, this file would be testing the string
    // parser alone while appearing to cover both.
    let mut structural = 0usize;
    let mut legality = 0usize;
    for (fen, _) in corpus() {
        match Board::from_fen(fen) {
            Err(e) if e.tier() == FenTier::Structural => structural += 1,
            Err(e) if e.tier() == FenTier::BoardLegality => legality += 1,
            Err(_) => {}
            Ok(_) => panic!("{fen:?} should have been rejected"),
        }
    }
    assert!(structural >= 40, "structural rows: {structural}");
    assert!(legality >= 12, "board-legality rows: {legality}");
}

#[test]
fn nbsp_separated_fen_is_rejected() {
    // The exact contamination D-0008 guards the fixture against, now guarded on our own
    // parser. `split_whitespace` would treat U+00A0 as a separator and parse this into a
    // perfectly valid position, while Stockfish parses the same bytes into a DIFFERENT one
    // and reports zero nodes. Every ASCII test in this file passes identically either way;
    // only this input tells them apart.
    let nbsp = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R\u{a0}w\u{a0}KQkq\u{a0}-";
    assert!(matches!(
        Board::from_fen_with_layout(nbsp),
        Err(FenError::NonAscii { .. })
    ));
    assert!(matches!(
        Board::from_fen(nbsp),
        Err(FenError::NonAscii { .. })
    ));
}

#[test]
fn the_ascii_check_comes_first() {
    // Order matters, and not only for tidiness: after this check every byte index is a
    // character boundary, which is what makes slicing in the rest of the parser incapable
    // of panicking. A FEN that is wrong in two ways must report the non-ASCII one.
    let doubly_wrong = "4k3/8/8/8/8/8/8/4K4 x KQkq \u{a0} 0 1";
    assert!(matches!(
        Board::from_fen(doubly_wrong),
        Err(FenError::NonAscii { .. })
    ));
}

#[test]
fn clock_bounds_are_exactly_the_field_widths() {
    // The bounds are the widths of the fields the board stores, not a chess rule. The
    // fifty-move rule stops a game at 100 plies and the seventy-five-move rule at 150, so
    // nothing representable is lost -- but rejecting at 100 would refuse real puzzle
    // exports, and saturating at 255 would silently rewrite the position (D-0023).
    assert!(Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 255 1").is_ok());
    assert!(Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 65535").is_ok());
    assert!(Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 100 9999").is_ok());

    assert!(matches!(
        Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 256 1"),
        Err(FenError::ClockOutOfRange { max: 255, .. })
    ));
    assert!(matches!(
        Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 65536"),
        Err(FenError::ClockOutOfRange { max: 65535, .. })
    ));
    assert!(matches!(
        Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 0"),
        Err(FenError::FullmoveNumberZero)
    ));
}

#[test]
fn non_canonical_spellings_are_errors_not_normalisations() {
    // The pairing that makes AC1 and AC2 one requirement: a lenient parser plus a
    // canonicalising emitter round-trips its own output forever, and never round-trips the
    // input it was given. AC1 cannot see that, because it never feeds a non-canonical
    // string in. This is the test that does.
    for fen in [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w qkQK - 0 1",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 00 1",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 001",
        "rnbqkbnr/pppppppp/44/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1\n",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR\tw KQkq - 0 1",
    ] {
        let result = Board::from_fen(fen);
        assert!(
            result.is_err(),
            "{fen:?} was accepted; if it parses, it must also emit back byte-identically, \
             and it cannot"
        );
    }
}

#[test]
fn castling_rights_must_be_backed_by_a_king_and_a_rook() {
    // Not pedantry, and not a rule invented here. Feeding Stockfish 16 the FEN
    // "4k3/8/8/8/8/8/8/4K3 w K - 0 1" -- a kingside right with no rook -- crashes it
    // outright (exit 139, no output), and "w Q" makes it generate a phantom queenside
    // castle. Issue #6 would see that as EngineError::Io from a dead child process.
    assert!(matches!(
        Board::from_fen("4k3/8/8/8/8/8/8/4K3 w K - 0 1"),
        Err(FenError::CastlingWithoutRook { .. })
    ));
    assert!(matches!(
        Board::from_fen("4k3/8/8/8/8/8/8/R3K3 w Q - 0 1"),
        Err(FenError::CastlingWithoutKing { .. })
    ));
    // And the well-formed version is accepted, so the rule is not simply "reject castling".
    assert!(Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").is_ok());
}

#[test]
fn shredder_notation_is_named_not_mapped() {
    // Stockfish maps HAha to KQkq in standard chess. Copying that would mean emitting a
    // different string than was parsed -- and would silently accept a Chess960 position
    // this crate has no way to represent.
    let error = Board::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w HAha - 0 1")
        .expect_err("Shredder-FEN castling must be rejected");
    assert!(matches!(error, FenError::CastlingShredderNotation { .. }));
    assert!(
        error.to_string().contains("Shredder"),
        "the message should name the notation so nobody 'fixes' it by mapping: {error}"
    );
}

#[test]
fn every_prefix_of_every_fixture_fen_is_rejected_or_parses() {
    // Truncation is how a FEN arrives over a socket in issue #7. No prefix may panic, and
    // only a complete FEN may parse.
    let cases = oracle::parse(ORACLE_TEXT).expect("fixture must parse");
    let mut checked = 0usize;
    let mut accepted = 0usize;

    for case in &cases {
        for end in 0..case.fen.len() {
            if !case.fen.is_char_boundary(end) {
                continue;
            }
            let prefix = case.fen.get(..end).expect("char boundary");
            checked += 1;
            if let Ok((board, layout)) = Board::from_fen_with_layout(prefix) {
                accepted += 1;
                assert_eq!(
                    board.to_fen_with_layout(layout),
                    prefix,
                    "a prefix that parses must still round-trip"
                );
            }
        }
    }

    assert!(checked > 400, "checked {checked} prefixes");
    assert_eq!(
        accepted, 0,
        "no proper prefix of a fixture FEN is itself a FEN"
    );
}

#[test]
fn single_byte_substitutions_never_panic() {
    // Roughly ten thousand near-miss FENs. Each one is a valid FEN with one byte replaced,
    // which is the shape of a real transcription error and the shape most likely to reach a
    // slicing or arithmetic edge.
    let cases = oracle::parse(ORACLE_TEXT).expect("fixture must parse");
    let replacements = b"0189/ -abhkqwxKQ\t";
    let mut checked = 0usize;
    let mut accepted = 0usize;

    for case in &cases {
        let bytes = case.fen.as_bytes();
        for offset in 0..bytes.len() {
            for &replacement in replacements {
                let mut mutated = bytes.to_vec();
                if let Some(slot) = mutated.get_mut(offset) {
                    if *slot == replacement {
                        continue;
                    }
                    *slot = replacement;
                }
                let Ok(text) = String::from_utf8(mutated) else {
                    continue;
                };
                checked += 1;
                if let Ok((board, layout)) = Board::from_fen_with_layout(&text) {
                    accepted += 1;
                    // Whatever survives must still be canonical, or AC1 is false for it.
                    assert_eq!(board.to_fen_with_layout(layout), text);
                }
            }
        }
    }

    assert!(checked > 5_000, "checked {checked} substitutions");
    assert!(
        accepted > 0,
        "if none of these parsed, the parser is rejecting everything and this file proves \
         nothing"
    );
}

#[test]
fn errors_name_the_field_they_are_about() {
    // The messages are read by whoever is convinced their move generator is broken. A
    // rejection that does not say what was wrong sends them looking in the wrong place --
    // the same argument oracle.rs makes for naming the line number.
    let empty_field = Board::from_fen("4k3/8/8/8/8/8/8/4K3  - - 0 1")
        .expect_err("a doubled separator must be rejected");
    assert!(matches!(
        empty_field,
        FenError::EmptyField {
            field: FenField::SideToMove
        }
    ));

    let contradiction = Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - e3 0 1")
        .expect_err("rank 3 with White to move is impossible");
    assert!(matches!(
        contradiction,
        FenError::EnPassantRankContradictsSideToMove {
            rank: '3',
            side_to_move: Colour::White
        }
    ));
    assert!(
        contradiction.to_string().contains("contradicts"),
        "{contradiction}"
    );
}

#[test]
fn from_fen_points_four_field_users_at_the_layout_aware_entry_point() {
    // The published Kiwipete FEN has four fields. `from_fen` is deliberately six-field
    // strict, so it must fail in a way that is obviously about the field count rather than
    // about the position.
    let four = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -";
    assert!(matches!(
        Board::from_fen(four),
        Err(FenError::FieldCount { found: 4 })
    ));
    let (_, layout) = Board::from_fen_with_layout(four).expect("four fields are valid here");
    assert_eq!(layout, FenLayout::FourField);
}
