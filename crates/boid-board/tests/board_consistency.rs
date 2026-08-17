//! The representation invariant, exercised through the editing primitives.
//!
//! `Board` stores the position twice — six piece bitboards and two colour bitboards on one
//! side, a 64-entry mailbox on the other. Two representations that must agree are two things
//! that can disagree, and a primitive that updates one and forgets the other produces a
//! board that answers every question correctly until it is asked the other way.
//!
//! [`Board::consistency`] is the question asked directly. These tests ask it after every
//! primitive, and after long sequences of them.

use boid_board::board::{Board, Inconsistency};
use boid_board::types::{CastlingRights, Colour, File, Piece, PieceKind, Square};
use boid_board::zobrist::splitmix64;

#[test]
fn place_then_take_restores_every_field() {
    // Full board equality, not just the key. A `take` that clears the bitboards and leaves
    // the mailbox entry behind passes a key-only assertion — the key is maintained by a
    // different line of the same function — and leaves a phantom piece the mailbox can see.
    let start = Board::startpos();
    let mut board = start;

    board.place(Square::E4, Piece::WhiteQueen);
    assert_ne!(board, start);
    assert_eq!(board.piece_at(Square::E4), Some(Piece::WhiteQueen));
    assert!(board.pieces(PieceKind::Queen).contains(Square::E4));
    assert!(board.colours(Colour::White).contains(Square::E4));

    assert_eq!(board.take(Square::E4), Some(Piece::WhiteQueen));
    assert_eq!(board, start, "place then take is the identity");
    assert_eq!(board.consistency(), Ok(()));
}

#[test]
fn take_on_an_empty_square_changes_nothing() {
    let start = Board::startpos();
    let mut board = start;
    assert_eq!(board.take(Square::E4), None);
    assert_eq!(board, start);
}

#[test]
#[should_panic(expected = "occupied")]
fn place_on_an_occupied_square_panics() {
    // The precondition exists so that a capture has to be spelled take-then-place. A silent
    // overwrite would set the square in two piece bitboards at once and leave the captured
    // piece in the key forever — an inconsistency that survives until something reads the
    // bitboards, which in issue #6 means a wrong node count a long way from here.
    let mut board = Board::startpos();
    board.place(Square::E2, Piece::BlackQueen);
}

#[test]
fn a_copy_is_independent_of_its_original() {
    // AC7's behavioural half. `board_is_copy` in board_layout.rs is a compile-time
    // assertion and can never go red; this one can. It is also the property the search
    // depends on: a child position is a copy of its parent with a move applied, and the
    // parent must not move with it.
    let parent = Board::startpos();
    let mut child = parent;

    child.take(Square::E2);
    child.place(Square::E4, Piece::WhitePawn);
    child.set_side_to_move(Colour::Black);

    assert_eq!(parent.piece_at(Square::E2), Some(Piece::WhitePawn));
    assert_eq!(parent.piece_at(Square::E4), None);
    assert_eq!(parent.side_to_move(), Colour::White);
    assert_eq!(parent.key(), Board::startpos().key());
    assert_ne!(child.key(), parent.key());
}

#[test]
fn mailbox_agrees_with_the_bitboards_after_every_primitive() {
    // A long deterministic sequence, checked after each edit. Deterministic on purpose: a
    // failure is replayable by re-running, which is the same reason the zobrist seed is
    // hardcoded rather than drawn from entropy.
    let mut board = Board::startpos();
    let mut state = 0xC0FF_EE00_1234_5678u64;
    let mut placements = 0u32;
    let mut removals = 0u32;

    for step in 0..1500u32 {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let roll = splitmix64(state);
        let square = Square::new((roll % 64) as u8).expect("below 64");

        match (roll >> 8) % 4 {
            0 | 1 => {
                board.take(square);
                board.place(square, Piece::ALL[((roll >> 16) % 12) as usize]);
                placements += 1;
            }
            2 => {
                if board.take(square).is_some() {
                    removals += 1;
                }
            }
            _ => {
                board.set_castling(
                    CastlingRights::from_bits(((roll >> 16) % 16) as u8).expect("four bits"),
                );
                board.set_en_passant(File::from_index(((roll >> 20) % 9) as u8));
            }
        }

        // Walk the mailbox independently of `consistency`, so this test does not depend
        // entirely on the function it is checking.
        for index in 0..64u8 {
            let probe = Square::new(index).expect("below 64");
            match board.piece_at(probe) {
                Some(piece) => {
                    assert!(
                        board.pieces(piece.kind()).contains(probe),
                        "step {step}: {probe} holds {piece:?} but its kind bitboard does not"
                    );
                    assert!(
                        board.colours(piece.colour()).contains(probe),
                        "step {step}: {probe} holds {piece:?} but its colour bitboard does not"
                    );
                }
                None => assert!(
                    !board.occupied().contains(probe),
                    "step {step}: {probe} is empty in the mailbox and occupied in the bitboards"
                ),
            }
        }

        assert_eq!(board.consistency(), Ok(()), "step {step}");
    }

    assert!(placements > 100, "the sequence placed {placements} pieces");
    assert!(removals > 50, "the sequence removed {removals} pieces");
}

#[test]
fn consistency_reports_the_first_problem_rather_than_a_bare_bool() {
    // `is_consistent()` is the convenience; `consistency()` is the one a failing test wants
    // to read. Pinned so the informative half cannot quietly become a wrapper over the bool.
    let board = Board::startpos();
    assert!(board.is_consistent());
    assert_eq!(board.consistency(), Ok(()));

    // And the error type is inhabited by something a caller can match on.
    let example = Inconsistency::KeyDrifted {
        stored: 1,
        recomputed: 2,
    };
    assert!(
        example.to_string().contains("drifted"),
        "the Display impl should say what went wrong: {example}"
    );
}
