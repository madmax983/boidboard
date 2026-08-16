//! `run_signals` — provenance of operator interventions (AC-34).

use crate::schema::run_signals;

/// The kinds of signal an operator can send to a live run.
pub mod kind {
    /// Change simulation parameters for subsequent batches (AC-34).
    pub const STEER: &str = "steer";
    /// Stop at the next batch boundary (AC-33).
    pub const CANCEL: &str = "cancel";

    /// Every valid signal kind.
    pub const ALL: [&str; 2] = [STEER, CANCEL];
}

/// A recorded operator intervention.
///
/// Recording the signal — not just applying it — is what lets a run explain its
/// own behaviour after the fact: a run whose parameters changed at tick 300
/// carries the evidence of why.
#[autumn_web::model]
pub struct RunSignal {
    #[id]
    pub id: i64,
    #[indexed]
    pub run_id: i64,
    pub tick: i32,
    pub kind: String,
    pub payload: serde_json::Value,
    #[default]
    pub created_at: chrono::NaiveDateTime,
}
