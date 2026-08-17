//! The board value type and the primitives it is built from.
//!
//! Squares are **LERF**: `a1 = 0`, `b1 = 1`, ... `h8 = 63`, so `square = rank * 8 + file`
//! and a `u64` bitboard's bit *n* is square *n*. Pieces are numbered `kind * 2 + colour`,
//! which puts the two pawn kinds at indices 0 and 1 and therefore puts every pawn key in
//! the contiguous prefix of the zobrist piece-square block — see `docs/DECISIONS.md`
//! D-0018, which froze both orderings before the first key was computed.

use core::fmt;

use crate::zobrist::{self, PawnKey, ZobristKey};

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
    ///
    /// Returns the concrete iterator rather than `impl Iterator`. Return-position impl
    /// Trait leaks only auto traits, so with the opaque form the `ExactSizeIterator` impl
    /// below was unreachable from outside this module -- `squares().len()` did not compile.
    /// Issue #5's move generation counts bitboard populations constantly.
    #[must_use]
    pub fn squares(self) -> BitboardIter {
        BitboardIter(self.0)
    }
}

/// Iterator over the squares of a [`Bitboard`], least-significant bit first.
#[derive(Debug, Clone)]
pub struct BitboardIter(u64);

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

// ---------------------------------------------------------------------------------
// The packed state word
// ---------------------------------------------------------------------------------

/// Bit 0 of [`Board::state_word`]: the side to move, `0` White and `1` Black.
pub const SIDE_TO_MOVE_SHIFT: u32 = 0;
/// Bits 1..=4: the castling rights.
pub const CASTLING_SHIFT: u32 = 1;
/// Bits 5..=8: the en-passant file, `0..=7` for `a..h` and [`NO_EN_PASSANT`] for none.
pub const EN_PASSANT_SHIFT: u32 = 5;
/// Bits 9..=24: the halfmove clock.
pub const HALFMOVE_SHIFT: u32 = 9;
/// Bits 25..=40: the fullmove number.
pub const FULLMOVE_SHIFT: u32 = 41 - 16;
/// The en-passant nibble's "no file" value. `8..=15` are otherwise unused.
pub const NO_EN_PASSANT: u64 = 8;

/// The bits of [`Board::state_word`] the zobrist key reads, and no others.
///
/// Side to move, castling rights and en-passant file — nine bits. The halfmove clock and
/// the fullmove number are deliberately outside it: a key that included them would make
/// every transposition-table probe miss, and no perft count at any depth would notice.
/// That the key depends on exactly these bits is asserted exhaustively over all 4,608
/// legal state words rather than argued.
pub const POSITION_MASK: u64 = 0x1FF;

/// The largest halfmove clock the packed word can hold.
pub const MAX_HALFMOVE_CLOCK: u16 = u16::MAX;
/// The largest fullmove number the packed word can hold.
pub const MAX_FULLMOVE_NUMBER: u16 = u16::MAX;

// ---------------------------------------------------------------------------------
// Board
// ---------------------------------------------------------------------------------

/// A chess position: the piece placement plus everything else a FEN records.
///
/// `Copy`, and 152 bytes. There is deliberately **no `unmake_move`**: [`apply_move`] takes
/// `self` by value and returns a new board, so there is no undo stack and no aliasing
/// between a parent and a child node.
///
/// The consequence is worth stating plainly, because engines get it wrong and no unit test
/// catches it: with no undo stack there is no natural place to look up whether a position
/// has occurred before. Repetition and fifty-move detection therefore require a zobrist
/// history **threaded separately through the search stack** (issues #8 and #9). [`key`]
/// exists to be the value that history holds.
///
/// [`apply_move`]: Board::apply_move
/// [`key`]: Board::key
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Board {
    /// One bitboard per piece kind, both colours together.
    pieces: [Bitboard; 6],
    /// One bitboard per colour.
    colors: [Bitboard; 2],
    /// Redundant piece-on-square lookup. Kept in step with the bitboards by construction;
    /// [`Board::check_invariants`] is what proves it.
    mailbox: [Option<Piece>; Square::COUNT],
    /// The incrementally maintained position key.
    key: ZobristKey,
    /// The incrementally maintained pawn-structure key.
    pawn_key: PawnKey,
    /// Side to move, castling rights, en-passant file, halfmove clock, fullmove number.
    state: u64,
}

