use crate::utils::get_env_var;
use anyhow::Error;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUploadResponse {
    pub id: String,
    pub max: usize,
    pub min: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadStatusResponse {
    pub status: String,
    pub timestamp: u64,
}

// Database setup
pub(crate) async fn init_db() -> Result<SqlitePool, Error> {
    let db_path = get_env_var("DB_PATH")?;

    // Create directory if it doesn't exist
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let pool = SqlitePool::connect(&format!("sqlite:{db_path}")).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS uploads (
            upload_id TEXT PRIMARY KEY,
            upload_key TEXT NOT NULL,
            s3_upload_id TEXT NOT NULL,
            chunk_size INTEGER,
            created_at INTEGER NOT NULL,
            failed_reason TEXT
        )
    "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS chunks (
            upload_id TEXT,
            part_number INTEGER,
            etag TEXT NOT NULL,
            size INTEGER NOT NULL,
            PRIMARY KEY (upload_id, part_number)
        )
    "#,
    )
    .execute(&pool)
    .await?;

    // New table to store completed upload information
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS completed_uploads (
            upload_id TEXT PRIMARY KEY,
            dataitem_id TEXT NOT NULL,
            owner_address TEXT,
            finalized_at INTEGER NOT NULL
        )
    "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InFlightUpload {
    pub upload_id: String,
    pub upload_key: String,
    pub s3_upload_id: String,
    pub chunk_size: Option<i64>,
    pub failed_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkInfo {
    pub part_number: i64,
    pub size: i64,
}

// Database functions
pub async fn create_upload_record(
    pool: &SqlitePool,
    upload_id: &str,
    upload_key: &str,
    s3_upload_id: &str,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO uploads (upload_id, upload_key, s3_upload_id, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(upload_id)
    .bind(upload_key)
    .bind(s3_upload_id)
    .bind(chrono::Utc::now().timestamp())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_upload(pool: &SqlitePool, upload_id: &str) -> Result<InFlightUpload, Error> {
    let row = sqlx::query("SELECT upload_id, upload_key, s3_upload_id, chunk_size, failed_reason FROM uploads WHERE upload_id = ?")
        .bind(upload_id)
        .fetch_one(pool)
        .await?;

    Ok(InFlightUpload {
        upload_id: row.get("upload_id"),
        upload_key: row.get("upload_key"),
        s3_upload_id: row.get("s3_upload_id"),
        chunk_size: row.get("chunk_size"),
        failed_reason: row.get("failed_reason"),
    })
}

pub async fn update_chunk_size(
    pool: &SqlitePool,
    upload_id: &str,
    chunk_size: i64,
) -> Result<(), Error> {
    sqlx::query("UPDATE uploads SET chunk_size = ? WHERE upload_id = ?")
        .bind(chunk_size)
        .bind(upload_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn save_chunk(
    pool: &SqlitePool,
    upload_id: &str,
    part_number: i64,
    etag: &str,
    size: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT OR REPLACE INTO chunks (upload_id, part_number, etag, size) VALUES (?, ?, ?, ?)",
    )
    .bind(upload_id)
    .bind(part_number)
    .bind(etag)
    .bind(size)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_chunks(pool: &SqlitePool, upload_id: &str) -> Result<Vec<ChunkInfo>, Error> {
    let rows = sqlx::query(
        "SELECT part_number, size FROM chunks WHERE upload_id = ? ORDER BY part_number",
    )
    .bind(upload_id)
    .fetch_all(pool)
    .await?;

    let chunks = rows
        .into_iter()
        .map(|row| ChunkInfo { part_number: row.get("part_number"), size: row.get("size") })
        .collect();

    Ok(chunks)
}

// Store completed upload information
pub async fn store_completed_upload(
    pool: &SqlitePool,
    upload_id: &str,
    dataitem_id: &str,
    owner_address: Option<&str>,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO completed_uploads (upload_id, dataitem_id, owner_address, finalized_at) VALUES (?, ?, ?, ?)"
    )
    .bind(upload_id)
    .bind(dataitem_id)
    .bind(owner_address)
    .bind(chrono::Utc::now().timestamp())
    .execute(pool)
    .await?;

    Ok(())
}

// Get completed upload information
pub async fn get_completed_upload(
    pool: &SqlitePool,
    upload_id: &str,
) -> Result<(String, Option<String>), Error> {
    let row =
        sqlx::query("SELECT dataitem_id, owner_address FROM completed_uploads WHERE upload_id = ?")
            .bind(upload_id)
            .fetch_one(pool)
            .await?;

    Ok((row.get("dataitem_id"), row.get("owner_address")))
}
