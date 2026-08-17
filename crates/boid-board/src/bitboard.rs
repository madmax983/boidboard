//! A set of squares, one bit each.
//!
//! Bit `n` is the square with LERF index `n`, so bit 0 is `a1` and bit 63 is `h8`
//! (`docs/DECISIONS.md` D-0019). Issue #5's magic bitboards are built on this type; nothing
//! here anticipates them beyond keeping the operations `const` and the representation a
//! plain `u64`.

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

use crate::types::Square;

/// A set of squares.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Hash)]
pub struct Bitboard(u64);

impl Bitboard {
    /// The empty set.
    pub const EMPTY: Bitboard = Bitboard(0);
    /// Every square.
    pub const FULL: Bitboard = Bitboard(u64::MAX);

    /// The set this bit pattern describes.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Bitboard {
        Bitboard(bits)
    }

    /// The bit pattern.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Whether the set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// How many squares the set contains.
    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Whether `square` is in the set.
    #[must_use]
    pub const fn contains(self, square: Square) -> bool {
        self.0 & (1u64 << square.index()) != 0
    }

    /// This set with `square` added.
    #[must_use]
    pub const fn with(self, square: Square) -> Bitboard {
        Bitboard(self.0 | (1u64 << square.index()))
    }

    /// This set with `square` removed.
    #[must_use]
    pub const fn without(self, square: Square) -> Bitboard {
        Bitboard(self.0 & !(1u64 << square.index()))
    }
}

impl IntoIterator for Bitboard {
    type Item = Square;
    type IntoIter = BitboardIter;

    fn into_iter(self) -> BitboardIter {
        BitboardIter(self.0)
    }
}

/// Iterates a [`Bitboard`]'s squares, lowest LERF index first.
#[derive(Clone, Copy, Debug)]
pub struct BitboardIter(u64);

impl Iterator for BitboardIter {
    type Item = Square;

    fn next(&mut self) -> Option<Square> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros();
        self.0 &= self.0 - 1;
        // `trailing_zeros` of a non-zero u64 is below 64, so this square exists.
        Square::new(index as u8)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.0.count_ones() as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for BitboardIter {}

impl BitAnd for Bitboard {
    type Output = Bitboard;
    fn bitand(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 & rhs.0)
    }
}

impl BitOr for Bitboard {
    type Output = Bitboard;
    fn bitor(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 | rhs.0)
    }
}

impl BitXor for Bitboard {
    type Output = Bitboard;
    fn bitxor(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 ^ rhs.0)
    }
}

impl Not for Bitboard {
    type Output = Bitboard;
    fn not(self) -> Bitboard {
        Bitboard(!self.0)
    }
}

impl BitAndAssign for Bitboard {
    fn bitand_assign(&mut self, rhs: Bitboard) {
        self.0 &= rhs.0;
    }
}

impl BitOrAssign for Bitboard {
    fn bitor_assign(&mut self, rhs: Bitboard) {
        self.0 |= rhs.0;
    }
}

impl BitXorAssign for Bitboard {
    fn bitxor_assign(&mut self, rhs: Bitboard) {
        self.0 ^= rhs.0;
    }
}
