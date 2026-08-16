//! Cycle 2: the oracle parser rejects malformed fixtures, and the committed fixture
//! satisfies its own structural invariants.
//!
//! Every negative test here is a way a transcription error could enter the fixture and sit
//! inert until issue #6, where it would silently bless a broken move generator. A parser
//! that is lenient about its oracle is worse than no oracle: it converts a loud failure
//! into a quiet wrong answer.

use boid_board::perft::oracle::{ORACLE_TEXT, OracleError, parse};

/// A structurally valid line, used as the baseline that negative cases mutate.
const VALID: &str = "x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1 | 1:20v 2:400v";

fn err(text: &str) -> OracleError {
    parse(text).expect_err("expected this fixture to be rejected")
}

// ---------------------------------------------------------------------------------
// Count-field grammar
// ---------------------------------------------------------------------------------

#[test]
fn rejects_thousands_separators() {
    // The wiki prints "1,486". A parser that strips commas would accept a value it had
    // silently rewritten; a parser that stops at the comma would read 1.
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1 | 1:1,486v");
    assert!(
        matches!(e, OracleError::NonNumericCount { .. }),
        "expected NonNumericCount, got {e:?}"
    );
}

#[test]
fn rejects_count_over_u64() {
    // The published depth-14 count for the initial position. It does not fit in u64 and
    // must be rejected rather than truncated or saturated.
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1 | 1:61885021521585529237v");
    assert!(
        matches!(e, OracleError::CountOverflowsU64 { .. }),
        "expected CountOverflowsU64, got {e:?}"
    );
}

#[test]
fn rejects_missing_provenance_flag() {
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1 | 1:20");
    assert!(
        matches!(e, OracleError::MissingProvenanceFlag { .. }),
        "expected MissingProvenanceFlag, got {e:?}"
    );
}

#[test]
fn rejects_unknown_provenance_flag() {
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1 | 1:20z");
    assert!(
        matches!(e, OracleError::MissingProvenanceFlag { .. }),
        "expected MissingProvenanceFlag, got {e:?}"
    );
}

#[test]
fn rejects_duplicate_depth() {
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1 | 1:20v 1:20v");
    assert!(
        matches!(e, OracleError::DuplicateDepth { depth: 1, .. }),
        "expected DuplicateDepth, got {e:?}"
    );
}

#[test]
fn rejects_non_contiguous_depths() {
    // A gap means a published row was dropped in transcription.
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1 | 1:20v 3:8902v");
    assert!(
        matches!(
            e,
            OracleError::DepthGap {
                previous: 1,
                found: 3,
                ..
            }
        ),
        "expected DepthGap, got {e:?}"
    );
}

#[test]
fn rejects_duplicate_id() {
    let text = format!("{VALID}\n{VALID}");
    let e = err(&text);
    assert!(
        matches!(e, OracleError::DuplicateId { .. }),
        "expected DuplicateId, got {e:?}"
    );
}

#[test]
fn rejects_wrong_field_count() {
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    assert!(
        matches!(e, OracleError::MalformedLine { fields: 2, .. }),
        "expected MalformedLine, got {e:?}"
    );
}

#[test]
fn rejects_empty_fixture() {
    assert!(matches!(err(""), OracleError::NoEntries));
    assert!(matches!(
        err("# only a comment\n\n"),
        OracleError::NoEntries
    ));
}

#[test]
fn tolerates_crlf_line_endings() {
    let cases = parse("x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1 | 1:20v\r\n")
        .expect("CRLF line endings must parse identically");
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].nodes_at(1), Some(20));
}

// ---------------------------------------------------------------------------------
// FEN structural validation
// ---------------------------------------------------------------------------------

#[test]
fn rejects_rank_not_summing_to_eight() {
    // "4K4" is nine files. This is the single most likely way a hand-edited FEN goes
    // wrong, and it produces a FEN that still looks plausible.
    let e = err("x | 4k3/8/8/8/8/8/8/4K4 w - - 0 1 | 1:20v");
    assert!(
        matches!(e, OracleError::MalformedFen { .. }),
        "expected MalformedFen, got {e:?}"
    );
}