/// A way in which a [`Board`]'s redundant representations can disagree.
///
/// Public and `Result`-returning on purpose: issue #5's acceptance criterion 2 needs to
/// prove that a corrupted board is detected, and it can do so by building one by hand and
/// calling [`Board::check_invariants`] — rather than by a test-only corruption hook inside
/// `apply_move`, which is the kind of test-only path issue #6 forbids elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardInvariant {
    /// The union of the colour bitboards is not the union of the piece bitboards.
    OccupancyDisagrees,
    /// A square is in both colour bitboards.
    ColorsOverlap {
        /// The offending square.
        square: Square,
    },
    /// A square is in two piece-kind bitboards.
    KindsOverlap {
        /// The offending square.
        square: Square,
    },
    /// The mailbox and the bitboards disagree about a square.
    MailboxDisagrees {
        /// The offending square.
        square: Square,
        /// What the mailbox says.
        mailbox: Option<Piece>,
        /// What the bitboards say.
        bitboards: Option<Piece>,
    },
    /// A side does not have exactly one king.
    KingCount {
        /// Which side.
        color: Color,
        /// How many kings it has.
        found: u32,
    },
    /// The incrementally maintained key disagrees with a recompute.
    KeyDisagrees {
        /// The stored key.
        stored: ZobristKey,
        /// The key recomputed from the mailbox.
        recomputed: ZobristKey,
    },
    /// The incrementally maintained pawn key disagrees with a recompute.
    PawnKeyDisagrees {
        /// The stored key.
        stored: PawnKey,
        /// The key recomputed from the pawn bitboard.
        recomputed: PawnKey,
    },
    /// A bit above the fullmove field is set.
    ReservedStateBitsSet {
        /// The offending state word.
        state: u64,
    },
    /// The en-passant nibble holds a value that is neither a file nor "none".
    EnPassantNibbleInvalid {
        /// The offending nibble.
        nibble: u64,
    },
    /// The fullmove number is zero, which no legal FEN records.
    FullmoveNumberIsZero,
}

impl fmt::Display for BoardInvariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OccupancyDisagrees => {
                write!(
                    f,
                    "the colour bitboards and the piece bitboards cover different squares"
                )
            }
            Self::ColorsOverlap { square } => write!(f, "square {square} is both White and Black"),
            Self::KindsOverlap { square } => {
                write!(f, "square {square} is in two piece-kind bitboards")
            }
            Self::MailboxDisagrees {
                square,
                mailbox,
                bitboards,
            } => write!(
                f,
                "square {square}: the mailbox says {mailbox:?} and the bitboards say {bitboards:?}"
            ),
            Self::KingCount { color, found } => {
                write!(f, "{color:?} has {found} kings, not exactly one")
            }
            Self::KeyDisagrees { stored, recomputed } => write!(
                f,
                "the incremental key {stored:x} disagrees with the recompute {recomputed:x}"
            ),
            Self::PawnKeyDisagrees { stored, recomputed } => write!(
                f,
                "the incremental pawn key {stored:x} disagrees with the recompute {recomputed:x}"
            ),
            Self::ReservedStateBitsSet { state } => {
                write!(f, "reserved state bits are set: {state:#018x}")
            }
            Self::EnPassantNibbleInvalid { nibble } => {
                write!(
                    f,
                    "the en-passant nibble is {nibble}, which is neither a file nor 'none'"
                )
            }
            Self::FullmoveNumberIsZero => write!(f, "the fullmove number is zero"),
        }
    }
}

impl core::error::Error for BoardInvariant {}

impl Board {
    /// The FEN of the initial position.
    pub const STARTPOS_FEN: &'static str =
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    /// The initial position.
    ///
    /// # Panics
    ///
    /// Never: [`Board::STARTPOS_FEN`] is a constant this crate's own parser accepts, and a
    /// test asserts it.
    #[must_use]
    pub fn startpos() -> Self {
        match Self::from_fen(Self::STARTPOS_FEN) {
            Ok(board) => board,
            Err(e) => unreachable!("the start position FEN must parse: {e}"),
        }
    }

