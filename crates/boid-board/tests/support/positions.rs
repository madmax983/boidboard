//! The generated position corpus: 200 legal positions, built the same way every time.
//!
//! AC1 asks for "200 randomly generated legal positions". Two words there need care.
//!
//! **Random.** The corpus is driven by this crate's own splitmix64 from a hardcoded seed,
//! not by proptest's value stream. That is the same argument issue #4 makes about the zobrist
//! keys — a reproducible corpus means a reproducible failure — and it has a second benefit:
//! the corpus fingerprint cannot move because a dependency changed its RNG. proptest is used
//! for what it is genuinely better at, in `fen_proptest.rs`: shrinking adversarial inputs.
//!
//! **Legal.** Bounded honestly. Each position has exactly one king per side, kings not
//! adjacent, no pawn on rank 1 or 8, at most eight pawns and sixteen pieces a side, castling
//! rights only where the king and rook stand on their home squares, an en-passant file only
//! where a double push could actually have produced it, and the side *not* to move is not in
//! check — which is what [`super::naive_attacks`] is for. It is **not** retrograde-legal: no
//! attempt is made to prove a position is reachable from the initial one.

use boid_board::board::Board;
use boid_board::types::{
    CastlingRight, CastlingRights, Colour, File, Piece, PieceKind, Rank, Square,
};
use boid_board::zobrist::splitmix64;

use super::naive_attacks::is_in_check;

/// How many positions the corpus holds. AC1's number.
pub const CORPUS_SIZE: usize = 200;

/// The corpus seed: ASCII `corpus_0`, big-endian, in the same spirit as the zobrist seed.
const CORPUS_SEED: u64 = 0x636F_7270_7573_5F30;

/// A splitmix64 stream, so the corpus is a pure function of `CORPUS_SEED`.
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        splitmix64(self.state)
    }

    /// A value in `0..bound`. Modulo bias is irrelevant here: this generates test positions,
    /// not cryptographic material.
    fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 { 0 } else { self.next() % bound }
    }

    fn chance(&mut self, numerator: u64, denominator: u64) -> bool {
        self.below(denominator) < numerator
    }
}

/// The 200-position corpus.
///
/// # Panics
///
/// Panics if generation cannot find enough legal positions, which would mean the generator
/// itself is broken — a loud failure is the point.
pub fn corpus() -> Vec<Board> {
    let mut rng = Rng::new(CORPUS_SEED);
    let mut positions = Vec::with_capacity(CORPUS_SIZE);
    let mut attempts = 0u32;

    while positions.len() < CORPUS_SIZE {
        attempts += 1;
        assert!(
            attempts < 100_000,
            "the generator produced only {} legal positions in {attempts} attempts",
            positions.len()
        );
        if let Some(board) = try_position(&mut rng) {
            positions.push(board);
        }
    }

    positions
}

