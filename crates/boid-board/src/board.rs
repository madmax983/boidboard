//! The board value type and the primitives it is built from.
//!
//! Squares are **LERF**: `a1 = 0`, `b1 = 1`, ... `h8 = 63`, so `square = rank * 8 + file`
//! and a `u64` bitboard's bit *n* is square *n*. Pieces are numbered `kind * 2 + colour`,
//! which puts the two pawn kinds at indices 0 and 1 and therefore puts every pawn key in
//! the contiguous prefix of the zobrist piece-square block — see `docs/DECISIONS.md`
//! D-0018, which froze both orderings before the first key was computed.

use core::fmt;

// ---------------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------------

/// Which side a piece belongs to, and which side is to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Color {
    /// The side that moves first.
    White = 0,
    /// The side that moves second.
    Black = 1,
}

impl Color {
    /// Both colours, in the order the piece numbering uses.
    pub const ALL: [Self; 2] = [Self::White, Self::Black];

    /// The other side.
    #[must_use]
    pub const fn flip(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }

    /// `0` for White, `1` for Black — the index into a two-element colour array.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The FEN side-to-move letter.
    #[must_use]
    pub const fn to_fen_char(self) -> char {
        match self {
            Self::White => 'w',
            Self::Black => 'b',
        }
    }
}

// ---------------------------------------------------------------------------------
// Piece kind and piece
// ---------------------------------------------------------------------------------

/// A piece type, without a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum PieceKind {
    /// Pawn. Kind `0`, which is what makes the pawn keys a contiguous prefix (D-0018).
    Pawn = 0,
    /// Knight.
    Knight = 1,
    /// Bishop.
    Bishop = 2,
    /// Rook.
    Rook = 3,
    /// Queen.
    Queen = 4,
    /// King.
    King = 5,
}

impl PieceKind {
    /// Every kind, in numbering order.
    pub const ALL: [Self; 6] = [
        Self::Pawn,
        Self::Knight,
        Self::Bishop,
        Self::Rook,
        Self::Queen,
        Self::King,
    ];

    /// The index into a six-element piece-kind array.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The four kinds a pawn may promote to.
    pub const PROMOTIONS: [Self; 4] = [Self::Knight, Self::Bishop, Self::Rook, Self::Queen];

    /// The lowercase FEN letter for this kind.
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            Self::Pawn => 'p',
            Self::Knight => 'n',
            Self::Bishop => 'b',
            Self::Rook => 'r',
            Self::Queen => 'q',
            Self::King => 'k',
        }
    }
}

/// A coloured piece.
///
/// The discriminant is `kind * 2 + colour` (D-0018). It is the index into the zobrist
/// piece-square block, and it is the byte stored in the mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Piece {
    /// White pawn — piece index 0.
    WhitePawn = 0,
    /// Black pawn — piece index 1.
    BlackPawn = 1,
    /// White knight.
    WhiteKnight = 2,
    /// Black knight.
    BlackKnight = 3,
    /// White bishop.
    WhiteBishop = 4,
    /// Black bishop.
    BlackBishop = 5,
    /// White rook.
    WhiteRook = 6,
    /// Black rook.
    BlackRook = 7,
    /// White queen.
    WhiteQueen = 8,
    /// Black queen.
    BlackQueen = 9,
    /// White king — piece index 10.
    WhiteKing = 10,
    /// Black king — piece index 11.
    BlackKing = 11,
}

impl Piece {
    /// How many distinct coloured pieces there are. `12 * 64 == 768`, the issue's
    /// piece-square key count.
    pub const COUNT: usize = 12;

