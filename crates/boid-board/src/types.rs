//! The vocabulary a board is described in: colours, piece kinds, pieces, files, ranks,
//! squares, and castling rights.
//!
//! Three encodings in this module are load-bearing elsewhere and are pinned by tests rather
//! than by comments (`docs/DECISIONS.md` D-0019, D-0020):
//!
//! 1. **Squares are LERF** — Little-Endian Rank-File: `index = rank * 8 + file`, so `a1` is
//!    0 and `h8` is 63. Every bitboard in the crate is read with this mapping.
//! 2. **`Piece`'s discriminant IS its zobrist piece index**, `colour * 6 + kind`. There is
//!    one index map in this crate, not two that can drift apart.
//! 3. **`CastlingRights` bits are in FEN `KQkq` order**, so bit 0 is White kingside. That is
//!    also the order the four zobrist castling keys are laid out in.

use crate::bitboard::Bitboard;

/// One of the two players. British spelling throughout, as D-0006's "colour-mirror" already
/// commits this repository to.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[repr(u8)]
pub enum Colour {
    /// The side that moves first.
    White = 0,
    /// The side that moves second.
    Black = 1,
}

impl Colour {
    /// Both colours, White first.
    pub const ALL: [Colour; 2] = [Colour::White, Colour::Black];

    /// The other colour.
    #[must_use]
    pub const fn flip(self) -> Colour {
        match self {
            Colour::White => Colour::Black,
            Colour::Black => Colour::White,
        }
    }

    /// The index this colour occupies in `Board`'s colour bitboards.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The FEN side-to-move character, `'w'` or `'b'`.
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            Colour::White => 'w',
            Colour::Black => 'b',
        }
    }

    /// The colour a FEN side-to-move character names, if it names one.
    #[must_use]
    pub const fn from_char(ch: char) -> Option<Colour> {
        match ch {
            'w' => Some(Colour::White),
            'b' => Some(Colour::Black),
            _ => None,
        }
    }
}

/// A kind of piece, without a colour.
///
/// The discriminants are the index into `Board`'s six piece bitboards, and the low half of
/// a [`Piece`]'s zobrist index.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[repr(u8)]
pub enum PieceKind {
    /// Pawn — the only kind the pawn hash covers (D-0022).
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
    /// Every kind, in bitboard-index order.
    pub const ALL: [PieceKind; 6] = [
        PieceKind::Pawn,
        PieceKind::Knight,
        PieceKind::Bishop,
        PieceKind::Rook,
        PieceKind::Queen,
        PieceKind::King,
    ];

    /// The index this kind occupies in `Board`'s piece bitboards.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A coloured piece.
///
/// `Piece as u8` **is** the zobrist piece index, `colour * 6 + kind` (D-0020). Keeping the
/// discriminant and the index the same means one map, one pin, and no way for the two to
/// disagree.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[repr(u8)]
pub enum Piece {
    /// White pawn, zobrist index 0.
    WhitePawn = 0,
    /// White knight, zobrist index 1.
    WhiteKnight = 1,
    /// White bishop, zobrist index 2.
    WhiteBishop = 2,
    /// White rook, zobrist index 3.
    WhiteRook = 3,
    /// White queen, zobrist index 4.
    WhiteQueen = 4,
    /// White king, zobrist index 5.
    WhiteKing = 5,
    /// Black pawn, zobrist index 6.
    BlackPawn = 6,
    /// Black knight, zobrist index 7.
    BlackKnight = 7,
    /// Black bishop, zobrist index 8.
    BlackBishop = 8,
    /// Black rook, zobrist index 9.
    BlackRook = 9,
    /// Black queen, zobrist index 10.
    BlackQueen = 10,
    /// Black king, zobrist index 11.
    BlackKing = 11,
}

impl Piece {
    /// Every piece, in zobrist index order.
    pub const ALL: [Piece; 12] = [
        Piece::WhitePawn,
        Piece::WhiteKnight,
        Piece::WhiteBishop,
        Piece::WhiteRook,
        Piece::WhiteQueen,
        Piece::WhiteKing,
        Piece::BlackPawn,
        Piece::BlackKnight,
        Piece::BlackBishop,
        Piece::BlackRook,
        Piece::BlackQueen,
        Piece::BlackKing,
    ];

