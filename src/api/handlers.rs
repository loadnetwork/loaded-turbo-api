use axum::Json;
use serde_json::Value;

pub async fn handle_route() -> Json<Value> {
    Json(serde_json::json!({
        "status": "running",
        "name": "loaded-turbo-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}
