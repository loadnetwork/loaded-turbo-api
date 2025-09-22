use crate::{
    api::interfaces::UploadNormalFileResponse,
    s3::store_signed_dataitem,
    utils::{
        DATA_CACHES, FAST_FINALITY_INDEXES, OBJECT_SIZE_LIMIT, extract_owner_address,
        reconstruct_dataitem_data,
    },
};
use axum::{
    Json,
    body::Bytes,
    extract::Path,
    http::{HeaderMap, StatusCode},
};
use serde_json::Value;

pub async fn handle_root() -> Json<Value> {
    Json(serde_json::json!({
        "status": "running",
        "name": "loaded-turbo-api",
        "version": env!("CARGO_PKG_VERSION"),
        "object_size_limit": OBJECT_SIZE_LIMIT,
        "data_caches": vec![DATA_CACHES.to_string()],
        "fast_finality_indexes": vec![FAST_FINALITY_INDEXES.to_string()]
    }))
}

pub async fn upload_tx_handler(
    Path(_token): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<UploadNormalFileResponse>, StatusCode> {
    if let Some(content_type) = headers.get("content-type") {
        if content_type != "application/octet-stream" {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let data = body.to_vec();

    let (dataitem, _content_type) = match reconstruct_dataitem_data(data.clone()) {
        Ok(result) => result,
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };

    let transaction_id = match store_signed_dataitem(data).await {
        Ok(id) => id,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let owner = extract_owner_address(&dataitem);

    let response = UploadNormalFileResponse {
        id: transaction_id,
        owner,
        winc: "0".to_string(),
        data_caches: vec![DATA_CACHES.to_string()],
        fast_finality_indexes: vec![FAST_FINALITY_INDEXES.to_string()],
    };

    Ok(Json(response))
}
