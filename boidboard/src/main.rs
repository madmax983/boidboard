use autumn_harvest::prelude::*;
use autumn_harvest_plugin::HarvestPlugin;
use autumn_web::prelude::*;

#[get("/")]
async fn index() -> &'static str {
    "Boidboard"
}

#[workflow]
async fn ping_workflow(ctx: &WorkflowContext, _input: serde_json::Value) -> HarvestResult<serde_json::Value> {
    ctx.execute_activity_raw("ping_activity", serde_json::json!({}), "default")
        .await
}

#[activity]
async fn ping_activity(
    _ctx: &ActivityContext,
    _input: serde_json::Value,
) -> HarvestResult<serde_json::Value> {
    Ok(serde_json::json!({ "pong": true }))
}

#[autumn_web::main]
async fn main() {
    autumn_web::app()
        .routes(routes![index])
        .plugin(
            HarvestPlugin::new()
                .workflows(workflows![ping_workflow])
                .activities(activities![ping_activity])
                .api("/api/harvest"),
        )
        .run()
        .await;
}