    /// The piece of this colour and kind.
    #[must_use]
    pub const fn new(colour: Colour, kind: PieceKind) -> Piece {
        match (colour, kind) {
            (Colour::White, PieceKind::Pawn) => Piece::WhitePawn,
            (Colour::White, PieceKind::Knight) => Piece::WhiteKnight,
            (Colour::White, PieceKind::Bishop) => Piece::WhiteBishop,
            (Colour::White, PieceKind::Rook) => Piece::WhiteRook,
            (Colour::White, PieceKind::Queen) => Piece::WhiteQueen,
            (Colour::White, PieceKind::King) => Piece::WhiteKing,
            (Colour::Black, PieceKind::Pawn) => Piece::BlackPawn,
            (Colour::Black, PieceKind::Knight) => Piece::BlackKnight,
            (Colour::Black, PieceKind::Bishop) => Piece::BlackBishop,
            (Colour::Black, PieceKind::Rook) => Piece::BlackRook,
            (Colour::Black, PieceKind::Queen) => Piece::BlackQueen,
            (Colour::Black, PieceKind::King) => Piece::BlackKing,
        }
    }

    /// This piece's colour.
    ///
    /// An exhaustive match rather than `self as u8 / 6`: the arithmetic form would make the
    /// `colour * 6 + kind` map a fact about a division that has to stay in step with the
    /// discriminants, and this way there is nothing to keep in step.
    #[must_use]
    pub const fn colour(self) -> Colour {
        match self {
            Piece::WhitePawn
            | Piece::WhiteKnight
            | Piece::WhiteBishop
            | Piece::WhiteRook
            | Piece::WhiteQueen
            | Piece::WhiteKing => Colour::White,
            Piece::BlackPawn
            | Piece::BlackKnight
            | Piece::BlackBishop
            | Piece::BlackRook
            | Piece::BlackQueen
            | Piece::BlackKing => Colour::Black,
        }
    }

    /// This piece's kind.
    #[must_use]
    pub const fn kind(self) -> PieceKind {
        match self {
            Piece::WhitePawn | Piece::BlackPawn => PieceKind::Pawn,
            Piece::WhiteKnight | Piece::BlackKnight => PieceKind::Knight,
            Piece::WhiteBishop | Piece::BlackBishop => PieceKind::Bishop,
            Piece::WhiteRook | Piece::BlackRook => PieceKind::Rook,
            Piece::WhiteQueen | Piece::BlackQueen => PieceKind::Queen,
            Piece::WhiteKing | Piece::BlackKing => PieceKind::King,
        }
    }

    /// The zobrist piece index, `colour * 6 + kind`.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The FEN character for this piece: uppercase for White, lowercase for Black.
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            Piece::WhitePawn => 'P',
            Piece::WhiteKnight => 'N',
            Piece::WhiteBishop => 'B',
            Piece::WhiteRook => 'R',
            Piece::WhiteQueen => 'Q',
            Piece::WhiteKing => 'K',
            Piece::BlackPawn => 'p',
            Piece::BlackKnight => 'n',
            Piece::BlackBishop => 'b',
            Piece::BlackRook => 'r',
            Piece::BlackQueen => 'q',
            Piece::BlackKing => 'k',
        }
    }

    /// The piece a FEN character names, if it names one.
    #[must_use]
    pub const fn from_char(ch: char) -> Option<Piece> {
        match ch {
            'P' => Some(Piece::WhitePawn),
            'N' => Some(Piece::WhiteKnight),
            'B' => Some(Piece::WhiteBishop),
            'R' => Some(Piece::WhiteRook),
            'Q' => Some(Piece::WhiteQueen),
            'K' => Some(Piece::WhiteKing),
            'p' => Some(Piece::BlackPawn),
            'n' => Some(Piece::BlackKnight),
            'b' => Some(Piece::BlackBishop),
            'r' => Some(Piece::BlackRook),
            'q' => Some(Piece::BlackQueen),
            'k' => Some(Piece::BlackKing),
            _ => None,
        }
    }
}

