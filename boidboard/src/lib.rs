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

pub mod analysis;
pub mod models;
pub mod presets;
pub mod reconcile;
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

/// Where the Harvest management API is mounted **when it is mounted at all**.
///
/// See [`harvest_plugin`] for why that is not the default.
pub const HARVEST_API_PATH: &str = "/api/harvest";

/// Environment variable that opts the Harvest management API in, and supplies
/// the bearer token every request to it must present.
///
/// Read from the process environment, falling back to autumn-web's `.env`
/// overlay (`.env`, `.env.local`, `.env.{profile}`, `.env.{profile}.local`), so
/// the token lives where a secret belongs — an ignored file or the deployment
/// environment — rather than in the tracked `autumn.toml`. Unset (or empty)
/// means the management API is not mounted.
pub const HARVEST_ADMIN_TOKEN_ENV: &str = "BOIDBOARD_HARVEST_ADMIN_TOKEN";

/// The configured Harvest management-API token, or `None` when the operator has
/// not opted in.
///
/// A real environment variable wins over a `.env` entry, matching how autumn-web
/// layers the two everywhere else. Whitespace-only is treated as unset: a
/// deployment that exports an empty variable means "off", not "the empty token
/// unlocks the engine".
#[must_use]
pub fn harvest_admin_api_token() -> Option<String> {
    if let Ok(token) = std::env::var(HARVEST_ADMIN_TOKEN_ENV)
        && !token.trim().is_empty()
    {
        return Some(token);
    }
    autumn_web::dotenv::resolve_process_dotenv()
        .ok()?
        .into_iter()
        .find(|(key, _)| key == HARVEST_ADMIN_TOKEN_ENV)
        .map(|(_, value)| value)
        .filter(|token| !token.trim().is_empty())
}

/// The workflow and activity registrations every configuration shares.
fn harvest_runtime() -> HarvestPlugin {
    HarvestPlugin::new()
        .workflows(workflows![simulation_workflow])
        .activities(activities![simulate_batch, finalize_run, record_signal])
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
///
/// # Why the management API is *not* mounted here
///
/// `HarvestPlugin::api(path)` mounts the full operator surface — start, cancel,
/// pause, resume, signal, plus an HTML admin UI and a read endpoint that dumps
/// the input and output of every execution — behind **no** middleware at all.
/// Boidboard has no accounts and no login flow, so there is no session for an
/// authentication layer to consult and nothing honest to gate that surface on.
/// A security review drove it anonymously and produced two unrecoverable
/// outcomes: a `steer` signal made a run simulate parameters its
/// `config_snapshot` does not record (so the detail page's reproducibility
/// fingerprint asserts something false), and an engine-level cancel killed an
/// execution *without* running `finalize_run`, stranding `runs.status` at
/// `running` forever — one anonymous POST per run, permanently.
///
/// Nothing in the application needs it. Runs are started in-process through the
/// `WorkflowHandleClient` this plugin installs, never over HTTP, and the only
/// operation a user performs is `POST /runs/{id}/cancel`, which sends the
/// graceful `cancel` **signal** the workflow finalizes on. So the default is to
/// not mount it: an unmounted route cannot be misconfigured.
///
/// An operator who genuinely needs the console opts in with
/// [`harvest_plugin_with_admin_api`], which is what `run` does when
/// [`HARVEST_ADMIN_TOKEN_ENV`] is set.
#[must_use]
pub fn harvest_plugin() -> HarvestPlugin {
    harvest_runtime()
}

/// The same plugin with the Harvest management API mounted at
/// [`HARVEST_API_PATH`], gated on a bearer token.
///
/// Every request to the nested router — and to any MCP tool route the plugin
/// generates — must present `Authorization: Bearer {admin_token}` or receive a
/// `401`. The comparison runs over the whole string in constant time, so a
/// caller cannot learn the token one byte at a time from response latency and a
/// value that merely *starts* with the token is refused.
///
/// This is the honest shape of "admin access" for an app with no users: a
/// single shared operator credential, supplied out of band, that the operator
/// can rotate by restarting with a different value. It is deliberately not a
/// login — it grants the whole engine surface to whoever holds it, so it
/// belongs in a secret store, not in `autumn.toml`.
#[must_use]
pub fn harvest_plugin_with_admin_api(admin_token: &str) -> HarvestPlugin {
    use autumn_web::reexports::axum::extract::Request;
    use autumn_web::reexports::axum::http::StatusCode;
    use autumn_web::reexports::axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
    use autumn_web::reexports::axum::middleware::{Next, from_fn};
    use autumn_web::reexports::axum::response::IntoResponse as _;

    let expected = admin_token.to_owned();
    harvest_runtime().api_with_auth(
        HARVEST_API_PATH,
        from_fn(move |request: Request, next: Next| {
            let expected = expected.clone();
            async move {
                let presented = request
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.strip_prefix("Bearer "))
                    .unwrap_or_default()
                    .trim()
                    .to_owned();

                if constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
                    next.run(request).await
                } else {
                    (
                        StatusCode::UNAUTHORIZED,
                        [(WWW_AUTHENTICATE, "Bearer realm=\"boidboard-harvest\"")],
                        "the Harvest management API requires an operator bearer token\n",
                    )
                        .into_response()
                }
            }
        }),
    )
}

