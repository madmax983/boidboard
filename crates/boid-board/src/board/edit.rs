//! Low-level representation editing.
//!
//! These seven primitives are the only way to change a [`Board`], and they are the only
//! place the zobrist key and the pawn hash are maintained incrementally. FEN parsing is
//! built on them, so the parser and the hash share exactly one code path rather than two
//! that can disagree.
//!
//! **They do not validate legality.** They will happily build a position with nine kings, a
//! pawn on the first rank, or castling rights with no rook to exercise them. That is
//! deliberate: legality needs attack generation, which is issue #5's, and a `Board` has to
//! be constructible one piece at a time before it can be judged as a whole. Issue #5's
//! `make_move` is the checked path; this is the representation's own.
//!
//! # The incremental contract
//!
//! Every setter **XORs out the value it is replacing before XORing in the value it stores**,
//! and no setter XORs unconditionally. Given that, path-independence — two different
//! sequences of edits reaching the same position producing the same key — is a theorem
//! rather than a hope: XOR is commutative and self-inverse, and the slots the primitives
//! touch are disjoint. Which is what the tests are really for: they test the XOR-*out*
//! discipline, not XOR's algebra.
//!
//! | primitive | `key` | `pawn_key` |
//! |---|---|---|
//! | [`Board::place`] | `^= piece_square[piece][square]` | same, if the piece is a pawn |
//! | [`Board::take`] | `^= piece_square[piece][square]` | same, if the piece is a pawn |
//! | [`Board::set_side_to_move`] | `^= side_to_move` if the side changed | — |
//! | [`Board::set_castling`] | `^= castling[r]` for each right that changed | — |
//! | [`Board::set_en_passant`] | `^= en_passant[old]`, then `^= en_passant[new]` | — |
//! | [`Board::set_halfmove_clock`] | not hashed | — |
//! | [`Board::set_fullmove_number`] | not hashed | — |
//!
//! The clocks being unhashed is a **requirement**, not an accident of implementation:
//! repetition detection compares the keys of positions reached at different move numbers,
//! and a key that moved with the clocks could never match.

use crate::board::Board;
use crate::types::{CastlingRights, Colour, File, Piece, Square};

impl Board {
    /// Put `piece` on `square`.
    ///
    /// # Panics
    ///
    /// Panics if `square` is occupied. A capture is [`Board::take`] then `place`, spelled
    /// out in that order, so that the captured piece is XORed out of the hash exactly once
    /// and by the same code path as every other removal.
    pub fn place(&mut self, square: Square, piece: Piece) {
        todo!("Board::place({square}, {piece:?})")
    }

    /// Lift whatever stands on `square`, and return it.
    pub fn take(&mut self, square: Square) -> Option<Piece> {
        todo!("Board::take({square})")
    }

    /// Set the side to move.
    ///
    /// A setter rather than a toggle: en-passant files and castling rights are set the same
    /// way, and a toggle is not idempotent, so a test that applies edits in two different
    /// orders could not use one.
    pub fn set_side_to_move(&mut self, colour: Colour) {
        todo!("Board::set_side_to_move({colour:?})")
    }

    /// Set the castling rights, as a whole mask.
    ///
    /// The whole mask rather than one right at a time, because issue #5's rook-capture side
    /// effect — capturing the a1 rook clears White's queenside right — is naturally a
    /// recomputation of the mask. Clearing bits one at a time is the classic depth-four
    /// perft bug.
    pub fn set_castling(&mut self, rights: CastlingRights) {
        todo!("Board::set_castling({rights:?})")
    }

    /// Set the en-passant file, or clear it.
    ///
    /// A file, not a square: the rank follows from the side to move (D-0021). Set the side
    /// to move first if you are changing both, or [`Board::en_passant_target`] will report
    /// the square for the wrong colour.
    pub fn set_en_passant(&mut self, file: Option<File>) {
        todo!("Board::set_en_passant({file:?})")
    }

    /// Set the halfmove clock, in plies since the last capture or pawn move.
    ///
    /// Not hashed.
    pub fn set_halfmove_clock(&mut self, plies: u8) {
        todo!("Board::set_halfmove_clock({plies})")
    }

    /// Set the fullmove number.
    ///
    /// Not hashed.
    pub fn set_fullmove_number(&mut self, number: u16) {
        todo!("Board::set_fullmove_number({number})")
    }
}
