//! The zobrist key set: 781 constants, generated at compile time from a hardcoded seed.
//!
//! Issue #4 is explicit that the keys are produced by splitmix64 from a hardcoded seed in a
//! `const fn` and **never from entropy**, because a reproducible bug repro and a reproducible
//! hashed-perft result are the whole point of having them. `docs/DECISIONS.md` D-0020
//! records the scheme and, more usefully, what each piece of evidence for that actually
//! proves.
//!
//! # The flat index space
//!
//! All 781 keys live in one index space, because the hygiene checks only mean anything when
//! run over the whole of it: a dependency *between* two sub-tables is exactly as fatal as
//! one inside a sub-table, and per-table checks cannot see it.
//!
//! | flat index | slot |
//! |------------|------|
//! | `0..768`   | `piece_square[piece][square]`, `i = piece * 64 + square` |
//! | `768`      | `side_to_move`, XORed when **Black** is to move |
//! | `769..773` | `castling`, in FEN `KQkq` order, XORed per right present |
//! | `773..781` | `en_passant`, by **file**, XORed whenever the FEN records an ep square |
//!
//! The piece index is `colour * 6 + kind`, which is exactly [`Piece`]'s discriminant, and
//! squares are LERF (`a1 = 0`).

use crate::types::{CastlingRight, File, Piece, Square};

/// The odd increment splitmix64 walks its state by — the golden-ratio constant from the
/// published algorithm.
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// The seed, as ASCII: `boidboar`, the first eight bytes of `boidboard`, read big-endian.
///
/// It was **not** searched for. No seed sweep was run, and the hygiene properties asserted
/// in `tests/zobrist_tables.rs` held on the first seed tried. A searched seed and an
/// arbitrary one are indistinguishable from the constant alone, so this says which it is.
const SEED: u64 = 0x626F_6964_626F_6172;

/// The splitmix64 finaliser.
///
/// Kept public because it is the one function whose correctness is pinned against a third
/// party: the published seed-0 output vectors.
#[must_use]
pub const fn splitmix64(z: u64) -> u64 {
    let mut z = z;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The key for one flat index.
///
/// The **indexed** form, not a running stream. splitmix64's state update is `state += GAMMA`,
/// so after `i` steps the state is `SEED + (i + 1) * GAMMA` and the two forms produce
/// identical output — but the indexed form makes each slot a pure function of its index, so
/// reordering the loops below cannot silently reassign keys.
///
/// Const evaluation checks integer overflow unconditionally, whatever the profile says, so
/// the `wrapping_*` calls here are enforced by the compiler rather than by discipline. That
/// is why the table cannot come out differently in a release build.
const fn key_at(index: usize) -> u64 {
    splitmix64(SEED.wrapping_add(((index as u64) + 1).wrapping_mul(GAMMA)))
}

/// The generated key set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Zobrist {
    piece_square: [[u64; 64]; 12],
    side_to_move: u64,
    castling: [u64; 4],
    en_passant: [u64; 8],
}

/// The key set, generated at compile time.
pub const ZOBRIST: Zobrist = Zobrist::generate();

impl Zobrist {
    /// How many keys the set holds: 768 piece-square, one side-to-move, four castling and
    /// eight en-passant file keys.
    pub const LEN: usize = 781;

    /// Generate the key set from [`SEED`].
    const fn generate() -> Zobrist {
        let mut piece_square = [[0u64; 64]; 12];
        let mut piece = 0;
        while piece < 12 {
            let mut square = 0;
            while square < 64 {
                piece_square[piece][square] = key_at(piece * 64 + square);
                square += 1;
            }
            piece += 1;
        }

        let side_to_move = key_at(768);

        let mut castling = [0u64; 4];
        let mut right = 0;
        while right < 4 {
            castling[right] = key_at(769 + right);
            right += 1;
        }

        let mut en_passant = [0u64; 8];
        let mut file = 0;
        while file < 8 {
            en_passant[file] = key_at(773 + file);
            file += 1;
        }

        Zobrist {
            piece_square,
            side_to_move,
            castling,
            en_passant,
        }
    }

    /// The key for `piece` standing on `square`.
    #[must_use]
    pub const fn piece_square(&self, piece: Piece, square: Square) -> u64 {
        self.piece_square[piece.index()][square.index() as usize]
    }

    /// The key XORed in when Black is to move.
    ///
    /// # Examples
    ///
    /// A doctest runs in its own process, so this pin is a second-process check of the key
    /// set for free — and it lives in a different file from the digest in
    /// `tests/zobrist_tables.rs`, which is what catches a careless "just update the
    /// constant" when the seed is changed.
    ///
    /// ```
    /// use boid_board::zobrist::ZOBRIST;
    ///
    /// assert_eq!(ZOBRIST.side_to_move(), 0x9C1E_D7FF_B9DE_746C);
    /// ```
    #[must_use]
    pub const fn side_to_move(&self) -> u64 {
        self.side_to_move
    }

    /// The key XORed in for each castling right the position carries.
    #[must_use]
    pub const fn castling(&self, right: CastlingRight) -> u64 {
        self.castling[right.index()]
    }

    /// The key XORed in when the position records an en-passant square on `file`.
    ///
    /// A file, not a square: the rank follows from the side to move, which the side-to-move
    /// key already distinguishes (D-0021).
    #[must_use]
    pub const fn en_passant(&self, file: File) -> u64 {
        self.en_passant[file.index()]
    }

    /// The key at `index` in the flat index space this module's documentation describes.
    ///
    /// This is the canonical serialisation order, and the order the pinned digest is taken
    /// over.
    ///
    /// # Panics
    ///
    /// Panics if `index` is not below [`Zobrist::LEN`].
    #[must_use]
    pub const fn flat(&self, index: usize) -> u64 {
        if index < 768 {
            self.piece_square[index / 64][index % 64]
        } else if index == 768 {
            self.side_to_move
        } else if index < 773 {
            self.castling[index - 769]
        } else if index < Zobrist::LEN {
            self.en_passant[index - 773]
        } else {
            panic!("zobrist flat index out of range")
        }
    }
}

/// Whether every key in the flat index space is non-zero.
///
/// O(n) and measured free at compile time. Its O(n^2) sibling — all keys distinct — was
/// measured at roughly +450 ms on every compile of this crate and lives in
/// `tests/zobrist_tables.rs` instead (D-0020).
const fn no_flat_key_is_zero(zobrist: &Zobrist) -> bool {
    let mut index = 0;
    while index < Zobrist::LEN {
        if zobrist.flat(index) == 0 {
            return false;
        }
        index += 1;
    }
    true
}

// Forcing the table through const evaluation is AC4's structural half: the const
// interpreter has no clock, no I/O, no entropy and no FFI, so a per-process key is not
// something these assertions detect — it is something they make impossible.
const _: () = assert!(ZOBRIST.flat(0) == 0xED3A_9728_7F13_1635);
const _: () = assert!(no_flat_key_is_zero(&ZOBRIST));
