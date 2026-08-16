//! Cycle 1: the zobrist key set exists, is generated the way the issue specifies, and has
//! the structural properties a key set must have to be usable as a hash.
//!
//! The tests here are deliberately about the *table*, not about positions. A key set can
//! be perfectly well-behaved statistically and still be the wrong key set — drawn by the
//! wrong mixer, indexed by the wrong arithmetic, or sized for 64 en-passant squares rather
//! than 8 en-passant files. Each of those is a separate assertion below, because none of
//! them is visible in a position's key.

use std::collections::HashSet;

use boid_board::board::{CastlingRights, File, Piece, Square};
use boid_board::zobrist::{
    self, CASTLING_INDEX, EN_PASSANT_INDEX, GAMMA, KEY_COUNT, SIDE_TO_MOVE_INDEX, ZOBRIST_SEED,
};

// ---------------------------------------------------------------------------------
// The generator
// ---------------------------------------------------------------------------------

/// The only assertion in this file transcribed from outside the project.
///
/// Everything else here would hold for *any* well-behaved mixer. This is what pins the
/// table to splitmix64 specifically: a stable, avalanched, structurally perfect table
/// derived by the wrong generator passes every other test in this file.
#[test]
fn splitmix64_matches_the_published_vectors() {
    let mut state = 0u64;
    let mut got = Vec::new();
    for _ in 0..4 {
        let (next, out) = zobrist::splitmix64(state);
        state = next;
        got.push(out);
    }
    assert_eq!(
        got,
        vec![
            0xE220_A839_7B1D_CDAF,
            0x6E78_9E6A_A1B9_65F4,
            0x06C4_5D18_8009_454F,
            0xF88B_B8A8_724C_81EC,
        ],
        "splitmix64 from state 0 must reproduce Vigna's published output sequence"
    );
}

#[test]
fn the_seed_is_the_project_name_in_big_endian_bytes() {
    assert_eq!(ZOBRIST_SEED, 0x626F_6964_626F_7264);
    assert_eq!(ZOBRIST_SEED.to_be_bytes(), *b"boidbord");
}

#[test]
fn gamma_is_vignas_published_increment() {
    assert_eq!(GAMMA, 0x9E37_79B9_7F4A_7C15);
}

/// The closed form and the iterated form are two independent derivations of the same
/// table. An index-arithmetic slip in either one shows up here rather than in a digest,
/// where it would be indistinguishable from a deliberate change.
#[test]
fn closed_form_agrees_with_the_iterated_form() {
    for (index, key) in zobrist::table().iter().enumerate() {
        assert_eq!(
            key.get(),
            zobrist::key_at(index),
            "closed form disagrees with the table at index {index}"
        );
    }
}

// ---------------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------------

/// Written as the issue's own arithmetic so that a change to any term is visible.
///
/// The two realistic wrong answers are 793 (64 en-passant *square* keys instead of 8 file
/// keys) and 784 (16 independently drawn castling combinations instead of 4 base keys).
#[test]
fn key_count_is_the_issues_arithmetic() {
    assert_eq!(KEY_COUNT, 768 + 1 + 4 + 8);
    assert_eq!(KEY_COUNT, 781);
    assert_eq!(zobrist::table().len(), 781);
}

#[test]
fn the_index_layout_is_the_one_the_decision_log_froze() {
    assert_eq!(SIDE_TO_MOVE_INDEX, 768);
    assert_eq!(CASTLING_INDEX, 769);
    assert_eq!(EN_PASSANT_INDEX, 773);
    assert_eq!(EN_PASSANT_INDEX + File::COUNT, KEY_COUNT);
}

/// Pins the accessor against a directly computed index.
///
/// Iterates `Piece × Square` rather than `0..768` deliberately: the range form is
/// `clippy::needless_range_loop`, and the mechanical fix for it turns this assertion into
/// a tautology that compares the table with itself. D-0016 requires an accessor that
/// assertions route through to be pinned against a direct computation.
#[test]
fn piece_square_agrees_with_a_directly_computed_index() {
    let table = zobrist::table();
    for piece in Piece::ALL {
        for index in 0..Square::COUNT {
            let square = Square::from_index(index as u8).expect("index is below 64");
            let expected = table[piece.index() * 64 + square.index()];
            assert_eq!(
                zobrist::piece_square(piece, square),
                expected,
                "piece_square({piece:?}, {square}) is not table[piece * 64 + square]"
            );
        }
    }
}

