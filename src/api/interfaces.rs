use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UploadNormalFileResponse {
    pub id: String,
    pub owner: String,
    pub winc: String,
    pub data_caches: Vec<String>,
    pub fast_finality_indexes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub version: String,
    pub gateway: String,
    pub free_upload_limit_bytes: u32,
    pub addresses: Vec<String>,
}
