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
use crate::types::{CastlingRight, CastlingRights, Colour, File, Piece, PieceKind, Square};
use crate::zobrist::ZOBRIST;

impl Board {
    /// Put `piece` on `square`.
    ///
    /// # Panics
    ///
    /// Panics if `square` is occupied. A capture is [`Board::take`] then `place`, spelled
    /// out in that order, so that the captured piece is XORed out of the hash exactly once
    /// and by the same code path as every other removal.
    pub fn place(&mut self, square: Square, piece: Piece) {
        assert!(
            self.piece_at(square).is_none(),
            "{square} is occupied by {:?}; a capture is take-then-place",
            self.piece_at(square)
        );
        self.write_square(square, Some(piece));
        self.hash_piece(square, piece);
    }

    /// Lift whatever stands on `square`, and return it.
    pub fn take(&mut self, square: Square) -> Option<Piece> {
        let piece = self.piece_at(square)?;
        self.write_square(square, None);
        // XOR is self-inverse, so removing a piece is the same operation as adding it.
        self.hash_piece(square, piece);
        Some(piece)
    }

    /// XOR `piece` on `square` into — or out of — both hashes.
    fn hash_piece(&mut self, square: Square, piece: Piece) {
        let key = ZOBRIST.piece_square(piece, square);
        self.key ^= key;
        if piece.kind() == PieceKind::Pawn {
            self.pawn_key ^= key;
        }
    }

    /// Set the side to move.
    ///
    /// A setter rather than a toggle: en-passant files and castling rights are set the same
    /// way, and a toggle is not idempotent, so a test that applies edits in two different
    /// orders could not use one.
    pub fn set_side_to_move(&mut self, colour: Colour) {
        if self.stm != colour {
            self.key ^= ZOBRIST.side_to_move();
            self.stm = colour;
        }
    }

    /// Set the castling rights, as a whole mask.
    ///
    /// The whole mask rather than one right at a time, because issue #5's rook-capture side
    /// effect — capturing the a1 rook clears White's queenside right — is naturally a
    /// recomputation of the mask. Clearing bits one at a time is the classic depth-four
    /// perft bug.
    pub fn set_castling(&mut self, rights: CastlingRights) {
        for right in CastlingRight::ALL {
            if self.castling.has(right) != rights.has(right) {
                self.key ^= ZOBRIST.castling(right);
            }
        }
        self.castling = rights;
    }

    /// Set the en-passant file, or clear it.
    ///
    /// A file, not a square: the rank follows from the side to move (D-0021). Set the side
    /// to move first if you are changing both, or [`Board::en_passant_target`] will report
    /// the square for the wrong colour.
    pub fn set_en_passant(&mut self, file: Option<File>) {
        // XOR out what is being replaced BEFORE XORing in what replaces it. The `None`
        // branch of the first half is the one engines forget, and the symptom is a key
        // that never comes back to a position it has already visited.
        if let Some(old) = self.ep {
            self.key ^= ZOBRIST.en_passant(old);
        }
        if let Some(new) = file {
            self.key ^= ZOBRIST.en_passant(new);
        }
        self.ep = file;
    }

    /// Set the halfmove clock, in plies since the last capture or pawn move.
    ///
    /// Not hashed.
    pub fn set_halfmove_clock(&mut self, plies: u8) {
        self.halfmove = plies;
    }

    /// Set the fullmove number.
    ///
    /// Not hashed.
    pub fn set_fullmove_number(&mut self, number: u16) {
        self.fullmove = number;
    }
}
