//! FEN text utilities for the round-trip tests: a colour mirror, and a generator.
//!
//! Everything here works on **strings**, never on a [`Board`]. That is the whole point of
//! the module. A corpus produced by calling `Board::to_fen` would be, by construction,
//! exactly the set of FENs the parser accepts, and round-tripping it would assert nothing
//! about either half. A generator that emits text can disagree with the parser, and when it
//! does, one of them is wrong and the test says so.
//!
//! [`Board`]: boid_board::board::Board

use proptest::prelude::*;

/// Colour-mirror a FEN: reflect the board top to bottom, swap every piece's colour, swap
/// the side to move, swap and reorder the castling rights, and reflect the en-passant rank.
///
/// This is the transformation the committed fixture already contains an oracle for:
/// `position4-mirror` is published as the colour mirror of `position4`, with identical
/// perft counts at every depth. So `flip` can be checked against data this project did not
/// author, rather than against itself.
#[must_use]
pub fn flip(fen: &str) -> String {
    let fields: Vec<&str> = fen.split(' ').collect();
    assert!(
        fields.len() == 4 || fields.len() == 6,
        "flip expects a well-formed FEN, got {fen:?}"
    );

    let placement = fields[0]
        .split('/')
        .rev()
        .map(|rank| {
            rank.chars()
                .map(|ch| {
                    if ch.is_ascii_uppercase() {
                        ch.to_ascii_lowercase()
                    } else if ch.is_ascii_lowercase() {
                        ch.to_ascii_uppercase()
                    } else {
                        ch
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/");

    let side = match fields[1] {
        "w" => "b",
        "b" => "w",
        other => panic!("flip expects a side to move, got {other:?}"),
    };

    // Case-swapping alone would give "kqKQ"; the canonical order has to be restored.
    let castling = if fields[2] == "-" {
        "-".to_owned()
    } else {
        let mut out = String::new();
        for (from, to) in [('k', 'K'), ('q', 'Q'), ('K', 'k'), ('Q', 'q')] {
            if fields[2].contains(from) {
                out.push(to);
            }
        }
        out
    };

    let ep = if fields[3] == "-" {
        "-".to_owned()
    } else {
        let bytes = fields[3].as_bytes();
        let rank = match bytes[1] {
            b'3' => '6',
            b'6' => '3',
            other => panic!("flip expects an ep square on rank 3 or 6, got {other:?}"),
        };
        format!("{}{rank}", bytes[0] as char)
    };

    if fields.len() == 4 {
        format!("{placement} {side} {castling} {ep}")
    } else {
        format!(
            "{placement} {side} {castling} {ep} {} {}",
            fields[4], fields[5]
        )
    }
}

/// The plan a single generated position is built from.
///
/// Kept as plain integers so proptest can shrink a failure down to a small, readable case
/// rather than to an opaque board.
type Plan = (bool, u8, Vec<(u8, u8)>, bool, u8, u16, u16, (u8, u8));

/// A strategy producing FEN **text** in the language `Board::from_fen` accepts.
///
/// Deliberately not called "legal": whether the side not to move is in check cannot be
/// decided without attack tables, which are issue #5's. Every position here is
/// structurally valid and satisfies every rule the parser enforces — two kings, no
/// back-rank pawns, castling rights only where the king and rook are home, an en-passant
/// file only where a real double push could have left one — and D-0026 records the
/// narrowing rather than letting "legal" pass unexamined.
pub fn arbitrary_fen() -> impl Strategy<Value = String> {
    (
        // Half the corpus uses the home layout, which is the only way castling rights can
        // be present at all; without it the corpus would have exactly one castling mask.
        any::<bool>(),
        0u8..16,
        prop::collection::vec((0u8..64, 0u8..12), 0..28),
        any::<bool>(),
        // 0..=7 request an en-passant file; 8 requests none.
        0u8..9,
        // Reaches past 100 often enough that the "fifty-move rule territory" bucket is
        // populated, and past 255 often enough to exercise the packed field's width.
        0u16..=400,
        1u16..=600,
        (0u8..64, 0u8..64),
    )
        .prop_map(build)
}

/// Turn a plan into a FEN, enforcing every rule the parser enforces.
///
/// Conflicts are resolved by *skipping* the offending piece rather than by retrying, so the
/// function is total: every plan yields a valid FEN, and proptest never has to discard a
/// case. A rejection loop here would be the thing that quietly leaves a corpus of 200
/// near-identical two-king positions.
fn build(plan: Plan) -> String {
    let (home, castle_bits, pieces, black_to_move, ep_choice, halfmove, fullmove, kings) = plan;
    let mut squares: [Option<char>; 64] = [None; 64];

    // Ordinary pieces first, so the kings and castling rooks placed below overwrite them
    // and the castling precondition cannot be broken by a stray piece landing on a corner.
    const LETTERS: [char; 12] = ['P', 'p', 'N', 'n', 'B', 'b', 'R', 'r', 'Q', 'q', 'K', 'k'];
    for (square, piece) in pieces {
        let square = usize::from(square % 64);
        let letter = LETTERS[usize::from(piece % 12)];
        if letter == 'K' || letter == 'k' {
            continue; // exactly two kings, placed below
        }
        let rank = square / 8;
        if (letter == 'P' || letter == 'p') && (rank == 0 || rank == 7) {
            continue; // no pawn may stand on a back rank
        }
        if squares[square].is_none() {
            squares[square] = Some(letter);
        }
    }

    let rights = if home {
        squares[4] = Some('K');
        squares[60] = Some('k');
        let mut field = String::new();
        for (bit, corner, rook, letter) in [
            (0b0001u8, 7usize, 'R', 'K'),
            (0b0010, 0, 'R', 'Q'),
            (0b0100, 63, 'r', 'k'),
            (0b1000, 56, 'r', 'q'),
        ] {
            if castle_bits & bit != 0 {
                squares[corner] = Some(rook);
                field.push(letter);
            }
        }
        if field.is_empty() {
            "-".to_owned()
        } else {
            field
        }
    } else {
        // Kings anywhere, as long as they are neither on the same square nor adjacent —
        // adjacent kings are not a rule this parser enforces, but generating them would
        // make the corpus useless to issue #5 the moment it can tell.
        let white = usize::from(kings.0 % 64);
        let mut black = usize::from(kings.1 % 64);
        while black == white || kings_touch(white, black) {
            black = (black + 1) % 64;
        }
        squares[white] = Some('K');
        squares[black] = Some('k');
        "-".to_owned()
    };

    // En passant last: it needs three specific squares, two of which must be empty.
    let mut ep = "-".to_owned();
    if ep_choice < 8 {
        let file = usize::from(ep_choice);
        // White to move means Black has just pushed h7-h5: the target is on rank 6
        // (0-based 5), the pawn that pushed stands on rank 5 (0-based 4), and the square it
        // LEFT is rank 7 (0-based 6) -- not rank 8. Getting that last one wrong is what the
        // parser caught the first time this generator ran, and it is the reason the corpus
        // is built from text rather than from Board.
        let (target, pawn, origin, letter) = if black_to_move {
            (2 * 8 + file, 3 * 8 + file, 8 + file, 'P')
        } else {
            (5 * 8 + file, 4 * 8 + file, 6 * 8 + file, 'p')
        };
        let clear = |sq: usize| !matches!(squares[sq], Some('K' | 'k'));
        if squares[target].is_none() && squares[origin].is_none() && clear(pawn) {
            squares[pawn] = Some(letter);
            let rank = if black_to_move { '3' } else { '6' };
            ep = format!("{}{rank}", (b'a' + ep_choice) as char);
        }
    }

    let mut placement = String::with_capacity(72);
    for rank in (0..8usize).rev() {
        let mut empty = 0u32;
        for file in 0..8usize {
            match squares[rank * 8 + file] {
                Some(letter) => {
                    if empty > 0 {
                        placement.push_str(&empty.to_string());
                        empty = 0;
                    }
                    placement.push(letter);
                }
                None => empty += 1,
            }
        }
        if empty > 0 {
            placement.push_str(&empty.to_string());
        }
        if rank > 0 {
            placement.push('/');
        }
    }

    let side = if black_to_move { 'b' } else { 'w' };
    format!("{placement} {side} {rights} {ep} {halfmove} {fullmove}")
}

/// Whether two squares are a king's move apart.
fn kings_touch(a: usize, b: usize) -> bool {
    let (ar, af) = (a / 8, a % 8);
    let (br, bf) = (b / 8, b % 8);
    ar.abs_diff(br) <= 1 && af.abs_diff(bf) <= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `flip` that is itself wrong would tautologically certify the emitter it is used to
    /// check, so it is checked first, against data this project did not author: the
    /// committed fixture publishes `position4-mirror` as the colour mirror of `position4`.
    #[test]
    fn flip_of_position4_is_the_published_mirror_row() {
        let position4 = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
        let mirror = "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1";
        assert_eq!(flip(position4), mirror);
    }

    #[test]
    fn flip_is_an_involution() {
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "4k3/8/8/4p3/8/8/8/4K3 w - e6 0 1",
        ] {
            assert_eq!(flip(&flip(fen)), fen);
        }
    }
}
