use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    body::Bytes,
    Json
};
use serde_json::Value;
use crate::api::interfaces::UploadNormalFileResponse;
use crate::s3::store_signed_dataitem;
use crate::utils::{reconstruct_dataitem_data, DATA_CACHES, FAST_FINALITY_INDEXES, extract_owner_address};


pub async fn handle_route() -> Json<Value> {
    Json(serde_json::json!({
        "status": "running",
        "name": "loaded-turbo-api",
        "version": env!("CARGO_PKG_VERSION")
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
