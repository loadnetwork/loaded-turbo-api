use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub version: String,
    pub gateway: String,
    pub free_upload_limit_bytes: u32,
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DataItemStatus {
    // default to "CONFIRMED" for LS3
    pub status: String,
    // LS3 do not settle onchain, so none
    pub bundle_id: Option<String>,
    // default to "HOT" for LS3
    pub info: String,
    // default to "0" for LS3
    pub winc: String,
    pub reason: Option<String>,
}