/// Compare two byte strings without an early return.
///
/// Length is not a secret worth protecting here (it leaks through the header
/// anyway), but the *content* is: a byte-at-a-time `==` on a token lets a
/// patient caller recover it from response timing. Every byte is folded into
/// one accumulator, so the work done is the same whether the first byte or the
/// last is wrong.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (x, y) in a.iter().zip(b.iter()) {
        difference |= x ^ y;
    }
    difference == 0
}

/// Build and run the Boidboard application.
///
/// The Harvest management API is mounted only when [`HARVEST_ADMIN_TOKEN_ENV`]
/// supplies a token; see [`harvest_plugin`] for why that is the default.
pub async fn run() {
    let plugin = harvest_admin_api_token().map_or_else(harvest_plugin, |token| {
        harvest_plugin_with_admin_api(&token)
    });

    autumn_web::app()
        .routes(all_routes())
        .tasks(autumn_web::tasks![reconcile::reconcile_stranded_runs])
        .migrations(MIGRATIONS)
        .plugin(plugin)
        .run()
        .await;
}

#[cfg(test)]
mod tests {
    use super::{HARVEST_ADMIN_TOKEN_ENV, constant_time_eq, harvest_admin_api_token};

    #[test]
    fn the_management_api_is_opt_in_and_off_by_default() {
        // The whole C1 fix rests on this: with nothing configured there is no
        // token, so `run` mounts the plugin without its management API. A
        // change that made an absent variable resolve to `Some("")` would
        // silently mount the engine console behind an empty bearer.
        assert!(
            std::env::var(HARVEST_ADMIN_TOKEN_ENV).is_err(),
            "this test asserts the unconfigured case; something set \
             {HARVEST_ADMIN_TOKEN_ENV} in the test process"
        );
        assert!(harvest_admin_api_token().is_none());
    }

    #[test]
    fn the_token_comparison_accepts_only_the_whole_token() {
        assert!(constant_time_eq(b"s3cret", b"s3cret"));
        assert!(constant_time_eq(b"", b""));
        // A prefix must not pass: this is the difference between a gate and a
        // suggestion.
        assert!(!constant_time_eq(b"s3cret-and-more", b"s3cret"));
        assert!(!constant_time_eq(b"s3cre", b"s3cret"));
        // Same length, wrong content — the case a length check alone misses.
        assert!(!constant_time_eq(b"s3crev", b"s3cret"));
        assert!(!constant_time_eq(b"S3CRET", b"s3cret"));
    }
}
