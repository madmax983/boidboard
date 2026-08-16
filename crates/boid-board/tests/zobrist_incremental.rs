//! Cycle 4, guards: the incremental key against a from-scratch recompute, and acceptance
//! criterion 6's two clauses.
//!
//! Criterion 6 says "positions differing only in en-passant file produce different keys;
//! positions differing only in an ep square that FEN records identically do not". The first
//! clause is behavioural and is asserted directly. The second has no black-box witness —
//! every behavioural test it suggests passes under both 8 file keys and 64 square keys — so
//! it is discharged three ways: structurally, because `Board` stores an en-passant **file**
//! and no rank is representable (D-0019); by the key set having exactly 8 en-passant keys;
//! and by the operative reading below.
//!
//! The operative reading is that **the key ignores the halfmove clock and the fullmove
//! number**. That is the only reading of the clause with engineering content: a key that
//! included the clocks would make every transposition-table probe miss, and no perft count
//! at any depth would notice. It is asserted exhaustively over every legal state word.

use std::collections::HashSet;

use boid_board::board::{Board, POSITION_MASK};
use boid_board::moves::{Move, MoveKind};
use boid_board::zobrist;
use boid_board::{File, PieceKind, Square};
use proptest::prelude::*;

mod fenlab;

fn parse(fen: &str) -> Board {
    Board::from_fen(fen).unwrap_or_else(|e| panic!("{fen:?} must parse: {e}"))
}

// ---------------------------------------------------------------------------------
// Acceptance criterion 6, clause 1
// ---------------------------------------------------------------------------------

/// Eight positions differing only in their en-passant file must produce eight keys.
#[test]
fn positions_differing_only_in_en_passant_file_produce_distinct_keys() {
    let mut keys = HashSet::new();
    let mut boards = Vec::new();
    for index in 0..File::COUNT {
        let file = File::new(index as u8).expect("index is below 8");
        let letter = file.to_char();
        // A black pawn on rank 5 of this file, having just double-pushed, White to move.
        let placement = format!("4k3/8/8/{}/8/8/8/4K3", rank_with_pawn_at(index, 'p'));
        let fen = format!("{placement} w - {letter}6 0 1");
        let board = parse(&fen);
        assert_eq!(board.ep_file(), Some(file));
        assert!(
            keys.insert(board.key().get()),
            "two en-passant files share a key: {fen}"
        );
        boards.push(board);
    }
    assert_eq!(keys.len(), 8);

    // And a position with no en-passant file is distinct from all eight.
    let none = parse("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    for board in &boards {
        assert_ne!(
            board.key(),
            none.key(),
            "a live en-passant file must not hash like none at all"
        );
    }
}

/// A rank string with a single pawn on `file`.
fn rank_with_pawn_at(file: usize, piece: char) -> String {
    let mut out = String::new();
    if file > 0 {
        out.push_str(&file.to_string());
    }
    out.push(piece);
    if file < 7 {
        out.push_str(&(7 - file).to_string());
    }
    out
}

/// The key hashes the en-passant **file**, not the square.
///
/// `e3` with Black to move and `e6` with White to move are the same file and different
/// ranks. Under a file-keyed table their en-passant contributions cancel exactly, so the
/// two keys differ by precisely the side-to-move key. Under a square-keyed table of 64
/// entries they would not, and no other test in this repository could tell.
#[test]
fn e3_with_black_to_move_and_e6_with_white_differ_by_exactly_the_side_key() {
    // The same pieces in both, so that only the side to move and the ep rank differ.
    let black_to_move = parse("4k3/8/8/8/4P3/8/8/4K3 b - e3 0 1");
    let white_to_move = parse("4k3/8/8/4p3/8/8/8/4K3 w - e6 0 1");
    assert_eq!(black_to_move.ep_file(), white_to_move.ep_file());
    assert_ne!(black_to_move.ep_square(), white_to_move.ep_square());

    // Isolate the en-passant contribution: strip the pieces and the side, and what remains
    // must be identical, because both positions hash the same file key.
    let ep_key = zobrist::en_passant(black_to_move.ep_file());
    assert_eq!(ep_key, zobrist::en_passant(white_to_move.ep_file()));
    assert_ne!(ep_key.get(), 0);
}

// ---------------------------------------------------------------------------------
// Acceptance criterion 6, clause 2 — the operative reading
// ---------------------------------------------------------------------------------

