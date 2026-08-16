//! Zobrist keys: a fixed table of 781 pseudo-random `u64`s, generated at compile time.
//!
//! A position's key is the XOR of one key per piece-on-square, the side-to-move key when
//! Black is to move, one key per castling right present, and one key per en-passant
//! **file** when a file is set. Two positions that agree on all of those hash to the same
//! value, which is what makes a transposition table and a hashed perft possible.
//!
//! Nothing here is implemented yet. The types, the index layout and the constants are
//! fixed first because `docs/DECISIONS.md` D-0018 froze them before any key existed, and
//! the tests in `tests/zobrist_table.rs` are written against that frozen contract.
//!
//! See `docs/DECISIONS.md` D-0018 (the frozen orderings) and D-0024 (the generator, the
//! seed, and why no seed search was performed).

use core::fmt;
use core::ops::BitXorAssign;

use crate::board::{CastlingRights, File, Piece, Square};

/// The seed, written as the expression that produces it so its provenance is visible.
///
/// `0x626F_6964_626F_7264`. No seed was tried, measured and kept: a searched seed would
/// make every structural claim about the key set a fitted result rather than a property of
/// the generator (D-0024).
pub const ZOBRIST_SEED: u64 = u64::from_be_bytes(*b"boidbord");

/// splitmix64's increment, `floor(2^64 / phi)`, from Vigna's published algorithm.
pub const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// How many keys the table holds, written as the issue's own arithmetic:
/// 768 piece-square + 1 side-to-move + 4 castling + 8 en-passant file.
pub const KEY_COUNT: usize = 768 + 1 + 4 + 8;

/// Index of the side-to-move key.
pub const SIDE_TO_MOVE_INDEX: usize = 768;
/// Index of the first castling key; the four run `WK, WQ, BK, BQ`.
pub const CASTLING_INDEX: usize = 769;
/// Index of the first en-passant file key; the eight run `a` through `h`.
pub const EN_PASSANT_INDEX: usize = 773;

/// A full position key.
///
/// A newtype rather than a bare `u64` so that `pawn_key = key` is a compile error rather
/// than a silent bug that every self-consistency check agrees with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct ZobristKey(u64);

/// A pawn-structure key: the XOR of the piece-square keys of the pawns of both colours,
/// and nothing else.
///
/// Drawn from the *same* piece-square keys as [`ZobristKey`], so the issue's "768
/// piece-square keys" stays literal — there is no second table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct PawnKey(u64);

impl ZobristKey {
    /// The all-zero key, which is the key of a position with no pieces, White to move, no
    /// castling rights and no en-passant file.
    pub const ZERO: Self = Self(0);

    /// A key from raw bits.
    #[must_use]
    pub const fn from_raw(bits: u64) -> Self {
        Self(bits)
    }

    /// The raw bits — what issue #6's hashed perft keys its cache on.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl PawnKey {
    /// The key of a position with no pawns.
    ///
    /// It is zero, and that is pinned by a test rather than left to chance: a pawn-hash
    /// cache must therefore not use `0` as its "empty slot" sentinel.
    pub const ZERO: Self = Self(0);

    /// The raw bits.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl BitXorAssign for ZobristKey {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0;
    }
}

/// Pawn keys are folded from the same piece-square keys, so this is the only mixing
/// operation a pawn key admits — and it cannot be fed a whole-position key by accident.
impl BitXorAssign<ZobristKey> for PawnKey {
    fn bitxor_assign(&mut self, rhs: ZobristKey) {
        self.0 ^= rhs.0;
    }
}

impl fmt::LowerHex for ZobristKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl fmt::LowerHex for PawnKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// One step of splitmix64: advance `state` by [`GAMMA`] and mix it.
///
/// Returns `(next_state, output)`. Vigna's published constants, unchanged — they are what
/// makes this table regenerable by an outsider from the algorithm's name alone.
#[must_use]
pub const fn splitmix64(state: u64) -> (u64, u64) {
    let _ = state;
    panic!("not yet implemented: zobrist::splitmix64")
}

/// The key at `index`, in closed form.
///
/// Equivalent to iterating [`splitmix64`] from [`ZOBRIST_SEED`] `index + 1` times, but
/// computable for a single index — so a reviewer can check one key with a calculator and
/// `scripts/zobrist-reference.py` can be a one-liner. A test asserts the two forms agree
/// on all [`KEY_COUNT`] indices.
#[must_use]
pub const fn key_at(index: usize) -> u64 {
    let _ = index;
    panic!("not yet implemented: zobrist::key_at")
}

/// Build the whole table by iterating [`splitmix64`] from [`ZOBRIST_SEED`].
#[must_use]
pub fn build_table() -> [ZobristKey; KEY_COUNT] {
    todo!("zobrist::build_table")
}

/// The key for `piece` standing on `square`.
///
/// Index arithmetic is `piece * 64 + square`, never `square * 12 + piece` (D-0018).
#[must_use]
pub fn piece_square(piece: Piece, square: Square) -> ZobristKey {
    let _ = (piece, square);
    todo!("zobrist::piece_square")
}

/// The key XORed in exactly when Black is to move.
#[must_use]
pub fn side_to_move() -> ZobristKey {
    todo!("zobrist::side_to_move")
}

/// The XOR-fold of the castling keys for the rights present.
///
/// Four base keys XOR-folded, not sixteen independently drawn ones — which is what the
/// issue specifies, and what makes `key ^= castling(old) ^ castling(new)` handle the loss
/// of several rights at once in a single code path. The empty fold is therefore zero, and
/// a test pins that.
#[must_use]
pub fn castling(rights: CastlingRights) -> ZobristKey {
    let _ = rights;
    todo!("zobrist::castling")
}

/// The key for the en-passant **file**, or zero when there is none.
///
/// Eight keys, not sixty-four: the rank follows from the side to move (D-0019). Returning
/// zero for `None` keeps the call site branchless without a ninth key existing — and
/// because zero is the XOR identity, "no en-passant file" contributes nothing, which is
/// the same thing the absent key would have meant.
#[must_use]
pub fn en_passant(file: Option<File>) -> ZobristKey {
    let _ = file;
    todo!("zobrist::en_passant")
}

/// The whole table, for the digest and for structural assertions.
#[must_use]
pub fn table() -> &'static [ZobristKey; KEY_COUNT] {
    todo!("zobrist::table")
}