/// Collects every key **through the public accessors** rather than out of the raw table.
///
/// Raw-table distinctness cannot see an accessor offset bug that aliases two roles onto
/// one slot — for instance an en-passant accessor based at 772, which would make the
/// a-file key and the last castling key the same value while the table stays perfectly
/// distinct.
#[test]
fn every_accessor_value_appears_exactly_once_in_the_table() {
    let mut seen = HashSet::new();
    for piece in Piece::ALL {
        for index in 0..Square::COUNT {
            let square = Square::from_index(index as u8).expect("index is below 64");
            assert!(
                seen.insert(zobrist::piece_square(piece, square).get()),
                "piece_square({piece:?}, {square}) collides with an earlier accessor value"
            );
        }
    }
    assert!(
        seen.insert(zobrist::side_to_move().get()),
        "the side-to-move key collides with a piece-square key"
    );
    for right in CastlingRights::EACH {
        assert!(
            seen.insert(zobrist::castling(right).get()),
            "the castling key for {right} collides with another accessor value"
        );
    }
    for i in 0..File::COUNT {
        let file = File::new(i as u8).expect("index is below 8");
        assert!(
            seen.insert(zobrist::en_passant(Some(file)).get()),
            "the en-passant key for file {file} collides with another accessor value"
        );
    }
    assert_eq!(
        seen.len(),
        KEY_COUNT,
        "the accessors must between them reach every key exactly once"
    );
}

// ---------------------------------------------------------------------------------
// Castling and en passant semantics
// ---------------------------------------------------------------------------------

/// The empty fold being zero is the giveaway that distinguishes four XOR-folded base keys
/// from sixteen independently drawn ones — an independently drawn table has a non-zero
/// value at mask 0.
#[test]
fn castling_folds_are_sixteen_distinct_values_and_the_empty_fold_is_zero() {
    let mut folds = HashSet::new();
    for bits in 0..16u8 {
        let rights = CastlingRights::from_bits(bits).expect("bits below 16 are valid");
        assert!(
            folds.insert(zobrist::castling(rights).get()),
            "two castling masks fold to the same key; the four base keys are not \
             GF(2)-independent"
        );
    }
    assert_eq!(folds.len(), 16);
    assert_eq!(
        zobrist::castling(CastlingRights::NONE).get(),
        0,
        "the empty fold must be the XOR identity"
    );
}

/// Encoding "no en passant" as file `a` would make every a-file en passant invisible to
/// the hash, which is a bug no perft count can see.
#[test]
fn the_en_passant_accessor_has_eight_distinct_nonzero_file_keys_and_returns_zero_for_none() {
    assert_eq!(zobrist::en_passant(None).get(), 0);
    let mut seen = HashSet::new();
    for i in 0..File::COUNT {
        let file = File::new(i as u8).expect("index is below 8");
        let key = zobrist::en_passant(Some(file)).get();
        assert_ne!(key, 0, "the key for file {file} must not be the identity");
        assert!(
            seen.insert(key),
            "file {file} shares a key with another file"
        );
    }
    assert_eq!(seen.len(), 8);
}

// ---------------------------------------------------------------------------------
// Structural quality
// ---------------------------------------------------------------------------------

#[test]
fn no_key_is_zero() {
    for (index, key) in zobrist::table().iter().enumerate() {
        assert_ne!(
            key.get(),
            0,
            "key {index} is zero, so it is the XOR identity and hashes as absent"
        );
    }
}

#[test]
fn all_keys_are_distinct() {
    let seen: HashSet<u64> = zobrist::table().iter().map(|k| k.get()).collect();
    assert_eq!(
        seen.len(),
        KEY_COUNT,
        "two keys are equal, so two distinct facts about a position hash identically"
    );
}

/// A weight-3 GF(2) dependency: one key equal to the XOR of two others means three
/// different positions collide by construction rather than by luck.
#[test]
fn no_key_is_the_xor_of_two_others() {
    let keys: Vec<u64> = zobrist::table().iter().map(|k| k.get()).collect();
    let all: HashSet<u64> = keys.iter().copied().collect();
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            let combined = a ^ b;
            assert!(
                !all.contains(&combined),
                "key {a:#018x} xor {b:#018x} is itself a key"
            );
        }
    }
}

/// A shift typo that leaves a whole byte lane constant is invisible to distinctness and to
/// any digest that nobody has an independent value for.
#[test]
fn every_bit_position_is_both_set_and_clear_somewhere_in_the_table() {
    let mut ones = 0u64;
    let mut zeros = 0u64;
    for key in zobrist::table() {
        ones |= key.get();
        zeros |= !key.get();
    }
    assert_eq!(ones, u64::MAX, "some bit is clear in every key");
    assert_eq!(zeros, u64::MAX, "some bit is set in every key");
}

/// Not a statistical test with a tuned threshold — a wide sanity band. A table whose mean
/// population count is far from 32 is not a mixer's output at all.
#[test]
fn the_mean_population_count_is_close_to_half_the_word() {
    let total: u32 = zobrist::table().iter().map(|k| k.get().count_ones()).sum();
    let mean = f64::from(total) / KEY_COUNT as f64;
    assert!(
        (28.0..=36.0).contains(&mean),
        "mean popcount {mean:.3} is not consistent with a 64-bit mixer's output"
    );
}