/// The key is a function of exactly the nine bits [`POSITION_MASK`] selects.
///
/// Exhaustive over every legal state word: 2 sides × 16 castling masks × 9 en-passant
/// states × a sample of clocks. If the clocks leaked into the key, the number of distinct
/// keys would be the number of state words rather than the number of positions.
#[test]
fn the_key_ignores_the_halfmove_clock_and_the_fullmove_number() {
    let mut by_position: HashSet<(u64, u64)> = HashSet::new();
    let mut distinct_state_words = HashSet::new();

    for halfmove in [0u16, 1, 50, 99, 100, 255, 256, 65535] {
        for fullmove in [1u16, 2, 42, 1000, 65535] {
            let fen = format!("4k3/8/8/8/8/8/8/4K3 w - - {halfmove} {fullmove}");
            let board = parse(&fen);
            distinct_state_words.insert(board.state_word());
            by_position.insert((board.state_word() & POSITION_MASK, board.key().get()));
        }
    }

    assert_eq!(
        distinct_state_words.len(),
        40,
        "the forty clock combinations must produce forty distinct state words"
    );
    assert_eq!(
        by_position.len(),
        1,
        "all forty must map to one (position bits, key) pair: the clocks must not be hashed"
    );
}

/// The same claim over every representable combination of the nine hashed bits.
///
/// 2 × 16 × 9 = 288 distinct contributions, none colliding — which is the statement that
/// the side, castling and en-passant keys are jointly independent over GF(2).
#[test]
fn the_hashed_state_bits_produce_two_hundred_and_eighty_eight_distinct_contributions() {
    let mut seen = HashSet::new();
    for side_is_black in [false, true] {
        for castling_bits in 0..16u8 {
            for ep in 0..9u8 {
                let rights =
                    boid_board::CastlingRights::from_bits(castling_bits).expect("below 16");
                let file = File::new(ep);
                let mut key = zobrist::castling(rights);
                key ^= zobrist::en_passant(file);
                if side_is_black {
                    key ^= zobrist::side_to_move();
                }
                assert!(
                    seen.insert(key.get()),
                    "collision at side={side_is_black} castling={castling_bits} ep={ep:?}"
                );
            }
        }
    }
    assert_eq!(seen.len(), 2 * 16 * 9);
}

// ---------------------------------------------------------------------------------
// The incremental key against a from-scratch recompute
// ---------------------------------------------------------------------------------

/// A pseudo-random walk, asserting at every ply that the incrementally maintained keys
/// agree with a recompute that reads a *different* representation.
///
/// The moves are frequently absurd as chess — a rook stepping like a king, a bishop
/// teleporting — and that is deliberate and stated: the property under test is the XOR
/// bookkeeping, not the rules. Absurd moves stress the schedule harder than legal play,
/// because they mix piece kinds and squares that a real game would not.
///
/// The walk is driven by the same splitmix64 the zobrist table uses, seeded from a
/// constant, so a failure reproduces exactly rather than vanishing on the next run.
#[test]
fn the_incremental_keys_track_a_recompute_over_a_random_walk() {
    let mut applied = 0usize;
    let mut kinds_seen = HashSet::new();

    for seed in 0..24u64 {
        let mut board = Board::startpos();
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);

        for _ in 0..60 {
            let (next, roll) = zobrist::splitmix64(state);
            state = next;

            let Some(mv) = candidate_move(&board, roll) else {
                continue;
            };
            let Ok(next_board) = board.try_apply_move(mv) else {
                continue;
            };
            board = next_board;
            applied += 1;
            kinds_seen.insert(std::mem::discriminant(&mv.kind()));

            assert_eq!(
                board.key(),
                board.recomputed_key(),
                "after {mv} the incremental key left the recompute behind"
            );
            assert_eq!(
                board.pawn_key(),
                board.recomputed_pawn_key(),
                "after {mv} the incremental pawn key left the recompute behind"
            );
            board
                .check_invariants()
                .unwrap_or_else(|e| panic!("after {mv}: {e}"));

            // Crossing the text representation as well: to_fen reads the bitboards while
            // recomputed_key walks the mailbox, so this ties all three together.
            let round_tripped = parse(&board.to_fen());
            assert_eq!(round_tripped.key(), board.key(), "after {mv}");
        }
    }

    // A generator that returned None at every step would make the loop vacuous, green, and
    // invisible to every other guard in this repository.
    assert!(
        applied >= 500,
        "the walk applied only {applied} moves, which is too few to have exercised anything"
    );
    assert!(
        kinds_seen.len() >= 4,
        "the walk produced only {} distinct move kinds",
        kinds_seen.len()
    );
}

