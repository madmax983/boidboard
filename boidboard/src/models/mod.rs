//! Persistence models.
//!
//! Simulation configuration, agent state and metrics are stored as `JSONB`
//! (`serde_json::Value`) so this layer carries no dependency on the kernel
//! crate's types — `boids-core` stays swappable and the schema stays stable
//! across kernel refactors.
//!
//! Each `#[autumn_web::model]` struct generates three sibling types used
//! throughout the app: `New{Model}` (insert), `Update{Model}` (a `Patch`-based
//! partial update) and `{Model}::factory()`.

pub mod config_hash;
pub mod frame;
pub mod run;
pub mod run_signal;
pub mod scenario;

pub use config_hash::canonical_config_hash;
pub use frame::{Frame, NewFrame, UpdateFrame};
pub use run::{NewRun, Run, UpdateRun};
pub use run_signal::{NewRunSignal, RunSignal, UpdateRunSignal};
pub use scenario::{NewScenario, Scenario, UpdateScenario};
