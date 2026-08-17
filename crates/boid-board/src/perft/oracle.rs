//! The perft oracle: published node counts, and the parser for them.
//!
//! The fixture at `tests/fixtures/perft_oracle.txt` is the external standard this engine
//! is judged against. It was committed before any engine code existed, and it is embedded
//! at compile time rather than read at runtime so that deleting or renaming it is a
//! compile error across the whole workspace rather than a silently skipped test.
//!
//! See `docs/DECISIONS.md` D-0006, D-0007, D-0008.

use std::error::Error;
use std::fmt;

use crate::fen::{ClockField, FenError, FenField};
use crate::types::{CastlingRight, Colour};

/// The perft oracle fixture, embedded at compile time.
///
/// Declared exactly once in the workspace. A missing or renamed fixture is a **compile
/// error**, not a skipped test — which is what makes the fixture load-bearing rather than
/// decorative (D-0007).
///
/// # Examples
///
/// ```
/// use boid_board::perft::oracle::{ORACLE_TEXT, parse};
///
/// let cases = parse(ORACLE_TEXT).expect("the committed fixture parses");
/// assert_eq!(cases.len(), 7);
/// assert_eq!(cases[0].id, "startpos");
/// assert_eq!(cases[0].nodes_at(3), Some(8902));
/// ```
pub const ORACLE_TEXT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/perft_oracle.txt"
));

/// How much this project trusts a single published node count.
///
/// The published table mixes exhaustively-computed counts with values the wiki itself
/// sources from forum estimate threads, so trust is recorded per depth, not per position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// `v` — independently re-derived with Stockfish 16 by this project.
    Verified,
    /// `p` — published only; not corroborated here.
    Published,
}

/// One `depth:nodes<flag>` datum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthCount {
    /// Ply depth searched.
    pub depth: u32,
    /// Leaf nodes at that depth. `u64`, because perft outgrows `u32` at Kiwipete depth 6.
    pub nodes: u64,
    /// Whether this project re-derived the count or merely copied it.
    pub provenance: Provenance,
}

/// One position from the oracle, with all of its published depths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerftCase<'a> {
    /// Stable identifier, unique within the fixture (`startpos`, `kiwipete`, ...).
    pub id: &'a str,
    /// The FEN exactly as stored in the fixture. May have four fields, not six — the
    /// published Kiwipete FEN omits the halfmove and fullmove counters (D-0008).
    pub fen: &'a str,
    /// Published counts, ascending by depth.
    pub counts: Vec<DepthCount>,
}

impl PerftCase<'_> {
    /// The published node count at `depth`, if the fixture carries one.
    #[must_use]
    pub fn nodes_at(&self, depth: u32) -> Option<u64> {
        self.counts
            .iter()
            .find(|c| c.depth == depth)
            .map(|c| c.nodes)
    }

    /// How many space-separated fields this position's FEN has: 6 normally, 4 for the
    /// published Kiwipete FEN, which omits the halfmove and fullmove counters (D-0008).
    #[must_use]
    pub fn fen_fields(&self) -> usize {
        self.fen.split(' ').count()
    }

    /// The counts this project independently re-derived, cheapest first.
    ///
    /// This is the replay set for a differential harness: [`Provenance::Published`] counts
    /// are documentation and must not be treated as checked.
    pub fn verified_counts(&self) -> impl Iterator<Item = &DepthCount> {
        self.counts
            .iter()
            .filter(|c| c.provenance == Provenance::Verified)
    }
}

