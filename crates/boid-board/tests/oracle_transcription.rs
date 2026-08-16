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

use boid_board::perft::oracle::{ORACLE_TEXT, Provenance, parse};

/// SHA-256 of `tests/fixtures/perft_oracle.txt`, pinned here rather than in the fixture:
/// a self-referential hash is unverifiable (D-0007).
///
/// If this test fails, the fixture changed. That is not automatically wrong — but it must
/// be deliberate, and the new value must be justified in the commit message.
const ORACLE_SHA256: &str = "65c02664c3143da3a3696d7b2577b36225e8318069c3c8dc4f12fa4760950886";

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

    assert_eq!(
        kiwipete.fen_fields(),
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

// -------------------------------------------------------------------------------------
// SHA-256, hand-rolled.
//
// boid-board is the dependency root of the workspace and takes no external dependencies,
// not even for tests (D-0014). Correctness of this implementation is not assumed: it is
// checked against the NIST published vectors below.
// -------------------------------------------------------------------------------------

#[rustfmt::skip]
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];

    let mut message = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for block in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (slot, word) in w.iter_mut().zip(block.chunks_exact(4)) {
            *slot = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (&k, &wi) in K.iter().zip(w.iter()) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k)
                .wrapping_add(wi);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }

    h.iter().map(|word| format!("{word:08x}")).collect()
}

#[test]
fn sha256_implementation_matches_published_vectors() {
    // NIST FIPS 180-2 examples, plus the empty string and a multi-block input. A hash
    // function that is wrong in the same way twice would otherwise "pin" nothing.
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    // 1,000,000 'a' — exercises many blocks and the length encoding.
    let million_a = vec![b'a'; 1_000_000];
    assert_eq!(
        sha256_hex(&million_a),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}