#[test]
fn rejects_wrong_rank_count() {
    let e = err("x | 4k3/8/8/8/8/8/4K3 w - - 0 1 | 1:20v");
    assert!(
        matches!(e, OracleError::MalformedFen { .. }),
        "expected MalformedFen, got {e:?}"
    );
}

#[test]
fn rejects_missing_king() {
    let e = err("x | 8/8/8/8/8/8/8/4K3 w - - 0 1 | 1:20v");
    assert!(
        matches!(e, OracleError::MalformedFen { .. }),
        "expected MalformedFen for a position with no black king, got {e:?}"
    );
}

#[test]
fn rejects_two_kings_of_one_colour() {
    let e = err("x | 4k3/8/8/8/8/8/8/3KK3 w - - 0 1 | 1:20v");
    assert!(
        matches!(e, OracleError::MalformedFen { .. }),
        "expected MalformedFen for two white kings, got {e:?}"
    );
}

#[test]
fn rejects_invalid_piece_character() {
    let e = err("x | 4k3/8/8/8/8/8/8/4K2X w - - 0 1 | 1:20v");
    assert!(
        matches!(e, OracleError::MalformedFen { .. }),
        "expected MalformedFen, got {e:?}"
    );
}

#[test]
fn rejects_bad_side_to_move() {
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 x - - 0 1 | 1:20v");
    assert!(
        matches!(e, OracleError::MalformedFen { .. }),
        "expected MalformedFen, got {e:?}"
    );
}

#[test]
fn rejects_castling_field_outside_kqkq() {
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 w KQkqX - 0 1 | 1:20v");
    assert!(
        matches!(e, OracleError::MalformedFen { .. }),
        "expected MalformedFen, got {e:?}"
    );
}

#[test]
fn rejects_bad_en_passant_square() {
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 w - e9 0 1 | 1:20v");
    assert!(
        matches!(e, OracleError::MalformedFen { .. }),
        "expected MalformedFen, got {e:?}"
    );
}

#[test]
fn rejects_fen_with_five_fields() {
    // Four fields (as published for Kiwipete) or six. Five means a field was dropped.
    let e = err("x | 4k3/8/8/8/8/8/8/4K3 w - - 0 | 1:20v");
    assert!(
        matches!(e, OracleError::MalformedFen { .. }),
        "expected MalformedFen, got {e:?}"
    );
}

#[test]
fn accepts_the_published_four_field_kiwipete_fen() {
    // The wiki publishes Kiwipete without halfmove/fullmove counters. The fixture stores
    // it that way and the parser must not demand six fields (D-0008).
    let text =
        "kiwipete | r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - | 1:48v";
    let cases = parse(text).expect("the published four-field FEN must be accepted");
    assert_eq!(cases[0].fen.split(' ').count(), 4);
}

// ---------------------------------------------------------------------------------
// Invariants over the committed fixture itself
// ---------------------------------------------------------------------------------

#[test]
fn committed_fixture_satisfies_its_own_invariants() {
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");

    for case in &cases {
        let fields = case.fen.split(' ').count();
        assert!(
            fields == 4 || fields == 6,
            "{}: FEN should have 4 or 6 fields, has {fields}",
            case.id
        );

        assert!(!case.counts.is_empty(), "{}: no counts", case.id);

        // perft(0) is 1 by definition: the root itself is the only leaf at depth 0.
        if let Some(zero) = case.nodes_at(0) {
            assert_eq!(zero, 1, "{}: perft(0) must be 1", case.id);
        }

        // True of all seven of these positions, though not of every position in general:
        // none of them is so constrained that the tree narrows with depth. Asserted as a
        // property of this fixture, and it catches a transposed pair of digits.
        for pair in case.counts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!(
                b.nodes > a.nodes,
                "{}: node counts must increase with depth, but depth {} = {} is not greater than depth {} = {}",
                case.id,
                b.depth,
                b.nodes,
                a.depth,
                a.nodes
            );
        }
    }
}
