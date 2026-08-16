//! Engines that can produce perft counts, and a driver for an external Stockfish process.
//!
//! This is issue #6's Stockfish differential harness with our own engine side still
//! absent. It is delivered a phase early because the *oracle* half can be finished and
//! proven now, and because issues #6, #9 and #16 all consume it.
//!
//! See `docs/DECISIONS.md` D-0015.

use std::error::Error;
use std::fmt;
use std::io::{self, Write};
use std::process::{Command, Stdio};

/// Environment variable naming the Stockfish binary. Defaults to `stockfish` on `PATH`.
pub const STOCKFISH_PATH_VAR: &str = "BOID_STOCKFISH";

/// A source of perft node counts.
///
/// Implemented by the external Stockfish driver now, and by boidboard's own move generator
/// in issue #6. The differential harness is then just: run both, compare.
pub trait PerftEngine {
    /// Count leaf nodes at `depth` plies from `fen`.
    ///
    /// `depth` must be at least 1: perft(0) is 1 by definition and no engine is needed to
    /// say so, while `go perft 0` is not a meaningful UCI command.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the engine could not be run, produced no node count, or
    /// was asked for a depth below 1.
    fn perft(&self, fen: &str, depth: u32) -> Result<u64, EngineError>;
}

/// Why an engine failed to produce a node count.
#[derive(Debug)]
pub enum EngineError {
    /// `depth` was below 1.
    DepthTooLow(u32),
    /// The FEN contained a non-ASCII byte.
    ///
    /// Its own variant because it is a *silent* failure otherwise: Stockfish's tokeniser
    /// is ASCII-only, so a FEN separated by U+00A0 parses as a different legal position
    /// and reports `Nodes searched: 0` rather than an error (D-0008).
    NonAsciiFen(String),
    /// The engine could not be spawned or communicated with.
    Io(io::Error),
    /// The engine ran but printed no `Nodes searched:` line.
    NoNodeCount {
        /// The tail of what it did print, for diagnosis.
        output: String,
    },
    /// The `Nodes searched:` line did not carry a parseable count.
    UnparseableCount(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DepthTooLow(d) => {
                write!(f, "perft depth must be at least 1, got {d}")
            }
            Self::NonAsciiFen(fen) => write!(
                f,
                "FEN contains a non-ASCII byte, which Stockfish would silently mis-parse \
                 into a different position: {fen:?}"
            ),
            Self::Io(e) => write!(f, "engine process error: {e}"),
            Self::NoNodeCount { output } => write!(
                f,
                "engine printed no 'Nodes searched:' line; last output was: {output}"
            ),
            Self::UnparseableCount(s) => {
                write!(f, "could not parse a node count from {s:?}")
            }
        }
    }
}

impl Error for EngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for EngineError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Drives an external Stockfish process over UCI, one process per query.
#[derive(Debug, Clone)]
pub struct StockfishEngine {
    path: String,
}

impl StockfishEngine {
    /// An engine at an explicit path.
    #[must_use]
    pub fn at(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// An engine located by [`STOCKFISH_PATH_VAR`], falling back to `stockfish` on `PATH`.
    #[must_use]
    pub fn from_env() -> Self {
        Self::at(std::env::var(STOCKFISH_PATH_VAR).unwrap_or_else(|_| "stockfish".to_owned()))
    }

    /// The binary this engine will run.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Whether the binary can be run at all.
    ///
    /// Used by tests to decide between skipping loudly and failing.
    #[must_use]
    pub fn is_available(&self) -> bool {
        Command::new(&self.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .is_ok_and(|mut child| {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(b"quit\n");
                }
                child.wait().is_ok()
            })
    }
}

impl Default for StockfishEngine {
    fn default() -> Self {
        Self::from_env()
    }
}

impl PerftEngine for StockfishEngine {
    fn perft(&self, fen: &str, depth: u32) -> Result<u64, EngineError> {
        let _ = (fen, depth);
        todo!("StockfishEngine::perft is not implemented yet")
    }
}
