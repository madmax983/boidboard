//! Incremental zobrist maintenance: does the key that was *maintained* equal the key the
//! position actually has, however you got there?
//!
//! AC5 asks for two positions reached by different move orders to hash the same. Given the
//! contract in `src/board/edit.rs` — every setter XORs out what it replaces before XORing in
//! what it stores — that is a theorem, because XOR is commutative and self-inverse. So these
//! tests are not really about XOR's algebra; they are about the XOR-**out** discipline,
//! which is where every real zobrist bug lives: the `None` branch of an en-passant clear,
//! the castling right that changed without being unhashed, the pawn hash that a promotion
//! walked past.
//!
//! The transposition here is chosen accordingly. `1.Nf3 Nf6 2.Ng1 Ng8` returns to the
//! starting position and is a *repetition*, not a transposition: it visits every slot an
//! even number of times, so it passes with a no-op `set_en_passant`, a no-op `set_castling`
//! and a side key that is never applied. It is kept, because it demonstrates something #8
//! needs — key equality is not board equality — but AC5's evidence is the five-ply
//! transposition below, whose two paths pass through *different* en-passant files and whose
//! terminal position is not the one it started from.
//!
//! The pinned literals were derived from D-0020's scheme in Python, independently of this
//! crate. They are what makes the table digest insufficient on its own: the digest is blind
//! to the index formula, to `key_at`'s `+1`, and to which way up the board is.

use boid_board::board::Board;
use boid_board::types::{
    CastlingRight, CastlingRights, Colour, File, Piece, PieceKind, Rank, Square,
};
use boid_board::zobrist::{ZOBRIST, splitmix64};

/// The starting position's key, derived independently. Blind spots the table digest has and
/// this literal does not: the flat index formula (`piece * 64 + square` versus
/// `square * 12 + piece`), the board's orientation, and the piece-index map.
const STARTPOS_KEY: u64 = 0x7CE2_53A8_B840_79FD;
/// The starting position's pawn hash.
const STARTPOS_PAWN_KEY: u64 = 0x01AE_05F1_3B46_7B94;
/// The position after `1.e4`, with the en-passant file recorded — as issue #4 requires it to
/// be, whether or not a capture is available (D-0021).
const AFTER_E4_WITH_EP: u64 = 0xC90E_3B7F_64FE_04CD;
/// The same position with no en-passant file. The difference between this and the value
/// above is exactly what AC6 clause 1 is about.
const AFTER_E4_WITHOUT_EP: u64 = 0xA67B_26F6_D340_37CF;
/// The starting position with White's queenside right removed (`Kkq`).
const STARTPOS_RIGHTS_KKQ: u64 = 0xD99C_0A05_181A_D6FA;
/// The terminal position of the five-ply transposition:
/// `rnbqkb1r/pppp1ppp/4pn2/8/2PP4/2N5/PP2PPPP/R1BQKBNR b KQkq -`.
const NIMZO_TERMINAL_KEY: u64 = 0xC9D7_3E55_51B8_BC8C;

/// Play a move with the primitives, maintaining the clocks and clearing the en-passant file
/// as a real move would.
///
/// Not a move generator and not a legality check — it does not know what a legal move is,
/// and issue #5 owns both. It exists so these tests exercise the primitives in the order and
/// combination that `make_move` eventually will.
fn play(board: &mut Board, from: Square, to: Square) {
    let piece = board.take(from).expect("a piece on the from-square");
    let captured = board.take(to);
    board.place(to, piece);
    board.set_en_passant(None);

    let resets = piece.kind() == PieceKind::Pawn || captured.is_some();
    let clock = if resets {
        0
    } else {
        board.halfmove_clock().saturating_add(1)
    };
    board.set_halfmove_clock(clock);
    if board.side_to_move() == Colour::Black {
        board.set_fullmove_number(board.fullmove_number().saturating_add(1));
    }
    board.set_side_to_move(board.side_to_move().flip());
}

/// Play a double pawn push, which records the en-passant file it passed over.
///
/// The file is recorded unconditionally, whether or not an enemy pawn could capture. That is
/// the convention issue #4 mandates and D-0021 records; Stockfish uses the other one.
fn double_push(board: &mut Board, from: Square, to: Square) {
    play(board, from, to);
    board.set_en_passant(Some(from.file()));
}