/// Why an oracle fixture was rejected.
///
/// Every variant names the offending line, because a fixture error found in issue #6
/// will be read by someone who is at that moment convinced their move generator is broken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleError {
    /// A data line did not have exactly three `|`-separated fields.
    MalformedLine {
        /// 1-based line number in the fixture.
        line: usize,
        /// Number of `|`-separated fields actually found.
        fields: usize,
    },
    /// A depth token was not of the form `depth:nodes<flag>`, or its depth was not a
    /// number.
    MalformedCountToken {
        /// 1-based line number in the fixture.
        line: usize,
        /// The offending token, verbatim.
        token: String,
    },
    /// A node count contained a byte that is not an ASCII digit.
    NonNumericCount {
        /// 1-based line number in the fixture.
        line: usize,
        /// The offending token, verbatim.
        token: String,
    },
    /// A node count did not fit in `u64`.
    CountOverflowsU64 {
        /// 1-based line number in the fixture.
        line: usize,
        /// The offending token, verbatim.
        token: String,
    },
    /// A `depth:nodes` token was missing its trailing provenance flag, or carried an
    /// unrecognised one.
    MissingProvenanceFlag {
        /// 1-based line number in the fixture.
        line: usize,
        /// The offending token, verbatim.
        token: String,
    },
    /// The same depth appeared twice for one position.
    DuplicateDepth {
        /// 1-based line number in the fixture.
        line: usize,
        /// The repeated depth.
        depth: u32,
    },
    /// Depths were not contiguous and ascending.
    DepthGap {
        /// 1-based line number in the fixture.
        line: usize,
        /// The depth preceding the gap.
        previous: u32,
        /// The depth that followed it.
        found: u32,
    },
    /// Two positions shared an identifier.
    DuplicateId {
        /// 1-based line number in the fixture.
        line: usize,
        /// The repeated identifier.
        id: String,
    },
    /// A FEN failed structural validation.
    MalformedFen {
        /// 1-based line number in the fixture.
        line: usize,
        /// What was wrong with it. A typed [`FenError`] since issue #4 discharged D-0014's
        /// deferral: a caller that wants to branch on *why* a FEN was rejected now can.
        reason: FenError,
    },
    /// A position carried no depth data at all.
    NoCounts {
        /// 1-based line number in the fixture.
        line: usize,
    },
    /// A depth was numeric but did not fit in `u32`.
    DepthOutOfRange {
        /// 1-based line number in the fixture.
        line: usize,
        /// The offending token, verbatim.
        token: String,
    },
    /// An id did not match the documented `[a-z0-9-]+` grammar.
    MalformedId {
        /// 1-based line number in the fixture.
        line: usize,
        /// The offending id, verbatim.
        id: String,
    },
    /// The fixture contained no positions.
    NoEntries,
}

impl fmt::Display for OracleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedLine { line, fields } => write!(
                f,
                "line {line}: expected 3 '|'-separated fields (id | fen | counts), found {fields}"
            ),
            Self::MalformedCountToken { line, token } => write!(
                f,
                "line {line}: token {token:?} must be of the form depth:nodes<flag>"
            ),
            Self::NonNumericCount { line, token } => {
                write!(
                    f,
                    "line {line}: node count is not all ASCII digits: {token:?}"
                )
            }
            Self::CountOverflowsU64 { line, token } => {
                write!(f, "line {line}: node count does not fit in u64: {token:?}")
            }
            Self::MissingProvenanceFlag { line, token } => write!(
                f,
                "line {line}: token {token:?} must end in a provenance flag, 'v' or 'p'"
            ),
            Self::DuplicateDepth { line, depth } => {
                write!(f, "line {line}: depth {depth} appears more than once")
            }
            Self::DepthGap {
                line,
                previous,
                found,
            } => write!(
                f,
                "line {line}: depths must be contiguous and ascending; {previous} is followed by {found}"
            ),
            Self::DuplicateId { line, id } => {
                write!(f, "line {line}: duplicate position id {id:?}")
            }
            Self::MalformedFen { line, reason } => {
                write!(f, "line {line}: malformed FEN: {reason}")
            }
            Self::NoCounts { line } => write!(f, "line {line}: position has no depth counts"),
            Self::DepthOutOfRange { line, token } => {
                write!(f, "line {line}: depth does not fit in u32: {token:?}")
            }
            Self::MalformedId { line, id } => write!(
                f,
                "line {line}: id {id:?} must match [a-z0-9-]+, as the fixture header documents"
            ),
            Self::NoEntries => write!(f, "oracle fixture contains no positions"),
        }
    }
}

impl Error for OracleError {
    /// The underlying [`FenError`], for a caller walking the chain.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MalformedFen { reason, .. } => Some(reason),
            _ => None,
        }
    }
}

