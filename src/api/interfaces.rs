use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub version: String,
    pub gateway: String,
    pub free_upload_limit_bytes: u32,
    pub addresses: Vec<String>,
}
