//! Repository for [`Scenario`](crate::models::Scenario).

use crate::models::{NewScenario, Scenario, UpdateScenario};
use crate::schema::scenarios;

/// Generated CRUD (`find_by_id`, `find_all`, `save`, `update`, `delete_by_id`,
/// `count`, `exists_by_id`, `paginate`) plus the derived lookups below, on
/// `PgScenarioRepository`.
#[autumn_web::repository(Scenario)]
pub trait ScenarioRepository {
    /// Every scenario sharing a canonical config hash — the "have I run this
    /// exact parameter set before?" query.
    fn find_by_config_hash(config_hash: String) -> Vec<Scenario>;
}