    /// A board with no pieces, White to move, no rights, no en-passant file, clock 0 and
    /// move number 1.
    ///
    /// Not public: it has no kings, so it fails [`Board::check_invariants`]. It exists as
    /// the starting point the FEN parser fills in.
    pub(crate) fn blank() -> Self {
        Self {
            pieces: [Bitboard::EMPTY; 6],
            colors: [Bitboard::EMPTY; 2],
            mailbox: [None; Square::COUNT],
            key: ZobristKey::ZERO,
            pawn_key: PawnKey::ZERO,
            state: 1 << FULLMOVE_SHIFT | NO_EN_PASSANT << EN_PASSANT_SHIFT,
        }
    }

    /// Put `piece` on an **empty** `square`.
    ///
    /// # Panics
    ///
    /// In debug builds, if the square is occupied.
    pub(crate) fn place(&mut self, piece: Piece, square: Square) {
        debug_assert!(
            self.mailbox[square.index()].is_none(),
            "place({piece:?}, {square}) onto an occupied square"
        );
        self.toggle(piece, square);
    }

    /// Take `piece` off `square`, where it must already stand.
    ///
    /// Asymmetric with [`Board::place`] on purpose. `toggle` alone is symmetric, so a
    /// caller that computed the wrong square ADDS a piece where it meant to remove one —
    /// and because the XOR is its own inverse, every invariant still agrees afterwards.
    /// That is a piece created from nothing with `check_invariants()` returning `Ok`, and
    /// it is what these two wrappers exist to make impossible.
    ///
    /// # Panics
    ///
    /// In debug builds, if `piece` is not the piece standing on `square`.
    pub(crate) fn remove(&mut self, piece: Piece, square: Square) {
        debug_assert_eq!(
            self.mailbox[square.index()],
            Some(piece),
            "remove({piece:?}, {square}) but that piece is not there"
        );
        self.toggle(piece, square);
    }

    /// Add or remove `piece` on `square`, keeping the bitboards, the mailbox and both keys
    /// in step.
    ///
    /// The single place any of those four is written, which is what makes them able to
    /// disagree only through a bug in this function rather than through a bug in any
    /// caller. Prefer [`Board::place`] and [`Board::remove`], which say which direction
    /// they mean and check it.
    fn toggle(&mut self, piece: Piece, square: Square) {
        let kind = piece.kind().index();
        let color = piece.color().index();
        if self.mailbox[square.index()].is_some() {
            self.pieces[kind] = self.pieces[kind].without(square);
            self.colors[color] = self.colors[color].without(square);
            self.mailbox[square.index()] = None;
        } else {
            self.pieces[kind] = self.pieces[kind].with(square);
            self.colors[color] = self.colors[color].with(square);
            self.mailbox[square.index()] = Some(piece);
        }
        let key = zobrist::piece_square(piece, square);
        self.key ^= key;
        if piece.is_pawn() {
            self.pawn_key ^= key;
        }
    }

    /// Set the packed state fields and fold their zobrist contribution into the key.
    ///
    /// Used by the FEN parser, which builds a board by placing pieces and then declaring
    /// the state once.
    pub(crate) fn set_state(
        &mut self,
        side: Color,
        rights: CastlingRights,
        ep: Option<File>,
        halfmove: u16,
        fullmove: u16,
    ) {
        // Remove the old contribution before installing the new one, so this is idempotent
        // rather than only correct on a blank board.
        self.key ^= zobrist::castling(self.castling());
        self.key ^= zobrist::en_passant(self.ep_file());
        if self.side_to_move() == Color::Black {
            self.key ^= zobrist::side_to_move();
        }

        let ep_nibble = ep.map_or(NO_EN_PASSANT, |f| f.index() as u64);
        self.state = (side as u64) << SIDE_TO_MOVE_SHIFT
            | u64::from(rights.bits()) << CASTLING_SHIFT
            | ep_nibble << EN_PASSANT_SHIFT
            | u64::from(halfmove) << HALFMOVE_SHIFT
            | u64::from(fullmove) << FULLMOVE_SHIFT;

        self.key ^= zobrist::castling(rights);
        self.key ^= zobrist::en_passant(ep);
        if side == Color::Black {
            self.key ^= zobrist::side_to_move();
        }
    }

