//! A deliberately naive "is this square attacked" routine, for the test suite only.
//!
//! The position generator has to reject positions where the side *not* to move is in check,
//! because such a position cannot arise in a game and calling it "legal" in AC1's evidence
//! would be a lie. Deciding that needs attack information — and attack generation is issue
//! #5's, built on magic bitboards.
//!
//! So this lives in the test suite, not in `src/`. It answers exactly one question, it
//! generates no moves, it knows nothing about legality, and it is written for obviousness
//! rather than speed: rays are walked one square at a time through `File` and `Rank`, never
//! by adding 7 or 9 to a square index, so a file wrap is impossible rather than merely
//! unlikely.
//!
//! Its own correctness is pinned by a hand-written table in `fen_proptest.rs`, committed
//! before the routine was written — the posture D-0006 takes towards the perft fixture,
//! applied to sixty lines of test support.
//!
//! Issue #5 should retire this by differential testing: `naive_attacks` against the magic
//! attack tables over every square and a spread of occupancies.

use boid_board::board::Board;
use boid_board::types::{Colour, File, Piece, PieceKind, Rank, Square};

/// The eight knight moves, as (file delta, rank delta).
const KNIGHT: [(i8, i8); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];

/// The four diagonal directions.
const DIAGONALS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, -1), (-1, 1)];

/// The four orthogonal directions.
const ORTHOGONALS: [(i8, i8); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];

/// The square `delta` away from `from`, or `None` if that leaves the board.
///
/// File and rank are stepped separately, so a move off the a-file cannot silently reappear
/// on the h-file — the bug that would quietly widen or narrow "legal" in the generator.
fn step(from: Square, delta: (i8, i8)) -> Option<Square> {
    let file = i8::try_from(from.file().index())
        .ok()?
        .checked_add(delta.0)?;
    let rank = i8::try_from(from.rank().index())
        .ok()?
        .checked_add(delta.1)?;
    let file = File::from_index(u8::try_from(file).ok()?)?;
    let rank = Rank::from_index(u8::try_from(rank).ok()?)?;
    Some(Square::from_file_rank(file, rank))
}

/// Whether `by` attacks `square` — occupied or not, legally or not.
///
/// "Attacks" in the pseudo-legal sense: pins and checks are not considered, which is exactly
/// what a check test needs.
pub fn is_square_attacked(board: &Board, square: Square, by: Colour) -> bool {
    // Pawns. A White pawn attacks diagonally *upward*, so a square is attacked by a White
    // pawn standing one rank below it.
    let pawn_rank_delta = match by {
        Colour::White => -1,
        Colour::Black => 1,
    };
    for file_delta in [-1, 1] {
        if let Some(from) = step(square, (file_delta, pawn_rank_delta))
            && board.piece_at(from) == Some(Piece::new(by, PieceKind::Pawn))
        {
            return true;
        }
    }

    for delta in KNIGHT {
        if let Some(from) = step(square, delta)
            && board.piece_at(from) == Some(Piece::new(by, PieceKind::Knight))
        {
            return true;
        }
    }

    for delta in DIAGONALS.into_iter().chain(ORTHOGONALS) {
        if let Some(from) = step(square, delta)
            && board.piece_at(from) == Some(Piece::new(by, PieceKind::King))
        {
            return true;
        }
    }

    for (directions, sliders) in [
        (DIAGONALS, [PieceKind::Bishop, PieceKind::Queen]),
        (ORTHOGONALS, [PieceKind::Rook, PieceKind::Queen]),
    ] {
        for delta in directions {
            let mut current = square;
            loop {
                let Some(next) = step(current, delta) else {
                    break;
                };
                current = next;
                match board.piece_at(current) {
                    None => continue,
                    Some(piece) => {
                        if piece.colour() == by && sliders.contains(&piece.kind()) {
                            return true;
                        }
                        break; // Any other piece blocks the ray.
                    }
                }
            }
        }
    }

    false
}

/// Whether `colour`'s king is attacked. A kingless board is never in check: the editing
/// primitives can build one, and this is not the place to complain about it.
pub fn is_in_check(board: &Board, colour: Colour) -> bool {
    match board.king_square(colour) {
        Some(king) => is_square_attacked(board, king, colour.flip()),
        None => false,
    }
}
