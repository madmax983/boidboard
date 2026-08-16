//! Cycle 1: the oracle fixture parses into the seven positions it claims to hold.
//!
//! These tests answer the question the whole of issue #3 turns on: *which test goes red if
//! I change a digit in `perft_oracle.txt`?* Deeper guarantees — structural validation,
//! tamper evidence, and replay against a real engine — live in `oracle_validation.rs`,
//! `oracle_transcription.rs`, and `stockfish_differential.rs`.

use boid_board::perft::oracle::{ORACLE_TEXT, Provenance, parse};

/// The seven ids the fixture must contain, in file order.
const EXPECTED_IDS: [&str; 7] = [
    "startpos",
    "kiwipete",
    "position3",
    "position4",
    "position4-mirror",
    "position5",
    "position6",
];

#[test]
fn parses_seven_positions() {
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");
    let ids: Vec<&str> = cases.iter().map(|c| c.id).collect();
    assert_eq!(ids, EXPECTED_IDS, "fixture ids, in file order");
}

#[test]
fn startpos_depth_three_is_8902() {
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");
    let startpos = cases
        .iter()
        .find(|c| c.id == "startpos")
        .expect("fixture must contain the initial position");

    assert_eq!(
        startpos.nodes_at(3),
        Some(8902),
        "perft(3) of the initial position is the single most-quoted number in chess \
         programming; if this is wrong the fixture is wrong"
    );

    let d3 = startpos
        .counts
        .iter()
        .find(|c| c.depth == 3)
        .expect("depth 3 must be present");
    assert_eq!(
        d3.provenance,
        Provenance::Verified,
        "depth 3 of the initial position was re-derived with Stockfish and must say so"
    );
}

#[test]
fn kiwipete_depth_six_exceeds_u32() {
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");
    let kiwipete = cases
        .iter()
        .find(|c| c.id == "kiwipete")
        .expect("fixture must contain Kiwipete");

    // 8_031_647_685 > u32::MAX (4_294_967_295). A u32 node-count field would not merely
    // fail this assertion, it would fail to compile the fixture's own value.
    assert_eq!(kiwipete.nodes_at(6), Some(8_031_647_685));
    assert!(kiwipete.nodes_at(6).unwrap() > u64::from(u32::MAX));
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let cases = parse(ORACLE_TEXT).expect("the committed fixture must parse");

    // The fixture is mostly header: a provenance block, a grammar, and three notes.
    let comment_lines = ORACLE_TEXT
        .lines()
        .filter(|l| l.trim_start().starts_with('#'))
        .count();
    assert!(
        comment_lines > 30,
        "the fixture is expected to carry a substantial provenance header, found {comment_lines} comment lines"
    );
    assert_eq!(
        cases.len(),
        7,
        "comment lines must not be parsed as positions"
    );
}