/// How many attempts the generator needs for the whole corpus, and how many it rejects.
///
/// Pinned by a test: a rejection count that moves means the legality filter or the attack
/// routine changed, and that deserves a look rather than a silent pass.
pub fn corpus_attempts() -> (usize, usize) {
    let mut rng = Rng::new(CORPUS_SEED);
    let (mut accepted, mut rejected) = (0usize, 0usize);
    while accepted < CORPUS_SIZE {
        if try_position(&mut rng).is_some() {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }
    (accepted, rejected)
}

/// One attempt at a legal position. `None` when the attempt violated a rule that is cheaper
/// to detect afterwards than to design around.
fn try_position(rng: &mut Rng) -> Option<Board> {
    let mut board = Board::empty();

    // Roughly a third of positions get the castling-shaped skeleton, so that castling rights
    // are actually reachable rather than a rarity the corpus never covers.
    let castling_shaped = rng.chance(1, 3);
    if castling_shaped {
        board.place(Square::E1, Piece::WhiteKing);
        board.place(Square::E8, Piece::BlackKing);
        for (square, piece) in [
            (Square::A1, Piece::WhiteRook),
            (Square::H1, Piece::WhiteRook),
            (Square::A8, Piece::BlackRook),
            (Square::H8, Piece::BlackRook),
        ] {
            if rng.chance(3, 4) {
                board.place(square, piece);
            }
        }
    } else {
        let white_king = square_of(rng.below(64));
        let black_king = square_of(rng.below(64));
        if white_king == black_king || chebyshev(white_king, black_king) < 2 {
            return None;
        }
        board.place(white_king, Piece::WhiteKing);
        board.place(black_king, Piece::BlackKing);
    }

    // Material. Promotions are allowed for, loosely: up to two queens and three of each
    // minor, capped so that no side exceeds sixteen pieces.
    for colour in Colour::ALL {
        let counts = [
            (PieceKind::Pawn, rng.below(9)),
            (PieceKind::Knight, rng.below(4)),
            (PieceKind::Bishop, rng.below(4)),
            (PieceKind::Rook, rng.below(4)),
            (PieceKind::Queen, rng.below(3)),
        ];
        for (kind, count) in counts {
            for _ in 0..count {
                if (board.colours(colour)).count() >= 16 {
                    break;
                }
                let square = square_of(rng.below(64));
                if board.piece_at(square).is_some() {
                    continue;
                }
                // No pawn may stand on the first or eighth rank.
                if kind == PieceKind::Pawn
                    && (square.rank() == Rank::R1 || square.rank() == Rank::R8)
                {
                    continue;
                }
                board.place(square, Piece::new(colour, kind));
            }
        }
    }

    board.set_side_to_move(if rng.chance(1, 2) {
        Colour::White
    } else {
        Colour::Black
    });

    // Castling rights, derived from the board and then thinned at random. Deriving first is
    // what keeps every generated position parseable: a right whose king or rook is missing
    // is rejected by `from_fen`.
    let mut rights = CastlingRights::NONE;
    for right in CastlingRight::ALL {
        let colour = right.colour();
        let king_home =
            board.piece_at(right.king_from()) == Some(Piece::new(colour, PieceKind::King));
        let rook_home =
            board.piece_at(right.rook_from()) == Some(Piece::new(colour, PieceKind::Rook));
        if king_home && rook_home && rng.chance(3, 4) {
            rights = rights.with(right);
        }
    }
    board.set_castling(rights);

    // En passant, only where a double push could have produced it. Candidates are computed
    // on the finished board, so the file is always one `from_fen` will accept.
    if rng.chance(1, 3) {
        let candidates = en_passant_candidates(&board);
        if !candidates.is_empty() {
            let pick = rng.below(candidates.len() as u64) as usize;
            board.set_en_passant(candidates.get(pick).copied());
        }
    }

    // Clocks, weighted towards the boundaries the parser cares about.
    board.set_halfmove_clock(match rng.below(8) {
        0 => 0,
        1 => 1,
        2 => u8::MAX,
        _ => rng.below(256) as u8,
    });
    board.set_fullmove_number(match rng.below(8) {
        0 => 1,
        1 => u16::MAX,
        _ => 1 + rng.below(500) as u16,
    });

    // The one filter that needs an attack routine: a position where the side that just moved
    // is still in check cannot arise in a game.
    if is_in_check(&board, board.side_to_move().flip()) {
        return None;
    }

    Some(board)
}

/// The en-passant files that would be well-formed on this board.
fn en_passant_candidates(board: &Board) -> Vec<File> {
    let (target_rank, origin_rank, pusher_rank) = match board.side_to_move() {
        Colour::Black => (Rank::R3, Rank::R2, Rank::R4),
        Colour::White => (Rank::R6, Rank::R7, Rank::R5),
    };
    let pusher = Piece::new(board.side_to_move().flip(), PieceKind::Pawn);

    File::ALL
        .into_iter()
        .filter(|file| {
            board
                .piece_at(Square::from_file_rank(*file, target_rank))
                .is_none()
                && board
                    .piece_at(Square::from_file_rank(*file, origin_rank))
                    .is_none()
                && board.piece_at(Square::from_file_rank(*file, pusher_rank)) == Some(pusher)
        })
        .collect()
}

fn square_of(index: u64) -> Square {
    Square::new((index % 64) as u8).unwrap_or(Square::A1)
}

/// Chebyshev distance: the number of king moves between two squares.
fn chebyshev(a: Square, b: Square) -> u8 {
    let file = a.file().index().abs_diff(b.file().index());
    let rank = a.rank().index().abs_diff(b.rank().index());
    u8::try_from(file.max(rank)).unwrap_or(u8::MAX)
}
