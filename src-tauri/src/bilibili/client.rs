use super::{metadata::parse_view_response, normalize_video_input, wbi, VideoId};
use crate::core::{ApiMode, AppConfig, ContentKind, OwnerInfo};
use reqwest::{header, redirect::Policy, Client};
use serde_json::Value;
use std::{collections::BTreeMap, time::Duration};

const UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36";

#[derive(Clone)]
pub struct BiliClient {
    pub http: Client,
    #[cfg(test)]
    pub(crate) api_origin: Option<String>,
    cookie: Option<String>,
    wbi_key: Option<String>,
    pub(crate) mode: ApiMode,
    pub(crate) token: Option<String>,
    pub(crate) codecs: Vec<String>,
    pub(crate) context: Option<super::engine::TaskContext>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Content {
    pub title: String,
    pub description: String,
    pub cover: String,
    pub owner: Option<OwnerInfo>,
    pub publish_time: Option<String>,
    pub kind: ContentKind,
    pub parts: Vec<ContentPart>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ContentPart {
    pub number: u32,
    pub aid: u64,
    pub bvid: String,
    pub cid: u64,
    pub episode_id: Option<u64>,
    pub title: String,
    pub duration: u64,
    pub cheese: bool,
    pub source: Option<PartMetadata>,
    pub watch_url: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PartMetadata {
    pub title: String,
    pub description: String,
    pub cover: String,
    pub owner: Option<OwnerInfo>,
    pub publish_time: Option<String>,
}

impl ContentPart {
    pub fn source_content(&self, list: &Content) -> Content {
        match &self.source {
            Some(source) => Content {
                title: source.title.clone(),
                description: source.description.clone(),
                cover: source.cover.clone(),
                owner: source.owner.clone(),
                publish_time: source.publish_time.clone(),
                kind: ContentKind::SingleVideo,
                parts: vec![self.clone()],
                warnings: vec![],
            },
            None => list.clone(),
        }
    }
}

impl BiliClient {
    pub fn new(config: &AppConfig) -> Result<Self, String> {
        let http = Client::builder()
            .no_deflate()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .redirect(Policy::limited(5))
            .user_agent(
                config
                    .auth
                    .user_agent
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(UA),
            )
            .build()
            .map_err(|error| format!("创建网络客户端失败: {error}"))?;
        Ok(Self {
            http,
            #[cfg(test)]
            api_origin: None,
            cookie: config.auth.cookie.clone().filter(|s| !s.trim().is_empty()),
            wbi_key: None,
            mode: config.auth.api_mode.clone(),
            token: config
                .auth
                .access_token
                .clone()
                .filter(|s| !s.trim().is_empty()),
            codecs: config.default_options.codec_priority.clone(),
            context: None,
        })
    }

    pub async fn api(&self, url: &str, query: &[(&str, String)]) -> Result<Value, String> {
        self.api_response(url, query)
            .await?
            .json()
            .await
            .map_err(network_error)
    }

    pub(crate) async fn api_response(
        &self,
        url: &str,
        query: &[(&str, String)],
    ) -> Result<reqwest::Response, String> {
        let target = self.api_target(url);
        let mut request = self.http.get(&target).query(query).header(
            header::REFERER,
            if url.contains(".biliintl.com/") || url.contains(".bilibili.tv/") {
                "https://www.bilibili.tv/"
            } else {
                "https://www.bilibili.com/"
            },
        );
        if let Some(cookie) = self.cookie.as_ref().filter(|_| domestic_host(url)) {
            request = request.header(header::COOKIE, cookie);
        }
        self.pace_api().await;
        let response = request
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(network_error)?;
        response.error_for_status().map_err(network_error)
    }

    pub(crate) fn api_target(&self, url: &str) -> String {
        #[cfg(test)]
        if let Some(origin) = &self.api_origin {
            let parsed = reqwest::Url::parse(url).unwrap();
            return format!(
                "{}{}{}",
                origin,
                parsed.path(),
                parsed.query().map(|q| format!("?{q}")).unwrap_or_default()
            );
        }
        url.to_string()
    }

    pub(crate) async fn api_map(
        &self,
        url: &str,
        query: &BTreeMap<String, String>,
    ) -> Result<Value, String> {
        let pairs = query
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect::<Vec<_>>();
        self.api(url, &pairs).await
    }

    pub(crate) async fn post_form(
        &self,
        url: &str,
        params: &BTreeMap<String, String>,
    ) -> Result<Value, String> {
        self.http
            .post(self.api_target(url))
            .timeout(Duration::from_secs(30))
            .form(params)
            .send()
            .await
            .map_err(network_error)?
            .error_for_status()
            .map_err(network_error)?
            .json()
            .await
            .map_err(network_error)
    }

    pub(crate) fn report(&self, message: impl Into<String>) -> Result<(), String> {
        if let Some(context) = &self.context {
            context.report(crate::task::TaskPhase::FetchingVideoInfo, message)?;
        }
        Ok(())
    }

    async fn pace_api(&self) {
        #[cfg(test)]
        if self.api_origin.is_some() {
            return;
        }
        static LAST: tokio::sync::Mutex<Option<std::time::Instant>> =
            tokio::sync::Mutex::const_new(None);
        let mut last = LAST.lock().await;
        if let Some(previous) = *last {
            let interval = Duration::from_millis(200);
            if previous.elapsed() < interval {
                tokio::time::sleep(interval.saturating_sub(previous.elapsed())).await;
            }
        }
        *last = Some(std::time::Instant::now());
    }

    pub async fn signed_api(
        &mut self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Value, String> {
        // A stale key is refreshed once on signature rejection.
        for attempt in 0..2 {
            if self.wbi_key.is_none() {
                let nav = self
                    .api("https://api.bilibili.com/x/web-interface/nav", &[])
                    .await?;
                let images = &nav["data"]["wbi_img"];
                self.wbi_key = Some(wbi::mixin_key(
                    string(&images["img_url"]),
                    string(&images["sub_url"]),
                )?);
            }
            let params: BTreeMap<_, _> = query
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            let signature = wbi::sign(params, self.wbi_key.as_deref().unwrap(), timestamp());
            let result = self
                .api(&format!("https://api.bilibili.com{path}?{signature}"), &[])
                .await?;
            if attempt == 0 && matches!(result["code"].as_i64(), Some(-403 | -352)) {
                self.wbi_key = None;
                continue;
            }
            return Ok(result);
        }
        unreachable!()
    }

    pub async fn resolve(&mut self, input: &str) -> Result<Content, String> {
        let input = self.expand_short_link(input).await?;
        if matches!(self.mode, ApiMode::Intl) {
            return super::intl::resolve(self, &input).await;
        }
        if let Some(list) = super::lists::identify_input(&input)? {
            return super::lists::resolve(self, list).await;
        }
        if let Some(id) =
            normalize_video_input(&input).or_else(|| input.trim().parse().ok().map(VideoId::Aid))
        {
            return self.resolve_video(&id).await;
        }
        let cheese = input.contains("/cheese/");
        let (field, id) = prefixed_id(&input, "ep")
            .map(|id| ("ep_id", id))
            .or_else(|| prefixed_id(&input, "ss").map(|id| ("season_id", id)))
            .ok_or_else(|| "请输入视频、番剧、课程、收藏夹、空间或合集链接".to_string())?;
        let path = if cheese {
            "/pugv/view/web/season"
        } else {
            "/pgc/view/web/season"
        };
        let response = self
            .api(
                &format!("https://api.bilibili.com{path}"),
                &[(field, id.to_string())],
            )
            .await?;
        parse_season(
            &response,
            if field == "ep_id" { Some(id) } else { None },
            cheese,
        )
    }

    pub(crate) async fn resolve_video(&self, video_id: &VideoId) -> Result<Content, String> {
        let query = match video_id {
            VideoId::Bvid(value) => ("bvid", value.clone()),
            VideoId::Aid(value) => ("aid", value.to_string()),
        };
        let value = self
            .api("https://api.bilibili.com/x/web-interface/view", &[query])
            .await?;
        let metadata = parse_view_response(&value.to_string())?;
        let owner = Some(OwnerInfo {
            mid: Some(metadata.owner.mid),
            name: Some(metadata.owner.name),
            face_url: Some(metadata.owner.face_url),
            space_url: Some(format!("https://space.bilibili.com/{}", metadata.owner.mid)),
        });
        let source = PartMetadata {
            title: metadata.title.clone(),
            description: metadata.description.clone(),
            cover: metadata.cover_url.clone(),
            owner: owner.clone(),
            publish_time: metadata.publish_time.clone(),
        };
        let parts = metadata
            .pages
            .into_iter()
            .map(|page| ContentPart {
                number: page.page_number,
                aid: metadata.aid,
                bvid: metadata.bvid.clone(),
                cid: page.cid,
                episode_id: None,
                title: page.title,
                duration: page.duration_seconds,
                cheese: false,
                source: Some(source.clone()),
                watch_url: Some(format!(
                    "https://www.bilibili.com/video/{}?p={}",
                    metadata.bvid, page.page_number
                )),
            })
            .collect::<Vec<_>>();
        if parts.is_empty() {
            return Err("视频没有可用的分 P".into());
        }
        Ok(Content {
            title: metadata.title,
            description: metadata.description,
            cover: metadata.cover_url,
            owner,
            publish_time: metadata.publish_time,
            kind: if parts.len() > 1 {
                ContentKind::MultiPartVideo
            } else {
                ContentKind::SingleVideo
            },
            parts,
            warnings: vec![],
        })
    }

    async fn expand_short_link(&self, input: &str) -> Result<String, String> {
        let short = input.split_whitespace().find_map(|word| {
            let word = word.trim_matches(|ch: char| matches!(ch, '，' | '。' | ')' | '）'));
            let url = reqwest::Url::parse(word).ok()?;
            (url.host_str() == Some("b23.tv") && matches!(url.scheme(), "https" | "http"))
                .then_some(url)
        });
        let Some(mut url) = short else {
            return Ok(input.trim().to_string());
        };
        let _ = url.set_scheme("https");
        // No account headers on short links; only the resolved identifier is used.
        let response = self.http.get(url).send().await.map_err(network_error)?;
        let host = response.url().host_str().unwrap_or("");
        if !(host == "bilibili.com" || host.ends_with(".bilibili.com")) {
            return Err("短链接没有跳转到 B 站".to_string());
        }
        Ok(response.url().to_string())
    }

    pub async fn play(&mut self, part: &ContentPart) -> Result<Value, String> {
        match self.mode {
            ApiMode::Tv => return super::tv::play(self, part).await,
            ApiMode::App => return super::app::play(self, part).await,
            ApiMode::Intl => return super::intl::play(self, part).await,
            ApiMode::Web => {}
        }
        let mut params = vec![
            ("avid", part.aid.to_string()),
            ("cid", part.cid.to_string()),
            ("qn", "127".to_string()),
            ("fnval", "4048".to_string()),
            ("fnver", "0".to_string()),
            ("fourk", "1".to_string()),
            ("otype", "json".to_string()),
            ("support_multi_audio", "true".to_string()),
        ];
        if let Some(id) = part.episode_id {
            params.push(("ep_id", id.to_string()));
            let prefix = if part.cheese { "pugv" } else { "pgc" };
            self.api(
                &format!("https://api.bilibili.com/{prefix}/player/web/v2/playurl"),
                &params,
            )
            .await
        } else {
            self.signed_api("/x/player/wbi/playurl", &params).await
        }
    }
}

pub fn check_api(value: &Value) -> Result<(), String> {
    let code = value["code"]
        .as_i64()
        .ok_or_else(|| "B 站响应缺少状态码".to_string())?;
    if code != 0 {
        if matches!(code, -352 | -412 | -799) {
            return Err(format!("B 站接口返回 {code}: 请求触发校验或频率限制，请在浏览器完成验证后导入 Cookie，或稍后重试"));
        }
        let message = value["message"]
            .as_str()
            .or(value["msg"].as_str())
            .unwrap_or("接口请求失败");
        return Err(format!("B 站接口返回 {code}: {message}"));
    }
    Ok(())
}

pub fn string(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
pub fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn network_error(error: reqwest::Error) -> String {
    if error
        .status()
        .is_some_and(|status| matches!(status.as_u16(), 412 | 429))
    {
        return format!(
            "B 站返回 HTTP {}，请求触发校验或频率限制，请在浏览器完成验证后导入 Cookie，或稍后重试",
            error.status().unwrap().as_u16()
        );
    }
    format!("网络请求失败: {}", error.without_url())
}
pub fn https(value: &str) -> String {
    if value.starts_with("//") {
        format!("https:{value}")
    } else {
        value.replacen("http://", "https://", 1)
    }
}

pub(crate) fn prefixed_id(input: &str, prefix: &str) -> Option<u64> {
    let lower = input.to_ascii_lowercase();
    lower.match_indices(prefix).find_map(|(start, _)| {
        if start > 0 && lower.as_bytes()[start - 1].is_ascii_alphanumeric() {
            return None;
        }
        let digits = lower[start + prefix.len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        digits.parse().ok()
    })
}

pub(crate) fn parse_season(
    response: &Value,
    requested: Option<u64>,
    cheese: bool,
) -> Result<Content, String> {
    check_api(response)?;
    let data = response
        .get("result")
        .or_else(|| response.get("data"))
        .ok_or("番剧响应缺少内容")?;
    let mut episodes = data["episodes"].as_array().cloned().unwrap_or_default();
    if let Some(sections) = data["section"].as_array() {
        for section in sections {
            if let Some(extra) = section["episodes"].as_array() {
                episodes.extend(extra.iter().cloned());
            }
        }
    }
    // An EP link addresses that episode; an SS link addresses the whole season.
    if let Some(id) = requested {
        episodes.retain(|ep| ep["id"].as_u64() == Some(id));
    }
    let mut seen = std::collections::HashSet::new();
    let parts = episodes
        .iter()
        .filter(|ep| seen.insert(ep["id"].as_u64()))
        .enumerate()
        .map(|(index, ep)| {
            Ok(ContentPart {
                number: index as u32 + 1,
                aid: ep["aid"].as_u64().ok_or("剧集缺少 aid")?,
                bvid: string(&ep["bvid"]).to_string(),
                cid: ep["cid"].as_u64().ok_or("剧集缺少 cid")?,
                episode_id: ep["id"].as_u64(),
                title: format!("{} {}", string(&ep["title"]), string(&ep["long_title"]))
                    .trim()
                    .to_string(),
                duration: ep["duration"].as_u64().unwrap_or(0) / if cheese { 1 } else { 1000 },
                cheese,
                source: None,
                watch_url: ep["id"].as_u64().map(|id| {
                    format!(
                        "https://www.bilibili.com/{}play/ep{id}",
                        if cheese { "cheese/" } else { "bangumi/" }
                    )
                }),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if parts.is_empty() {
        return Err("没有找到可用剧集".to_string());
    }
    Ok(Content {
        title: string(&data["title"]).to_string(),
        description: string(if cheese {
            &data["subtitle"]
        } else {
            &data["evaluate"]
        })
        .to_string(),
        cover: https(string(&data["cover"])),
        owner: if cheese {
            Some(OwnerInfo {
                mid: data["up_info"]["mid"].as_u64(),
                name: Some(string(&data["up_info"]["uname"]).to_string()),
                face_url: None,
                space_url: data["up_info"]["mid"]
                    .as_u64()
                    .map(|id| format!("https://space.bilibili.com/{id}")),
            })
        } else {
            None
        },
        publish_time: data["publish"]["pub_time"].as_str().map(str::to_string),
        kind: if parts.len() > 1 {
            ContentKind::BangumiMultiEpisode
        } else {
            ContentKind::BangumiEpisode
        },
        parts,
        warnings: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn episode_selects_requested_id_including_special_sections() {
        let response = serde_json::json!({"code":0,"result":{"title":"番剧","episodes":[{"id":1,"aid":10,"cid":20,"title":"1","duration":10000}],"section":[{"episodes":[{"id":2,"aid":11,"cid":21,"title":"SP","duration":20000}]}]}});
        let episode = parse_season(&response, Some(2), false).unwrap();
        assert_eq!(episode.parts.len(), 1);
        assert_eq!(episode.parts[0].episode_id, Some(2));
        assert_eq!(episode.parts[0].duration, 20);
        assert_eq!(parse_season(&response, None, false).unwrap().parts.len(), 2);
    }
}

fn domestic_host(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .is_some_and(|host| host == "bilibili.com" || host.ends_with(".bilibili.com"))
}

#[cfg(test)]
mod live_tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires TV and APP live API access"]
    async fn live_tv_and_app_playback() {
        for mode in [ApiMode::Tv, ApiMode::App] {
            let mut config = crate::core::default_config();
            config.auth.api_mode = mode.clone();
            let mut client = BiliClient::new(&config).unwrap();
            let content = client
                .resolve_video(&VideoId::Bvid("BV17x411w7KC".into()))
                .await
                .unwrap();
            let response = client.play(&content.parts[0]).await;
            assert!(response.is_ok(), "{mode:?}: {:?}", response.as_ref().err());
            let streams = super::super::streams::parse_play_response(
                &response.unwrap(),
                content.parts[0].duration,
            );
            assert!(streams.is_ok(), "{mode:?}: {:?}", streams.as_ref().err());
        }
    }
}
