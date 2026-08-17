//! FEN parsing and emission.
//!
//! # Strictness follows from byte-identical round-trip
//!
//! AC1 requires that parsing a FEN and emitting it again reproduces the input byte for byte.
//! The emitter has exactly one spelling for any position, so **any non-canonical input the
//! parser accepts is a round-trip failure by construction**. That single observation decides
//! every judgement call in this module: a spelling that could be normalised is an *error*,
//! never a normalisation. Castling rights out of `KQkq` order, a leading zero in a clock,
//! two adjacent digits in a rank, an uppercase en-passant square, a doubled separator — each
//! is rejected rather than quietly rewritten (`docs/DECISIONS.md` D-0023).
//!
//! The same rule runs the other way: **parsing never normalises the position either.** An
//! en-passant square with no pawn able to capture is kept, not cleared (D-0021); castling
//! rights are never dropped for being unexercisable. Stockfish does both of those on input,
//! and either would break the round-trip.
//!
//! # Two strictness tiers
//!
//! [`FenTier::Structural`] is everything decidable from the string. [`FenTier::BoardLegality`]
//! is everything decidable from the assembled board *without generating moves*: king counts,
//! pawns on the back ranks, castling rights whose king or rook is missing, and the
//! en-passant triple. "The side not to move is in check" is **not** here — it needs attack
//! generation, which is issue #5's — and neither are kings-adjacent or material bounds
//! (D-0025).
//!
//! # Panic-freedom
//!
//! AC2 requires that an invalid FEN returns `Err` and never panics. Two mechanisms rather
//! than a hope: the first thing [`Board::from_fen`] does is reject any non-ASCII byte, after
//! which every byte index is a character boundary and slicing cannot split a code point; and
//! this module denies the lints that would let a panic in at all.

#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    clippy::arithmetic_side_effects
)]

use core::fmt;
use core::str::FromStr;

use crate::board::Board;
use crate::types::{CastlingRight, CastlingRights, Colour, File, Piece, PieceKind, Rank, Square};

/// The longest FEN this crate can emit, in bytes.
///
/// 71 for a full board (64 pieces and 7 slashes), then one space and one side-to-move
/// character, one space and four castling characters, one space and a two-character
/// en-passant square, one space and a three-digit halfmove clock, one space and a
/// five-digit fullmove number.
pub const FEN_MAX_LEN: usize = 91;

/// How many fields a FEN was written with.
///
/// The published Kiwipete FEN has four: the wiki omits the halfmove and fullmove counters,
/// and `tests/fixtures/perft_oracle.txt` stores it as published (D-0008). Round-tripping it
/// byte-identically therefore needs the layout to survive the parse — but as a value handed
/// back by the parser, **not** as a field inside [`Board`], where it would make two
/// otherwise identical positions compare unequal and would mean nothing at all after a move
/// (D-0024).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FenLayout {
    /// Placement, side to move, castling, en passant — the counters omitted.
    FourField,
    /// All six fields.
    SixField,
}

/// Which of a FEN's six fields an error is about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FenField {
    /// Field 1: piece placement.
    Placement,
    /// Field 2: side to move.
    SideToMove,
    /// Field 3: castling availability.
    Castling,
    /// Field 4: en-passant target square.
    EnPassant,
    /// Field 5: halfmove clock.
    Halfmove,
    /// Field 6: fullmove number.
    Fullmove,
}

/// Which of the two counters an error is about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClockField {
    /// The halfmove clock, in plies since the last capture or pawn move.
    Halfmove,
    /// The fullmove number.
    Fullmove,
}

impl ClockField {
    /// The largest value this crate stores for this counter.
    #[must_use]
    pub const fn max(self) -> u32 {
        match self {
            ClockField::Halfmove => u8::MAX as u32,
            ClockField::Fullmove => u16::MAX as u32,
        }
    }
}

/// How strict a rule is: decidable from the string, or from the assembled board.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FenTier {
    /// Decidable from the FEN string alone.
    Structural,
    /// Decidable from the assembled board, without generating a single move.
    BoardLegality,
}

