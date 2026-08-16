//! Boidboard — a durable boids experiment bench.
//!
//! Everything the application is made of lives in this library crate; the
//! binary (`src/main.rs`) is a thin shim that calls [`run`]. That split is what
//! lets `tests/` import the real handlers, models and repositories — a binary
//! crate cannot be imported by its own integration tests.

use autumn_harvest::prelude::*;
use autumn_harvest_plugin::HarvestPlugin;
use autumn_web::migrate::{EmbeddedMigrations, embed_migrations};
use autumn_web::prelude::*;

pub mod models;
pub mod presets;
pub mod repositories;
pub mod routes;
pub mod schema;
pub mod views;
pub mod workflow;

// The `workflows![]` / `activities![]` macros take bare identifiers and resolve
// the companion `*_info()` items the `#[workflow]` / `#[activity]` macros
// generate alongside each function, so both names must be in scope here.
#[allow(unused_imports)]
use workflow::{
    __autumn_activity_info_finalize_run, __autumn_activity_info_record_signal,
    __autumn_activity_info_simulate_batch, __autumn_workflow_info_simulation_workflow,
    finalize_run, record_signal, simulate_batch, simulation_workflow,
};

/// Embedded Diesel migrations for the Boidboard schema.
///
/// Applied automatically on the `dev` profile by the Autumn app builder; run
/// explicitly in tests via [`autumn_web::migrate::run_pending`].
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

/// Version stamp of the simulation kernel, recorded on every run for provenance
/// (AC-38).
///
/// Tracks the workspace version, which `boids-core` and `boidboard` share, so a
/// release that changes simulation results also changes what runs claim to have
/// been produced by. A kernel change that alters results but does *not* bump the
/// workspace version would make two runs falsely comparable — so bump it.
pub const KERNEL_VERSION: &str = concat!("boids-core@", env!("CARGO_PKG_VERSION"));

#[get("/")]
#[public]
pub async fn index() -> &'static str {
    "Boidboard"
}

/// Every route the application mounts.
///
/// Kept as a function rather than inlined into [`run`] so integration tests can
/// boot the identical route table through `TestApp` instead of a copy of it.
pub fn all_routes() -> Vec<autumn_web::Route> {
    routes![
        index,
        routes::run_list,
        routes::new_run_form,
        routes::create_run,
        routes::cancel_run,
        routes::run_detail,
        routes::run_progress,
        routes::compare,
    ]
}

/// The Harvest plugin the application mounts.
///
/// Kept as a function for the same reason [`all_routes`] is: `tests/integration.rs`
/// boots the **identical** plugin through `TestApp` — same workflow, same
/// activities, same API mount — rather than a hand-copied lookalike that could
/// drift from what production runs. A plugin registered here but missing from
/// the test would make the end-to-end proof prove nothing.
///
/// The plugin injects the application's database pool into activity state as
/// `AppDbPool`, which is how `simulate_batch` reaches Postgres without the
/// workflow body ever performing IO itself, and installs an in-process
/// `WorkflowHandleClient` into `AppState` — the seam
/// [`routes::create_run`](crate::routes::create_run) starts runs through.
#[must_use]
pub fn harvest_plugin() -> HarvestPlugin {
    HarvestPlugin::new()
        .workflows(workflows![simulation_workflow])
        .activities(activities![simulate_batch, finalize_run, record_signal])
        .api("/api/harvest")
}

/// Build and run the Boidboard application.
pub async fn run() {
    autumn_web::app()
        .routes(all_routes())
        .migrations(MIGRATIONS)
        .plugin(harvest_plugin())
        .run()
        .await;
}
