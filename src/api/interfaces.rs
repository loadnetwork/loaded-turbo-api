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