#[test]
fn startpos_key_is_pinned() {
    // Passes the moment `startpos()` exists, so no red is claimed for it. It is here
    // because the digest in zobrist_tables.rs structurally cannot see what this sees: the
    // digest fixes the 781 keys, this fixes how a position selects among them.
    assert_eq!(Board::startpos().key(), STARTPOS_KEY);
    assert_eq!(Board::startpos().pawn_key(), STARTPOS_PAWN_KEY);
}

#[test]
fn empty_board_with_black_to_move_hashes_the_side_key() {
    // Makes the empty board's zero key non-vacuous: zero is also what a key that was never
    // computed looks like, and this shows the difference.
    let mut board = Board::empty();
    assert_eq!(board.key(), 0);
    board.set_side_to_move(Colour::Black);
    assert_eq!(board.key(), ZOBRIST.side_to_move());
    assert_eq!(board.key(), board.recomputed_key());
    assert_eq!(board.pawn_key(), 0, "the side key is not a pawn");
}

#[test]
fn transposition_through_different_en_passant_states_hashes_equal() {
    // 1.d4 Nf6 2.c4 e6 3.Nc3   and   1.c4 Nf6 2.d4 e6 3.Nc3
    //
    // The same position by two move orders. Both paths pass through a double push, so both
    // record an en-passant file — different ones, in a different order. That is what makes
    // this AC5's evidence rather than a tautology: a `set_en_passant` that did nothing at
    // all would still pass a transposition test whose paths never set one.
    let mut first = Board::startpos();
    double_push(&mut first, Square::D2, Square::D4);
    let first_after_one = first;
    play(&mut first, Square::G8, Square::F6);
    double_push(&mut first, Square::C2, Square::C4);
    play(&mut first, Square::E7, Square::E6);
    play(&mut first, Square::B1, Square::C3);

    let mut second = Board::startpos();
    double_push(&mut second, Square::C2, Square::C4);
    let second_after_one = second;
    play(&mut second, Square::G8, Square::F6);
    double_push(&mut second, Square::D2, Square::D4);
    play(&mut second, Square::E7, Square::E6);
    play(&mut second, Square::B1, Square::C3);

    assert_ne!(
        first_after_one.key(),
        second_after_one.key(),
        "the paths must genuinely diverge, or they are not two paths: after 1.d4 the ep \
         file is d, after 1.c4 it is c"
    );
    assert_eq!(first_after_one.en_passant_file(), Some(File::D));
    assert_eq!(second_after_one.en_passant_file(), Some(File::C));

    assert_eq!(first, second, "the same position, reached two ways");
    assert_eq!(first.key(), second.key(), "AC5");
    assert_eq!(
        first.key(),
        first.recomputed_key(),
        "maintained == recomputed"
    );
    assert_eq!(first.key(), NIMZO_TERMINAL_KEY);
    assert_ne!(
        first.key(),
        STARTPOS_KEY,
        "and the terminal position is not where it started, so the equality is not the \
         trivial one"
    );
    assert_eq!(first.consistency(), Ok(()));
}

#[test]
fn move_order_returning_to_the_start_is_a_repetition_not_a_transposition() {
    // 1.Nf3 Nf6 2.Ng1 Ng8. The key returns to the starting position's; the board does not,
    // because the clocks moved. Issue #8 needs exactly this distinction: repetition is a
    // question about keys, and `Board`'s own equality — which includes the clocks — is the
    // wrong relation to ask it with.
    let mut board = Board::startpos();
    play(&mut board, Square::G1, Square::F3);
    play(&mut board, Square::G8, Square::F6);
    play(&mut board, Square::F3, Square::G1);
    play(&mut board, Square::F6, Square::G8);

    assert_eq!(board.key(), STARTPOS_KEY, "the same position");
    assert_eq!(board.pawn_key(), STARTPOS_PAWN_KEY);
    assert_ne!(board, Board::startpos(), "but not the same board");
    assert_eq!(board.halfmove_clock(), 4);
    assert_eq!(board.fullmove_number(), 3);
    assert_eq!(board.consistency(), Ok(()));
}

