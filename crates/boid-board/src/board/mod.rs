//! The board: a `Copy` value type carrying the position and nothing else.
//!
//! # Layout
//!
//! Six piece bitboards, two colour bitboards, a redundant mailbox for piece-on-square
//! lookup, an incrementally maintained zobrist key, a pawn hash, and the state that a FEN's
//! last five fields describe. 152 bytes, `Copy`, no allocation, no interior mutability
//! (`docs/DECISIONS.md` D-0019).
//!
//! The mailbox is `[Option<Piece>; 64]` rather than the issue's `[u8; 64]`. That is a
//! declared narrowing, not a substitution: `Option<Piece>` is one byte and the array is 64,
//! so the representation is identical, while the 244 `u8` values that name no piece stop
//! existing. Issue #5's magic bitboards are the one place this workspace permits `unsafe`,
//! and an out-of-range index reaching an unchecked lookup is memory corruption that surfaces
//! as a wrong perft count rather than as a crash.
//!
//! # The redundancy, and how it is checked
//!
//! The bitboards and the mailbox say the same thing twice. That is the point — each answers
//! a different question cheaply — and it is also a hazard: a bug in one is invisible for as
//! long as every reader goes through the other. So the two are read by *different*
//! consumers on purpose. [`Board::recomputed_key`] walks the **bitboards**; FEN emission
//! walks the **mailbox**. Any test that round-trips a FEN and checks a key therefore
//! cross-checks the redundancy for free. [`Board::consistency`] states the invariant
//! directly, for the tests that would rather ask than infer.
//!
//! # What is deliberately absent
//!
//! No move generation, no attack tables, no legality beyond what a FEN can decide without
//! them — those are issue #5. No zobrist history: with a `Copy` board and no `unmake_move`
//! there is no undo stack, so repetition and fifty-move detection need a history threaded
//! through the search stack, which is issue #8's to own (D-0027).

use core::fmt;

use crate::bitboard::Bitboard;
use crate::types::{CastlingRights, Colour, File, Piece, PieceKind, Rank, Square};

/// A chess position.
///
/// `Copy`, because the search makes a child position by copying the parent and applying a
/// move to the copy. There is no `unmake_move` and no undo stack anywhere in this crate.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Board {
    pieces: [Bitboard; 6],
    colours: [Bitboard; 2],
    key: u64,
    pawn_key: u64,
    mailbox: [Option<Piece>; 64],
    fullmove: u16,
    stm: Colour,
    castling: CastlingRights,
    ep: Option<File>,
    halfmove: u8,
}

/// AC3, as a compile-time fact rather than only a test: a `Board` that outgrew the budget
/// would fail the build rather than one assertion.
const _: () = assert!(size_of::<Board>() <= 256);

/// Which of the board's redundant representations disagreed, and where.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Inconsistency {
    /// Two piece bitboards claim the same square.
    KindsOverlap {
        /// The first kind claiming it.
        a: PieceKind,
        /// The second kind claiming it.
        b: PieceKind,
        /// The square both claim.
        square: Square,
    },
    /// Both colour bitboards claim the same square.
    ColoursOverlap {
        /// The square both claim.
        square: Square,
    },
    /// The union of the colour bitboards is not the union of the piece bitboards.
    ColourUnionIsNotOccupancy {
        /// A square one union contains and the other does not.
        square: Square,
    },
    /// The mailbox and the bitboards disagree about a square.
    MailboxDisagreesWithBitboards {
        /// The square they disagree about.
        square: Square,
        /// What the mailbox says.
        mailbox: Option<Piece>,
        /// What the bitboards say.
        bitboards: Option<Piece>,
    },
    /// The incrementally maintained key is not the key this position hashes to.
    KeyDrifted {
        /// The stored key.
        stored: u64,
        /// The key a from-scratch walk produces.
        recomputed: u64,
    },
    /// The incrementally maintained pawn hash is not the one this position hashes to.
    PawnKeyDrifted {
        /// The stored pawn hash.
        stored: u64,
        /// The pawn hash a from-scratch walk produces.
        recomputed: u64,
    },
}

impl fmt::Display for Inconsistency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KindsOverlap { a, b, square } => {
                write!(f, "{square}: claimed by both {a:?} and {b:?}")
            }
            Self::ColoursOverlap { square } => {
                write!(f, "{square}: claimed by both colours")
            }
            Self::ColourUnionIsNotOccupancy { square } => write!(
                f,
                "{square}: the colour bitboards and the piece bitboards disagree about \
                 whether this square is occupied"
            ),
            Self::MailboxDisagreesWithBitboards {
                square,
                mailbox,
                bitboards,
            } => write!(
                f,
                "{square}: mailbox says {mailbox:?}, bitboards say {bitboards:?}"
            ),
            Self::KeyDrifted { stored, recomputed } => write!(
                f,
                "zobrist key drifted: stored {stored:#018x}, recomputed {recomputed:#018x}"
            ),
            Self::PawnKeyDrifted { stored, recomputed } => write!(
                f,
                "pawn key drifted: stored {stored:#018x}, recomputed {recomputed:#018x}"
            ),
        }
    }
}

impl core::error::Error for Inconsistency {}

impl Board {
    /// An empty board: no pieces, White to move, no castling rights, no en-passant file,
    /// halfmove clock 0, fullmove number 1.
    ///
    /// Its key is 0, because a zobrist key is the XOR of the contributions a position makes
    /// and this position makes none.
    #[must_use]
    pub fn empty() -> Board {
        todo!("Board::empty")
    }

    /// The standard starting position.
    #[must_use]
    pub fn startpos() -> Board {
        todo!("Board::startpos")
    }