/// Why a FEN was rejected.
///
/// Every variant carries a **bounded** locator — a square, a character, an index — and never
/// a slice of the input. `Board::from_fen` is reachable from issue #7's UCI loop with
/// attacker-controlled input, and an error holding a megabyte of that input is a small
/// denial of service the moment anything logs it. Being payload-free in that sense is also
/// what lets this type be `Copy`, which matters because [`crate::perft::oracle::OracleError`]
/// embeds it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FenError {
    /// A byte outside US-ASCII. Checked first, before anything else looks at the string.
    ///
    /// Not pedantry: six of the seven published perft FENs separate their fields with
    /// U+00A0, and Stockfish's ASCII-only tokeniser mis-parses those into a *different legal
    /// position* rather than failing (D-0008).
    NonAscii {
        /// Byte offset of the first non-ASCII byte.
        byte_offset: usize,
    },
    /// The FEN did not have four or six space-separated fields.
    FieldCount {
        /// How many fields were found.
        found: usize,
    },
    /// A field was empty — a doubled, leading or trailing separator.
    EmptyField {
        /// The field that was empty.
        field: FenField,
    },
    /// The placement field did not have eight `/`-separated ranks.
    RankCount {
        /// How many ranks were found.
        found: usize,
    },
    /// A rank did not describe exactly eight files.
    RankWidth {
        /// The rank, 8 down to 1 in FEN order.
        rank: u8,
        /// How many files it described.
        files: u8,
    },
    /// A placement character named no piece.
    PieceChar {
        /// The rank it appeared on.
        rank: u8,
        /// The offending character.
        ch: char,
    },
    /// A placement digit was `0` or `9`.
    DigitOutOfRange {
        /// The rank it appeared on.
        rank: u8,
        /// The offending digit.
        ch: char,
    },
    /// Two adjacent digits in one rank: `44` is not canonical FEN for `8`.
    ConsecutiveDigits {
        /// The rank they appeared on.
        rank: u8,
    },
    /// The side-to-move field was not `w` or `b`.
    SideToMove {
        /// The first character found, if there was one.
        found: Option<char>,
    },
    /// The castling field held a character that is not `K`, `Q`, `k`, `q` or `-`.
    CastlingChar {
        /// The offending character.
        ch: char,
    },
    /// The castling field named the same right twice.
    CastlingDuplicate {
        /// The repeated character.
        ch: char,
    },
    /// The castling field was not in `KQkq` order.
    CastlingOrder {
        /// The character that appeared out of order.
        ch: char,
    },
    /// The castling field used Shredder-FEN / X-FEN file letters.
    ///
    /// Named rather than mapped. Stockfish silently reads `HAha` as `KQkq` in standard
    /// chess; doing the same here would emit a different string than it parsed and break
    /// AC1, and it would silently accept a Chess960 position this crate cannot represent.
    CastlingShredderNotation {
        /// The offending character.
        ch: char,
    },
    /// The en-passant field was neither `-` nor two characters long.
    EnPassantSyntax {
        /// How many characters it had.
        len: usize,
    },
    /// The en-passant field was two characters, but not a lowercase square on rank 3 or 6.
    EnPassantSquare {
        /// The file character.
        file: char,
        /// The rank character.
        rank: char,
    },
    /// The en-passant rank contradicts the side to move.
    ///
    /// After White pushes two squares the target is on rank 3 and it is Black's turn. The
    /// contradiction is decidable without a board, so it is caught here.
    EnPassantRankContradictsSideToMove {
        /// The rank character found.
        rank: char,
        /// The side to move it contradicts.
        side_to_move: Colour,
    },
    /// A counter held a character that is not an ASCII digit.
    ClockNotANumber {
        /// Which counter.
        field: ClockField,
        /// The offending character.
        ch: char,
    },
    /// A counter had a leading zero: `01` is not canonical, and `0` is written `0`.
    ClockLeadingZero {
        /// Which counter.
        field: ClockField,
    },
    /// A counter was larger than this crate stores.
    ///
    /// Rejected rather than saturated: saturation is a silent rewrite, and a silently
    /// rewritten counter cannot round-trip.
    ClockOutOfRange {
        /// Which counter.
        field: ClockField,
        /// The largest accepted value.
        max: u32,
    },
    /// The fullmove number was `0`. FEN fullmove numbers start at 1.
    FullmoveNumberZero,
    /// A side did not have exactly one king.
    KingCount {
        /// The side.
        side: Colour,
        /// How many kings it had.
        found: u8,
    },
    /// A pawn stood on the first or eighth rank, where no pawn can be.
    PawnOnBackRank {
        /// Where it stood.
        square: Square,
    },
    /// A castling right was claimed with no king on its home square.
    CastlingWithoutKing {
        /// The unsupported right.
        right: CastlingRight,
    },
    /// A castling right was claimed with no rook on its corner.
    CastlingWithoutRook {
        /// The unsupported right.
        right: CastlingRight,
    },
    /// The en-passant target square is occupied, so no pawn passed over it.
    EnPassantTargetOccupied {
        /// The target square.
        square: Square,
    },
    /// The square the double-pushing pawn started from is occupied.
    EnPassantOriginOccupied {
        /// The origin square.
        square: Square,
    },
    /// There is no pawn of the right colour on the square a double push would have reached.
    EnPassantNoDoublePushedPawn {
        /// The target square whose pusher is missing.
        square: Square,
    },
}

