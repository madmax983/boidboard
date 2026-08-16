//! A packed move, and what applying one does.
//!
//! # Scope
//!
//! This module is the boundary between issue #4 and issue #5, and D-0023 states it in one
//! sentence: **move application needs no attack tables; move generation and legality need
//! nothing else.** So `Move` and [`Board::apply_move`] live here, and there is no attack
//! table, no `attackers_to`, no check detection and no move generation anywhere in this
//! crate. [`Board::try_apply_move`] checks the *structural* preconditions of a move — a
//! piece of the right colour on `from`, an empty destination for a quiet move — and says
//! nothing about whether the move leaves its own king en prise. That question is issue #5's.
//!
//! # Encoding
//!
//! ```text
//!  bits 15..12 | bits 11..6 | bits 5..0
//!     flags    |    from    |    to
//! ```
//!
//! `to` occupies the low bits because it is the field a copy-make engine reads most often,
//! and `mv.0 & 0x3F` is then a single mask.
//!
//! Castling is encoded king-from / king-to (`e1g1`), never the Chess960 rook convention.
//! Issue #5 puts Chess960 out of scope, which is what licenses that; issue #6's `divide`
//! comparison depends on it, because Stockfish prints `e1h1` instead when `UCI_Chess960` is
//! on.

use core::fmt;

use crate::board::{Board, CastlingRights, Color, File, Piece, PieceKind, Square};

/// What kind of move this is — everything the board cannot work out for itself.
///
/// Note what is **not** here: an ordinary capture is [`MoveKind::Capture`], but whether a
/// quiet-looking move happens to land on an enemy piece is read from the board rather than
/// trusted from the flag, so a mislabelled move cannot silently corrupt the position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MoveKind {
    /// A move to an empty square that is none of the below.
    Quiet,
    /// A pawn's two-square advance. Sets the en-passant file (D-0019).
    DoublePawnPush,
    /// King-side castling, `e1g1` or `e8g8`.
    KingCastle,
    /// Queen-side castling, `e1c1` or `e8c8`.
    QueenCastle,
    /// A capture of the piece standing on the destination.
    Capture,
    /// An en-passant capture. The captured pawn is **not** on the destination square.
    EnPassant,
    /// A pawn reaching the last rank on an empty square.
    Promotion(PieceKind),
    /// A pawn reaching the last rank by capturing.
    PromoCapture(PieceKind),
}

/// A move, packed into 16 bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Move(u16);

/// Why a UCI string could not be read as a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveParseError {
    /// Not four or five characters.
    BadLength,
    /// One of the two squares was not a coordinate.
    BadSquare,
    /// The fifth character was not one of `nbrq`.
    BadPromotionPiece,
    /// There is no piece on the origin square.
    NoPieceOnFrom,
    /// The piece on the origin square belongs to the other side.
    NotSideToMove,
    /// A promotion was named for a move that does not reach the last rank, or omitted for
    /// one that does.
    PromotionMismatch,
}

impl fmt::Display for MoveParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadLength => "a UCI move is four characters, or five for a promotion",
            Self::BadSquare => "one of the squares is not a coordinate",
            Self::BadPromotionPiece => "the promotion piece must be one of n, b, r, q",
            Self::NoPieceOnFrom => "there is no piece on the origin square",
            Self::NotSideToMove => "the piece on the origin square is not the side to move's",
            Self::PromotionMismatch => {
                "a promotion piece was given for a move that does not reach the last rank, \
                 or omitted for one that does"
            }
        })
    }
}

impl core::error::Error for MoveParseError {}

