//! Cycle 2, guards: tamper evidence and a second, independent transcription.
//!
//! These assertions passed on their first run. They are regression guards, not a TDD
//! cycle — no red phase is claimed for them.
//!
//! What they defend against is the failure mode no amount of engine work can recover from:
//! a single wrong digit or a substituted FEN in the oracle. Such an error sits inert until
//! issue #6, where it silently blesses a broken move generator, and it will be blamed on
//! the move generator for days.
//!
//! Three independent layers:
//!
//! 1. **A pinned content hash.** Editing any byte of the fixture fails
//!    [`fixture_body_hash_is_pinned`], which forces whoever edited it to state their
//!    intent by updating the constant.
//! 2. **A second transcription.** The constants below were typed by hand from the fetched
//!    wiki page. The fixture itself was generated programmatically from that same saved
//!    page. Two different transcription paths can only agree if both are right.
//! 3. **Replay against a real engine**, in `stockfish_differential.rs`.
//!
//! The mirror row exists for a reason no count-based check can serve: `position4` and
//! `position4-mirror` have identical counts at every depth, so substituting one for the
//! other is undetectable by any assertion about numbers. Only byte-identity of the FEN
//! strings catches it.

mod support;

use boid_board::perft::oracle::{ORACLE_TEXT, Provenance, parse};
use support::sha256_hex;

/// SHA-256 of `tests/fixtures/perft_oracle.txt`, pinned here rather than in the fixture:
/// a self-referential hash is unverifiable (D-0007).
///
/// If this test fails, the fixture changed. That is not automatically wrong — but it must
/// be deliberate, and the new value must be justified in the commit message.
const ORACLE_SHA256: &str = "2b3471fe7115d4bfd435a37c0683d04c17e14939628a71de7c4551116599de05";

/// The seven FENs, typed by hand from https://www.chessprogramming.org/Perft_Results
/// (fetched 2026-08-16), with U+00A0 separators normalised to U+0020 per D-0008.
const PUBLISHED_FENS: [(&str, &str); 7] = [
    (
        "startpos",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    ),
    (
        "kiwipete",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -",
    ),
    ("position3", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"),
    (
        "position4",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    ),
    (
        "position4-mirror",
        "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
    ),
    (
        "position5",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    ),
    (
        "position6",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    ),
];

/// Node counts, typed by hand from the same page. Deliberately a second copy.
const PUBLISHED_COUNTS: [(&str, &[u64]); 6] = [
    // depth 1 upward.
    ("startpos", &[20, 400, 8902, 197281, 4865609, 119060324]),
    (
        "kiwipete",
        &[48, 2039, 97862, 4085603, 193690690, 8031647685],
    ),
    (
        "position3",
        &[
            14, 191, 2812, 43238, 674624, 11030083, 178633661, 3009794393,
        ],
    ),
    ("position4", &[6, 264, 9467, 422333, 15833292, 706045033]),
    ("position5", &[44, 1486, 62379, 2103487, 89941194]),
    (
        "position6",
        &[46, 2079, 89890, 3894594, 164075551, 6923051137],
    ),
];

#[test]
fn fixture_is_ascii_only() {
    // The NBSP guard. Six of the seven published FENs use U+00A0 separators; Stockfish's
    // tokeniser is ASCII-only and silently mis-parses them into a different legal position
    // reporting "Nodes searched: 0", while Rust's split_whitespace accepts them (D-0008).
    assert!(
        ORACLE_TEXT.is_ascii(),
        "the fixture must contain no byte outside U+0000..U+007F"
    );
    assert!(
        !ORACLE_TEXT.contains('\t'),
        "the fixture must not contain tabs"
    );
}

#[test]
fn fixture_body_hash_is_pinned() {
    let actual = sha256_hex(ORACLE_TEXT.as_bytes());
    assert_eq!(
        actual, ORACLE_SHA256,
        "the perft oracle fixture changed.\n\
         If that was deliberate, update ORACLE_SHA256 in this file and say why in the \
         commit message. If it was not, revert it: this file is the standard the engine \
         is judged against, and it is not supposed to move."
    );
}

#[test]
fn published_fens_are_byte_identical() {
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");
    assert_eq!(cases.len(), PUBLISHED_FENS.len());

    for (case, (id, fen)) in cases.iter().zip(PUBLISHED_FENS) {
        assert_eq!(case.id, id, "position order changed");
        assert_eq!(
            case.fen, fen,
            "{id}: FEN does not match the independently transcribed published string"
        );
    }
}

#[test]
fn canonical_counts_match_the_second_transcription() {
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");

    for (id, counts) in PUBLISHED_COUNTS {
        let case = cases
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id} missing from fixture"));

        for (offset, &expected) in counts.iter().enumerate() {
            let depth = u32::try_from(offset + 1).expect("depth fits in u32");
            assert_eq!(
                case.nodes_at(depth),
                Some(expected),
                "{id} depth {depth}: fixture disagrees with the hand transcription"
            );
        }
    }
}

