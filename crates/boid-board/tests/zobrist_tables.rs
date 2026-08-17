//! The zobrist key set: is it the one this project committed to, and is it fit to hash with?
//!
//! Two different questions, and both are asked here.
//!
//! **Identity.** The SHA-256 below was derived from the scheme in `docs/DECISIONS.md`
//! D-0020 by an independent implementation, in a different language, before any of
//! `src/zobrist.rs` existed. That ordering is the point, and `git show` on the commit that
//! introduced this file is the evidence: the constant cannot have been read off the code,
//! because the code produced zeros when the constant was typed. It is the same posture
//! `ORACLE_SHA256` takes towards the perft fixture (D-0007).
//!
//! **Fitness.** A key set can be perfectly reproducible and still useless. Two identical
//! keys make two different positions hash the same forever; a key that is the XOR of two
//! others makes a *transposition* collide, which is worse, because a move is a three- or
//! four-key delta and such a collision is systematic rather than a one-in-2^64 accident.
//! Both are checked over the whole flat index space rather than per sub-table: a dependency
//! between `en_passant[e]` and `castling[K] ^ side_to_move` is exactly as fatal as one
//! inside a single table, and per-table checks cannot see it.

use boid_board::types::{CastlingRight, CastlingRights, File, Piece, Square};
use boid_board::zobrist::{ZOBRIST, Zobrist, splitmix64};

#[allow(dead_code)]
mod support;
use support::sha256::sha256_hex;

/// SHA-256 of the 781 keys, serialised little-endian in flat-index order — 6248 bytes.
///
/// If this fails, the key set moved. That is not automatically wrong, but every position
/// key in `zobrist_incremental.rs`, every stored perft-table entry and every published bug
/// repro moved with it, so it must be deliberate and the commit message must say why.
const ZOBRIST_SHA256: &str = "79dbfe5ac62eb22d3e1961835c668ca10c79b467fa36b9c63391e78f5ea985a2";

/// splitmix64's published output for seed 0 — the first five values of the reference
/// implementation's stream. The only constant in issue #4 that comes from a third party.
const PUBLISHED_SPLITMIX64_SEED_ZERO: [u64; 5] = [
    0xE220_A839_7B1D_CDAF,
    0x6E78_9E6A_A1B9_65F4,
    0x06C4_5D18_8009_454F,
    0xF88B_B8A8_724C_81EC,
    0x1B39_896A_51A8_749B,
];

/// The golden-ratio increment, restated here rather than imported: a test that reads the
/// implementation's own constant would agree with it however wrong it was.
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

fn flat_keys() -> Vec<u64> {
    (0..Zobrist::LEN).map(|i| ZOBRIST.flat(i)).collect()
}

/// Little-endian, because a digest is only a pin if the byte order it was taken over is
/// fixed. Big-endian over the same keys gives `79a94893…`, a completely different and
/// equally plausible-looking constant.
fn serialise(keys: &[u64]) -> Vec<u8> {
    keys.iter().flat_map(|k| k.to_le_bytes()).collect()
}

#[test]
fn splitmix64_matches_the_published_seed_zero_vectors() {
    // The published algorithm is a stream: the state advances by GAMMA and the finaliser is
    // applied to each state. `key_at` uses the closed form of the same walk, so pinning the
    // finaliser against the published stream pins the arithmetic without pinning our index
    // convention on top of it.
    let mut state = 0u64;
    let mut produced = [0u64; 5];
    for slot in &mut produced {
        state = state.wrapping_add(GAMMA);
        *slot = splitmix64(state);
    }

    assert_eq!(
        produced, PUBLISHED_SPLITMIX64_SEED_ZERO,
        "splitmix64 does not reproduce its published output; a mistyped multiplier or \
         shift would otherwise produce a table that is deterministic, plausible and wrong"
    );
}

#[test]
fn every_zobrist_key_is_non_zero() {
    let keys = flat_keys();
    assert_eq!(keys.len(), Zobrist::LEN);

    // A zero piece-square key makes that piece invisible to the hash on that square: the
    // position with it and the position without it become the same key.
    let zeros: Vec<usize> = keys
        .iter()
        .enumerate()
        .filter(|&(_, &k)| k == 0)
        .map(|(i, _)| i)
        .collect();
    assert!(
        zeros.is_empty(),
        "{} of {} zobrist keys are zero; first offenders: {:?}",
        zeros.len(),
        Zobrist::LEN,
        &zeros[..zeros.len().min(8)]
    );
}

#[test]
fn all_781_zobrist_keys_are_distinct() {
    let keys = flat_keys();
    let distinct: std::collections::HashSet<u64> = keys.iter().copied().collect();

    // Not birthday-paradox theatre: 781 draws from 2^64 collide with probability around
    // 1.7e-14, so this is a check on the LAYOUT — an off-by-one in the flat index, or two
    // sub-tables overlapping a range — not on the generator's luck.
    assert_eq!(
        distinct.len(),
        Zobrist::LEN,
        "the {} zobrist keys take only {} distinct values",
        Zobrist::LEN,
        distinct.len()
    );
}

