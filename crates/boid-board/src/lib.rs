//! Board representation, move generation, and the perft oracle.
//!
//! This is the dependency root of the workspace: every other crate sits above it and it
//! depends on nothing. Phase 1 (issues #4, #5, #6) fills in `Board`, FEN I/O, Zobrist
//! hashing, magic bitboard attack tables, and the perft harness itself.
//!
//! The value type is named `Board`, as issue #4 names it. Earlier prose in this repository —
//! this module's own doc comment, and D-0010's — called it `Position`; D-0019 records that
//! the customer's word wins.
//!
//! Two things exist so far: the external oracle, which had to exist *before* an engine did
//! (see [`perft::oracle`]), and the board representation itself.

pub mod bitboard;
pub mod board;
pub mod perft;
pub mod types;
pub mod zobrist;

pub use bitboard::Bitboard;
pub use board::Board;
pub use types::{CastlingRight, CastlingRights, Colour, File, Piece, PieceKind, Rank, Square};
