//! FEN reading and writing, and the typed error a rejection carries.
//!
//! # The accepted language
//!
//! `from_fen` accepts canonical FEN with six fields, and the four-field abbreviation the
//! published Kiwipete position is stored in (D-0008), which omits the halfmove clock and
//! the fullmove number. `to_fen` always emits six fields, so the round trip obeys one law
//! with no third case (D-0020):
//!
//! ```text
//! to_fen(from_fen(f)) == f            when f has six fields
//! to_fen(from_fen(f)) == f + " 0 1"   when f has four fields
//! ```
//!
//! # Strictness
//!
//! The parser is strict, and every rule it does *not* enforce is written down in D-0021
//! with an owner, plus a test in `tests/fen_language.rs` asserting that a FEN violating it
//! is accepted. The largest deliberate omission is that a position with the side **not** to
//! move already in check is accepted: deciding that needs attack tables, which are issue
//! #5's.
//!
//! # Why this parser is not lenient about whitespace
//!
//! Fields are split on `' '`, never with `split_whitespace`. Six of the seven published
//! FENs this project transcribed use U+00A0 NON-BREAKING SPACE as a separator, and Rust's
//! `split_whitespace` treats U+00A0 as whitespace while Stockfish's tokeniser does not —
//! so a lenient parser would accept a string that Stockfish silently reads as a *different
//! legal position* reporting zero nodes (D-0008). A non-ASCII byte is rejected outright.

use core::fmt;
use core::str::FromStr;

use crate::board::{Board, CastlingRights, Color, Square};

/// Which FEN field a rejection came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FenField {
    /// Field 1, the piece placement.
    Placement,
    /// Field 2, the side to move.
    SideToMove,
    /// Field 3, the castling availability.
    Castling,
    /// Field 4, the en-passant target square.
    EnPassant,
    /// Field 5, the halfmove clock.
    HalfmoveClock,
    /// Field 6, the fullmove number.
    FullmoveNumber,
}

impl fmt::Display for FenField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Placement => "piece placement",
            Self::SideToMove => "side to move",
            Self::Castling => "castling availability",
            Self::EnPassant => "en passant target",
            Self::HalfmoveClock => "halfmove clock",
            Self::FullmoveNumber => "fullmove number",
        })
    }
}