impl FenError {
    /// Whether this rule is about the string or about the assembled board.
    ///
    /// An exhaustive match with no wildcard arm, under this module's
    /// `deny(clippy::wildcard_enum_match_arm)`: adding a variant without classifying it is a
    /// compile error, which is a stronger guarantee than any test could give.
    #[must_use]
    pub const fn tier(self) -> FenTier {
        match self {
            FenError::NonAscii { .. }
            | FenError::FieldCount { .. }
            | FenError::EmptyField { .. }
            | FenError::RankCount { .. }
            | FenError::RankWidth { .. }
            | FenError::PieceChar { .. }
            | FenError::DigitOutOfRange { .. }
            | FenError::ConsecutiveDigits { .. }
            | FenError::SideToMove { .. }
            | FenError::CastlingChar { .. }
            | FenError::CastlingDuplicate { .. }
            | FenError::CastlingOrder { .. }
            | FenError::CastlingShredderNotation { .. }
            | FenError::EnPassantSyntax { .. }
            | FenError::EnPassantSquare { .. }
            | FenError::EnPassantRankContradictsSideToMove { .. }
            | FenError::ClockNotANumber { .. }
            | FenError::ClockLeadingZero { .. }
            | FenError::ClockOutOfRange { .. }
            | FenError::FullmoveNumberZero => FenTier::Structural,

            FenError::KingCount { .. }
            | FenError::PawnOnBackRank { .. }
            | FenError::CastlingWithoutKing { .. }
            | FenError::CastlingWithoutRook { .. }
            | FenError::EnPassantTargetOccupied { .. }
            | FenError::EnPassantOriginOccupied { .. }
            | FenError::EnPassantNoDoublePushedPawn { .. } => FenTier::BoardLegality,
        }
    }
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FenError::NonAscii { byte_offset } => write!(
                f,
                "byte {byte_offset} is not ASCII; a FEN separated by U+00A0 parses as a \
                 different legal position in an ASCII-only tokeniser"
            ),
            FenError::FieldCount { found } => {
                write!(f, "expected 4 or 6 space-separated fields, found {found}")
            }
            FenError::EmptyField { field } => {
                write!(
                    f,
                    "the {field:?} field is empty; fields are separated by exactly one space"
                )
            }
            FenError::RankCount { found } => {
                write!(f, "piece placement must have 8 ranks, found {found}")
            }
            FenError::RankWidth { rank, files } => {
                write!(f, "rank {rank} describes {files} files, not 8")
            }
            FenError::PieceChar { rank, ch } => {
                write!(f, "rank {rank} contains invalid character {ch:?}")
            }
            FenError::DigitOutOfRange { rank, ch } => write!(
                f,
                "rank {rank} contains the digit {ch:?}; empty runs are 1 to 8"
            ),
            FenError::ConsecutiveDigits { rank } => write!(
                f,
                "rank {rank} uses consecutive digits, which is not canonical FEN"
            ),
            FenError::SideToMove { found } => match found {
                Some(ch) => write!(f, "side to move must be 'w' or 'b', found {ch:?}"),
                None => write!(f, "side to move is empty"),
            },
            FenError::CastlingChar { ch } => {
                write!(f, "castling field contains invalid character {ch:?}")
            }
            FenError::CastlingDuplicate { ch } => write!(f, "castling field repeats {ch:?}"),
            FenError::CastlingOrder { ch } => write!(
                f,
                "castling field is not in KQkq order: {ch:?} appears out of order"
            ),
            FenError::CastlingShredderNotation { ch } => write!(
                f,
                "castling field uses Shredder-FEN notation ({ch:?}); this crate reads \
                 standard chess castling only, and mapping the letter would emit a \
                 different FEN than it parsed"
            ),
            FenError::EnPassantSyntax { len } => write!(
                f,
                "en passant target must be '-' or a two-character square, found {len} characters"
            ),
            FenError::EnPassantSquare { file, rank } => write!(
                f,
                "en passant target must be a lowercase square on rank 3 or 6, found {file:?}{rank:?}"
            ),
            FenError::EnPassantRankContradictsSideToMove { rank, side_to_move } => write!(
                f,
                "en passant target on rank {rank} contradicts {} to move",
                side_to_move.to_char()
            ),
            FenError::ClockNotANumber { field, ch } => {
                write!(f, "the {field:?} counter is not a number: {ch:?}")
            }
            FenError::ClockLeadingZero { field } => {
                write!(f, "the {field:?} counter has a leading zero")
            }
            FenError::ClockOutOfRange { field, max } => {
                write!(f, "the {field:?} counter exceeds the maximum of {max}")
            }
            FenError::FullmoveNumberZero => {
                write!(
                    f,
                    "the fullmove number is 0; FEN fullmove numbers start at 1"
                )
            }
            FenError::KingCount { side, found } => write!(
                f,
                "{} has {found} kings; a position has exactly one per side",
                side.to_char()
            ),
            FenError::PawnOnBackRank { square } => {
                write!(f, "a pawn stands on {square}, where no pawn can be")
            }
            FenError::CastlingWithoutKing { right } => write!(
                f,
                "castling right {:?} is claimed with no king on {}",
                right.to_char(),
                right.king_from()
            ),
            FenError::CastlingWithoutRook { right } => write!(
                f,
                "castling right {:?} is claimed with no rook on {}",
                right.to_char(),
                right.rook_from()
            ),
            FenError::EnPassantTargetOccupied { square } => write!(
                f,
                "the en passant target {square} is occupied, so no pawn passed over it"
            ),
            FenError::EnPassantOriginOccupied { square } => write!(
                f,
                "{square} is occupied, so the pawn claimed to have double-pushed could not \
                 have started there"
            ),
            FenError::EnPassantNoDoublePushedPawn { square } => write!(
                f,
                "the en passant target is {square}, but there is no pawn on the square a \
                 double push would have reached"
            ),
        }
    }
}

