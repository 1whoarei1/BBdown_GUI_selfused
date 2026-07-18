use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    pub aid: u64,
    pub bvid: String,
    pub title: String,
    pub description: String,
    pub cover_url: String,
    pub owner: OwnerMetadata,
    pub duration_seconds: u64,
    pub pages: Vec<PageMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerMetadata {
    pub mid: u64,
    pub name: String,
    pub face_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMetadata {
    pub page_number: u32,
    pub cid: u64,
    pub title: String,
    pub duration_seconds: u64,
}
