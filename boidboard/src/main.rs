//! Thin binary shim. All wiring lives in the `boidboard` library crate so that
//! integration tests can drive the real application.

#[autumn_web::main]
async fn main() {
    boidboard::run().await;
}