/// Why a move could not be applied to a position.
///
/// Structural only. None of these says anything about check — see D-0023.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveNotApplicable {
    /// No piece stands on the origin square.
    NoPieceOnFrom,
    /// The piece on the origin square is not the side to move's.
    NotSideToMove,
    /// The destination holds one of the mover's own pieces.
    OwnPieceOnDestination,
    /// The move's kind disagrees with the board: a `Capture` onto an empty square, a
    /// `Quiet` onto an occupied one, and so on.
    KindDisagreesWithBoard {
        /// The kind the move claimed.
        claimed: MoveKind,
    },
    /// An en-passant capture whose destination is not the board's en-passant square.
    NotTheEnPassantSquare,
    /// A castling move whose right is not available.
    CastlingRightAbsent,
    /// A castling move whose path between king and rook is not empty.
    CastlingPathOccupied,
    /// A promotion by a piece that is not a pawn, or onto a rank that is not the last.
    NotAPromotion,
    /// A double push that is not a pawn's, is not from the pawn's home rank, or is blocked.
    NotADoublePush,
}

impl fmt::Display for MoveNotApplicable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPieceOnFrom => write!(f, "no piece stands on the origin square"),
            Self::NotSideToMove => write!(f, "that piece is not the side to move's"),
            Self::OwnPieceOnDestination => write!(f, "the destination holds one of our own"),
            Self::KindDisagreesWithBoard { claimed } => {
                write!(f, "the board does not support a {claimed:?}")
            }
            Self::NotTheEnPassantSquare => {
                write!(
                    f,
                    "the destination is not this position's en-passant square"
                )
            }
            Self::CastlingRightAbsent => write!(f, "that castling right is not available"),
            Self::CastlingPathOccupied => write!(f, "the castling path is not empty"),
            Self::NotAPromotion => write!(f, "that move does not promote"),
            Self::NotADoublePush => write!(f, "that move is not an available double push"),
        }
    }
}

impl core::error::Error for MoveNotApplicable {}

impl Move {
    /// A move from `from` to `to` of kind `kind`, or `None` if `from == to`.
    ///
    /// Rejecting `from == to` means a `Move` can never encode a null move. Null moves are
    /// issue #5's acceptance criterion 4, and giving them their own function rather than a
    /// degenerate `Move` keeps "a move moves something" true.
    #[must_use]
    pub const fn new(from: Square, to: Square, kind: MoveKind) -> Option<Self> {
        if from.index() == to.index() {
            return None;
        }
        let flags = kind_to_flags(kind);
        Some(Self(
            (flags as u16) << 12 | (from.index() as u16) << 6 | to.index() as u16,
        ))
    }

    /// A move from its packed representation, or `None` if the bits do not encode one.
    #[must_use]
    pub const fn from_bits(bits: u16) -> Option<Self> {
        let flags = (bits >> 12) as u8;
        // 6 and 7 are the two unused flag values. Admitting them would let a raw-bits entry
        // point construct a move whose kind() no code has a case for.
        if flags == 6 || flags == 7 {
            return None;
        }
        if (bits >> 6) & 0x3F == bits & 0x3F {
            return None;
        }
        Some(Self(bits))
    }

    /// The origin square.
    #[must_use]
    pub const fn from(self) -> Square {
        match Square::from_index(((self.0 >> 6) & 0x3F) as u8) {
            Some(square) => square,
            None => unreachable!(),
        }
    }

    /// The destination square.
    #[must_use]
    pub const fn to(self) -> Square {
        match Square::from_index((self.0 & 0x3F) as u8) {
            Some(square) => square,
            None => unreachable!(),
        }
    }

    /// What kind of move this is.
    #[must_use]
    pub const fn kind(self) -> MoveKind {
        flags_to_kind((self.0 >> 12) as u8)
    }

    /// Whether this move captures — including en passant, where the captured pawn is not
    /// on the destination square.
    #[must_use]
    pub const fn is_capture(self) -> bool {
        (self.0 >> 12) & 0b0100 != 0
    }

    /// Whether this move promotes.
    #[must_use]
    pub const fn is_promotion(self) -> bool {
        (self.0 >> 12) & 0b1000 != 0
    }