impl core::error::Error for FenError {}

/// The FEN fields, in order, for reporting an empty one.
const FIELD_ORDER: [FenField; 6] = [
    FenField::Placement,
    FenField::SideToMove,
    FenField::Castling,
    FenField::EnPassant,
    FenField::Halfmove,
    FenField::Fullmove,
];

impl Board {
    /// Parse a six-field FEN.
    ///
    /// For the four-field form the wiki publishes Kiwipete in, use
    /// [`Board::from_fen_with_layout`], which reports which form it read.
    ///
    /// # Errors
    ///
    /// Returns the first [`FenError`] the string or the resulting position violates.
    ///
    /// # Examples
    ///
    /// ```
    /// use boid_board::Board;
    ///
    /// let board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")?;
    /// assert_eq!(board, Board::startpos());
    /// assert_eq!(board.to_fen(), "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    /// # Ok::<(), boid_board::FenError>(())
    /// ```
    pub fn from_fen(fen: &str) -> Result<Board, FenError> {
        let (board, _) = parse(fen, false)?;
        Ok(board)
    }

    /// Parse a four- or six-field FEN, reporting which it was.
    ///
    /// Absent counters default to a halfmove clock of 0 and a fullmove number of 1.
    ///
    /// # Errors
    ///
    /// Returns the first [`FenError`] the string or the resulting position violates.
    ///
    /// # Examples
    ///
    /// The published Kiwipete FEN omits the counters, and round-trips as published:
    ///
    /// ```
    /// use boid_board::{Board, FenLayout};
    ///
    /// let published = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -";
    /// let (board, layout) = Board::from_fen_with_layout(published)?;
    ///
    /// assert_eq!(layout, FenLayout::FourField);
    /// assert_eq!(board.to_fen_with_layout(layout), published);
    /// assert_eq!(board.to_fen(), format!("{published} 0 1"));
    /// # Ok::<(), boid_board::FenError>(())
    /// ```
    pub fn from_fen_with_layout(fen: &str) -> Result<(Board, FenLayout), FenError> {
        parse(fen, true)
    }

