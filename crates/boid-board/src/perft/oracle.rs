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

/// The perft oracle fixture, embedded at compile time.
///
/// Declared exactly once in the workspace. A missing or renamed fixture is a **compile
/// error**, not a skipped test — which is what makes the fixture load-bearing rather than
/// decorative (D-0007).
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
        /// What was wrong with it.
        reason: String,
    },
    /// A position carried no depth data at all.
    NoCounts {
        /// 1-based line number in the fixture.
        line: usize,
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
            Self::NoEntries => write!(f, "oracle fixture contains no positions"),
        }
    }
}

impl Error for OracleError {}

/// Parse the oracle fixture into one [`PerftCase`] per position, in file order.
///
/// Comment lines (first non-whitespace byte `#`) and blank lines are ignored.
///
/// # Errors
///
/// Returns [`OracleError`] if any line violates the grammar documented in the fixture
/// header: wrong field count, a non-numeric or oversized node count, a missing provenance
/// flag, duplicate or non-contiguous depths, a duplicate id, or a structurally invalid FEN.
pub fn parse(text: &str) -> Result<Vec<PerftCase<'_>>, OracleError> {
    let _ = text;
    todo!("oracle::parse is not implemented yet")
}