    /// The packed representation. Issue #6's `divide` sorts on this.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// The UCI spelling: `e2e4`, or `a7a8q` for a promotion.
    ///
    /// The promotion letter is lowercase for both colours, which is what UCI specifies and
    /// what Stockfish prints — `b2a1q`, never `b2a1Q`.
    #[must_use]
    pub fn to_uci(self) -> String {
        let mut out = format!("{}{}", self.from(), self.to());
        if let MoveKind::Promotion(kind) | MoveKind::PromoCapture(kind) = self.kind() {
            out.push(kind.to_char());
        }
        out
    }

    /// Read a UCI move in the context of `board`, which supplies the kind.
    ///
    /// The board is needed because UCI does not distinguish a quiet move from a capture,
    /// a pawn's two-square advance from any other, an en-passant capture from a quiet
    /// diagonal step, or a king's two-square move from castling. Getting that wrong makes
    /// issue #6's `divide` comparison disagree with Stockfish on moves that are in fact
    /// identical.
    ///
    /// # Errors
    ///
    /// Returns [`MoveParseError`] if the string is malformed or names a square holding no
    /// piece of the side to move.
    pub fn from_uci(text: &str, board: &Board) -> Result<Self, MoveParseError> {
        if text.len() != 4 && text.len() != 5 {
            return Err(MoveParseError::BadLength);
        }
        let from = Square::from_uci(&text[0..2]).ok_or(MoveParseError::BadSquare)?;
        let to = Square::from_uci(&text[2..4]).ok_or(MoveParseError::BadSquare)?;
        let promotion = match text.len() {
            5 => Some(match &text[4..5] {
                "n" => PieceKind::Knight,
                "b" => PieceKind::Bishop,
                "r" => PieceKind::Rook,
                "q" => PieceKind::Queen,
                _ => return Err(MoveParseError::BadPromotionPiece),
            }),
            _ => None,
        };

        let piece = board.piece_at(from).ok_or(MoveParseError::NoPieceOnFrom)?;
        if piece.color() != board.side_to_move() {
            return Err(MoveParseError::NotSideToMove);
        }
        let captures = board.piece_at(to).is_some();
        let last_rank = match piece.color() {
            Color::White => 7,
            Color::Black => 0,
        };

        let kind = if piece.kind() == PieceKind::Pawn && to.rank() == last_rank {
            let Some(promotion) = promotion else {
                return Err(MoveParseError::PromotionMismatch);
            };
            if captures {
                MoveKind::PromoCapture(promotion)
            } else {
                MoveKind::Promotion(promotion)
            }
        } else if promotion.is_some() {
            return Err(MoveParseError::PromotionMismatch);
        } else if piece.kind() == PieceKind::King
            && from.file().index().abs_diff(to.file().index()) == 2
        {
            if to.file().index() > from.file().index() {
                MoveKind::KingCastle
            } else {
                MoveKind::QueenCastle
            }
        } else if piece.kind() == PieceKind::Pawn && from.rank().abs_diff(to.rank()) == 2 {
            MoveKind::DoublePawnPush
        } else if piece.kind() == PieceKind::Pawn
            && from.file() != to.file()
            && !captures
            && board.ep_square() == Some(to)
        {
            MoveKind::EnPassant
        } else if captures {
            MoveKind::Capture
        } else {
            MoveKind::Quiet
        };

        Self::new(from, to, kind).ok_or(MoveParseError::BadSquare)
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({:?})", self.to_uci(), self.kind())
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_uci())
    }
}

/// The flag nibble for a kind.
const fn kind_to_flags(kind: MoveKind) -> u8 {
    match kind {
        MoveKind::Quiet => 0,
        MoveKind::DoublePawnPush => 1,
        MoveKind::KingCastle => 2,
        MoveKind::QueenCastle => 3,
        MoveKind::Capture => 4,
        MoveKind::EnPassant => 5,
        MoveKind::Promotion(kind) => 8 + promotion_index(kind),
        MoveKind::PromoCapture(kind) => 12 + promotion_index(kind),
    }
}