    /// Emit this position as a six-field FEN.
    #[must_use]
    pub fn to_fen(&self) -> String {
        self.to_fen_with_layout(FenLayout::SixField)
    }

    /// Emit this position in the given layout.
    ///
    /// [`FenLayout::FourField`] omits the counters, which is lossy for any position whose
    /// clocks are not 0 and 1 — deliberately so, and not fallible: it exists to reproduce a
    /// published four-field FEN byte for byte, and a position that came from one has the
    /// defaults anyway.
    #[must_use]
    pub fn to_fen_with_layout(&self, layout: FenLayout) -> String {
        let mut fen = String::with_capacity(FEN_MAX_LEN);
        // Writing into a String cannot fail: its `fmt::Write` impl never returns Err. The
        // result is discarded rather than unwrapped because this module denies both
        // `unwrap_used` and `expect_used`, and a panic here would be a panic in the
        // emitter, which AC2 is about not having.
        let _ = self.write_fen(&mut fen, layout);
        fen
    }

    /// Write this position's FEN into an existing buffer.
    ///
    /// The primitive the other two are built on, so that issue #7's UCI loop and issue #19's
    /// PGN writer can emit into a buffer they already own.
    ///
    /// Reads the **mailbox**, while [`Board::recomputed_key`] reads the bitboards. That
    /// crossing is deliberate (D-0019): it makes every round-trip test that also checks a
    /// key into a check on the board's redundancy, for free.
    ///
    /// # Errors
    ///
    /// Propagates whatever `w` returns.
    pub fn write_fen<W: fmt::Write>(&self, w: &mut W, layout: FenLayout) -> fmt::Result {
        for (index, rank) in Rank::ALL.iter().rev().enumerate() {
            if index > 0 {
                w.write_char('/')?;
            }
            let mut empty: u32 = 0;
            for file in File::ALL {
                match self.piece_at(Square::from_file_rank(file, *rank)) {
                    Some(piece) => {
                        write_empty_run(w, empty)?;
                        empty = 0;
                        w.write_char(piece.to_char())?;
                    }
                    None => empty = empty.saturating_add(1),
                }
            }
            write_empty_run(w, empty)?;
        }

        w.write_char(' ')?;
        w.write_char(self.side_to_move().to_char())?;

        w.write_char(' ')?;
        if self.castling().is_empty() {
            w.write_char('-')?;
        } else {
            for right in CastlingRight::ALL {
                if self.castling().has(right) {
                    w.write_char(right.to_char())?;
                }
            }
        }

        w.write_char(' ')?;
        match self.en_passant_target() {
            Some(square) => write!(w, "{square}")?,
            None => w.write_char('-')?,
        }

        if layout == FenLayout::SixField {
            write!(w, " {} {}", self.halfmove_clock(), self.fullmove_number())?;
        }

        Ok(())
    }
}