    /// Every piece, in numbering order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::WhitePawn,
        Self::BlackPawn,
        Self::WhiteKnight,
        Self::BlackKnight,
        Self::WhiteBishop,
        Self::BlackBishop,
        Self::WhiteRook,
        Self::BlackRook,
        Self::WhiteQueen,
        Self::BlackQueen,
        Self::WhiteKing,
        Self::BlackKing,
    ];

    /// Build a piece from its colour and kind.
    #[must_use]
    pub const fn new(color: Color, kind: PieceKind) -> Self {
        // Safe by construction rather than by transmute: the match is exhaustive over
        // 12 cases and `unsafe` is denied in this workspace (D-0009).
        match (color, kind) {
            (Color::White, PieceKind::Pawn) => Self::WhitePawn,
            (Color::Black, PieceKind::Pawn) => Self::BlackPawn,
            (Color::White, PieceKind::Knight) => Self::WhiteKnight,
            (Color::Black, PieceKind::Knight) => Self::BlackKnight,
            (Color::White, PieceKind::Bishop) => Self::WhiteBishop,
            (Color::Black, PieceKind::Bishop) => Self::BlackBishop,
            (Color::White, PieceKind::Rook) => Self::WhiteRook,
            (Color::Black, PieceKind::Rook) => Self::BlackRook,
            (Color::White, PieceKind::Queen) => Self::WhiteQueen,
            (Color::Black, PieceKind::Queen) => Self::BlackQueen,
            (Color::White, PieceKind::King) => Self::WhiteKing,
            (Color::Black, PieceKind::King) => Self::BlackKing,
        }
    }

    /// The piece with index `i`, or `None` if `i >= 12`.
    #[must_use]
    pub const fn from_index(i: usize) -> Option<Self> {
        if i >= Self::COUNT {
            return None;
        }
        Some(Self::ALL[i])
    }

    /// The index into the zobrist piece-square block and into the mailbox encoding.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// This piece's colour. The low bit of the discriminant, by D-0018's ordering.
    #[must_use]
    pub const fn color(self) -> Color {
        if (self as u8) & 1 == 0 {
            Color::White
        } else {
            Color::Black
        }
    }

    /// This piece's kind. The discriminant halved, by D-0018's ordering.
    #[must_use]
    pub const fn kind(self) -> PieceKind {
        PieceKind::ALL[(self as usize) >> 1]
    }

    /// Whether this is a pawn of either colour.
    ///
    /// One comparison rather than a match, because the pawns are indices 0 and 1 — the
    /// property D-0018's piece ordering exists to buy.
    #[must_use]
    pub const fn is_pawn(self) -> bool {
        (self as u8) < 2
    }

    /// The FEN letter: uppercase for White, lowercase for Black.
    #[must_use]
    pub const fn to_fen_char(self) -> char {
        let lower = self.kind().to_char();
        match self.color() {
            Color::White => lower.to_ascii_uppercase(),
            Color::Black => lower,
        }
    }

    /// The piece a FEN letter denotes, or `None` if it denotes no piece.
    #[must_use]
    pub const fn from_fen_char(ch: char) -> Option<Self> {
        Some(match ch {
            'P' => Self::WhitePawn,
            'N' => Self::WhiteKnight,
            'B' => Self::WhiteBishop,
            'R' => Self::WhiteRook,
            'Q' => Self::WhiteQueen,
            'K' => Self::WhiteKing,
            'p' => Self::BlackPawn,
            'n' => Self::BlackKnight,
            'b' => Self::BlackBishop,
            'r' => Self::BlackRook,
            'q' => Self::BlackQueen,
            'k' => Self::BlackKing,
            _ => return None,
        })
    }
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_fen_char())
    }
}

// ---------------------------------------------------------------------------------
// File and square
// ---------------------------------------------------------------------------------

/// A board file, `a` through `h`.
///
/// A distinct type because the en-passant state is stored as a *file*, never a square:
/// the rank follows from the side to move, so a rank is not representable and therefore
/// cannot be wrong (D-0019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct File(u8);

impl File {
    /// How many files there are — and how many en-passant zobrist keys.
    pub const COUNT: usize = 8;

    /// The file with index `i` (`0 = a`), or `None` if `i >= 8`.
    #[must_use]
    pub const fn new(i: u8) -> Option<Self> {
        if i as usize >= Self::COUNT {
            return None;
        }
        Some(Self(i))
    }

    /// This file's index, `0` for `a` through `7` for `h`.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The file letter.
    #[must_use]
    pub const fn to_char(self) -> char {
        (b'a' + self.0) as char
    }