/// The kind a flag nibble denotes.
const fn flags_to_kind(flags: u8) -> MoveKind {
    match flags {
        0 => MoveKind::Quiet,
        1 => MoveKind::DoublePawnPush,
        2 => MoveKind::KingCastle,
        3 => MoveKind::QueenCastle,
        4 => MoveKind::Capture,
        5 => MoveKind::EnPassant,
        8..=11 => MoveKind::Promotion(promotion_kind(flags - 8)),
        12..=15 => MoveKind::PromoCapture(promotion_kind(flags - 12)),
        // 6 and 7 are rejected by every constructor, so this is unreachable in practice.
        _ => MoveKind::Quiet,
    }
}

/// Promotion pieces occupy the low two bits of a promotion flag: N, B, R, Q.
const fn promotion_index(kind: PieceKind) -> u8 {
    match kind {
        PieceKind::Knight => 0,
        PieceKind::Bishop => 1,
        PieceKind::Rook => 2,
        // A pawn or a king cannot be promoted to; queen is the only remaining case and is
        // also the sane default for a nonsensical one.
        _ => 3,
    }
}

/// The inverse of [`promotion_index`].
const fn promotion_kind(index: u8) -> PieceKind {
    match index {
        0 => PieceKind::Knight,
        1 => PieceKind::Bishop,
        2 => PieceKind::Rook,
        _ => PieceKind::Queen,
    }
}

/// Which castling rights a piece leaving or arriving at each square destroys.
///
/// Indexed by square. Six entries are non-zero: the four corners, which hold the rooks,
/// and the two king home squares. Applying it to **both** `from` and `to` is what makes a
/// rook *captured* on its home square lose the right, not only a rook that moves — the
/// single commonest perft bug in the genre.
pub(crate) const RIGHTS_LOST: [u8; 64] = {
    let mut table = [0u8; 64];
    table[0] = CastlingRights::WHITE_QUEEN.bits(); // a1
    table[7] = CastlingRights::WHITE_KING.bits(); // h1
    table[4] = CastlingRights::WHITE_KING.bits() | CastlingRights::WHITE_QUEEN.bits(); // e1
    table[56] = CastlingRights::BLACK_QUEEN.bits(); // a8
    table[63] = CastlingRights::BLACK_KING.bits(); // h8
    table[60] = CastlingRights::BLACK_KING.bits() | CastlingRights::BLACK_QUEEN.bits(); // e8
    table
};

