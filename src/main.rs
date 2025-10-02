use crate::{
    api::{
        handlers::{
            handle_bundler_metrics, handle_dataitem_status, handle_health, handle_info,
            handle_load_info, handle_tx_offsets, upload_tx_handler,
        },
        multipart_uploads::{
            create_multipart_upload_handler, finalize_multipart_upload_handler,
            get_multipart_upload_handler, get_multipart_upload_status_handler, post_chunk_handler,
        },
    },
    db::init_db,
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
mod arbundles;
mod db;
mod s3;
mod utils;

#[tokio::main]
async fn main() {
    // Load environment variables from a .env file if present
    dotenv().ok();

    let db_pool = init_db().await.expect("Failed to initialize database");

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let router = Router::new()
        .route("/", get(handle_info))
        .route("/info", get(handle_info))
        .route("/internal", get(handle_load_info))
        .route("/bundler_metrics", get(handle_bundler_metrics))
        .route("/health", get(handle_health))
        .route("/v1/tx/{dataitem_id}/status", get(handle_dataitem_status))
        .route("/v1/tx/{token}", post(upload_tx_handler))
        .route("/v1/tx/{dataitem_id}/offsets", get(handle_tx_offsets))
        // multipart upload
        .route("/v1/chunks/{token}/-1/-1", get(create_multipart_upload_handler))
        .route("/v1/chunks/{token}/{upload_id}/-1", get(get_multipart_upload_handler))
        .route("/v1/chunks/{token}/{upload_id}/status", get(get_multipart_upload_status_handler))
        .route("/v1/chunks/{token}/{upload_id}/{offset}", post(post_chunk_handler))
        .route("/v1/chunks/{token}/{upload_id}/finalize", post(finalize_multipart_upload_handler))
        .layer(DefaultBodyLimit::max(OBJECT_SIZE_LIMIT))
        .layer(RequestBodyLimitLayer::new(OBJECT_SIZE_LIMIT))
        .layer(cors)
        .with_state(db_pool);

    // Use SERVER_PORT from env if set, otherwise default to the constant
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| SERVER_PORT.to_string());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
    println!("Server running on PORT: {port}");
    axum::serve(listener, router).await.unwrap();
}