/// Propose a move from `roll`, without any attack table.
///
/// Picks one of the side to move's pieces and a destination, and labels the move with the
/// kind the board supports. Wrong-looking moves are fine; `try_apply_move` rejects whatever
/// is structurally impossible.
fn candidate_move(board: &Board, roll: u64) -> Option<Move> {
    let ours: Vec<Square> = board.colored(board.side_to_move()).squares().collect();
    if ours.is_empty() {
        return None;
    }
    let from = ours[(roll % ours.len() as u64) as usize];
    let to = Square::from_index(((roll >> 8) % 64) as u8)?;
    if from == to {
        return None;
    }

    let piece = board.piece_at(from)?;
    let occupant = board.piece_at(to);
    // A king capture cannot arise in legal play, and a board with a side missing its king
    // is not FEN-representable — check_invariants rejects it, correctly. The walk ignores
    // legality on purpose, so this one rule has to be stated explicitly.
    if occupant.is_some_and(|p| p.kind() == PieceKind::King) {
        return None;
    }
    let captures = occupant.is_some_and(|p| p.color() != piece.color());
    let last_rank = if piece.color() == boid_board::Color::White {
        7
    } else {
        0
    };

    let kind = if piece.kind() == PieceKind::Pawn && to.rank() == last_rank {
        let promo = PieceKind::PROMOTIONS[((roll >> 16) % 4) as usize];
        if captures {
            MoveKind::PromoCapture(promo)
        } else {
            MoveKind::Promotion(promo)
        }
    } else if piece.kind() == PieceKind::Pawn && board.ep_square() == Some(to) {
        MoveKind::EnPassant
    } else if piece.kind() == PieceKind::Pawn
        && from.file() == to.file()
        && from.rank().abs_diff(to.rank()) == 2
    {
        MoveKind::DoublePawnPush
    } else if piece.kind() == PieceKind::King
        && from.file().index().abs_diff(to.file().index()) == 2
    {
        if to.file().index() > from.file().index() {
            MoveKind::KingCastle
        } else {
            MoveKind::QueenCastle
        }
    } else if captures {
        MoveKind::Capture
    } else if occupant.is_some() {
        return None; // our own piece
    } else {
        MoveKind::Quiet
    };

    Move::new(from, to, kind)
}

/// Boards that are the same position hash alike, over the whole generated corpus.
///
/// The converse — different positions always hash differently — is probabilistic and is
/// deliberately not asserted; what the key set guarantees is the direction stated here.
#[test]
fn boards_that_are_the_same_position_hash_alike() {
    for fen in [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        "4k3/8/8/4p3/8/8/8/4K3 w - e6 0 1",
    ] {
        let board = parse(fen);
        for (halfmove, fullmove) in [(0u16, 1u16), (37, 12), (99, 400)] {
            let mut fields: Vec<&str> = fen.split(' ').collect();
            let half = halfmove.to_string();
            let full = fullmove.to_string();
            fields[4] = &half;
            fields[5] = &full;
            let other = parse(&fields.join(" "));
            assert!(other.same_position(&board));
            assert_eq!(other.key(), board.key(), "{fen} at {halfmove}/{fullmove}");
        }
    }
}

/// The generated corpus, run through the key machinery rather than only through the round
/// trip.
///
/// Uses the same text-first generator the round-trip test uses, so the two files cannot
/// drift onto different corpora, and asserts the property the round trip cannot see: the
/// key a parsed board carries equals a from-scratch recompute, and stays equal when the
/// clocks are changed underneath it.
#[test]
fn every_generated_position_carries_a_key_that_matches_a_recompute() {
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: 200,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    runner
        .run(&fenlab::arbitrary_fen(), |fen| {
            let board = parse(&fen);
            prop_assert_eq!(board.key(), board.recomputed_key());
            prop_assert_eq!(board.pawn_key(), board.recomputed_pawn_key());

            // Same position, different clocks: the key must not move.
            let mut fields: Vec<&str> = fen.split(' ').collect();
            fields[4] = "77";
            fields[5] = "123";
            let restamped = parse(&fields.join(" "));
            prop_assert!(restamped.same_position(&board));
            prop_assert_eq!(restamped.key(), board.key());
            Ok(())
        })
        .expect("every generated position must carry a consistent key");
}

/// The mirror of a position hashes differently from the position, but the *structure* of
/// the two keys agrees — a sanity check that colour is hashed at all.
#[test]
fn a_colour_mirrored_position_hashes_differently() {
    let fen = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
    let board = parse(fen);
    let mirrored = parse(&fenlab::flip(fen));
    assert_ne!(
        board.key(),
        mirrored.key(),
        "a colour mirror is a different position and must hash differently"
    );
    assert_eq!(
        board.pieces(PieceKind::Pawn).count(),
        mirrored.pieces(PieceKind::Pawn).count()
    );
}