/// Parse the oracle fixture into one [`PerftCase`] per position, in file order.
///
/// Comment lines (first non-whitespace byte `#`) and blank lines are ignored.
///
/// # Errors
///
/// Returns [`OracleError`] if any line violates the grammar documented in the fixture
/// header: wrong field count, a non-numeric or oversized node count, a missing provenance
/// flag, duplicate or non-contiguous depths, a duplicate id, or a structurally invalid FEN.
///
/// # Examples
///
/// ```
/// use boid_board::perft::oracle::{self, Provenance};
///
/// // Built by concatenation rather than as a multi-line literal: rustdoc strips lines
/// // beginning with '#' from doc examples, which would silently eat the comment line
/// // this example exists to demonstrate.
/// let text = [
///     "# comments and blank lines are ignored",
///     "",
///     "startpos | rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 | 1:20v 2:400p",
/// ]
/// .join("\n");
///
/// let cases = oracle::parse(&text)?;
/// assert_eq!(cases.len(), 1);
/// assert_eq!(cases[0].nodes_at(1), Some(20));
/// assert_eq!(cases[0].counts[0].provenance, Provenance::Verified);
/// assert_eq!(cases[0].counts[1].provenance, Provenance::Published);
/// # Ok::<(), oracle::OracleError>(())
/// ```
///
/// A node count must be digits only. Thousands separators, as the wiki prints them, are
/// an error rather than something to silently rewrite:
///
/// ```
/// use boid_board::perft::oracle::{self, OracleError};
///
/// let bad = "x | 4k3/8/8/8/8/8/8/4K3 w - - 0 1 | 1:1,486v";
/// assert!(matches!(
///     oracle::parse(bad),
///     Err(OracleError::NonNumericCount { .. })
/// ));
/// ```
pub fn parse(text: &str) -> Result<Vec<PerftCase<'_>>, OracleError> {
    let mut cases: Vec<PerftCase<'_>> = Vec::new();

    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let fields: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        if fields.len() != 3 {
            return Err(OracleError::MalformedLine {
                line,
                fields: fields.len(),
            });
        }
        let (id, fen, counts_field) = (fields[0], fields[1], fields[2]);

        // The header documents `id  [a-z0-9-]+`. A documented grammar that is not enforced
        // is a comment, not a rule.
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(OracleError::MalformedId {
                line,
                id: id.to_owned(),
            });
        }

        if cases.iter().any(|c| c.id == id) {
            return Err(OracleError::DuplicateId {
                line,
                id: id.to_owned(),
            });
        }

        validate_fen(fen).map_err(|reason| OracleError::MalformedFen { line, reason })?;

        let counts = parse_counts(line, counts_field)?;
        if counts.is_empty() {
            return Err(OracleError::NoCounts { line });
        }

        cases.push(PerftCase { id, fen, counts });
    }

    if cases.is_empty() {
        return Err(OracleError::NoEntries);
    }
    Ok(cases)
}