impl Board {
    /// Apply a move, returning the resulting position.
    ///
    /// Takes `self` by value and returns a new board: `Board` is `Copy`, there is no undo
    /// stack, and there is deliberately no `unmake_move`.
    ///
    /// The move must be applicable — see [`Board::try_apply_move`], which is this function
    /// with the preconditions checked. In debug builds the result's incrementally
    /// maintained keys are checked against a from-scratch recompute on every call.
    ///
    /// # Panics
    ///
    /// If no piece stands on the move's origin square, or if a capture names an empty
    /// destination. In debug builds, additionally if the incremental key update disagrees
    /// with a recompute.
    #[must_use]
    pub fn apply_move(self, mv: Move) -> Self {
        let mover = self.side_to_move();
        let piece = self
            .piece_at(mv.from())
            .expect("apply_move requires a piece on the origin square");

        let mut board = self;

        // 1. The victim leaves first, so that placing the mover on the destination is
        //    always an addition to an empty square. En passant is the case that makes this
        //    ordering matter: the victim is NOT on the destination.
        match mv.kind() {
            MoveKind::EnPassant => {
                let square = en_passant_victim_square(mv.to(), mover)
                    .expect("an en-passant destination always has a square behind it");
                let victim = Piece::new(mover.flip(), PieceKind::Pawn);
                board.toggle(victim, square);
            }
            MoveKind::Capture | MoveKind::PromoCapture(_) => {
                let victim = self
                    .piece_at(mv.to())
                    .expect("a capture requires a piece on the destination");
                board.toggle(victim, mv.to());
            }
            _ => {}
        }

        // 2. The mover leaves its origin and arrives — as a different piece, if promoting.
        board.toggle(piece, mv.from());
        match mv.kind() {
            MoveKind::Promotion(kind) | MoveKind::PromoCapture(kind) => {
                board.toggle(Piece::new(mover, kind), mv.to());
            }
            _ => board.toggle(piece, mv.to()),
        }

        // 3. Castling moves a rook too.
        if let Some((rook_from, rook_to)) = castling_rook_squares(mv.kind(), mover) {
            let rook = Piece::new(mover, PieceKind::Rook);
            board.toggle(rook, rook_from);
            board.toggle(rook, rook_to);
        }

        // 4. Rights are revoked by BOTH squares. Applying the table to `to` as well as to
        //    `from` is what makes a rook CAPTURED on its home square lose the right, not
        //    only a rook that moves — and it is what makes a promotion-capture onto a
        //    corner do the same.
        let mut rights = self.castling();
        for square in [mv.from(), mv.to()] {
            if let Some(lost) = CastlingRights::from_bits(RIGHTS_LOST[square.index()]) {
                rights = rights.without(lost);
            }
        }

        // 5. A double push sets the en-passant file, whether or not a capture is available
        //    (D-0019). Every other move clears it.
        let ep = match mv.kind() {
            MoveKind::DoublePawnPush => Some(double_push_file(mv.to())),
            _ => None,
        };

        // 6. The clocks. Castling resets neither: it is not a pawn move and not a capture.
        let halfmove = if piece.kind() == PieceKind::Pawn || mv.is_capture() {
            0
        } else {
            self.halfmove_clock().saturating_add(1)
        };
        let fullmove = match mover {
            Color::Black => self.fullmove_number().saturating_add(1),
            Color::White => self.fullmove_number(),
        };

        board.set_state(mover.flip(), rights, ep, halfmove, fullmove);

        // The recompute reads the mailbox while the incremental update wrote bitboards,
        // mailbox and keys together, so this is not the same computation twice.
        debug_assert!(
            board.check_invariants().is_ok(),
            "applying {mv} to {} produced an inconsistent board: {:?}",
            self.to_fen(),
            board.check_invariants()
        );
        board
    }

