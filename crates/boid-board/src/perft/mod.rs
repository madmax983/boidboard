//! Perft: the oracle, and the engines that can be measured against it.
//!
//! `perft(position, depth)` counts leaf nodes in the move tree at a fixed depth. It is the
//! standard correctness oracle for a chess move generator, because the published counts
//! are exact, third-party, and unforgiving: a single illegal or missing move anywhere in
//! the tree changes the total.
//!
//! [`oracle`] holds the published counts and the parser for them. [`engine`] holds the
//! trait an engine implements to be measured, and a driver for an external Stockfish
//! process, which is issue #6's differential harness with our own engine side still
//! absent.

pub mod engine;
pub mod oracle;