/// Check that `fen` is structurally well-formed.
///
/// Structural, plus the one plausibility rule that costs nothing and catches real
/// transcription damage: **exactly one king per side**. That rule is strictly speaking
/// about legality rather than structure, and it is applied deliberately — a FEN with two
/// white kings is well-formed but is never a chess position, and in an oracle fixture it
/// means a rank was mistyped.
///
/// Everything beyond that is out of scope and belongs to issue #4, which needs a
/// `Position` to express it: side-not-to-move in check, pawns on the first or eighth rank,
/// castling rights without the matching rook, en passant squares with no pawn to capture.
///
/// Accepts four-field FENs as well as six-field ones, because the published Kiwipete FEN
/// omits the halfmove and fullmove counters and is stored as published (D-0008).
///
/// # Errors
///
/// Returns a [`FenError`], which the caller wraps with the line number. Typed since issue
/// #4 discharged D-0014's deferral -- as a fixture validator this only ever needed a
/// message, but the same vocabulary is what `Board::from_fen` speaks, and one vocabulary
/// with two implementations is what makes the two checkable against each other.
///
/// # Examples
///
/// ```
/// use boid_board::perft::oracle::validate_fen;
///
/// assert!(validate_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").is_ok());
///
/// // Four fields is valid: the published Kiwipete FEN omits the move counters.
/// assert!(validate_fen("4k3/8/8/8/8/8/8/4K3 w - -").is_ok());
///
/// // "4K4" describes nine files.
/// assert!(validate_fen("4k3/8/8/8/8/8/8/4K4 w - - 0 1").is_err());
///
/// // A board needs exactly one king per side.
/// assert!(validate_fen("8/8/8/8/8/8/8/4K3 w - - 0 1").is_err());
/// ```
pub fn validate_fen(fen: &str) -> Result<(), FenError> {
    let fields: Vec<&str> = fen.split(' ').collect();
    if fields.len() != 4 && fields.len() != 6 {
        return Err(FenError::FieldCount {
            found: fields.len(),
        });
    }

    // --- 1. piece placement ---
    let placement = fields.first().copied().unwrap_or_default();
    let ranks: Vec<&str> = placement.split('/').collect();
    if ranks.len() != 8 {
        return Err(FenError::RankCount { found: ranks.len() });
    }
    let (mut white_kings, mut black_kings) = (0u8, 0u8);
    for (i, rank) in ranks.iter().enumerate() {
        let rank_number = 8u8.saturating_sub(u8::try_from(i).unwrap_or(u8::MAX));
        let mut files = 0u8;
        let mut previous_was_digit = false;
        for ch in rank.chars() {
            if ch.is_ascii_digit() {
                if !('1'..='8').contains(&ch) {
                    return Err(FenError::DigitOutOfRange {
                        rank: rank_number,
                        ch,
                    });
                }
                if previous_was_digit {
                    return Err(FenError::ConsecutiveDigits { rank: rank_number });
                }
                previous_was_digit = true;
            } else {
                previous_was_digit = false;
            }
            match ch {
                '1'..='8' => {
                    files = files
                        .saturating_add(u8::try_from(ch as u32 - '0' as u32).unwrap_or(u8::MAX));
                }
                'K' => {
                    white_kings = white_kings.saturating_add(1);
                    files = files.saturating_add(1);
                }
                'k' => {
                    black_kings = black_kings.saturating_add(1);
                    files = files.saturating_add(1);
                }
                'p' | 'n' | 'b' | 'r' | 'q' | 'P' | 'N' | 'B' | 'R' | 'Q' => {
                    files = files.saturating_add(1);
                }
                _ => {
                    return Err(FenError::PieceChar {
                        rank: rank_number,
                        ch,
                    });
                }
            }
        }
        if files != 8 {
            return Err(FenError::RankWidth {
                rank: rank_number,
                files,
            });
        }
    }
    for (side, found) in [(Colour::White, white_kings), (Colour::Black, black_kings)] {
        if found != 1 {
            return Err(FenError::KingCount { side, found });
        }
    }

    // --- 2. side to move ---
    let side_field = fields.get(1).copied().unwrap_or_default();
    let side_to_move = match side_field.chars().next() {
        Some(ch) if side_field.chars().count() == 1 => {
            Colour::from_char(ch).ok_or(FenError::SideToMove { found: Some(ch) })?
        }
        found => return Err(FenError::SideToMove { found }),
    };

    // --- 3. castling availability ---
    let castling = fields.get(2).copied().unwrap_or_default();
    if castling != "-" {
        if castling.is_empty() {
            return Err(FenError::EmptyField {
                field: FenField::Castling,
            });
        }
        let mut seen = String::new();
        for ch in castling.chars() {
            if CastlingRight::from_char(ch).is_none() {
                if ch.is_ascii_alphabetic() && matches!(ch.to_ascii_lowercase(), 'a'..='h') {
                    return Err(FenError::CastlingShredderNotation { ch });
                }
                return Err(FenError::CastlingChar { ch });
            }
            if seen.contains(ch) {
                return Err(FenError::CastlingDuplicate { ch });
            }
            seen.push(ch);
        }
    }

    // --- 4. en passant target ---
    let ep = fields.get(3).copied().unwrap_or_default();
    if ep != "-" {
        let mut chars = ep.chars();
        let (Some(file), Some(rank), None) = (chars.next(), chars.next(), chars.next()) else {
            return Err(FenError::EnPassantSyntax {
                len: ep.chars().count(),
            });
        };
        if !('a'..='h').contains(&file) || (rank != '3' && rank != '6') {
            return Err(FenError::EnPassantSquare { file, rank });
        }
        // The rank follows from the side to move: after white pushes a pawn two squares
        // the target is on rank 3 and it is black's turn, and vice versa. A contradiction
        // here is decidable without a board, so it is caught here rather than in #4.
        let expected_rank = match side_to_move {
            Colour::White => '6',
            Colour::Black => '3',
        };
        if rank != expected_rank {
            return Err(FenError::EnPassantRankContradictsSideToMove { rank, side_to_move });
        }
    }

    // --- 5, 6. halfmove clock and fullmove number, when present ---
    if fields.len() == 6 {
        for (which, value) in [
            (
                ClockField::Halfmove,
                fields.get(4).copied().unwrap_or_default(),
            ),
            (
                ClockField::Fullmove,
                fields.get(5).copied().unwrap_or_default(),
            ),
        ] {
            match value.chars().find(|ch| !ch.is_ascii_digit()) {
                Some(ch) => return Err(FenError::ClockNotANumber { field: which, ch }),
                None if value.is_empty() => {
                    return Err(FenError::ClockNotANumber {
                        field: which,
                        ch: ' ',
                    });
                }
                None => {}
            }
        }
    }

    Ok(())
}