    /// Apply a move after checking its structural preconditions.
    ///
    /// "Structural" is the operative word, and D-0023 fixes its meaning: this checks that
    /// the move describes something the *position* supports — a piece of the right colour
    /// on `from`, a destination that is not our own piece, a castling right that exists, an
    /// en-passant destination that is the board's en-passant square. It does **not** check
    /// whether the move leaves the mover's own king attacked, because that needs attack
    /// tables, which are issue #5's. A move that passes here may still be illegal.
    ///
    /// # Errors
    ///
    /// Returns [`MoveNotApplicable`] describing the first precondition that fails.
    pub fn try_apply_move(self, mv: Move) -> Result<Self, MoveNotApplicable> {
        let mover = self.side_to_move();
        let piece = self
            .piece_at(mv.from())
            .ok_or(MoveNotApplicable::NoPieceOnFrom)?;
        if piece.color() != mover {
            return Err(MoveNotApplicable::NotSideToMove);
        }
        let destination = self.piece_at(mv.to());
        if destination.is_some_and(|p| p.color() == mover) {
            return Err(MoveNotApplicable::OwnPieceOnDestination);
        }

        let kind = mv.kind();
        let disagrees = Err(MoveNotApplicable::KindDisagreesWithBoard { claimed: kind });
        let last_rank = match mover {
            Color::White => 7,
            Color::Black => 0,
        };

        match kind {
            MoveKind::Quiet if destination.is_some() => return disagrees,
            MoveKind::Capture if destination.is_none() => return disagrees,
            MoveKind::EnPassant => {
                if piece.kind() != PieceKind::Pawn || destination.is_some() {
                    return disagrees;
                }
                if self.ep_square() != Some(mv.to()) {
                    return Err(MoveNotApplicable::NotTheEnPassantSquare);
                }
            }
            MoveKind::Promotion(_) | MoveKind::PromoCapture(_) => {
                if piece.kind() != PieceKind::Pawn || mv.to().rank() != last_rank {
                    return Err(MoveNotApplicable::NotAPromotion);
                }
                if matches!(kind, MoveKind::PromoCapture(_)) == destination.is_none() {
                    return disagrees;
                }
            }
            MoveKind::DoublePawnPush => {
                let home_rank = match mover {
                    Color::White => 1,
                    Color::Black => 6,
                };
                let skipped = mv
                    .from()
                    .offset_rank(if mover == Color::White { 1 } else { -1 });
                if piece.kind() != PieceKind::Pawn
                    || mv.from().rank() != home_rank
                    || mv.from().file() != mv.to().file()
                    || mv.from().rank().abs_diff(mv.to().rank()) != 2
                    || destination.is_some()
                    || skipped.is_none_or(|square| self.piece_at(square).is_some())
                {
                    return Err(MoveNotApplicable::NotADoublePush);
                }
            }
            MoveKind::KingCastle | MoveKind::QueenCastle => {
                let right = match (kind, mover) {
                    (MoveKind::KingCastle, Color::White) => CastlingRights::WHITE_KING,
                    (MoveKind::QueenCastle, Color::White) => CastlingRights::WHITE_QUEEN,
                    (MoveKind::KingCastle, Color::Black) => CastlingRights::BLACK_KING,
                    _ => CastlingRights::BLACK_QUEEN,
                };
                if !self.castling().contains(right) {
                    return Err(MoveNotApplicable::CastlingRightAbsent);
                }
                for index in castling_path(kind, mover) {
                    let square =
                        Square::from_index(*index).expect("the castling path is on the board");
                    if self.piece_at(square).is_some() {
                        return Err(MoveNotApplicable::CastlingPathOccupied);
                    }
                }
            }
            MoveKind::Quiet | MoveKind::Capture => {}
        }

        Ok(self.apply_move(mv))
    }
}

/// The square the pawn captured en passant actually stands on.
///
/// Not the destination: the capturing pawn lands *behind* its victim. Getting this wrong is
/// the classic en-passant bug, and it leaves a phantom pawn on the board that no FEN test
/// notices until a perft count is off by thousands.
#[must_use]
pub(crate) fn en_passant_victim_square(to: Square, mover: Color) -> Option<Square> {
    match mover {
        Color::White => to.offset_rank(-1),
        Color::Black => to.offset_rank(1),
    }
}

/// The file a double push to `to` leaves as the en-passant file.
#[must_use]
pub(crate) fn double_push_file(to: Square) -> File {
    to.file()
}

/// The rook's origin and destination for a castling move.
#[must_use]
pub(crate) fn castling_rook_squares(kind: MoveKind, color: Color) -> Option<(Square, Square)> {
    let (from_index, to_index) = match (kind, color) {
        (MoveKind::KingCastle, Color::White) => (7u8, 5u8),
        (MoveKind::QueenCastle, Color::White) => (0, 3),
        (MoveKind::KingCastle, Color::Black) => (63, 61),
        (MoveKind::QueenCastle, Color::Black) => (56, 59),
        _ => return None,
    };
    Some((
        Square::from_index(from_index)?,
        Square::from_index(to_index)?,
    ))
}

/// The squares that must be empty between the king and the rook.
#[must_use]
pub(crate) fn castling_path(kind: MoveKind, color: Color) -> &'static [u8] {
    match (kind, color) {
        (MoveKind::KingCastle, Color::White) => &[5, 6],
        // b1 must be empty for queen-side castling even though the king never crosses it.
        (MoveKind::QueenCastle, Color::White) => &[1, 2, 3],
        (MoveKind::KingCastle, Color::Black) => &[61, 62],
        (MoveKind::QueenCastle, Color::Black) => &[57, 58, 59],
        _ => &[],
    }
}
