//! Board representation, move generation, and the perft oracle.
//!
//! This is the dependency root of the workspace: every other crate sits above it and it
//! depends on nothing. Phase 1 fills it in: issue #4 the [`Board`] value type, FEN I/O and
//! [`zobrist`] hashing; issue #5 magic bitboard attack tables and move generation; issue
//! #6 the perft harness itself.
//!
//! The oldest part is the one that had to exist *before* an engine did: the external
//! oracle. See [`perft::oracle`].

pub mod board;
pub mod perft;
pub mod zobrist;

pub use board::{Bitboard, CastlingRights, Color, File, Piece, PieceKind, Square};