/// Parse one position's whitespace-separated `depth:nodes<flag>` tokens.
///
/// Depths must be ascending and contiguous with no duplicates: a gap means a published
/// row was dropped in transcription, which is exactly the kind of quiet omission this
/// fixture exists to prevent.
fn parse_counts(line: usize, field: &str) -> Result<Vec<DepthCount>, OracleError> {
    let mut counts: Vec<DepthCount> = Vec::new();

    for token in field.split_whitespace() {
        let malformed = || OracleError::MalformedCountToken {
            line,
            token: token.to_owned(),
        };

        let (depth_str, remainder) = token.split_once(':').ok_or_else(malformed)?;
        if depth_str.is_empty() || !depth_str.bytes().all(|b| b.is_ascii_digit()) {
            return Err(malformed());
        }
        // A depth wider than u32 is a malformed token, but say so precisely rather than
        // through MalformedCountToken's "must be of the form depth:nodes<flag>" message,
        // which would misdescribe a token that has exactly that form.
        let depth: u32 = depth_str
            .parse()
            .map_err(|_| OracleError::DepthOutOfRange {
                line,
                token: token.to_owned(),
            })?;

        // The provenance flag is the final byte. Its absence is an error rather than a
        // default: an unflagged count would silently claim whichever trust level the
        // reader assumed.
        let flag = remainder.chars().next_back().ok_or_else(malformed)?;
        let provenance = match flag {
            'v' => Provenance::Verified,
            'p' => Provenance::Published,
            _ => {
                return Err(OracleError::MissingProvenanceFlag {
                    line,
                    token: token.to_owned(),
                });
            }
        };
        let digits = &remainder[..remainder.len() - flag.len_utf8()];

        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(OracleError::NonNumericCount {
                line,
                token: token.to_owned(),
            });
        }
        let nodes: u64 = digits.parse().map_err(|_| OracleError::CountOverflowsU64 {
            line,
            token: token.to_owned(),
        })?;

        if counts.iter().any(|c| c.depth == depth) {
            return Err(OracleError::DuplicateDepth { line, depth });
        }
        if let Some(previous) = counts.last().map(|c| c.depth)
            && Some(depth) != previous.checked_add(1)
        {
            return Err(OracleError::DepthGap {
                line,
                previous,
                found: depth,
            });
        }

        counts.push(DepthCount {
            depth,
            nodes,
            provenance,
        });
    }

    // Adjacent-pair contiguity cannot see a row dropped from the FRONT: 2,3,4 is as
    // contiguous as 1,2,3. Every published table starts at depth 0 or 1, so anchor it.
    if let Some(first) = counts.first()
        && first.depth > 1
    {
        return Err(OracleError::DepthGap {
            line,
            previous: 0,
            found: first.depth,
        });
    }

    Ok(counts)
}
