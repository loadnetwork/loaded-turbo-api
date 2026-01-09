use anyhow::{Context, Result};
use chrono::Utc;
use clickhouse::{Client, Row};
use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::collections::BTreeSet;

const TABLE_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS dataitem_tags
(
    dataitem_id String,
    content_type String,
    created_at   DateTime64(3, 'UTC'),
    dataitem_size Nullable(UInt64),
    owner Nullable(String),
    target Nullable(String),
    tag_key      String,
    tag_value    String
)
ENGINE = ReplacingMergeTree(created_at)
ORDER BY (tag_key, tag_value, dataitem_id);
"#;

static CLIENT: OnceCell<Client> = OnceCell::new();

#[derive(Debug, Deserialize, Row)]
struct ExistingTable {
    engine: String,
}

#[derive(Debug, Clone)]
struct ClickhouseConfig {
    url: String,
    database: String,
    user: Option<String>,
    password: Option<String>,
}

impl ClickhouseConfig {
    fn load() -> Result<Self> {
        let url = std::env::var("CLICKHOUSE_URL").context("CLICKHOUSE_URL env var not set")?;
        let database = std::env::var("CLICKHOUSE_DATABASE").unwrap_or_default();
        let user = std::env::var("CLICKHOUSE_USER").ok().filter(|v| !v.is_empty());
        let password = std::env::var("CLICKHOUSE_PASSWORD").ok().filter(|v| !v.is_empty());
        Ok(Self { url, database, user, password })
    }
}

fn client() -> Result<&'static Client> {
    CLIENT.get_or_try_init(|| {
        let cfg = ClickhouseConfig::load()?;
        let mut builder = Client::default().with_url(cfg.url).with_database(cfg.database);
        if let Some(user) = cfg.user {
            builder = builder.with_user(user);
        }
        if let Some(password) = cfg.password {
            builder = builder.with_password(password);
        }
        Ok(builder)
    })
}

async fn ensure_schema() -> Result<()> {
    let client = client()?;

    let mut needs_create = true;
    let table_info: Option<ExistingTable> = client
        .query(
            "SELECT engine \
             FROM system.tables \
             WHERE database = currentDatabase() AND name = 'dataitem_tags'",
        )
        .fetch_optional()
        .await
        .context("failed to inspect existing dataitem_tags table")?;

    if let Some(info) = table_info {
        if info.engine == "ReplacingMergeTree" {
            needs_create = false;
        } else {
            client
                .query("DROP TABLE IF EXISTS dataitem_tags")
                .execute()
                .await
                .context("failed to drop legacy dataitem_tags table")?;
        }
    }

    if needs_create {
        client.query(TABLE_DDL).execute().await?;
    }

    client
        .query(
            "ALTER TABLE dataitem_tags \
             ADD COLUMN IF NOT EXISTS dataitem_size Nullable(UInt64) \
             AFTER created_at",
        )
        .execute()
        .await
        .context("failed to ensure dataitem_size column")?;

    client
        .query(
            "ALTER TABLE dataitem_tags \
             ADD COLUMN IF NOT EXISTS owner Nullable(String) \
             AFTER dataitem_size",
        )
        .execute()
        .await
        .context("failed to ensure owner column")?;

    client
        .query(
            "ALTER TABLE dataitem_tags \
             ADD COLUMN IF NOT EXISTS target Nullable(String) \
             AFTER owner",
        )
        .execute()
        .await
        .context("failed to ensure target column")?;

    Ok(())
}

fn normalize_tags(tags: &[(String, String)]) -> Vec<(String, String)> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for (key, value) in tags {
        let key_trimmed = key.trim();
        let value_trimmed = value.trim();
        if key_trimmed.is_empty() || value_trimmed.is_empty() {
            continue;
        }
        if key_trimmed.len() > 1024 || value_trimmed.len() > 1024 {
            continue;
        }
        let key_owned = key_trimmed.to_string();
        let value_owned = value_trimmed.to_string();
        if seen.insert((key_owned.clone(), value_owned.clone())) {
            normalized.push((key_owned, value_owned));
        }
    }
    normalized
}

pub async fn index_dataitem(
    dataitem_id: &str,
    content_type: &str,
    tags: &[(String, String)],
    dataitem_size: usize,
    owner: Option<String>,
    target: Option<String>,
) -> Result<()> {
    ensure_schema().await?;
    let client = client()?;
    let created_at_sql = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
    let mut base_tags: Vec<(String, String)> =
        tags.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    base_tags.push(("Storage-Provider".to_string(), "Load-S3".to_string()));
    base_tags.push(("Client".to_string(), "Loaded-Turbo-API".to_string()));
    let normalized = normalize_tags(&base_tags);
    let dataitem_size = u64::try_from(dataitem_size).context("dataitem_size overflows u64")?;

    if normalized.is_empty() {
        return Ok(());
    }

    for (tag_key, tag_value) in normalized.iter() {
        client
            .query(
                "INSERT INTO dataitem_tags \
                 (dataitem_id, content_type, created_at, dataitem_size, owner, target, tag_key, tag_value) \
                 VALUES (?, ?, toDateTime64(?, 3, 'UTC'), ?, ?, ?, ?, ?)",
            )
            .bind(dataitem_id)
            .bind(content_type)
            .bind(&created_at_sql)
            .bind(dataitem_size)
            .bind(owner.clone())
            .bind(target.clone())
            .bind(tag_key)
            .bind(tag_value)
            .execute()
            .await
            .with_context(|| {
                format!("failed to insert tag ({tag_key}, {tag_value}) for dataitem {dataitem_id}")
            })?;
    }
    Ok(())
}