/// A file, `a` through `h`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[repr(u8)]
pub enum File {
    /// The a-file.
    A = 0,
    /// The b-file.
    B = 1,
    /// The c-file.
    C = 2,
    /// The d-file.
    D = 3,
    /// The e-file.
    E = 4,
    /// The f-file.
    F = 5,
    /// The g-file.
    G = 6,
    /// The h-file.
    H = 7,
}

impl File {
    /// Every file, a through h.
    pub const ALL: [File; 8] = [
        File::A,
        File::B,
        File::C,
        File::D,
        File::E,
        File::F,
        File::G,
        File::H,
    ];

    /// The file with this index, if the index is below 8.
    #[must_use]
    pub const fn from_index(index: u8) -> Option<File> {
        match index {
            0 => Some(File::A),
            1 => Some(File::B),
            2 => Some(File::C),
            3 => Some(File::D),
            4 => Some(File::E),
            5 => Some(File::F),
            6 => Some(File::G),
            7 => Some(File::H),
            _ => None,
        }
    }

    /// This file's index, 0 for the a-file.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The lowercase letter naming this file.
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            File::A => 'a',
            File::B => 'b',
            File::C => 'c',
            File::D => 'd',
            File::E => 'e',
            File::F => 'f',
            File::G => 'g',
            File::H => 'h',
        }
    }

    /// The file a lowercase letter names, if it names one. Uppercase is deliberately not
    /// accepted: FEN squares are lowercase, and accepting `E3` would make the parser
    /// lenient in a way the byte-identical round-trip cannot survive (D-0023).
    #[must_use]
    pub const fn from_char(ch: char) -> Option<File> {
        match ch {
            'a' => Some(File::A),
            'b' => Some(File::B),
            'c' => Some(File::C),
            'd' => Some(File::D),
            'e' => Some(File::E),
            'f' => Some(File::F),
            'g' => Some(File::G),
            'h' => Some(File::H),
            _ => None,
        }
    }
}

/// A rank, 1 through 8.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[repr(u8)]
pub enum Rank {
    /// Rank 1, White's back rank.
    R1 = 0,
    /// Rank 2.
    R2 = 1,
    /// Rank 3 — where a White double push leaves its en-passant target.
    R3 = 2,
    /// Rank 4.
    R4 = 3,
    /// Rank 5.
    R5 = 4,
    /// Rank 6 — where a Black double push leaves its en-passant target.
    R6 = 5,
    /// Rank 7.
    R7 = 6,
    /// Rank 8, Black's back rank.
    R8 = 7,
}

impl Rank {
    /// Every rank, 1 through 8.
    pub const ALL: [Rank; 8] = [
        Rank::R1,
        Rank::R2,
        Rank::R3,
        Rank::R4,
        Rank::R5,
        Rank::R6,
        Rank::R7,
        Rank::R8,
    ];

    /// The rank with this index, if the index is below 8.
    #[must_use]
    pub const fn from_index(index: u8) -> Option<Rank> {
        match index {
            0 => Some(Rank::R1),
            1 => Some(Rank::R2),
            2 => Some(Rank::R3),
            3 => Some(Rank::R4),
            4 => Some(Rank::R5),
            5 => Some(Rank::R6),
            6 => Some(Rank::R7),
            7 => Some(Rank::R8),
            _ => None,
        }
    }

    /// This rank's index, 0 for rank 1.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The digit naming this rank.
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            Rank::R1 => '1',
            Rank::R2 => '2',
            Rank::R3 => '3',
            Rank::R4 => '4',
            Rank::R5 => '5',
            Rank::R6 => '6',
            Rank::R7 => '7',
            Rank::R8 => '8',
        }
    }

    /// The rank a digit names, if it names one.
    #[must_use]
    pub const fn from_char(ch: char) -> Option<Rank> {
        match ch {
            '1' => Some(Rank::R1),
            '2' => Some(Rank::R2),
            '3' => Some(Rank::R3),
            '4' => Some(Rank::R4),
            '5' => Some(Rank::R5),
            '6' => Some(Rank::R6),
            '7' => Some(Rank::R7),
            '8' => Some(Rank::R8),
            _ => None,
        }
    }
}

/// One of the 64 squares, LERF-indexed: `index = rank * 8 + file`, `a1 = 0`, `h8 = 63`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Square(u8);