#[test]
fn different_first_moves_hash_differently() {
    // Negative controls. AC5 gets *easier* the more broken the hash is — a key that is
    // always zero satisfies every transposition test ever written — so the ability to tell
    // positions apart has to be asserted too.
    let start = Board::startpos();

    let mut after_e4 = start;
    double_push(&mut after_e4, Square::E2, Square::E4);
    assert_eq!(after_e4.key(), AFTER_E4_WITH_EP);
    assert_ne!(after_e4.key(), start.key());

    let mut after_nf3 = start;
    play(&mut after_nf3, Square::G1, Square::F3);
    let mut after_nc3 = start;
    play(&mut after_nc3, Square::B1, Square::C3);
    assert_ne!(
        after_nf3.key(),
        after_nc3.key(),
        "two knight moves to different squares"
    );
    assert_ne!(after_nf3.key(), start.key());
    assert_eq!(
        after_nf3.pawn_key(),
        STARTPOS_PAWN_KEY,
        "a knight move leaves the pawn hash alone"
    );
}

#[test]
fn uncapturable_en_passant_still_changes_the_key() {
    // AC6, and the point of D-0021. After 1.e4 no black pawn can capture on e3, and
    // Stockfish would drop the ep square from its FEN and from its key. This project keeps
    // it, because the published perft counts use that convention, and because the failure
    // modes are asymmetric: an unnecessary distinction costs a transposition-table miss,
    // while a missing one costs a table *hit* on a different position.
    let mut with_ep = Board::startpos();
    double_push(&mut with_ep, Square::E2, Square::E4);

    let mut without_ep = with_ep;
    without_ep.set_en_passant(None);

    assert_eq!(with_ep.key(), AFTER_E4_WITH_EP);
    assert_eq!(without_ep.key(), AFTER_E4_WITHOUT_EP);
    assert_ne!(with_ep.key(), without_ep.key());
    assert_eq!(
        with_ep.key() ^ without_ep.key(),
        ZOBRIST.en_passant(File::E),
        "the difference is exactly the e-file key"
    );
    assert_eq!(
        with_ep.pawn_key(),
        without_ep.pawn_key(),
        "the en-passant file is not a pawn"
    );
}

#[test]
fn the_clocks_do_not_enter_the_key() {
    // AC6 clause 2, in its most consequential form. If the clocks were hashed, repetition
    // detection would be broken from the first day and no perft count would ever notice.
    let start = Board::startpos();
    let mut moved_clocks = start;
    moved_clocks.set_halfmove_clock(99);
    moved_clocks.set_fullmove_number(42);

    assert_eq!(moved_clocks.key(), start.key());
    assert_eq!(moved_clocks.pawn_key(), start.pawn_key());
    assert_eq!(moved_clocks.key(), moved_clocks.recomputed_key());
    assert_ne!(moved_clocks, start, "the boards still differ");
}

#[test]
fn castling_rights_change_the_key_but_not_the_pawn_key() {
    let start = Board::startpos();
    let mut fewer = start;
    fewer.set_castling(CastlingRights::ALL.without(CastlingRight::WhiteQueenside));

    assert_eq!(fewer.key(), STARTPOS_RIGHTS_KKQ);
    assert_eq!(
        start.key() ^ fewer.key(),
        ZOBRIST.castling(CastlingRight::WhiteQueenside)
    );
    assert_eq!(fewer.pawn_key(), start.pawn_key());
    assert_eq!(fewer.key(), fewer.recomputed_key());
}

#[test]
fn side_to_move_change_is_exactly_the_side_key() {
    let white = Board::startpos();
    let mut black = white;
    black.set_side_to_move(Colour::Black);

    assert_eq!(white.key() ^ black.key(), ZOBRIST.side_to_move());
    assert_eq!(black.key(), black.recomputed_key());

    // Setting the same side twice must not toggle it back — the reason these are setters
    // rather than a `flip_side_to_move()` toggle.
    let mut again = black;
    again.set_side_to_move(Colour::Black);
    assert_eq!(again.key(), black.key());
    assert_eq!(again, black);
}

#[test]
fn set_en_passant_round_trip_restores_the_key() {
    // The single most common zobrist bug: the clear path XORs nothing, so the old file's
    // key stays in forever.
    let start = Board::startpos();
    let mut board = start;
    board.set_en_passant(Some(File::E));
    assert_ne!(board.key(), start.key());
    board.set_en_passant(None);
    assert_eq!(board.key(), start.key());
    assert_eq!(board, start);
}