    /// The piece standing on `square`, if any.
    #[must_use]
    pub const fn piece_at(&self, square: Square) -> Option<Piece> {
        self.mailbox[square.index()]
    }

    /// The squares holding a piece of `kind`, of either colour.
    #[must_use]
    pub const fn pieces(&self, kind: PieceKind) -> Bitboard {
        self.pieces[kind.index()]
    }

    /// The squares holding a piece of `color`.
    #[must_use]
    pub const fn colored(&self, color: Color) -> Bitboard {
        self.colors[color.index()]
    }

    /// The squares holding any piece.
    #[must_use]
    pub const fn occupied(&self) -> Bitboard {
        self.colors[0].union(self.colors[1])
    }

    /// The squares holding a piece of `kind` and `color`.
    #[must_use]
    pub const fn pieces_colored(&self, kind: PieceKind, color: Color) -> Bitboard {
        self.pieces[kind.index()].intersection(self.colors[color.index()])
    }

    /// Whose turn it is.
    #[must_use]
    pub const fn side_to_move(&self) -> Color {
        if self.state & 1 == 0 {
            Color::White
        } else {
            Color::Black
        }
    }

    /// The castling rights still available.
    #[must_use]
    pub const fn castling(&self) -> CastlingRights {
        match CastlingRights::from_bits(((self.state >> CASTLING_SHIFT) & 0b1111) as u8) {
            Some(rights) => rights,
            None => unreachable!(),
        }
    }

    /// The en-passant file, if a pawn just made a double push.
    ///
    /// A file, not a square: the rank follows from the side to move (D-0019).
    #[must_use]
    pub const fn ep_file(&self) -> Option<File> {
        let nibble = (self.state >> EN_PASSANT_SHIFT) & 0b1111;
        if nibble >= NO_EN_PASSANT {
            return None;
        }
        File::new(nibble as u8)
    }

    /// The en-passant target square, reconstructed from the file and the side to move.
    ///
    /// Rank 6 when White is to move — Black has just pushed — and rank 3 when Black is.
    #[must_use]
    pub const fn ep_square(&self) -> Option<Square> {
        let file = match self.ep_file() {
            Some(file) => file,
            None => return None,
        };
        let rank = match self.side_to_move() {
            Color::White => 5,
            Color::Black => 2,
        };
        Square::new(file, rank)
    }

    /// Plies since the last capture or pawn move.
    #[must_use]
    pub const fn halfmove_clock(&self) -> u16 {
        ((self.state >> HALFMOVE_SHIFT) & 0xFFFF) as u16
    }

    /// The move number, starting at 1 and incremented after Black moves.
    #[must_use]
    pub const fn fullmove_number(&self) -> u16 {
        ((self.state >> FULLMOVE_SHIFT) & 0xFFFF) as u16
    }

    /// The packed state word, for tests that assert the packing itself.
    #[must_use]
    pub const fn state_word(&self) -> u64 {
        self.state
    }

    /// The incrementally maintained position key.
    ///
    /// This is the value a search stack threads to detect repetition — see the type-level
    /// note about there being no undo stack.
    #[must_use]
    pub const fn key(&self) -> ZobristKey {
        self.key
    }

    /// The incrementally maintained pawn-structure key.
    #[must_use]
    pub const fn pawn_key(&self) -> PawnKey {
        self.pawn_key
    }

    /// The position key, recomputed from scratch by walking the **mailbox**.
    ///
    /// Deliberately reads a different representation from the one [`Board::to_fen`] reads
    /// and from the one [`Board::recomputed_pawn_key`] reads, so that agreement between
    /// them is evidence rather than two views of one mistake.
    #[must_use]
    pub fn recomputed_key(&self) -> ZobristKey {
        let mut key = ZobristKey::ZERO;
        for index in 0..Square::COUNT {
            let square = match Square::from_index(index as u8) {
                Some(square) => square,
                None => unreachable!("index is below 64"),
            };
            if let Some(piece) = self.mailbox[index] {
                key ^= zobrist::piece_square(piece, square);
            }
        }
        key ^= zobrist::castling(self.castling());
        key ^= zobrist::en_passant(self.ep_file());
        if self.side_to_move() == Color::Black {
            key ^= zobrist::side_to_move();
        }
        key
    }

