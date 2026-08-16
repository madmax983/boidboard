//! `scenarios` — a named, immutable-once-used parameter set.

use crate::schema::scenarios;

/// A named simulation parameter set.
///
/// `config` is opaque `JSONB`: the persistence layer never interprets it, which
/// keeps this crate's schema independent of the kernel's types. `config_hash`
/// is [`canonical_config_hash`](super::canonical_config_hash) of `config`.
///
/// A scenario is editable — but editing it can never reach a run that already
/// started, because a run copies the config into its own `config_snapshot`
/// (AC-39).
#[autumn_web::model]
pub struct Scenario {
    #[id]
    pub id: i64,
    pub name: String,
    pub config: serde_json::Value,
    #[indexed]
    pub config_hash: String,
    #[default]
    pub created_at: chrono::NaiveDateTime,
}