#[test]
fn set_en_passant_is_idempotent_and_does_not_accumulate() {
    let start = Board::startpos();

    let mut twice = start;
    twice.set_en_passant(Some(File::E));
    twice.set_en_passant(Some(File::E));
    let mut once = start;
    once.set_en_passant(Some(File::E));
    assert_eq!(twice.key(), once.key(), "setting the same file twice");

    // Some -> Some, which the None round-trip above does not exercise: if the old file is
    // not XORed out on this path, e and d accumulate and the key never comes back.
    let mut wandered = start;
    wandered.set_en_passant(Some(File::E));
    wandered.set_en_passant(Some(File::D));
    wandered.set_en_passant(Some(File::E));
    assert_eq!(wandered.key(), once.key(), "e, then d, then e again");
    assert_eq!(wandered.key(), wandered.recomputed_key());
}

#[test]
fn every_castling_transition_maintains_the_key() {
    // All 256 transitions between the 16 masks. "XORed the wrong subset" is not a bug a
    // few hand-picked cases find, and the whole space is 256 iterations.
    for from_bits in 0..16u8 {
        for to_bits in 0..16u8 {
            let mut board = Board::startpos();
            board.set_castling(CastlingRights::from_bits(from_bits).expect("four bits"));
            board.set_castling(CastlingRights::from_bits(to_bits).expect("four bits"));

            assert_eq!(
                board.key(),
                board.recomputed_key(),
                "castling {from_bits:#06b} -> {to_bits:#06b}"
            );
            assert_eq!(board.castling().bits(), to_bits);
        }
    }
}

#[test]
fn pawn_key_is_invariant_under_every_non_pawn_edit() {
    // A `kind == Pawn` test that was mutated to `(kind as u8) <= 1` would also update the
    // pawn hash for knights, and a test that only tried knights, or only tried one kind,
    // would miss it. All five non-pawn kinds, plus the three state setters.
    let start = Board::startpos();

    for kind in PieceKind::ALL {
        if kind == PieceKind::Pawn {
            continue;
        }
        let mut board = start;
        board.take(Square::E4);
        board.place(Square::E4, Piece::new(Colour::White, kind));
        assert_eq!(
            board.pawn_key(),
            start.pawn_key(),
            "placing a {kind:?} moved the pawn hash"
        );
        assert_ne!(board.key(), start.key(), "but it must move the key");
        assert_eq!(board.pawn_key(), board.recomputed_pawn_key());
    }

    let mut state_only = start;
    state_only.set_side_to_move(Colour::Black);
    state_only.set_castling(CastlingRights::NONE);
    state_only.set_en_passant(Some(File::C));
    state_only.set_halfmove_clock(30);
    state_only.set_fullmove_number(20);
    assert_eq!(state_only.pawn_key(), start.pawn_key());
    assert_eq!(state_only.pawn_key(), state_only.recomputed_pawn_key());
}

#[test]
fn pawn_key_tracks_a_promotion() {
    // The promotion is where a pawn hash goes wrong in a way that shows up much later as a
    // nondeterministic-looking evaluation cache bug. Here it is a property of `take` and
    // `place` themselves, so no move-application path can forget it.
    let mut board = Board::empty();
    board.place(Square::A7, Piece::WhitePawn);
    let before_key = board.key();
    let before_pawn = board.pawn_key();

    let promoted = board.take(Square::A7).expect("the pawn");
    board.place(Square::A8, Piece::WhiteQueen);

    assert_eq!(promoted, Piece::WhitePawn);
    assert_eq!(
        board.pawn_key(),
        before_pawn ^ ZOBRIST.piece_square(Piece::WhitePawn, Square::A7),
        "the pawn left the pawn hash and the queen never entered it"
    );
    assert_eq!(board.pawn_key(), 0, "no pawns left");
    assert_eq!(
        board.key(),
        before_key
            ^ ZOBRIST.piece_square(Piece::WhitePawn, Square::A7)
            ^ ZOBRIST.piece_square(Piece::WhiteQueen, Square::A8)
    );
    assert_eq!(board.key(), board.recomputed_key());
}