    /// The pawn key, recomputed from scratch by folding the **pawn bitboard**.
    #[must_use]
    pub fn recomputed_pawn_key(&self) -> PawnKey {
        let mut key = PawnKey::ZERO;
        for color in Color::ALL {
            let piece = Piece::new(color, PieceKind::Pawn);
            for square in self.pieces_colored(PieceKind::Pawn, color).squares() {
                key ^= zobrist::piece_square(piece, square);
            }
        }
        key
    }

    /// Whether two boards are the same *position* — everything the zobrist key reads.
    ///
    /// Differs from `==` exactly in ignoring the halfmove clock and the fullmove number,
    /// which is the relation acceptance criterion 5 is about: two boards reached by
    /// different move orders can be the same position while their move numbers differ.
    #[must_use]
    pub fn same_position(&self, other: &Self) -> bool {
        self.pieces == other.pieces
            && self.colors == other.colors
            && self.state & POSITION_MASK == other.state & POSITION_MASK
    }

    /// Check every redundancy in the representation against the others.
    ///
    /// # Errors
    ///
    /// Returns the first [`BoardInvariant`] that does not hold.
    pub fn check_invariants(&self) -> Result<(), BoardInvariant> {
        if self.state >> 41 != 0 {
            return Err(BoardInvariant::ReservedStateBitsSet { state: self.state });
        }
        let nibble = (self.state >> EN_PASSANT_SHIFT) & 0b1111;
        if nibble > NO_EN_PASSANT {
            return Err(BoardInvariant::EnPassantNibbleInvalid { nibble });
        }
        if self.fullmove_number() == 0 {
            return Err(BoardInvariant::FullmoveNumberIsZero);
        }

        let mut kind_union = Bitboard::EMPTY;
        for (i, board) in self.pieces.iter().enumerate() {
            for (j, other) in self.pieces.iter().enumerate() {
                if i < j
                    && let Some(square) = board.intersection(*other).squares().next()
                {
                    return Err(BoardInvariant::KindsOverlap { square });
                }
            }
            kind_union = kind_union.union(*board);
        }
        if let Some(square) = self.colors[0].intersection(self.colors[1]).squares().next() {
            return Err(BoardInvariant::ColorsOverlap { square });
        }
        if kind_union != self.occupied() {
            return Err(BoardInvariant::OccupancyDisagrees);
        }

        for index in 0..Square::COUNT {
            let square = match Square::from_index(index as u8) {
                Some(square) => square,
                None => unreachable!("index is below 64"),
            };
            let from_bitboards = self.piece_from_bitboards(square);
            if self.mailbox[index] != from_bitboards {
                return Err(BoardInvariant::MailboxDisagrees {
                    square,
                    mailbox: self.mailbox[index],
                    bitboards: from_bitboards,
                });
            }
        }

        for color in Color::ALL {
            let kings = self.pieces_colored(PieceKind::King, color).count();
            if kings != 1 {
                return Err(BoardInvariant::KingCount {
                    color,
                    found: kings,
                });
            }
        }

        let recomputed = self.recomputed_key();
        if recomputed != self.key {
            return Err(BoardInvariant::KeyDisagrees {
                stored: self.key,
                recomputed,
            });
        }
        let recomputed = self.recomputed_pawn_key();
        if recomputed != self.pawn_key {
            return Err(BoardInvariant::PawnKeyDisagrees {
                stored: self.pawn_key,
                recomputed,
            });
        }
        Ok(())
    }

    /// The FEN piece-placement field, read from the **bitboards**.
    ///
    /// Deliberately not read from the mailbox: [`Board::recomputed_key`] walks the mailbox,
    /// so `from_fen(&board.to_fen()) == board` crosses both representations in one
    /// assertion rather than checking one of them twice.
    #[must_use]
    pub fn placement_field(&self) -> String {
        let mut out = String::with_capacity(72);
        for rank in (0..8u8).rev() {
            let mut empty = 0u32;
            for file in 0..8u8 {
                let file = match File::new(file) {
                    Some(file) => file,
                    None => unreachable!("index is below 8"),
                };
                let square = match Square::new(file, rank) {
                    Some(square) => square,
                    None => unreachable!("rank is below 8"),
                };
                match self.piece_from_bitboards(square) {
                    Some(piece) => {
                        if empty > 0 {
                            out.push_str(&empty.to_string());
                            empty = 0;
                        }
                        out.push(piece.to_fen_char());
                    }
                    None => empty += 1,
                }
            }
            if empty > 0 {
                out.push_str(&empty.to_string());
            }
            if rank > 0 {
                out.push('/');
            }
        }
        out
    }

