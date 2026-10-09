use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolPaths {
    pub ffmpeg_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    pub api_mode: ApiMode,
    pub cookie: Option<String>,
    pub access_token: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ApiMode {
    Web,
    Tv,
    App,
    Intl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaOptions {
    pub video: bool,
    pub audio: bool,
    pub danmaku: bool,
    pub subtitle: bool,
    pub cover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOptions {
    pub media: MediaOptions,
    pub page_selection: String,
    pub codec_priority: Vec<String>,
    pub dfn_priority: Vec<String>,
    pub mux: bool,
    pub skip_ai_subtitle: bool,
    pub multi_thread: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedOptions {
    pub force_http: bool,
    pub allow_pcdn: bool,
    pub video_ascending: bool,
    pub audio_ascending: bool,
    pub file_pattern: Option<String>,
    pub multi_file_pattern: Option<String>,
    pub language: Option<String>,
    pub delay_per_page: Option<u32>,
    pub upos_host: Option<String>,
    pub force_replace_host: Option<bool>,
    pub save_archives_to_file: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub download_manager: DownloadManagerOptions,
    pub tools: ToolPaths,
    pub work_dir: String,
    pub auth: AuthConfig,
    pub default_options: DownloadOptions,
    pub advanced: AdvancedOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DownloadManagerOptions {
    pub max_concurrent_tasks: u32,
    pub connections_per_task: u32,
    pub resume_on_start: bool,
}
impl Default for DownloadManagerOptions {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 2,
            connections_per_task: 4,
            resume_on_start: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseRequest {
    pub input: String,
    pub config: AppConfig,
    pub show_all_parts: bool,
    pub use_cache: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub input: String,
    pub config: AppConfig,
    pub parse_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentKind {
    SingleVideo,
    MultiPartVideo,
    BangumiEpisode,
    BangumiMultiEpisode,
    VideoList,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub kind: StreamKind,
    pub raw_text: String,
    pub quality_label: Option<String>,
    pub resolution: Option<String>,
    pub codec: Option<String>,
    pub fps: Option<String>,
    pub bitrate: Option<String>,
    pub approx_size: Option<String>,
    pub badges: Vec<StreamBadge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamKind {
    Video,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamBadge {
    #[serde(rename = "8k")]
    EightK,
    #[serde(rename = "dolby")]
    Dolby,
    #[serde(rename = "hdr")]
    Hdr,
    #[serde(rename = "4k")]
    FourK,
    #[serde(rename = "high1080p")]
    High1080p,
    #[serde(rename = "normal")]
    Normal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartInfo {
    pub page_number: u32,
    pub cid: Option<String>,
    pub title: String,
    pub duration: Option<String>,
    pub video_streams: Vec<StreamInfo>,
    pub audio_streams: Vec<StreamInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseResult {
    pub id: String,
    pub input: String,
    pub content_kind: ContentKind,
    pub aid_or_episode_id: Option<String>,
    pub bvid: Option<String>,
    pub title: String,
    pub owner_name: Option<String>,
    pub owner_space_url: Option<String>,
    pub publish_time: Option<String>,
    pub duration: Option<String>,
    pub part_count: Option<u32>,
    pub cover_path: Option<String>,
    pub save_path: Option<String>,
    pub parts: Vec<PartInfo>,
    pub raw_output: String,
    pub warnings: Vec<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerInfo {
    pub mid: Option<u64>,
    pub name: Option<String>,
    pub face_url: Option<String>,
    pub space_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartInfoV2 {
    pub watch_url: Option<String>,
    pub page_number: u32,
    pub cid: Option<u64>,
    pub aid: Option<u64>,
    pub bvid: Option<String>,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub video_streams: Vec<StreamInfo>,
    pub audio_streams: Vec<StreamInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataSource {
    Bilibili,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseResultV2 {
    pub schema_version: u32,
    pub id: String,
    pub input: String,
    pub content_kind: ContentKind,
    pub aid: Option<u64>,
    pub bvid: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub owner: Option<OwnerInfo>,
    pub publish_time: Option<String>,
    pub duration_seconds: Option<u64>,
    pub cover_url: Option<String>,
    pub parts: Vec<PartInfoV2>,
    pub metadata_source: MetadataSource,
    pub warnings: Vec<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    #[serde(default)]
    pub token_configured: bool,
    pub is_logged_in: bool,
    pub mid: Option<u64>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub vip_label: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDetectionResult {
    pub core_available: bool,
    pub ffmpeg_found: bool,
    pub ffmpeg_version: Option<String>,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPreview {
    pub executable: String,
    pub args: Vec<String>,
    pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRunResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub output: String,
}

pub fn default_config() -> AppConfig {
    AppConfig {
        download_manager: DownloadManagerOptions::default(),
        tools: ToolPaths {
            ffmpeg_path: Some("ffmpeg".to_string()),
        },
        work_dir: "../download".to_string(),
        auth: AuthConfig {
            api_mode: ApiMode::Web,
            cookie: None,
            access_token: None,
            user_agent: None,
        },
        default_options: DownloadOptions {
            media: MediaOptions {
                video: true,
                audio: true,
                danmaku: true,
                subtitle: true,
                cover: true,
            },
            page_selection: "1".to_string(),
            codec_priority: vec!["avc".to_string(), "hevc".to_string(), "av1".to_string()],
            dfn_priority: vec![
                "4K 超清".to_string(),
                "1080P 高码率".to_string(),
                "1080P 高清".to_string(),
            ],
            mux: true,
            skip_ai_subtitle: true,
            multi_thread: true,
        },
        advanced: AdvancedOptions {
            force_http: false,
            allow_pcdn: false,
            video_ascending: false,
            audio_ascending: false,
            file_pattern: None,
            multi_file_pattern: None,
            language: None,
            delay_per_page: Some(0),
            upos_host: None,
            force_replace_host: None,
            save_archives_to_file: None,
        },
    }
}

pub fn default_config_for_paths(ffmpeg_path: Option<String>, work_dir: String) -> AppConfig {
    let mut config = default_config();
    config.tools.ffmpeg_path = ffmpeg_path;
    config.work_dir = work_dir;
    config
}

pub fn candidate_existing_file(paths: impl IntoIterator<Item = PathBuf>) -> Option<String> {
    paths
        .into_iter()
        .find(|path| path.exists())
        .map(|path| path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_tool_fields_are_removed_without_losing_download_preferences() {
        let mut value = serde_json::to_value(default_config()).unwrap();
        value["tools"]["bbdownPath"] = "old/BBDown.exe".into();
        value["tools"]["aria2cPath"] = "old/aria2c.exe".into();
        value["tools"]["mp4boxPath"] = "old/MP4Box.exe".into();
        value["advanced"]["useAria2c"] = true.into();
        value["advanced"]["useMp4box"] = true.into();
        value["defaultOptions"]["pageSelection"] = "2-4".into();
        value["workDir"] = "D:/my downloads".into();
        let config: AppConfig = serde_json::from_value(value).unwrap();
        assert_eq!(config.work_dir, "D:/my downloads");
        assert_eq!(config.default_options.page_selection, "2-4");
        let saved = serde_json::to_value(config).unwrap();
        assert_eq!(saved["auth"]["apiMode"], "WEB");
        assert!(saved["tools"].get("bbdownPath").is_none());
        assert!(saved["advanced"].get("useAria2c").is_none());
    }
    #[test]
    fn all_api_modes_are_readable_and_supported() {
        let mut config = default_config();
        config.auth.api_mode = serde_json::from_str("\"TV\"").unwrap();
        assert!(crate::bilibili::client::BiliClient::new(&config).is_ok());
    }
}
