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

use crate::board::{
    Board, CastlingRights, Color, File, MAX_FULLMOVE_NUMBER, MAX_HALFMOVE_CLOCK, Piece, PieceKind,
    Square,
};

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
        if fen.is_empty() {
            return Err(FenError::Empty);
        }
        // Before anything else: a non-ASCII byte is rejected rather than normalised, so a
        // U+00A0-separated FEN cannot reach the field split and be silently accepted
        // (D-0008). `bytes().position` gives the offset in bytes, which is what a reader
        // staring at a hex dump wants.
        if let Some(offset) = fen.bytes().position(|b| !b.is_ascii()) {
            return Err(FenError::NonAscii { offset });
        }

        // split(' '), never split_whitespace: the latter would silently re-admit the
        // separators the check above exists to reject, and would also swallow the empty
        // fields that a doubled or leading space produces.
        // Counted before collecting. `collect()` on a megabyte of spaces allocates a Vec
        // with a million entries before anyone looks at its length, and an allocation
        // failure aborts the process rather than unwinding -- which no caller can contain.
        let found = fen.split(' ').count();
        let six = match found {
            4 => false,
            6 => true,
            _ => return Err(FenError::WrongFieldCount { found }),
        };
        let fields: Vec<&str> = fen.split(' ').collect();

        for (field, name) in [
            (fields[0], FenField::Placement),
            (fields[1], FenField::SideToMove),
            (fields[2], FenField::Castling),
            (fields[3], FenField::EnPassant),
        ] {
            if field.is_empty() {
                return Err(FenError::EmptyField { field: name });
            }
        }

        let mut board = Self::blank();
        parse_placement(&mut board, fields[0])?;

        let side = match fields[1] {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err(FenError::BadSideToMove),
        };

        let rights = parse_castling(&board, fields[2])?;
        let ep = parse_en_passant(&board, fields[3], side)?;

        let (halfmove, fullmove) = if six {
            for (field, name) in [
                (fields[4], FenField::HalfmoveClock),
                (fields[5], FenField::FullmoveNumber),
            ] {
                if field.is_empty() {
                    return Err(FenError::EmptyField { field: name });
                }
            }
            let halfmove = parse_number(fields[4], FenField::HalfmoveClock, MAX_HALFMOVE_CLOCK)?;
            let fullmove = parse_number(fields[5], FenField::FullmoveNumber, MAX_FULLMOVE_NUMBER)?;
            if fullmove == 0 {
                return Err(FenError::FullmoveNumberIsZero);
            }
            (halfmove, fullmove)
        } else {
            // The four-field abbreviation D-0008 stores Kiwipete in. The defaults are the
            // values Stockfish itself supplies when fed the same four fields, so the
            // expansion is corroborated rather than invented (D-0020).
            (0, 1)
        };

        board.set_state(side, rights, ep, halfmove, fullmove);
        Ok(board)
    }

    /// Emit the canonical six-field FEN.
    ///
    /// Always six fields, even for a board parsed from the four-field form: `Board` keeps
    /// no memory of its source text, because such a field would be compared by `PartialEq`
    /// and copied on every `apply_move` (D-0020).
    #[must_use]
    pub fn to_fen(&self) -> String {
        let mut out = String::with_capacity(MAX_FEN_LEN);
        self.write_fen(&mut out)
            .expect("writing to a String cannot fail");
        out
    }

    /// Write the canonical six-field FEN into any [`fmt::Write`].
    ///
    /// # Errors
    ///
    /// Only whatever `w` returns.
    pub fn write_fen(&self, w: &mut impl fmt::Write) -> fmt::Result {
        // The placement is read from the BITBOARDS while `recomputed_key` walks the
        // mailbox, so `from_fen(&b.to_fen()) == b` crosses both representations.
        write!(w, "{}", self.placement_field())?;
        write!(w, " {}", self.side_to_move().to_fen_char())?;
        write!(w, " {}", self.castling())?;
        match self.ep_square() {
            Some(square) => write!(w, " {square}")?,
            None => w.write_str(" -")?,
        }
        write!(w, " {} {}", self.halfmove_clock(), self.fullmove_number())
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

/// The longest FEN this crate can emit.
///
/// 71 placement bytes at most (64 piece letters plus 7 separators), one side letter, four
/// castling letters, two en-passant letters, five halfmove digits, five fullmove digits and
/// five spaces: 94. Used only to size the emitter's buffer, so an over-estimate costs
/// nothing and an under-estimate costs one reallocation.
pub const MAX_FEN_LEN: usize = 94;

/// Fill `board`'s pieces from a FEN placement field.
fn parse_placement(board: &mut Board, placement: &str) -> Result<(), FenError> {
    let ranks: Vec<&str> = placement.split('/').collect();
    if ranks.len() != 8 {
        return Err(FenError::WrongRankCount { found: ranks.len() });
    }

    for (index, text) in ranks.iter().enumerate() {
        // FEN writes rank 8 first, so the zero-based rank counts down.
        let rank = 7 - index as u8;
        let label = rank + 1;
        let mut file = 0u32;
        let mut previous_was_digit = false;

        for ch in text.chars() {
            if ch.is_ascii_digit() {
                if !('1'..='8').contains(&ch) {
                    return Err(FenError::BadSkipDigit {
                        found: ch,
                        rank: label,
                    });
                }
                if previous_was_digit {
                    return Err(FenError::ConsecutiveSkipDigits { rank: label });
                }
                previous_was_digit = true;
                file += ch as u32 - '0' as u32;
                continue;
            }
            previous_was_digit = false;

            let piece = Piece::from_fen_char(ch).ok_or(FenError::BadPieceChar {
                found: ch,
                rank: label,
            })?;
            // Placing beyond the eighth file is caught by the file-count check below, but
            // the square has to exist before then, so bail here rather than index badly.
            let Some(file_index) = File::new(file as u8) else {
                return Err(FenError::WrongFileCount {
                    rank: label,
                    found: file + 1,
                });
            };
            let Some(square) = Square::new(file_index, rank) else {
                return Err(FenError::WrongFileCount {
                    rank: label,
                    found: file + 1,
                });
            };
            if piece.kind() == PieceKind::Pawn && (rank == 0 || rank == 7) {
                return Err(FenError::PawnOnBackRank { square });
            }
            board.place(piece, square);
            file += 1;
        }

        if file != 8 {
            return Err(FenError::WrongFileCount {
                rank: label,
                found: file,
            });
        }
    }

    for color in Color::ALL {
        let kings = board.pieces_colored(PieceKind::King, color).count();
        if kings != 1 {
            return Err(FenError::WrongKingCount {
                color,
                found: kings,
            });
        }
    }
    Ok(())
}

/// Parse the castling field, checking each claimed right against the pieces on the board.
fn parse_castling(board: &Board, field: &str) -> Result<CastlingRights, FenError> {
    if field == "-" {
        return Ok(CastlingRights::NONE);
    }

    let mut rights = CastlingRights::NONE;
    let mut last_rank = 0usize;
    for ch in field.chars() {
        let (right, order) = match ch {
            'K' => (CastlingRights::WHITE_KING, 1),
            'Q' => (CastlingRights::WHITE_QUEEN, 2),
            'k' => (CastlingRights::BLACK_KING, 3),
            'q' => (CastlingRights::BLACK_QUEEN, 4),
            _ => return Err(FenError::BadCastlingChar { found: ch }),
        };
        if rights.contains(right) {
            return Err(FenError::RepeatedCastlingRight { found: ch });
        }
        if order < last_rank {
            return Err(FenError::NonCanonicalCastlingOrder);
        }
        last_rank = order;
        rights = rights.with(right);
    }

    // A right with no rook behind it is the commonest transcription damage there is, and
    // it silently changes move generation in issue #5. Standard chess only: Chess960's
    // castling is out of scope, which is what licenses assuming these home squares.
    for right in CastlingRights::EACH {
        if !rights.contains(right) {
            continue;
        }
        let (king_square, rook_square, color) = castling_home_squares(right);
        let king = Some(Piece::new(color, PieceKind::King));
        let rook = Some(Piece::new(color, PieceKind::Rook));
        if board.piece_at(king_square) != king || board.piece_at(rook_square) != rook {
            return Err(FenError::CastlingRightWithoutPieces { right });
        }
    }
    Ok(rights)
}

/// Parse the en-passant field into the file the board stores (D-0019).
fn parse_en_passant(board: &Board, field: &str, side: Color) -> Result<Option<File>, FenError> {
    if field == "-" {
        return Ok(None);
    }
    let square = Square::from_uci(field).ok_or(FenError::BadEnPassantSquare)?;
    if square.rank() != 2 && square.rank() != 5 {
        return Err(FenError::BadEnPassantSquare);
    }

    // The rank follows from the side to move: after White pushes two squares the target is
    // on rank 3 and it is Black's turn. Decidable without looking at a piece.
    let expected_rank = match side {
        Color::White => 5,
        Color::Black => 2,
    };
    if square.rank() != expected_rank {
        return Err(FenError::EnPassantRankContradictsSideToMove { square });
    }

    // And the double push must actually have been available: the target empty, the pawn
    // that made it standing beyond the target, and the square it left empty.
    let mover = side.flip();
    // Which way the pusher was travelling. White pushed e2-e4 and left the pawn one rank
    // ABOVE the e3 target; Black pushed e7-e5 and left it one rank BELOW the e6 target.
    let (to_pawn, to_origin) = match mover {
        Color::White => (1i8, -1i8),
        Color::Black => (-1i8, 1i8),
    };
    let pawn_square = square
        .offset_rank(to_pawn)
        .ok_or(FenError::EnPassantNotReachable { square })?;
    let origin = square
        .offset_rank(to_origin)
        .ok_or(FenError::EnPassantNotReachable { square })?;
    let pawn = Some(Piece::new(mover, PieceKind::Pawn));
    if board.piece_at(square).is_some()
        || board.piece_at(origin).is_some()
        || board.piece_at(pawn_square) != pawn
    {
        return Err(FenError::EnPassantNotReachable { square });
    }

    Ok(Some(square.file()))
}

/// Parse one of the two numeric fields.
fn parse_number(field: &str, name: FenField, max: u16) -> Result<u16, FenError> {
    if field.is_empty() || !field.bytes().all(|b| b.is_ascii_digit()) {
        return Err(FenError::BadNumber { field: name });
    }
    // Canonical FEN has no leading zeros, and accepting them would break the round-trip
    // law: "01" parses to 1 and emits as "1".
    if field.len() > 1 && field.starts_with('0') {
        return Err(FenError::LeadingZero { field: name });
    }
    field
        .parse::<u32>()
        .ok()
        .filter(|value| *value <= u32::from(max))
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(FenError::NumberOutOfRange { field: name })
}

/// The king's and rook's home squares for a castling right, and whose right it is.
fn castling_home_squares(right: CastlingRights) -> (Square, Square, Color) {
    if right == CastlingRights::WHITE_KING {
        (Square::E1, Square::H1, Color::White)
    } else if right == CastlingRights::WHITE_QUEEN {
        (Square::E1, Square::A1, Color::White)
    } else if right == CastlingRights::BLACK_KING {
        (Square::E8, Square::H8, Color::Black)
    } else {
        (Square::E8, Square::A8, Color::Black)
    }
}
