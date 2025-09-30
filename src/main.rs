use crate::{
    api::handlers::{
        handle_bundler_metrics, handle_health, handle_info, handle_load_info, upload_tx_handler,
    },
    utils::{OBJECT_SIZE_LIMIT, SERVER_PORT},
};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use dotenvy::dotenv;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};
mod api;
mod s3;
mod utils;

#[tokio::main]
async fn main() {
    // Load environment variables from a .env file if present
    dotenv().ok();

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let router = Router::new()
        .route("/", get(handle_info))
        .route("/info", get(handle_info))
        .route("/bundler_metrics", get(handle_bundler_metrics))
        .route("/health", get(handle_health))
        .route("/internal", get(handle_load_info))
        .route("/v1/tx/{token}", post(upload_tx_handler))
        .layer(DefaultBodyLimit::max(OBJECT_SIZE_LIMIT))
        .layer(RequestBodyLimitLayer::new(OBJECT_SIZE_LIMIT))
        .layer(cors);

    // Use SERVER_PORT from env if set, otherwise default to the constant
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| SERVER_PORT.to_string());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
    println!("Server running on PORT: {port}");
    axum::serve(listener, router).await.unwrap();
}