    /// The squares occupied by pieces of this kind, either colour.
    #[must_use]
    pub const fn pieces(&self, kind: PieceKind) -> Bitboard {
        self.pieces[kind.index()]
    }

    /// The squares occupied by this colour's pieces.
    #[must_use]
    pub const fn colours(&self, colour: Colour) -> Bitboard {
        self.colours[colour.index()]
    }

    /// Every occupied square.
    #[must_use]
    pub fn occupied(&self) -> Bitboard {
        self.colours[Colour::White.index()] | self.colours[Colour::Black.index()]
    }

    /// The piece standing on `square`, if any.
    ///
    /// The only accessor onto the mailbox, deliberately: it hands out a `Piece`, never the
    /// byte behind it.
    #[must_use]
    pub const fn piece_at(&self, square: Square) -> Option<Piece> {
        self.mailbox[square.index() as usize]
    }

    /// Where this colour's king stands, if it has one.
    ///
    /// `Option`, not a `Square`: the editing primitives can build a kingless board, and a
    /// FEN that describes one is rejected by the parser rather than by this accessor.
    #[must_use]
    pub fn king_square(&self, colour: Colour) -> Option<Square> {
        todo!("Board::king_square({colour:?})")
    }

    /// The side to move.
    #[must_use]
    pub const fn side_to_move(&self) -> Colour {
        self.stm
    }

    /// The castling rights still available.
    #[must_use]
    pub const fn castling(&self) -> CastlingRights {
        self.castling
    }

    /// The file of the en-passant target square, if the position records one.
    ///
    /// A file rather than a square: the rank follows from the side to move, so storing the
    /// file makes a rank that contradicts the side to move unconstructible (D-0021).
    #[must_use]
    pub const fn en_passant_file(&self) -> Option<File> {
        self.ep
    }

    /// The en-passant **target** square — the square a capturing pawn would move *to*, not
    /// the square the pawn that double-pushed stands on.
    ///
    /// The rank follows from the side to move: Black to move means White has just pushed
    /// two squares, so the target is on rank 3.
    #[must_use]
    pub fn en_passant_target(&self) -> Option<Square> {
        self.ep.map(|file| {
            let rank = match self.stm {
                Colour::White => Rank::R6,
                Colour::Black => Rank::R3,
            };
            Square::from_file_rank(file, rank)
        })
    }

    /// Plies since the last capture or pawn move.
    ///
    /// Plies, not moves. The fifty-move rule counts to 100 of these, and the threshold
    /// belongs to the search (D-0027), not here.
    #[must_use]
    pub const fn halfmove_clock(&self) -> u8 {
        self.halfmove
    }

    /// The fullmove number, which starts at 1 and increments after each Black move.
    #[must_use]
    pub const fn fullmove_number(&self) -> u16 {
        self.fullmove
    }

    /// The incrementally maintained zobrist key.
    #[must_use]
    pub const fn key(&self) -> u64 {
        self.key
    }

    /// The incrementally maintained pawn hash: the pawns of both colours and nothing else
    /// (D-0022).
    #[must_use]
    pub const fn pawn_key(&self) -> u64 {
        self.pawn_key
    }

    /// The zobrist key this position hashes to, computed from scratch.
    ///
    /// Walks the **bitboards**, while FEN emission walks the mailbox. That crossing is
    /// deliberate: it is what makes a round-trip test also a redundancy test (D-0019).
    #[must_use]
    pub fn recomputed_key(&self) -> u64 {
        todo!("Board::recomputed_key")
    }

    /// The pawn hash this position hashes to, computed from scratch.
    #[must_use]
    pub fn recomputed_pawn_key(&self) -> u64 {
        todo!("Board::recomputed_pawn_key")
    }

    /// Check the representation invariant: the bitboards agree with each other, the mailbox
    /// agrees with the bitboards, and both hashes agree with a from-scratch computation.
    ///
    /// Representation only — never legality. `Board::empty()` satisfies this, and so does
    /// every intermediate state inside the FEN parser's placement loop, which is what lets
    /// the editing primitives be checked against it.
    ///
    /// # Errors
    ///
    /// Returns the first [`Inconsistency`] found.
    pub fn consistency(&self) -> Result<(), Inconsistency> {
        todo!("Board::consistency")
    }

    /// Whether the representation invariant holds.
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.consistency().is_ok()
    }
}

impl fmt::Debug for Board {
    /// The placement, the state and both hashes — never the 64 raw mailbox entries.
    ///
    /// Hand-written because assertion messages are evidence in this repository: a derived
    /// `Debug` makes `assert_eq!` between two boards a wall of `None,` and unreadable.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Board {{ placement: \"")?;
        for rank in Rank::ALL.iter().rev() {
            let mut empty = 0u8;
            for file in File::ALL {
                match self.piece_at(Square::from_file_rank(file, *rank)) {
                    Some(piece) => {
                        if empty > 0 {
                            write!(f, "{empty}")?;
                            empty = 0;
                        }
                        write!(f, "{}", piece.to_char())?;
                    }
                    None => empty += 1,
                }
            }
            if empty > 0 {
                write!(f, "{empty}")?;
            }
            if *rank != Rank::R1 {
                write!(f, "/")?;
            }
        }
        write!(
            f,
            "\", stm: {:?}, castling: {:#06b}, ep: {:?}, clocks: {}/{}, \
             key: {:#018x}, pawn_key: {:#018x} }}",
            self.stm,
            self.castling.bits(),
            self.ep,
            self.halfmove,
            self.fullmove,
            self.key,
            self.pawn_key
        )
    }
}