#[test]
fn pairwise_key_xors_are_distinct_and_non_zero() {
    let keys = flat_keys();
    let mut xors = std::collections::HashSet::new();
    let mut zero_xors = 0usize;
    let mut duplicate_xors = 0usize;

    for (i, &a) in keys.iter().enumerate() {
        for &b in &keys[i + 1..] {
            let x = a ^ b;
            if x == 0 {
                zero_xors += 1;
            }
            if !xors.insert(x) {
                duplicate_xors += 1;
            }
        }
    }

    let pairs = Zobrist::LEN * (Zobrist::LEN - 1) / 2;
    // Only the counts are observed, never the set's iteration order: a HashSet iterates
    // non-deterministically and anything derived from that order would be a test that
    // changes its mind between runs.
    assert_eq!(zero_xors, 0, "{zero_xors} pairs of keys are equal");
    assert_eq!(
        xors.len(),
        pairs,
        "{duplicate_xors} of {pairs} pairwise XORs repeat; a repeated XOR is a systematic \
         transposition collision, not a chance one"
    );
}

#[test]
fn zobrist_accessors_agree_with_the_flat_index() {
    // D-0016: an accessor that every other assertion routes through is a single point at
    // which all of them can be made to lie. The flat index is recomputed here from the
    // documented formula rather than read from the implementation.
    for piece in Piece::ALL {
        for index in 0..64u8 {
            let square = Square::new(index).expect("index below 64 is a square");
            let flat = piece.index() * 64 + index as usize;
            assert_eq!(
                ZOBRIST.piece_square(piece, square),
                ZOBRIST.flat(flat),
                "piece_square({piece:?}, {square}) should be flat index {flat}"
            );
        }
    }

    assert_eq!(ZOBRIST.side_to_move(), ZOBRIST.flat(768));

    for right in CastlingRight::ALL {
        assert_eq!(
            ZOBRIST.castling(right),
            ZOBRIST.flat(769 + right.index()),
            "castling({right:?})"
        );
    }

    for file in File::ALL {
        assert_eq!(
            ZOBRIST.en_passant(file),
            ZOBRIST.flat(773 + file.index()),
            "en_passant({file:?})"
        );
    }
}

#[test]
fn serialisation_is_little_endian() {
    // Pins the digest's byte order independently of the table, so that a silent switch to
    // `to_be_bytes` cannot be papered over by regenerating the constant.
    assert_eq!(
        serialise(&[0x0102_0304_0506_0708]),
        vec![0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
    );
}

#[test]
fn zobrist_table_digest_is_pinned() {
    let bytes = serialise(&flat_keys());
    assert_eq!(bytes.len(), Zobrist::LEN * 8, "6248 bytes of key material");

    assert_eq!(
        sha256_hex(&bytes),
        ZOBRIST_SHA256,
        "the zobrist key set changed.\n\
         Every position key, every stored transposition entry and every published bug repro \
         moved with it. If that was deliberate, update ZOBRIST_SHA256 and say why in the \
         commit message and in a decision-log entry superseding D-0020."
    );
}

#[test]
fn castling_masks_fold_to_sixteen_distinct_values() {
    // Four independent keys do not automatically give sixteen distinct subset-XORs — that
    // is exactly the pairwise-independence property above, restated in the form the board
    // actually uses it in. It costs nothing to check and it is what makes a castling right
    // observable in the key.
    let mut folded = Vec::new();
    for bits in 0..16u8 {
        let rights = CastlingRights::from_bits(bits).expect("four bits is a valid mask");
        let mut key = 0u64;
        for right in CastlingRight::ALL {
            if rights.has(right) {
                key ^= ZOBRIST.castling(right);
            }
        }
        folded.push(key);
    }

    let distinct: std::collections::HashSet<u64> = folded.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        16,
        "the sixteen castling-rights subsets fold to only {} distinct keys",
        distinct.len()
    );
}

#[test]
fn nine_en_passant_states_are_pairwise_distinct() {
    // Eight files plus "no en-passant square", which contributes nothing. All nine must be
    // distinguishable, and pairwise is the only honest way to say so: an index bug like
    // `en_passant[file & 3]` passes a two-file comparison and collapses a/e, b/f, c/g, d/h.
    let mut states = vec![("none", 0u64)];
    for file in File::ALL {
        states.push(("file", ZOBRIST.en_passant(file)));
    }

    for (i, &(name_i, key_i)) in states.iter().enumerate() {
        for &(name_j, key_j) in &states[i + 1..] {
            assert_ne!(
                key_i, key_j,
                "en-passant states {i} ({name_i}) and ({name_j}) hash identically"
            );
        }
    }
}
