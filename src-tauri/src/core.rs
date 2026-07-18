use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolPaths {
    pub bbdown_path: String,
    pub ffmpeg_path: Option<String>,
    pub mp4box_path: Option<String>,
    pub aria2c_path: Option<String>,
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
pub enum ApiMode {
    WEB,
    TV,
    APP,
    INTL,
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
    pub use_aria2c: bool,
    pub aria2c_args: Option<String>,
    pub use_mp4box: bool,
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
    pub tools: ToolPaths,
    pub work_dir: String,
    pub auth: AuthConfig,
    pub default_options: DownloadOptions,
    pub advanced: AdvancedOptions,
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
    pub page_number: u32,
    pub cid: Option<u64>,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub video_streams: Vec<StreamInfo>,
    pub audio_streams: Vec<StreamInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataSource {
    BilibiliAndBbdown,
    BbdownOnly,
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
    pub is_logged_in: bool,
    pub mid: Option<u64>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub vip_label: Option<String>,
    pub source: String,
}

impl ParseResult {
    pub fn placeholder(input: String, warning: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            input,
            content_kind: ContentKind::Unknown,
            aid_or_episode_id: None,
            bvid: None,
            title: "无法解析".to_string(),
            owner_name: None,
            owner_space_url: None,
            publish_time: None,
            duration: None,
            part_count: None,
            cover_path: None,
            save_path: None,
            parts: vec![],
            raw_output: String::new(),
            warnings: vec![warning.to_string()],
            error_message: Some(warning.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDetectionResult {
    pub bbdown_found: bool,
    pub ffmpeg_found: bool,
    pub mp4box_found: bool,
    pub aria2c_found: bool,
    pub bbdown_version: Option<String>,
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
        tools: ToolPaths {
            bbdown_path: "../bin/BBDown.exe".to_string(),
            ffmpeg_path: Some("../bin/ffmpeg.exe".to_string()),
            mp4box_path: None,
            aria2c_path: None,
        },
        work_dir: "../download".to_string(),
        auth: AuthConfig {
            api_mode: ApiMode::WEB,
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
            use_aria2c: false,
            aria2c_args: None,
            use_mp4box: false,
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

pub fn default_config_for_paths(
    bbdown_path: String,
    ffmpeg_path: Option<String>,
    work_dir: String,
) -> AppConfig {
    let mut config = default_config();
    config.tools.bbdown_path = bbdown_path;
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