impl Square {
    /// The square with this index, if it is below 64.
    #[must_use]
    pub const fn new(index: u8) -> Option<Square> {
        if index < 64 {
            Some(Square(index))
        } else {
            None
        }
    }

    /// The square at the intersection of `file` and `rank`.
    #[must_use]
    pub const fn from_file_rank(file: File, rank: Rank) -> Square {
        Square((rank as u8) * 8 + file as u8)
    }

    /// This square's LERF index.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }

    /// The file this square stands on.
    #[must_use]
    pub const fn file(self) -> File {
        match File::from_index(self.0 % 8) {
            Some(file) => file,
            // Unreachable: `self.0` is below 64 by construction, so `self.0 % 8` is below 8.
            None => File::A,
        }
    }

    /// The rank this square stands on.
    #[must_use]
    pub const fn rank(self) -> Rank {
        match Rank::from_index(self.0 / 8) {
            Some(rank) => rank,
            // Unreachable: `self.0` is below 64 by construction, so `self.0 / 8` is below 8.
            None => Rank::R1,
        }
    }

    /// A bitboard containing exactly this square.
    #[must_use]
    pub const fn bitboard(self) -> Bitboard {
        Bitboard::from_bits(1u64 << self.0)
    }
}

impl core::fmt::Display for Square {
    /// Algebraic notation, lowercase: `e4`.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}{}", self.file().to_char(), self.rank().to_char())
    }
}

macro_rules! square_constants {
    ($($name:ident = $index:expr, $text:literal;)*) => {
        impl Square {
            $(
                #[doc = concat!("The square `", $text, "`, LERF index ", stringify!($index), ".")]
                pub const $name: Square = Square($index);
            )*
        }
    };
}

square_constants! {
A1 = 0, "a1";
B1 = 1, "b1";
C1 = 2, "c1";
D1 = 3, "d1";
E1 = 4, "e1";
F1 = 5, "f1";
G1 = 6, "g1";
H1 = 7, "h1";
A2 = 8, "a2";
B2 = 9, "b2";
C2 = 10, "c2";
D2 = 11, "d2";
E2 = 12, "e2";
F2 = 13, "f2";
G2 = 14, "g2";
H2 = 15, "h2";
A3 = 16, "a3";
B3 = 17, "b3";
C3 = 18, "c3";
D3 = 19, "d3";
E3 = 20, "e3";
F3 = 21, "f3";
G3 = 22, "g3";
H3 = 23, "h3";
A4 = 24, "a4";
B4 = 25, "b4";
C4 = 26, "c4";
D4 = 27, "d4";
E4 = 28, "e4";
F4 = 29, "f4";
G4 = 30, "g4";
H4 = 31, "h4";
A5 = 32, "a5";
B5 = 33, "b5";
C5 = 34, "c5";
D5 = 35, "d5";
E5 = 36, "e5";
F5 = 37, "f5";
G5 = 38, "g5";
H5 = 39, "h5";
A6 = 40, "a6";
B6 = 41, "b6";
C6 = 42, "c6";
D6 = 43, "d6";
E6 = 44, "e6";
F6 = 45, "f6";
G6 = 46, "g6";
H6 = 47, "h6";
A7 = 48, "a7";
B7 = 49, "b7";
C7 = 50, "c7";
D7 = 51, "d7";
E7 = 52, "e7";
F7 = 53, "f7";
G7 = 54, "g7";
H7 = 55, "h7";
A8 = 56, "a8";
B8 = 57, "b8";
C8 = 58, "c8";
D8 = 59, "d8";
E8 = 60, "e8";
F8 = 61, "f8";
G8 = 62, "g8";
H8 = 63, "h8";}

/// One of the four castling rights, in FEN `KQkq` order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[repr(u8)]
pub enum CastlingRight {
    /// White may castle kingside — FEN `K`.
    WhiteKingside = 0,
    /// White may castle queenside — FEN `Q`.
    WhiteQueenside = 1,
    /// Black may castle kingside — FEN `k`.
    BlackKingside = 2,
    /// Black may castle queenside — FEN `q`.
    BlackQueenside = 3,
}