    /// The file a letter denotes, or `None`.
    #[must_use]
    pub const fn from_char(ch: char) -> Option<Self> {
        if ch.is_ascii_lowercase() && (ch as u8) <= b'h' {
            Self::new(ch as u8 - b'a')
        } else {
            None
        }
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

/// One of the 64 squares, LERF-numbered: `a1 = 0`, `h8 = 63`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Square(u8);

impl Square {
    /// How many squares there are.
    pub const COUNT: usize = 64;

    /// `a1`, the origin of the LERF numbering.
    pub const A1: Self = Self(0);
    /// `e1`, White's king's home square.
    pub const E1: Self = Self(4);
    /// `h1`.
    pub const H1: Self = Self(7);
    /// `a8`.
    pub const A8: Self = Self(56);
    /// `e8`, Black's king's home square.
    pub const E8: Self = Self(60);
    /// `h8`.
    pub const H8: Self = Self(63);

    /// The square with LERF index `i`, or `None` if `i >= 64`.
    #[must_use]
    pub const fn from_index(i: u8) -> Option<Self> {
        if i as usize >= Self::COUNT {
            return None;
        }
        Some(Self(i))
    }

    /// The square at `file` and `rank`, both zero-based, or `None` if either is out of
    /// range.
    #[must_use]
    pub const fn new(file: File, rank: u8) -> Option<Self> {
        if rank >= 8 {
            return None;
        }
        Some(Self(rank * 8 + file.0))
    }

    /// The LERF index.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// This square's file.
    #[must_use]
    pub const fn file(self) -> File {
        File(self.0 % 8)
    }

    /// This square's zero-based rank: `0` is rank 1, `7` is rank 8.
    #[must_use]
    pub const fn rank(self) -> u8 {
        self.0 / 8
    }

    /// The square `delta` ranks away, or `None` if that leaves the board.
    #[must_use]
    pub const fn offset_rank(self, delta: i8) -> Option<Self> {
        let rank = self.rank() as i8 + delta;
        if rank < 0 || rank >= 8 {
            return None;
        }
        Some(Self((rank as u8) * 8 + self.0 % 8))
    }

    /// The square a two-letter coordinate names, or `None`.
    #[must_use]
    pub fn from_uci(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.len() != 2 {
            return None;
        }
        let file = File::from_char(bytes[0] as char)?;
        let rank = bytes[1].checked_sub(b'1')?;
        Self::new(file, rank)
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.file().to_char(), self.rank() + 1)
    }
}

// ---------------------------------------------------------------------------------
// Castling rights
// ---------------------------------------------------------------------------------

/// The four castling rights, as a bitset.
///
/// The bit order `WK, WQ, BK, BQ` is the order of the four zobrist castling keys and of
/// the FEN `KQkq` field (D-0018).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct CastlingRights(u8);

impl CastlingRights {
    /// No rights — the FEN `-`.
    pub const NONE: Self = Self(0);
    /// White may castle king-side — the FEN `K`.
    pub const WHITE_KING: Self = Self(1 << 0);
    /// White may castle queen-side — the FEN `Q`.
    pub const WHITE_QUEEN: Self = Self(1 << 1);
    /// Black may castle king-side — the FEN `k`.
    pub const BLACK_KING: Self = Self(1 << 2);
    /// Black may castle queen-side — the FEN `q`.
    pub const BLACK_QUEEN: Self = Self(1 << 3);
    /// All four rights — the FEN `KQkq`.
    pub const ALL: Self = Self(0b1111);

    /// The four rights individually, in D-0018's order.
    pub const EACH: [Self; 4] = [
        Self::WHITE_KING,
        Self::WHITE_QUEEN,
        Self::BLACK_KING,
        Self::BLACK_QUEEN,
    ];

    /// The raw bits, `0..=15`.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Rights from raw bits, or `None` if any bit above the low four is set.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits > 0b1111 {
            return None;
        }
        Some(Self(bits))
    }

    /// Whether every right in `other` is present.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// These rights with `other` added.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// These rights with `other` removed.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Whether no right is present.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for CastlingRights {
    /// The FEN castling field: `KQkq` in that fixed order, or `-` when empty.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("-");
        }
        for (right, ch) in Self::EACH.iter().zip(['K', 'Q', 'k', 'q']) {
            if self.contains(*right) {
                write!(f, "{ch}")?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------
// Bitboard
// ---------------------------------------------------------------------------------

/// A set of squares, one bit per square, LERF-numbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Bitboard(u64);

impl Bitboard {
    /// The empty set.
    pub const EMPTY: Self = Self(0);

    /// A set from raw bits.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// The raw bits.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// A set holding exactly `square`.
    #[must_use]
    pub const fn from_square(square: Square) -> Self {
        Self(1u64 << square.index())
    }

    /// Whether `square` is in the set.
    #[must_use]
    pub const fn contains(self, square: Square) -> bool {
        self.0 & (1u64 << square.index()) != 0
    }

    /// This set with `square` added.
    #[must_use]
    pub const fn with(self, square: Square) -> Self {
        Self(self.0 | (1u64 << square.index()))
    }

    /// This set with `square` removed.
    #[must_use]
    pub const fn without(self, square: Square) -> Self {
        Self(self.0 & !(1u64 << square.index()))
    }

    /// This set's union with `other`.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// This set's intersection with `other`.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// How many squares are in the set.
    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Whether the set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The squares in the set, in ascending LERF order.
    pub fn squares(self) -> impl Iterator<Item = Square> {
        BitboardIter(self.0)
    }
}

/// Iterator over the squares of a [`Bitboard`], least-significant bit first.
#[derive(Debug, Clone)]
struct BitboardIter(u64);

impl Iterator for BitboardIter {
    type Item = Square;

    fn next(&mut self) -> Option<Square> {
        if self.0 == 0 {
            return None;
        }
        // `trailing_zeros` of a non-zero u64 is at most 63, so the square is in range.
        let index = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Square::from_index(index)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.0.count_ones() as usize;
        (n, Some(n))
    }
}

impl ExactSizeIterator for BitboardIter {}