/// Why a FEN was rejected.
///
/// Flat, `Copy`, and scalar-payloaded: it replaces the `Result<(), String>` that
/// `perft::oracle::validate_fen` used to return, which D-0014 recorded as wrong for a
/// parser precisely because a caller could not branch on *why* a FEN was rejected
/// (D-0022).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FenError {
    /// The FEN was empty.
    Empty,
    /// A byte outside US-ASCII. Rejected rather than normalised, because U+00A0 separators
    /// are silently mis-parsed by ASCII-only tokenisers into a different legal position
    /// (D-0008).
    NonAscii {
        /// Byte offset of the first offending byte.
        offset: usize,
    },
    /// Not four or six space-separated fields.
    WrongFieldCount {
        /// How many were found.
        found: usize,
    },
    /// A field was empty — two adjacent separators, or a leading or trailing space.
    EmptyField {
        /// Which field.
        field: FenField,
    },
    /// The placement did not have eight `/`-separated ranks.
    WrongRankCount {
        /// How many were found.
        found: usize,
    },
    /// A character in the placement is neither a piece letter nor a skip digit.
    BadPieceChar {
        /// The offending character.
        found: char,
        /// The rank it appeared on, 1..=8.
        rank: u8,
    },
    /// A skip digit was `0` or `9`.
    BadSkipDigit {
        /// The offending digit.
        found: char,
        /// The rank it appeared on, 1..=8.
        rank: u8,
    },
    /// Two skip digits in a row. `44` sums to eight files but is not canonical FEN, and
    /// accepting it would break the round-trip law, which no emitter can produce.
    ConsecutiveSkipDigits {
        /// The rank, 1..=8.
        rank: u8,
    },
    /// A rank described more or fewer than eight files.
    WrongFileCount {
        /// The rank, 1..=8.
        rank: u8,
        /// How many files it described.
        found: u32,
    },
    /// A side did not have exactly one king.
    WrongKingCount {
        /// Which side.
        color: Color,
        /// How many kings it had.
        found: u32,
    },
    /// A pawn stood on rank 1 or rank 8, where no pawn can legally be.
    PawnOnBackRank {
        /// The offending square.
        square: Square,
    },
    /// The side-to-move field was not `w` or `b`.
    BadSideToMove,
    /// The castling field contained a character other than `KQkq`.
    ///
    /// Shredder and X-FEN notation (`HAha`) lands here: Chess960 is out of scope (issue
    /// #5), and naming the rejection is more useful than a generic parse failure when a
    /// wider EPD suite arrives in issue #6.
    BadCastlingChar {
        /// The offending character.
        found: char,
    },
    /// The castling field repeated a right.
    RepeatedCastlingRight {
        /// The repeated character.
        found: char,
    },
    /// The castling field was not in the canonical `KQkq` order.
    NonCanonicalCastlingOrder,
    /// A castling right was claimed without the king or rook that makes it possible.
    CastlingRightWithoutPieces {
        /// The right that has no pieces behind it.
        right: CastlingRights,
    },
    /// The en-passant field was neither `-` nor a square.
    BadEnPassantSquare,
    /// The en-passant target was on a rank the side to move cannot have produced.
    EnPassantRankContradictsSideToMove {
        /// The square given.
        square: Square,
    },
    /// The en-passant target square is not empty, or the pawn that would have double
    /// pushed is not behind it, or the square it came from is not empty.
    EnPassantNotReachable {
        /// The square given.
        square: Square,
    },
    /// A clock field was empty or had a non-digit.
    BadNumber {
        /// Which field.
        field: FenField,
    },
    /// A clock field had a leading zero, which no canonical FEN emits and which would
    /// break the round-trip law.
    LeadingZero {
        /// Which field.
        field: FenField,
    },
    /// A clock field did not fit the packed state word.
    NumberOutOfRange {
        /// Which field.
        field: FenField,
    },
    /// The fullmove number was zero. Move numbering starts at one.
    FullmoveNumberIsZero,
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the FEN is empty"),
            Self::NonAscii { offset } => write!(
                f,
                "byte {offset} is outside US-ASCII; an ASCII-only tokeniser would read this \
                 as a different position"
            ),
            Self::WrongFieldCount { found } => {
                write!(f, "expected 4 or 6 space-separated fields, found {found}")
            }
            Self::EmptyField { field } => write!(f, "the {field} field is empty"),
            Self::WrongRankCount { found } => {
                write!(f, "piece placement must have 8 ranks, found {found}")
            }
            Self::BadPieceChar { found, rank } => {
                write!(f, "rank {rank} contains invalid character {found:?}")
            }
            Self::BadSkipDigit { found, rank } => {
                write!(f, "rank {rank} contains the invalid skip digit {found:?}")
            }
            Self::ConsecutiveSkipDigits { rank } => write!(
                f,
                "rank {rank} uses consecutive skip digits, which is not canonical FEN"
            ),
            Self::WrongFileCount { rank, found } => {
                write!(f, "rank {rank} describes {found} files, not 8")
            }
            Self::WrongKingCount { color, found } => {
                write!(f, "{color:?} has {found} kings, not exactly one")
            }
            Self::PawnOnBackRank { square } => {
                write!(f, "a pawn stands on {square}, where no pawn can legally be")
            }
            Self::BadSideToMove => write!(f, "side to move must be 'w' or 'b'"),
            Self::BadCastlingChar { found } => write!(
                f,
                "castling field contains {found:?}; only KQkq is accepted, and Shredder or \
                 X-FEN notation is out of scope while Chess960 is"
            ),
            Self::RepeatedCastlingRight { found } => {
                write!(f, "castling field repeats {found:?}")
            }
            Self::NonCanonicalCastlingOrder => {
                write!(f, "castling field must be in the order KQkq")
            }
            Self::CastlingRightWithoutPieces { right } => write!(
                f,
                "castling right {right} is claimed but its king or rook is not on its home \
                 square"
            ),
            Self::BadEnPassantSquare => {
                write!(
                    f,
                    "en passant target must be '-' or a square on rank 3 or 6"
                )
            }
            Self::EnPassantRankContradictsSideToMove { square } => write!(
                f,
                "en passant target {square} contradicts the side to move: after White \
                 double pushes the target is on rank 3 and it is Black's turn"
            ),
            Self::EnPassantNotReachable { square } => write!(
                f,
                "en passant target {square} could not have arisen: the target must be \
                 empty, the double-pushed pawn must stand beyond it, and the square it came \
                 from must be empty"
            ),
            Self::BadNumber { field } => {
                write!(f, "the {field} must be a decimal number")
            }
            Self::LeadingZero { field } => {
                write!(
                    f,
                    "the {field} has a leading zero, which canonical FEN does not use"
                )
            }
            Self::NumberOutOfRange { field } => {
                write!(f, "the {field} does not fit the packed state word")
            }
            Self::FullmoveNumberIsZero => write!(f, "the fullmove number must be at least 1"),
        }
    }
}

impl core::error::Error for FenError {}

impl Board {
    /// Parse a FEN.
    ///
    /// # Errors
    ///
    /// Returns [`FenError`] for any input outside the accepted language. Never panics, for
    /// any input at all — that is asserted over generated garbage as well as over hand-
    /// written cases.
    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        let _ = fen;
        todo!("Board::from_fen")
    }

    /// Emit the canonical six-field FEN.
    #[must_use]
    pub fn to_fen(&self) -> String {
        todo!("Board::to_fen")
    }
}

impl FromStr for Board {
    type Err = FenError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_fen(s)
    }
}

impl fmt::Display for Board {
    /// The canonical six-field FEN.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_fen())
    }
}