/// Write a run of empty squares as a single digit, or nothing if the run is empty.
fn write_empty_run<W: fmt::Write>(w: &mut W, empty: u32) -> fmt::Result {
    if empty == 0 {
        return Ok(());
    }
    match char::from_digit(empty, 10) {
        Some(digit) => w.write_char(digit),
        // Unreachable: a run cannot exceed the eight files of a rank.
        None => Err(fmt::Error),
    }
}

/// Parse a FEN, optionally accepting the four-field form.
fn parse(fen: &str, accept_four_fields: bool) -> Result<(Board, FenLayout), FenError> {
    // FIRST, before anything else looks at the string. After this, every byte index is a
    // character boundary, so no later slice can split a code point — which is what turns
    // AC2's "never panics" from a claim about testing into a property of the code.
    if let Some(byte_offset) = fen.bytes().position(|byte| !byte.is_ascii()) {
        return Err(FenError::NonAscii { byte_offset });
    }

    // `split(' ')`, never `split_whitespace`: the latter treats U+00A0 as a separator, so an
    // NBSP-separated FEN would parse HERE into a valid position while Stockfish parses the
    // same bytes into a different one (D-0008). The check above makes that unreachable, and
    // this keeps it unreachable if the check is ever moved.
    let mut count: usize = 0;
    for (index, field) in fen.split(' ').enumerate() {
        count = count.saturating_add(1);
        if field.is_empty() {
            return Err(FenError::EmptyField {
                field: field_at(index),
            });
        }
    }

    let layout = match count {
        6 => FenLayout::SixField,
        4 if accept_four_fields => FenLayout::FourField,
        _ => return Err(FenError::FieldCount { found: count }),
    };

    let mut fields = fen.split(' ');
    let placement = fields.next().ok_or(FenError::FieldCount { found: count })?;
    let side = fields.next().ok_or(FenError::FieldCount { found: count })?;
    let castling = fields.next().ok_or(FenError::FieldCount { found: count })?;
    let en_passant = fields.next().ok_or(FenError::FieldCount { found: count })?;

    let side_to_move = parse_side_to_move(side)?;

    let mut board = Board::empty();
    board.set_side_to_move(side_to_move);
    parse_placement(&mut board, placement)?;
    board.set_castling(parse_castling(castling)?);
    board.set_en_passant(parse_en_passant(en_passant, side_to_move)?);

    if layout == FenLayout::SixField {
        let halfmove = fields.next().ok_or(FenError::FieldCount { found: count })?;
        let fullmove = fields.next().ok_or(FenError::FieldCount { found: count })?;
        board.set_halfmove_clock(parse_clock(halfmove, ClockField::Halfmove)? as u8);
        let number = parse_clock(fullmove, ClockField::Fullmove)?;
        if number == 0 {
            return Err(FenError::FullmoveNumberZero);
        }
        board.set_fullmove_number(number as u16);
    }

    check_board_legality(&board)?;
    Ok((board, layout))
}

/// Which field an index names, for reporting an empty one. An index past the sixth field can
/// only come from a trailing separator, which is reported against the last field.
fn field_at(index: usize) -> FenField {
    match FIELD_ORDER.get(index) {
        Some(field) => *field,
        None => FenField::Fullmove,
    }
}

fn parse_side_to_move(field: &str) -> Result<Colour, FenError> {
    let mut chars = field.chars();
    let first = chars.next();
    match (first, chars.next()) {
        (Some(ch), None) => Colour::from_char(ch).ok_or(FenError::SideToMove { found: Some(ch) }),
        _ => Err(FenError::SideToMove { found: first }),
    }
}