#[test]
fn pawnless_positions_have_a_zero_pawn_key() {
    let mut board = Board::empty();
    board.place(Square::E1, Piece::WhiteKing);
    board.place(Square::E8, Piece::BlackKing);
    board.place(Square::D4, Piece::WhiteRook);
    assert_eq!(
        board.pawn_key(),
        0,
        "the zero base, pinned so it cannot drift"
    );
    assert_ne!(board.key(), 0);

    board.place(Square::A2, Piece::WhitePawn);
    assert_ne!(board.pawn_key(), 0);
    assert_eq!(
        board.pawn_key(),
        ZOBRIST.piece_square(Piece::WhitePawn, Square::A2)
    );
}

#[test]
fn key_xor_pawn_key_is_the_non_pawn_contribution() {
    // Both hashes draw from the same piece-square table, which is what makes this
    // expressible at all: with two independently seeded tables there would be no algebraic
    // relation between the two keys to state.
    let mut board = Board::empty();
    board.place(Square::G1, Piece::WhiteKing);
    board.place(Square::B8, Piece::BlackKing);
    board.place(Square::F2, Piece::WhitePawn);
    board.place(Square::A7, Piece::BlackPawn);
    board.set_side_to_move(Colour::Black);
    board.set_en_passant(Some(File::H));

    let expected_non_pawn = ZOBRIST.piece_square(Piece::WhiteKing, Square::G1)
        ^ ZOBRIST.piece_square(Piece::BlackKing, Square::B8)
        ^ ZOBRIST.side_to_move()
        ^ ZOBRIST.en_passant(File::H);

    assert_eq!(board.key() ^ board.pawn_key(), expected_non_pawn);
}

#[test]
fn incremental_key_equals_recomputation_after_every_edit() {
    // Two thousand pseudo-random edits, checked after each one. Deterministic: the driver
    // is the crate's own splitmix64, seeded by a constant, so a failure here is replayable
    // by running it again rather than by hoping.
    let mut board = Board::startpos();
    let mut state = 0xB01D_B0A4_D004_5EEDu64;

    for step in 0..2000u32 {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let roll = splitmix64(state);
        let square = Square::new((roll % 64) as u8).expect("below 64");

        match (roll >> 8) % 6 {
            0 => {
                board.take(square);
                let piece = Piece::ALL[((roll >> 16) % 12) as usize];
                board.place(square, piece);
            }
            1 => {
                board.take(square);
            }
            2 => board.set_side_to_move(if (roll >> 16).is_multiple_of(2) {
                Colour::White
            } else {
                Colour::Black
            }),
            3 => board.set_castling(
                CastlingRights::from_bits(((roll >> 16) % 16) as u8).expect("four bits"),
            ),
            4 => board.set_en_passant(File::from_index(((roll >> 16) % 9) as u8)),
            _ => {
                board.set_halfmove_clock(((roll >> 16) % 256) as u8);
                board.set_fullmove_number(((roll >> 24) % 65_536) as u16);
            }
        }

        assert_eq!(
            board.key(),
            board.recomputed_key(),
            "key drifted at step {step}: {board:?}"
        );
        assert_eq!(
            board.pawn_key(),
            board.recomputed_pawn_key(),
            "pawn key drifted at step {step}: {board:?}"
        );
        assert_eq!(board.consistency(), Ok(()), "at step {step}");
    }

    // The sequence must actually have done something, or this is two thousand assertions
    // about the starting position.
    assert_ne!(board, Board::startpos());
    assert_ne!(board.occupied(), Board::startpos().occupied());
}

#[test]
fn en_passant_target_follows_the_side_to_move() {
    // The file is stored; the rank is derived. Getting the derivation backwards round-trips
    // through FEN perfectly and puts the target on the wrong side of the board.
    let mut board = Board::empty();
    board.set_side_to_move(Colour::Black);
    board.set_en_passant(Some(File::E));
    assert_eq!(
        board.en_passant_target(),
        Some(Square::E3),
        "Black to move means White just pushed two squares, so the target is on rank 3"
    );
    assert_eq!(Square::E3.rank(), Rank::R3);

    board.set_side_to_move(Colour::White);
    assert_eq!(board.en_passant_target(), Some(Square::E6));
    assert_eq!(Square::E6.rank(), Rank::R6);

    board.set_en_passant(None);
    assert_eq!(board.en_passant_target(), None);
}
