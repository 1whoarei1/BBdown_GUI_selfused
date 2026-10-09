use super::VideoMetadata;
use crate::bilibili::models::{OwnerMetadata, PageMetadata};
use crate::core::AccountInfo;
use reqwest::{header, redirect::Policy, Client};
use serde::Deserialize;
use std::time::Duration;

const NAV_API: &str = "https://api.bilibili.com/x/web-interface/nav";
const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 BBDownNext/0.1";

pub struct MetadataClient {
    client: Client,
}

impl MetadataClient {
    pub fn new(user_agent: Option<&str>) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(Policy::limited(3))
            .user_agent(
                user_agent
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(DEFAULT_USER_AGENT),
            )
            .build()
            .map_err(|error| format!("创建 B 站元数据客户端失败: {error}"))?;
        Ok(Self { client })
    }

    pub async fn fetch_account(&self, cookie: &str, source: &str) -> Result<AccountInfo, String> {
        let response = self
            .client
            .get(NAV_API)
            .header(header::COOKIE, cookie)
            .send()
            .await
            .map_err(|error| format!("请求 B 站账号信息失败: {error}"))?
            .error_for_status()
            .map_err(|error| format!("B 站账号信息 HTTP 错误: {error}"))?;
        let raw = response
            .text()
            .await
            .map_err(|error| format!("读取 B 站账号信息失败: {error}"))?;
        parse_nav_response(&raw, source)
    }
}

pub(crate) fn parse_view_response(raw: &str) -> Result<VideoMetadata, String> {
    let response: ApiResponse =
        serde_json::from_str(raw).map_err(|error| format!("解析 B 站视频元数据失败: {error}"))?;
    if response.code != 0 {
        return Err(format!(
            "B 站视频元数据接口返回错误 {}: {}",
            response.code, response.message
        ));
    }
    let data = response
        .data
        .ok_or_else(|| "B 站视频元数据接口缺少 data".to_string())?;

    Ok(VideoMetadata {
        aid: data.aid,
        bvid: data.bvid,
        title: data.title,
        description: data.desc,
        cover_url: normalize_https(data.pic),
        owner: OwnerMetadata {
            mid: data.owner.mid,
            name: data.owner.name,
            face_url: normalize_https(data.owner.face),
        },
        duration_seconds: data.duration,
        publish_time: data.pubdate.map(|value| value.to_string()),
        pages: data
            .pages
            .into_iter()
            .map(|page| PageMetadata {
                page_number: page.page,
                cid: page.cid,
                title: page.part,
                duration_seconds: page.duration,
            })
            .collect(),
    })
}

fn normalize_https(value: String) -> String {
    if value.starts_with("http://") {
        value.replacen("http://", "https://", 1)
    } else if value.starts_with("//") {
        format!("https:{value}")
    } else {
        value
    }
}

pub(crate) fn parse_nav_response(raw: &str, source: &str) -> Result<AccountInfo, String> {
    let response: NavResponse =
        serde_json::from_str(raw).map_err(|error| format!("解析 B 站账号信息失败: {error}"))?;
    if response.code != 0 {
        return Err(format!(
            "B 站账号接口返回错误 {}: {}",
            response.code, response.message
        ));
    }
    let data = response
        .data
        .ok_or_else(|| "B 站账号接口缺少 data".to_string())?;
    if !data.is_login {
        return Ok(AccountInfo {
            token_configured: false,
            is_logged_in: false,
            mid: None,
            name: None,
            avatar_url: None,
            vip_label: None,
            source: source.to_string(),
        });
    }

    Ok(AccountInfo {
        token_configured: false,
        is_logged_in: true,
        mid: Some(data.mid),
        name: non_empty(data.uname),
        avatar_url: non_empty(normalize_https(data.face)),
        vip_label: data
            .vip_label
            .and_then(|label| non_empty(label.text))
            .or_else(|| vip_label(data.vip_status, data.vip_type)),
        source: source.to_string(),
    })
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn vip_label(status: u8, kind: u8) -> Option<String> {
    if status == 0 {
        None
    } else if kind == 2 {
        Some("年度大会员".to_string())
    } else {
        Some("大会员".to_string())
    }
}

#[derive(Deserialize)]
struct ApiResponse {
    code: i32,
    #[serde(default)]
    message: String,
    data: Option<ViewData>,
}

#[derive(Deserialize)]
struct ViewData {
    aid: u64,
    bvid: String,
    title: String,
    #[serde(default)]
    desc: String,
    pic: String,
    duration: u64,
    pubdate: Option<u64>,
    owner: ViewOwner,
    #[serde(default)]
    pages: Vec<ViewPage>,
}

#[derive(Deserialize)]
struct ViewOwner {
    mid: u64,
    name: String,
    #[serde(default)]
    face: String,
}

#[derive(Deserialize)]
struct ViewPage {
    cid: u64,
    page: u32,
    part: String,
    duration: u64,
}

#[derive(Deserialize)]
struct NavResponse {
    code: i32,
    #[serde(default)]
    message: String,
    data: Option<NavData>,
}

#[derive(Deserialize)]
struct NavData {
    #[serde(rename = "isLogin")]
    is_login: bool,
    #[serde(default)]
    mid: u64,
    #[serde(default)]
    uname: String,
    #[serde(default)]
    face: String,
    #[serde(rename = "vipStatus", default)]
    vip_status: u8,
    #[serde(rename = "vipType", default)]
    vip_type: u8,
    #[serde(rename = "vip_label", default)]
    vip_label: Option<NavVipLabel>,
}

#[derive(Deserialize)]
struct NavVipLabel {
    #[serde(default)]
    text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_video_fixture() {
        let raw = include_str!("../../tests/fixtures/bilibili_view_single_video.json");

        let metadata = parse_view_response(raw).expect("fixture should parse");

        assert_eq!(metadata.bvid, "BV1btKG6PEKU");
        assert_eq!(metadata.owner.name, "食贫道");
        assert_eq!(metadata.pages.len(), 1);
        assert_eq!(metadata.pages[0].cid, 40002191561);
        assert_eq!(metadata.duration_seconds, 7444);
        assert!(metadata.cover_url.starts_with("https://"));
    }

    #[test]
    fn returns_api_error_message() {
        let error = parse_view_response(r#"{"code":-404,"message":"啥都木有"}"#)
            .expect_err("error response should fail");

        assert!(error.contains("-404"));
        assert!(error.contains("啥都木有"));
    }

    #[test]
    fn parses_logged_in_account() {
        let raw = r#"{
            "code": 0,
            "data": {
                "isLogin": true,
                "mid": 123,
                "uname": "测试用户",
                "face": "//i0.hdslb.com/face.jpg",
                "vipStatus": 1,
                "vipType": 2,
                "vip_label": { "text": "年度大会员" }
            }
        }"#;

        let account = parse_nav_response(raw, "nativeScan").expect("account should parse");

        assert!(account.is_logged_in);
        assert_eq!(account.mid, Some(123));
        assert_eq!(account.name.as_deref(), Some("测试用户"));
        assert_eq!(account.source, "nativeScan");
        assert_eq!(account.vip_label.as_deref(), Some("年度大会员"));
        assert!(account
            .avatar_url
            .as_deref()
            .unwrap()
            .starts_with("https://"));
    }

    #[test]
    fn parses_logged_out_account() {
        let account = parse_nav_response(r#"{"code":0,"data":{"isLogin":false}}"#, "manualCookie")
            .expect("logged out response should parse");

        assert!(!account.is_logged_in);
        assert_eq!(account.source, "manualCookie");
    }
}