/// Fill `board`'s squares from the placement field.
///
/// Validates a rank completely before placing any of it, so a rejected FEN never leaves a
/// half-built position behind and a run that overflows the eighth file cannot index off the
/// end of a rank while being counted.
fn parse_placement(board: &mut Board, field: &str) -> Result<(), FenError> {
    let mut ranks: usize = 0;
    for _ in field.split('/') {
        ranks = ranks.saturating_add(1);
    }
    if ranks != 8 {
        return Err(FenError::RankCount { found: ranks });
    }

    for (index, rank_field) in field.split('/').enumerate() {
        // FEN writes rank 8 first. `index` counts down from there.
        let rank_number = 8u8.saturating_sub(index as u8);
        let rank = Rank::from_index(7u8.saturating_sub(index as u8))
            .ok_or(FenError::RankCount { found: ranks })?;

        let mut files: u8 = 0;
        let mut previous_was_digit = false;
        for ch in rank_field.chars() {
            if ch.is_ascii_digit() {
                // Digit validity before adjacency: '0' and '9' are never legal in a
                // placement field whatever their neighbours are, and reporting "consecutive
                // digits" for "30" would send the reader looking at the wrong rule.
                let run = match ch.to_digit(10) {
                    Some(run @ 1..=8) => run,
                    _ => {
                        return Err(FenError::DigitOutOfRange {
                            rank: rank_number,
                            ch,
                        });
                    }
                };
                if previous_was_digit {
                    // "44" sums to 8 and is not canonical FEN for "8". Accepting it would
                    // mean emitting a different string than was parsed.
                    return Err(FenError::ConsecutiveDigits { rank: rank_number });
                }
                previous_was_digit = true;
                files = files.saturating_add(run as u8);
            } else {
                previous_was_digit = false;
                if Piece::from_char(ch).is_none() {
                    return Err(FenError::PieceChar {
                        rank: rank_number,
                        ch,
                    });
                }
                files = files.saturating_add(1);
            }
        }
        if files != 8 {
            return Err(FenError::RankWidth {
                rank: rank_number,
                files,
            });
        }

        // Second pass: the rank is known to be well-formed, so every square exists.
        let mut file_index: u8 = 0;
        for ch in rank_field.chars() {
            match ch.to_digit(10) {
                Some(run) => file_index = file_index.saturating_add(run as u8),
                None => {
                    let piece = Piece::from_char(ch).ok_or(FenError::PieceChar {
                        rank: rank_number,
                        ch,
                    })?;
                    let file = File::from_index(file_index).ok_or(FenError::RankWidth {
                        rank: rank_number,
                        files: file_index,
                    })?;
                    board.place(Square::from_file_rank(file, rank), piece);
                    file_index = file_index.saturating_add(1);
                }
            }
        }
    }

    Ok(())
}

/// Parse the castling field.
///
/// The rights must appear as a strictly increasing subsequence of `KQkq`, which gets
/// duplicate detection and order checking out of one scan — and which is what makes emission
/// the exact inverse of parsing.
fn parse_castling(field: &str) -> Result<CastlingRights, FenError> {
    if field == "-" {
        return Ok(CastlingRights::NONE);
    }

    let mut rights = CastlingRights::NONE;
    let mut highest: Option<usize> = None;
    for ch in field.chars() {
        let Some(right) = CastlingRight::from_char(ch) else {
            // Shredder-FEN and X-FEN name the rook's file instead. Named rather than
            // mapped: mapping would emit a different string than was parsed, and would
            // silently accept a Chess960 position this crate cannot represent.
            if ch.is_ascii_alphabetic() && matches!(ch.to_ascii_lowercase(), 'a'..='h') {
                return Err(FenError::CastlingShredderNotation { ch });
            }
            return Err(FenError::CastlingChar { ch });
        };
        if rights.has(right) {
            return Err(FenError::CastlingDuplicate { ch });
        }
        if let Some(previous) = highest
            && right.index() <= previous
        {
            return Err(FenError::CastlingOrder { ch });
        }
        highest = Some(right.index());
        rights = rights.with(right);
    }

    Ok(rights)
}