impl CastlingRight {
    /// All four rights, in FEN `KQkq` order — which is also the order of the four zobrist
    /// castling keys and the bit order of [`CastlingRights`].
    pub const ALL: [CastlingRight; 4] = [
        CastlingRight::WhiteKingside,
        CastlingRight::WhiteQueenside,
        CastlingRight::BlackKingside,
        CastlingRight::BlackQueenside,
    ];

    /// This right's index, 0 for White kingside.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The FEN character for this right.
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            CastlingRight::WhiteKingside => 'K',
            CastlingRight::WhiteQueenside => 'Q',
            CastlingRight::BlackKingside => 'k',
            CastlingRight::BlackQueenside => 'q',
        }
    }

    /// The right a FEN character names, if it names one.
    #[must_use]
    pub const fn from_char(ch: char) -> Option<CastlingRight> {
        match ch {
            'K' => Some(CastlingRight::WhiteKingside),
            'Q' => Some(CastlingRight::WhiteQueenside),
            'k' => Some(CastlingRight::BlackKingside),
            'q' => Some(CastlingRight::BlackQueenside),
            _ => None,
        }
    }

    /// The colour whose right this is.
    #[must_use]
    pub const fn colour(self) -> Colour {
        match self {
            CastlingRight::WhiteKingside | CastlingRight::WhiteQueenside => Colour::White,
            CastlingRight::BlackKingside | CastlingRight::BlackQueenside => Colour::Black,
        }
    }

    /// The square the king must stand on for this right to be well-formed.
    #[must_use]
    pub const fn king_from(self) -> Square {
        match self {
            CastlingRight::WhiteKingside | CastlingRight::WhiteQueenside => Square::E1,
            CastlingRight::BlackKingside | CastlingRight::BlackQueenside => Square::E8,
        }
    }

    /// The square the rook must stand on for this right to be well-formed.
    #[must_use]
    pub const fn rook_from(self) -> Square {
        match self {
            CastlingRight::WhiteKingside => Square::H1,
            CastlingRight::WhiteQueenside => Square::A1,
            CastlingRight::BlackKingside => Square::H8,
            CastlingRight::BlackQueenside => Square::A8,
        }
    }

    /// The square the king lands on when this right is exercised.
    #[must_use]
    pub const fn king_to(self) -> Square {
        match self {
            CastlingRight::WhiteKingside => Square::G1,
            CastlingRight::WhiteQueenside => Square::C1,
            CastlingRight::BlackKingside => Square::G8,
            CastlingRight::BlackQueenside => Square::C8,
        }
    }

    /// The square the rook lands on when this right is exercised.
    #[must_use]
    pub const fn rook_to(self) -> Square {
        match self {
            CastlingRight::WhiteKingside => Square::F1,
            CastlingRight::WhiteQueenside => Square::D1,
            CastlingRight::BlackKingside => Square::F8,
            CastlingRight::BlackQueenside => Square::D8,
        }
    }
}

/// The set of castling rights a position carries.
///
/// Bit `i` is [`CastlingRight::ALL`]`[i]`, so the mask reads as FEN `KQkq`: `K` is 1, `Q` is
/// 2, `k` is 4, `q` is 8.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Hash)]
pub struct CastlingRights(u8);

impl CastlingRights {
    /// No rights — FEN `-`.
    pub const NONE: CastlingRights = CastlingRights(0);
    /// All four rights — FEN `KQkq`.
    pub const ALL: CastlingRights = CastlingRights(0b1111);

    /// The rights this bit mask describes, if no bit above the fourth is set.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<CastlingRights> {
        if bits <= 0b1111 {
            Some(CastlingRights(bits))
        } else {
            None
        }
    }

    /// The bit mask, `K` = 1, `Q` = 2, `k` = 4, `q` = 8.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Whether this set contains `right`.
    #[must_use]
    pub const fn has(self, right: CastlingRight) -> bool {
        self.0 & (1 << (right as u8)) != 0
    }

    /// This set with `right` added.
    #[must_use]
    pub const fn with(self, right: CastlingRight) -> CastlingRights {
        CastlingRights(self.0 | (1 << (right as u8)))
    }

    /// This set with `right` removed.
    #[must_use]
    pub const fn without(self, right: CastlingRight) -> CastlingRights {
        CastlingRights(self.0 & !(1 << (right as u8)))
    }

    /// Whether this set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}
