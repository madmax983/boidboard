//! Board representation, move generation, and the perft oracle.
//!
//! This is the dependency root of the workspace: every other crate sits above it and it
//! depends on nothing. Phase 1 (issues #4, #5, #6) fills in `Position`, FEN I/O, Zobrist
//! hashing, magic bitboard attack tables, and the perft harness itself.
//!
//! What exists today is the part that must exist *before* an engine does: the external
//! oracle. See [`perft::oracle`].

pub mod perft;
