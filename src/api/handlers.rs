use crate::{
    api::interfaces::Info,
    s3::store_signed_dataitem,
    utils::{
        DATA_CACHES, FAST_FINALITY_INDEXES, FREE_UPLOAD_LIMIT_BYTES, OBJECT_SIZE_LIMIT,
        RECEIPT_HEIGHT_DEADLINE, RECEIPT_VERSION, UPLOADER_AR_ADDRESS, extract_owner_address,
        reconstruct_dataitem_data,
    },
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::arbundles::{SignedReceipt, UnsignedReceipt, sign_receipt};
use axum::{
    Json,
    body::Bytes,
    extract::Path,
    http::{HeaderMap, StatusCode},
};
use serde_json::Value;

pub async fn handle_load_info() -> Json<Value> {
    Json(serde_json::json!({
        "status": "running",
        "name": "loaded-turbo-api",
        "version": env!("CARGO_PKG_VERSION"),
        "object_size_limit": OBJECT_SIZE_LIMIT,
        "data_caches": vec![DATA_CACHES.to_string()],
        "fast_finality_indexes": vec![FAST_FINALITY_INDEXES.to_string()]
    }))
}

pub async fn handle_info() -> Json<Value> {
    let res = Info {
        version: env!("CARGO_PKG_VERSION").to_string(),
        gateway: DATA_CACHES.to_string(),
        free_upload_limit_bytes: FREE_UPLOAD_LIMIT_BYTES,
        addresses: vec![UPLOADER_AR_ADDRESS.to_string()],
    };
    Json(serde_json::to_value(res).unwrap())
}

pub async fn handle_bundler_metrics() -> &'static str {
    "# ALL BUENO"
}

pub async fn handle_health() -> &'static str {
    "OK"
}

pub async fn upload_tx_handler(
    Path(_token): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<SignedReceipt>, StatusCode> {
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

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

    let unsigned_receipt = UnsignedReceipt {
        id: transaction_id,
        owner,
        data_caches: vec![DATA_CACHES.to_string()],
        fast_finality_indexes: vec![FAST_FINALITY_INDEXES.to_string()],
        winc: "0".to_string(),
        version: RECEIPT_VERSION.to_string(),
        deadline_height: RECEIPT_HEIGHT_DEADLINE,
        timestamp,
    };

    let signed_receipt: SignedReceipt = sign_receipt(unsigned_receipt).unwrap();

    Ok(Json(signed_receipt))
}
