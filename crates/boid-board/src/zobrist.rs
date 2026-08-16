//! Zobrist keys: a fixed table of 781 pseudo-random `u64`s, generated at compile time.
//!
//! A position's key is the XOR of one key per piece-on-square, the side-to-move key when
//! Black is to move, one key per castling right present, and one key per en-passant
//! **file** when a file is set. Two positions that agree on all of those hash to the same
//! value, which is what makes a transposition table and a hashed perft possible.
//!
//! # Never from entropy
//!
//! The table is a `const fn` of a hardcoded seed, and the proof of that is structural
//! rather than behavioural: [`build_table`] is forced through compile-time evaluation by a
//! `const` item, and a call to an entropy source inside a `const fn` is a **compile
//! error**. A test that builds the table twice in one process and compares would be green
//! for a `OnceLock` seeded from the operating system, which is the exact defect it appears
//! to exclude.
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
///
/// # Examples
///
/// The published output sequence from state zero:
///
/// ```
/// use boid_board::zobrist::splitmix64;
///
/// let (s, a) = splitmix64(0);
/// let (_, b) = splitmix64(s);
/// assert_eq!(a, 0xE220_A839_7B1D_CDAF);
/// assert_eq!(b, 0x6E78_9E6A_A1B9_65F4);
/// ```
#[must_use]
pub const fn splitmix64(state: u64) -> (u64, u64) {
    let state = state.wrapping_add(GAMMA);
    let mut z = state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (state, z ^ (z >> 31))
}

/// The key at `index`, in closed form.
///
/// Equivalent to iterating [`splitmix64`] from [`ZOBRIST_SEED`] `index + 1` times, but
/// computable for a single index — so a reviewer can check one key with a calculator and
/// `scripts/zobrist-reference.py` can be a one-liner. A test asserts the two forms agree
/// on all [`KEY_COUNT`] indices.
#[must_use]
pub const fn key_at(index: usize) -> u64 {
    let advanced = ZOBRIST_SEED.wrapping_add((index as u64).wrapping_mul(GAMMA));
    let (_, out) = splitmix64(advanced);
    out
}

/// Build the whole table by iterating [`splitmix64`] from [`ZOBRIST_SEED`].
#[must_use]
pub const fn build_table() -> [ZobristKey; KEY_COUNT] {
    let mut table = [ZobristKey::ZERO; KEY_COUNT];
    let mut state = ZOBRIST_SEED;
    let mut i = 0;
    while i < KEY_COUNT {
        let (next, out) = splitmix64(state);
        state = next;
        table[i] = ZobristKey(out);
        i += 1;
    }
    table
}

/// The table itself.
///
/// `static`, not `const`: a `const` array is semantically copied into every use site,
/// which for 6,248 bytes would be a per-call-site copy rather than one `.rodata` block.
static ZOBRIST: [ZobristKey; KEY_COUNT] = build_table();

/// Forces the table through compile-time evaluation.
///
/// This item is the load-bearing half of "never from entropy" (D-0024). It is not a test
/// and cannot be skipped, filtered out, or run in a configuration that omits it: if
/// `build_table` ever reached for a clock, a file, or an operating-system entropy source,
/// **this line would stop compiling**.
const _TABLE_IS_CONST_EVALUATED: [ZobristKey; KEY_COUNT] = build_table();

/// The key for `piece` standing on `square`.
///
/// Index arithmetic is `piece * 64 + square`, never `square * 12 + piece` (D-0018).
#[must_use]
pub fn piece_square(piece: Piece, square: Square) -> ZobristKey {
    ZOBRIST[piece.index() * Square::COUNT + square.index()]
}

/// The key XORed in exactly when Black is to move.
#[must_use]
pub fn side_to_move() -> ZobristKey {
    ZOBRIST[SIDE_TO_MOVE_INDEX]
}

/// The XOR-fold of the castling keys for the rights present.
///
/// Four base keys XOR-folded, not sixteen independently drawn ones — which is what the
/// issue specifies, and what makes `key ^= castling(old) ^ castling(new)` handle the loss
/// of several rights at once in a single code path. The empty fold is therefore zero, and
/// a test pins that.
#[must_use]
pub fn castling(rights: CastlingRights) -> ZobristKey {
    let mut key = ZobristKey::ZERO;
    for (i, right) in CastlingRights::EACH.iter().enumerate() {
        if rights.contains(*right) {
            key ^= ZOBRIST[CASTLING_INDEX + i];
        }
    }
    key
}

/// The key for the en-passant **file**, or zero when there is none.
///
/// Eight keys, not sixty-four: the rank follows from the side to move (D-0019). Returning
/// zero for `None` keeps the call site branchless without a ninth key existing — and
/// because zero is the XOR identity, "no en-passant file" contributes nothing, which is
/// the same thing the absent key would have meant.
#[must_use]
pub fn en_passant(file: Option<File>) -> ZobristKey {
    match file {
        Some(f) => ZOBRIST[EN_PASSANT_INDEX + f.index()],
        None => ZobristKey::ZERO,
    }
}

/// The whole table, for the digest and for structural assertions.
#[must_use]
pub fn table() -> &'static [ZobristKey; KEY_COUNT] {
    &ZOBRIST
}