    /// What the bitboards alone say stands on `square`.
    fn piece_from_bitboards(&self, square: Square) -> Option<Piece> {
        let color = if self.colors[0].contains(square) {
            Color::White
        } else if self.colors[1].contains(square) {
            Color::Black
        } else {
            return None;
        };
        for kind in PieceKind::ALL {
            if self.pieces[kind.index()].contains(square) {
                return Some(Piece::new(color, kind));
            }
        }
        None
    }
}

impl fmt::Debug for Board {
    /// Decodes the packed word into named fields.
    ///
    /// The only real cost of packing five things into one integer is an opaque number in a
    /// debugger at three in the morning, and that cost is removable for ten lines.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Board")
            .field("placement", &self.placement_field())
            .field("side_to_move", &self.side_to_move())
            .field("castling", &format_args!("{}", self.castling()))
            .field("ep_file", &self.ep_file().map(|f| f.to_char()))
            .field("halfmove_clock", &self.halfmove_clock())
            .field("fullmove_number", &self.fullmove_number())
            .field("key", &format_args!("{:x}", self.key))
            .field("pawn_key", &format_args!("{:x}", self.pawn_key))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for [`Board::check_invariants`].
    //!
    //! These live inside the crate rather than in `tests/` for a reason a review made
    //! concrete: `Board`'s fields are private and every public constructor maintains the
    //! invariants, so from an integration test there is **no way to build a corrupt board**.
    //! A reviewer replaced the whole body of `check_invariants` with `Ok(())` and the entire
    //! suite stayed green — the guard was unfalsifiable, and so was every `BoardInvariant`
    //! variant.
    //!
    //! D-0023 promises issue #5 that it can prove a corrupted board is detected by calling
    //! the public `check_invariants` rather than by a corruption hook inside `apply_move`.
    //! That promise needs these tests to be true.

    use super::*;

    /// A square by name, for the corruption sites below.
    fn sq(name: &str) -> Square {
        Square::from_uci(name).expect("a square")
    }

    fn startpos() -> Board {
        Board::from_fen(Board::STARTPOS_FEN).expect("the start position parses")
    }

    #[test]
    fn a_healthy_board_satisfies_every_invariant() {
        assert_eq!(startpos().check_invariants(), Ok(()));
    }

    #[test]
    fn a_desynced_mailbox_byte_is_detected() {
        let mut board = startpos();
        board.mailbox[sq("e2").index()] = None;
        assert!(
            matches!(
                board.check_invariants(),
                Err(BoardInvariant::MailboxDisagrees { .. })
            ),
            "got {:?}",
            board.check_invariants()
        );
    }

    #[test]
    fn a_stale_key_is_detected() {
        let mut board = startpos();
        board.key = ZobristKey::from_raw(board.key.get() ^ 1);
        assert!(matches!(
            board.check_invariants(),
            Err(BoardInvariant::KeyDisagrees { .. })
        ));
    }

    #[test]
    fn a_stale_pawn_key_is_detected() {
        let mut board = startpos();
        board.pawn_key = PawnKey::ZERO;
        assert!(matches!(
            board.check_invariants(),
            Err(BoardInvariant::PawnKeyDisagrees { .. })
        ));
    }

    #[test]
    fn a_missing_king_is_detected() {
        let mut board = startpos();
        board.remove(Piece::WhiteKing, Square::E1);
        assert_eq!(
            board.check_invariants(),
            Err(BoardInvariant::KingCount {
                color: Color::White,
                found: 0
            })
        );
    }

