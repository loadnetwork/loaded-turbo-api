use crate::{
    arbundles::{SignedReceipt, UnsignedReceipt, sign_receipt},
    db::{
        create_upload_record, get_chunks, get_completed_upload, get_upload, save_chunk,
        update_chunk_size,
    },
    s3::{create_s3_multipart, finalize_multipart_upload, upload_part_s3},
    utils::{
        CHUNK_MAX_SIZE, CHUNK_MIN_SIZE, DATA_CACHES, DEFAULT_CHUNK_SIZE, FAST_FINALITY_INDEXES,
        RECEIPT_HEIGHT_DEADLINE, RECEIPT_VERSION,
    },
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use chrono;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct GetUploadResponse {
    pub id: String,
    pub max: usize,
    pub min: usize,
    pub size: i64,
    pub chunks: Vec<[i64; 2]>, // [offset, size] pairs
    #[serde(rename = "failedReason")]
    pub failed_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MultipartUploadStatus {
    pub status: String,
    pub receipt: SignedReceipt,
}

pub async fn create_multipart_upload_handler(
    Path(_token): Path<String>,
    State(pool): State<SqlitePool>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let upload_id = Uuid::new_v4().to_string();
    let upload_key = format!("multipart-{}", Uuid::new_v4().to_string());

    let s3_upload_id =
        create_s3_multipart(&upload_key).await.map_err(|_e| StatusCode::INTERNAL_SERVER_ERROR)?;

    // store in db
    let _ = create_upload_record(&pool, &upload_id, &upload_key, &s3_upload_id)
        .await
        .map_err(|_e| StatusCode::INTERNAL_SERVER_ERROR);

    // return the format Turbo-sdk expects to progress to upload phase
    let response = serde_json::json!({
        "id": upload_id,
        "max": CHUNK_MAX_SIZE,
        "min": CHUNK_MIN_SIZE,
        "size": CHUNK_MAX_SIZE,
        "chunks": []
    });

    Ok(Json(response))
}

pub async fn get_multipart_upload_handler(
    Path((_token, upload_id)): Path<(String, String)>,
    State(pool): State<SqlitePool>,
) -> Result<Json<GetUploadResponse>, StatusCode> {
    let upload = get_upload(&pool, &upload_id).await.map_err(|_| StatusCode::NOT_FOUND)?;

    let chunks =
        get_chunks(&pool, &upload_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let chunk_size = upload.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE);

    // convert chunks to [offset, size] format
    let chunk_offsets: Vec<[i64; 2]> = chunks
        .into_iter()
        .map(|chunk| {
            let offset = chunk_size * (chunk.part_number - 1); // 0-indexed offsets
            [offset, chunk.size]
        })
        .collect();

    Ok(Json(GetUploadResponse {
        id: upload.upload_id,
        max: CHUNK_MAX_SIZE,
        min: CHUNK_MIN_SIZE,
        size: chunk_size,
        chunks: chunk_offsets,
        failed_reason: upload.failed_reason,
    }))
}

pub async fn post_chunk_handler(
    Path((_token, upload_id, chunk_offset)): Path<(String, String, usize)>,
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<StatusCode, StatusCode> {
    let content_length = headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let upload = get_upload(&pool, &upload_id).await.map_err(|_e| StatusCode::NOT_FOUND)?;

    if upload.failed_reason.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let chunk_size =
        if upload.chunk_size.is_none() || content_length as i64 > upload.chunk_size.unwrap_or(0) {
            match update_chunk_size(&pool, &upload_id, content_length as i64).await {
                Ok(_) => content_length,
                Err(e) => {
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        } else {
            let size = upload.chunk_size.unwrap() as usize;
            size
        };

    // validate Turbo standards alignment
    if chunk_offset % chunk_size != 0 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let part_number = (chunk_offset / chunk_size) + 1;
    if part_number > 10_000 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let etag =
        upload_part_s3(&upload.upload_key, &upload.s3_upload_id, part_number as i32, body.to_vec())
            .await
            .map_err(|_e| StatusCode::INTERNAL_SERVER_ERROR)?;

    let _ = save_chunk(&pool, &upload_id, part_number as i64, &etag, content_length as i64)
        .await
        .map_err(|_e| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

pub async fn finalize_multipart_upload_handler(
    Path((_token, upload_id)): Path<(String, String)>, // Accept token parameter
    State(pool): State<SqlitePool>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match finalize_multipart_upload(&pool, &upload_id).await {
        Ok(dataitem_id) => {
            let unsigned_receipt = UnsignedReceipt {
                id: dataitem_id,
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                version: RECEIPT_VERSION.to_string(),
                deadline_height: RECEIPT_HEIGHT_DEADLINE,
                data_caches: vec![DATA_CACHES.to_string()],
                fast_finality_indexes: vec![FAST_FINALITY_INDEXES.to_string()],
                owner: "".to_string(),
                winc: "".to_string(),
            };

            Ok(Json(serde_json::to_value(unsigned_receipt).unwrap_or_default()))
        }
        Err(_e) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_multipart_upload_status_handler(
    Path((_token, upload_id)): Path<(String, String)>,
    State(pool): State<SqlitePool>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // check if upload is still in progress
    match get_upload(&pool, &upload_id).await {
        Ok(_upload) => Ok(Json(serde_json::json!({
            "status": "ASSEMBLING",
            "timestamp": chrono::Utc::now().timestamp_millis()
        }))),
        Err(_) => {
            // completed multipart upload
            match get_completed_upload(&pool, &upload_id).await {
                Ok((dataitem_id, owner_address)) => {
                    let owner = owner_address.unwrap_or_else(|| "unknown".to_string());

                    let unsigned_receipt: UnsignedReceipt = UnsignedReceipt {
                        id: dataitem_id,
                        deadline_height: RECEIPT_HEIGHT_DEADLINE,
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                        version: RECEIPT_VERSION.to_string(),
                        owner,
                        data_caches: vec![DATA_CACHES.to_string()],
                        fast_finality_indexes: vec![FAST_FINALITY_INDEXES.to_string()],
                        winc: "0".to_string(),
                    };
                    let signed_receipt = sign_receipt(unsigned_receipt)
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    let res = MultipartUploadStatus {
                        status: "FINALIZED".to_string(),
                        receipt: signed_receipt,
                    };
                    Ok(Json(serde_json::to_value(res).unwrap_or_default()))
                }
                Err(_) => Err(StatusCode::NOT_FOUND),
            }
        }
    }
}