/// Parse the en-passant field into the file it names.
///
/// The rank is checked against the side to move and then discarded: the board stores the
/// file, because the rank follows from the side to move (D-0021).
fn parse_en_passant(field: &str, side_to_move: Colour) -> Result<Option<File>, FenError> {
    if field == "-" {
        return Ok(None);
    }

    let mut chars = field.chars();
    let (Some(file_char), Some(rank_char), None) = (chars.next(), chars.next(), chars.next())
    else {
        return Err(FenError::EnPassantSyntax {
            len: field.chars().count(),
        });
    };

    let Some(file) = File::from_char(file_char) else {
        return Err(FenError::EnPassantSquare {
            file: file_char,
            rank: rank_char,
        });
    };
    if rank_char != '3' && rank_char != '6' {
        return Err(FenError::EnPassantSquare {
            file: file_char,
            rank: rank_char,
        });
    }

    // After White pushes two squares the target is on rank 3 and it is Black's turn. The
    // contradiction is decidable without a board, so it is caught here rather than below.
    let expected = match side_to_move {
        Colour::White => '6',
        Colour::Black => '3',
    };
    if rank_char != expected {
        return Err(FenError::EnPassantRankContradictsSideToMove {
            rank: rank_char,
            side_to_move,
        });
    }

    Ok(Some(file))
}

/// Parse one counter. Rejects rather than saturates: a silently clamped counter cannot
/// round-trip, and AC1 says it must.
fn parse_clock(field: &str, which: ClockField) -> Result<u32, FenError> {
    if let Some(ch) = field.chars().find(|ch| !ch.is_ascii_digit()) {
        return Err(FenError::ClockNotANumber { field: which, ch });
    }
    if field.len() > 1 && field.starts_with('0') {
        return Err(FenError::ClockLeadingZero { field: which });
    }

    let value: u32 = field.parse().map_err(|_| FenError::ClockOutOfRange {
        field: which,
        max: which.max(),
    })?;
    if value > which.max() {
        return Err(FenError::ClockOutOfRange {
            field: which,
            max: which.max(),
        });
    }
    Ok(value)
}

/// The rules that need the assembled board but not a single generated move.
///
/// Deliberately absent: "the side not to move is in check", which needs attack generation
/// and belongs to issue #5; kings-adjacent; and material bounds (D-0025).
fn check_board_legality(board: &Board) -> Result<(), FenError> {
    for side in Colour::ALL {
        let kings = (board.pieces(PieceKind::King) & board.colours(side)).count();
        if kings != 1 {
            return Err(FenError::KingCount {
                side,
                found: u8::try_from(kings).unwrap_or(u8::MAX),
            });
        }
    }

    for square in board.pieces(PieceKind::Pawn) {
        if square.rank() == Rank::R1 || square.rank() == Rank::R8 {
            return Err(FenError::PawnOnBackRank { square });
        }
    }

    for right in CastlingRight::ALL {
        if !board.castling().has(right) {
            continue;
        }
        let colour = right.colour();
        if board.piece_at(right.king_from()) != Some(Piece::new(colour, PieceKind::King)) {
            return Err(FenError::CastlingWithoutKing { right });
        }
        if board.piece_at(right.rook_from()) != Some(Piece::new(colour, PieceKind::Rook)) {
            return Err(FenError::CastlingWithoutRook { right });
        }
    }

    if let Some(file) = board.en_passant_file() {
        // The pawn that double-pushed belongs to the side NOT to move. Black to move means
        // White has just played, so the target is on rank 3, the pawn came from rank 2 and
        // now stands on rank 4.
        let (target_rank, origin_rank, pusher_rank) = match board.side_to_move() {
            Colour::Black => (Rank::R3, Rank::R2, Rank::R4),
            Colour::White => (Rank::R6, Rank::R7, Rank::R5),
        };
        let target = Square::from_file_rank(file, target_rank);
        let origin = Square::from_file_rank(file, origin_rank);
        let pusher_square = Square::from_file_rank(file, pusher_rank);
        let pusher = Piece::new(board.side_to_move().flip(), PieceKind::Pawn);

        if board.piece_at(target).is_some() {
            return Err(FenError::EnPassantTargetOccupied { square: target });
        }
        if board.piece_at(origin).is_some() {
            return Err(FenError::EnPassantOriginOccupied { square: origin });
        }
        if board.piece_at(pusher_square) != Some(pusher) {
            return Err(FenError::EnPassantNoDoublePushedPawn { square: target });
        }
    }

    Ok(())
}

impl FromStr for Board {
    type Err = FenError;

    /// Six-field FEN, as [`Board::from_fen`].
    fn from_str(fen: &str) -> Result<Board, FenError> {
        Board::from_fen(fen)
    }
}