    #[test]
    fn overlapping_colours_are_detected() {
        let mut board = startpos();
        board.colors[Color::Black.index()] = board.colors[Color::Black.index()].with(sq("e2"));
        assert!(matches!(
            board.check_invariants(),
            Err(BoardInvariant::ColorsOverlap { .. })
        ));
    }

    #[test]
    fn overlapping_piece_kinds_are_detected() {
        let mut board = startpos();
        board.pieces[PieceKind::Rook.index()] =
            board.pieces[PieceKind::Rook.index()].with(sq("e2"));
        assert!(matches!(
            board.check_invariants(),
            Err(BoardInvariant::KindsOverlap { .. })
        ));
    }

    #[test]
    fn an_occupancy_disagreement_is_detected() {
        let mut board = startpos();
        // A square in a colour bitboard but in no piece bitboard.
        board.colors[Color::White.index()] = board.colors[Color::White.index()].with(sq("e4"));
        assert_eq!(
            board.check_invariants(),
            Err(BoardInvariant::OccupancyDisagrees)
        );
    }

    #[test]
    fn reserved_state_bits_are_detected() {
        let mut board = startpos();
        board.state |= 1 << 63;
        assert!(matches!(
            board.check_invariants(),
            Err(BoardInvariant::ReservedStateBitsSet { .. })
        ));
    }

    #[test]
    fn an_invalid_en_passant_nibble_is_detected() {
        let mut board = startpos();
        // 9..=15 are neither a file nor the "none" sentinel.
        board.state = (board.state & !(0b1111 << EN_PASSANT_SHIFT)) | (12 << EN_PASSANT_SHIFT);
        assert_eq!(
            board.check_invariants(),
            Err(BoardInvariant::EnPassantNibbleInvalid { nibble: 12 })
        );
    }

    #[test]
    fn a_zero_fullmove_number_is_detected() {
        let mut board = startpos();
        board.state &= !(0xFFFF << FULLMOVE_SHIFT);
        assert_eq!(
            board.check_invariants(),
            Err(BoardInvariant::FullmoveNumberIsZero)
        );
    }

    /// Every variant of the enum is produced by one of the tests above.
    #[test]
    fn every_board_invariant_variant_is_reachable() {
        let mut board = startpos();
        let mut seen: Vec<BoardInvariant> = Vec::new();

        board.state |= 1 << 63;
        seen.push(board.check_invariants().expect_err("reserved bits"));
        board = startpos();
        board.state = (board.state & !(0b1111 << EN_PASSANT_SHIFT)) | (12 << EN_PASSANT_SHIFT);
        seen.push(board.check_invariants().expect_err("ep nibble"));
        board = startpos();
        board.state &= !(0xFFFF << FULLMOVE_SHIFT);
        seen.push(board.check_invariants().expect_err("fullmove"));
        board = startpos();
        board.pieces[PieceKind::Rook.index()] =
            board.pieces[PieceKind::Rook.index()].with(sq("e2"));
        seen.push(board.check_invariants().expect_err("kinds overlap"));
        board = startpos();
        board.colors[Color::Black.index()] = board.colors[Color::Black.index()].with(sq("e2"));
        seen.push(board.check_invariants().expect_err("colours overlap"));
        board = startpos();
        board.colors[Color::White.index()] = board.colors[Color::White.index()].with(sq("e4"));
        seen.push(board.check_invariants().expect_err("occupancy"));
        board = startpos();
        board.mailbox[sq("e2").index()] = None;
        seen.push(board.check_invariants().expect_err("mailbox"));
        board = startpos();
        board.remove(Piece::WhiteKing, Square::E1);
        seen.push(board.check_invariants().expect_err("king count"));
        board = startpos();
        board.key = ZobristKey::from_raw(board.key.get() ^ 1);
        seen.push(board.check_invariants().expect_err("key"));
        board = startpos();
        board.pawn_key = PawnKey::ZERO;
        seen.push(board.check_invariants().expect_err("pawn key"));

        assert_eq!(
            seen.len(),
            10,
            "BoardInvariant has ten variants and each must be produced by a corrupt board"
        );
        // And every one of them renders a distinct message, so a failure names its cause.
        let mut messages: Vec<String> = seen.iter().map(ToString::to_string).collect();
        messages.sort();
        messages.dedup();
        assert_eq!(messages.len(), 10);
    }
}