#[test]
fn kiwipete_fen_has_four_fields() {
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");
    let kiwipete = cases
        .iter()
        .find(|c| c.id == "kiwipete")
        .expect("fixture must contain Kiwipete");

    // Deliberately asserts on the RAW FEN, not via PerftCase::fen_fields(). During review
    // a mutation replacing that accessor's body with a hardcoded `4` survived the entire
    // suite, because every call site routed through it — the accessor could lie and no
    // test disagreed.
    assert_eq!(
        kiwipete.fen.split(' ').count(),
        4,
        "Kiwipete is published without halfmove/fullmove counters and is stored as \
         published; appending \" 0 1\" would make the fixture disagree with its source"
    );
}

#[test]
fn mirror_row_differs_in_fen_but_matches_in_counts() {
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");
    let original = cases
        .iter()
        .find(|c| c.id == "position4")
        .expect("position4");
    let mirror = cases
        .iter()
        .find(|c| c.id == "position4-mirror")
        .expect("position4-mirror");

    assert_ne!(
        original.fen, mirror.fen,
        "the mirror must be a different position string, or committing it proves nothing"
    );
    assert_eq!(
        original.fen.split(' ').nth(1),
        Some("w"),
        "position4 is white to move"
    );
    assert_eq!(
        mirror.fen.split(' ').nth(1),
        Some("b"),
        "the colour-mirror is black to move"
    );

    let counts_of = |c: &boid_board::perft::oracle::PerftCase<'_>| {
        c.counts
            .iter()
            .map(|d| (d.depth, d.nodes))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        counts_of(original),
        counts_of(mirror),
        "the wiki publishes identical counts for the mirror at every depth"
    );
}

#[test]
fn every_count_carries_a_provenance_flag_and_some_are_verified() {
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");

    let verified = cases
        .iter()
        .flat_map(|c| &c.counts)
        .filter(|c| c.provenance == Provenance::Verified)
        .count();
    let total: usize = cases.iter().map(|c| c.counts.len()).sum();

    // Parsing already rejects an unflagged count, so the reachable claim is about the
    // balance: an oracle whose every row is merely "published" has not been checked.
    assert!(
        verified >= 40,
        "expected at least 40 independently re-derived counts, found {verified} of {total}"
    );
}

#[test]
fn fen_fields_accessor_agrees_with_the_raw_fen() {
    // Pins the accessor against a direct computation, and pins WHICH position is the
    // four-field one. A hardcoded return value now fails on both counts.
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");
    let mut four_field = Vec::new();

    for case in &cases {
        assert_eq!(
            case.fen_fields(),
            case.fen.split(' ').count(),
            "{}: fen_fields() disagrees with the FEN it reports on",
            case.id
        );
        if case.fen_fields() == 4 {
            four_field.push(case.id);
        }
    }

    assert_eq!(
        four_field,
        vec!["kiwipete"],
        "kiwipete is the only position the wiki publishes without move counters"
    );
}

#[test]
fn verified_counts_excludes_published_only_rows() {
    // A mutation dropping the provenance filter from verified_counts() survived the suite:
    // every 'p' count in the real fixture is also over the replay budget, so the
    // differential harness filtered them out for the wrong reason. This pins the semantics
    // directly, on a fixture built for the purpose.
    let text = "x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1 | 1:20v 2:400p 3:8902v";
    let cases = parse(text).expect("must parse");
    let case = &cases[0];

    let verified: Vec<u32> = case.verified_counts().map(|c| c.depth).collect();
    assert_eq!(
        verified,
        vec![1, 3],
        "verified_counts() must yield only Verified rows, not every row"
    );
    assert_eq!(case.counts.len(), 3, "all three rows are still parsed");

    for count in case.verified_counts() {
        assert_eq!(count.provenance, Provenance::Verified);
    }
}

#[test]
fn fixture_provenance_totals_are_what_the_project_claims() {
    // The published-vs-verified split is quoted in the README, the fixture header and the
    // pull request. During review those quoted totals turned out to be wrong, so they are
    // now pinned here and cannot drift again unnoticed.
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");
    let total: usize = cases.iter().map(|c| c.counts.len()).sum();
    let verified: usize = cases.iter().map(|c| c.verified_counts().count()).sum();

    assert_eq!(total, 55, "total node counts in the fixture");
    assert_eq!(
        verified, 44,
        "counts independently re-derived with Stockfish"
    );
    assert_eq!(total - verified, 11, "published-only counts");
}
