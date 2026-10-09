use super::client::{check_api, https, prefixed_id, string, BiliClient, Content, ContentPart};
use crate::core::ContentKind;
use serde_json::{json, Value};

pub(crate) fn identify(input: &str) -> Result<(&'static str, u64), String> {
    if let Ok(url) = reqwest::Url::parse(input.trim()) {
        if url
            .host_str()
            .is_some_and(|h| h == "bilibili.tv" || h.ends_with(".bilibili.tv"))
        {
            let segments = url
                .path_segments()
                .map(|s| s.collect::<Vec<_>>())
                .unwrap_or_default();
            if let Some(index) = segments.iter().position(|s| *s == "play") {
                if let Some(id) = segments
                    .get(index + 2)
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|id| *id > 0)
                {
                    return Ok(("ep_id", id));
                }
                if let Some(id) = segments
                    .get(index + 1)
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|id| *id > 0)
                {
                    return Ok(("season_id", id));
                }
            }
        }
    }
    prefixed_id(input, "ep")
        .map(|id| ("ep_id", id))
        .or_else(|| prefixed_id(input, "ss").map(|id| ("season_id", id)))
        .filter(|(_, id)| *id > 0)
        .ok_or_else(|| "INTL 模式用于国际版番剧，请输入 EP / SS 或 bilibili.tv 番剧链接".into())
}

fn params(client: &BiliClient, field: &str, id: u64) -> Vec<(&'static str, String)> {
    let field = if field == "ep_id" {
        "ep_id"
    } else {
        "season_id"
    };
    let mut params = vec![
        (field, id.to_string()),
        ("platform", "android".into()),
        ("s_locale", "zh_SG".into()),
        ("mobi_app", "bstar_a".into()),
    ];
    if let Some(token) = &client.token {
        params.push(("access_key", token.clone()));
    }
    params
}

async fn season(client: &BiliClient, field: &str, id: u64) -> Result<Value, String> {
    let mut error = "国际版番剧接口不可用".to_string();
    for host in ["api.biliintl.com", "api.bilibili.tv"] {
        match client
            .api(
                &format!("https://{host}/intl/gateway/v2/ogv/view/app/season"),
                &params(client, field, id),
            )
            .await
        {
            Ok(value) => match check_api(&value) {
                Ok(()) => return Ok(value),
                Err(e) => error = e,
            },
            Err(e) => error = e,
        }
    }
    Err(error)
}

pub async fn resolve(client: &BiliClient, input: &str) -> Result<Content, String> {
    let (field, id) = identify(input)?;
    let mut value = season(client, field, id).await?;
    let data = value
        .get("result")
        .or_else(|| value.get("data"))
        .ok_or("国际版响应缺少番剧信息")?;
    // Current app metadata may omit episode modules. The WEB endpoint supplies
    // public episode identifiers, while playurl still enforces viewing permissions.
    if episodes(data).is_empty() {
        let season_id = number(&data["season_id"])
            .or_else(|| (field == "season_id").then_some(id))
            .ok_or("国际版响应缺少季 ID")?;
        let web = client
            .api(
                "https://api.biliintl.com/intl/gateway/web/v2/ogv/play/episodes",
                &[("season_id", season_id.to_string())],
            )
            .await?;
        check_api(&web)?;
        let target = if value.get("result").is_some() {
            &mut value["result"]
        } else {
            &mut value["data"]
        };
        target["sections"] = web["data"]["sections"].clone();
    }
    parse_season(&value, if field == "ep_id" { Some(id) } else { None })
}

pub(crate) fn episodes(data: &Value) -> Vec<Value> {
    let mut episodes = data["episodes"].as_array().cloned().unwrap_or_default();
    for field in ["modules", "section", "sections"] {
        if let Some(sections) = data[field].as_array() {
            for section in sections {
                if let Some(items) = section["data"]["episodes"]
                    .as_array()
                    .or_else(|| section["episodes"].as_array())
                {
                    episodes.extend(items.iter().cloned());
                }
            }
        }
    }
    episodes
}
pub(crate) fn number(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}
pub(crate) fn parse_season(value: &Value, requested: Option<u64>) -> Result<Content, String> {
    check_api(value)?;
    let data = value
        .get("result")
        .or_else(|| value.get("data"))
        .ok_or("国际版响应缺少番剧信息")?;
    let mut seen = std::collections::HashSet::new();
    let items = episodes(data);
    let parts = items
        .iter()
        .filter_map(|ep| {
            let id = number(&ep["id"])
                .or_else(|| number(&ep["ep_id"]))
                .or_else(|| number(&ep["episode_id"]))?;
            if requested.is_some_and(|wanted| wanted != id) || !seen.insert(id) {
                return None;
            }
            Some(ContentPart {
                number: seen.len() as u32,
                aid: number(&ep["aid"]).unwrap_or(0),
                bvid: string(&ep["bvid"]).into(),
                cid: number(&ep["cid"]).unwrap_or(0),
                episode_id: Some(id),
                watch_url: number(&data["season_id"])
                    .map(|season| format!("https://www.bilibili.tv/en/play/{season}/{id}")),
                title: ep["title_display"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        format!(
                            "{} {}",
                            ep["title"]
                                .as_str()
                                .or_else(|| ep["short_title_display"].as_str())
                                .unwrap_or(""),
                            ep["long_title"]
                                .as_str()
                                .or_else(|| ep["long_title_display"].as_str())
                                .unwrap_or("")
                        )
                        .trim()
                        .to_string()
                    }),
                duration: number(&ep["duration"]).unwrap_or(0) / 1000,
                ..Default::default()
            })
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Err("国际版接口没有返回所选剧集，请检查地区和观看权限".into());
    }
    let title = string(&data["title"]);
    let cover = if string(&data["cover"]).is_empty() {
        items.first().map(|ep| string(&ep["cover"])).unwrap_or("")
    } else {
        string(&data["cover"])
    };
    Ok(Content {
        title: if title.is_empty() {
            "国际版番剧".into()
        } else {
            title.into()
        },
        description: string(&data["evaluate"]).into(),
        cover: https(cover),
        owner: None,
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

pub async fn play(client: &BiliClient, part: &ContentPart) -> Result<Value, String> {
    let ep = part.episode_id.ok_or("INTL 接口仅支持国际版番剧")?;
    let mut video = vec![];
    let mut audio = vec![];
    let mut duration = part.duration * 1000;
    let mut error = None;
    for codec in [0, 1] {
        let mut params = vec![
            ("aid", part.aid.to_string()),
            ("cid", part.cid.to_string()),
            ("ep_id", ep.to_string()),
            ("platform", "android".into()),
            ("prefer_code_type", codec.to_string()),
            ("qn", "127".into()),
            ("s_locale", "zh_SG".into()),
        ];
        if let Some(token) = &client.token {
            params.push(("access_key", token.clone()));
        }
        let value = match client
            .api(
                "https://api.biliintl.com/intl/gateway/v2/ogv/playurl",
                &params,
            )
            .await
            .and_then(|v| normalize_play(&v))
        {
            Ok(v) => v,
            Err(e) => {
                error = Some(e);
                continue;
            }
        };
        duration = value["data"]["timelength"].as_u64().unwrap_or(duration);
        video.extend(
            value["data"]["dash"]["video"]
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
        audio.extend(
            value["data"]["dash"]["audio"]
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
    }
    let mut seen = std::collections::HashSet::new();
    video.retain(|t| seen.insert((number(&t["id"]), number(&t["codecid"]))));
    seen.clear();
    audio.retain(|t| seen.insert((number(&t["id"]), number(&t["codecid"]))));
    if video.is_empty() && audio.is_empty() {
        return Err(error.unwrap_or_else(|| "国际版没有返回可播放媒体".into()));
    }
    Ok(json!({"code":0,"data":{"timelength":duration,"dash":{"video":video,"audio":audio}}}))
}

pub(crate) fn normalize_play(value: &Value) -> Result<Value, String> {
    check_api(value)?;
    let info = &value["data"]["video_info"];
    if info.is_null() {
        return Err("国际版响应缺少 video_info".into());
    }
    if info["is_preview"].as_bool() == Some(true) || number(&info["is_preview"]) == Some(1) {
        return Err("国际版当前只返回试看内容，请检查账号权限".into());
    }
    let mut video = vec![];
    if let Some(streams) = info["stream_list"].as_array() {
        for stream in streams {
            let mut node = stream["dash_video"].clone();
            if string(&node["base_url"]).is_empty() {
                continue;
            }
            node["id"] = json!(number(&stream["stream_info"]["quality"]).unwrap_or(0));
            video.push(node);
        }
    }
    Ok(
        json!({"code":0,"data":{"timelength":number(&info["timelength"]).unwrap_or(0),"dash":{"video":video,"audio":info["dash_audio"].as_array().cloned().unwrap_or_default()}}}),
    )
}

pub async fn subtitles(client: &BiliClient, part: &ContentPart) -> Result<Vec<Value>, String> {
    let id = part.episode_id.ok_or("国际版字幕需要剧集 ID")?;
    let primary = client
        .api(
            "https://api.biliintl.com/intl/gateway/web/v2/subtitle",
            &[("episode_id", id.to_string())],
        )
        .await;
    if let Ok(value) = primary {
        if check_api(&value).is_ok() {
            if let Some(subs) = value["data"]["subtitles"]
                .as_array()
                .filter(|subs| !subs.is_empty())
            {
                return Ok(normalize_subtitles(subs));
            }
        }
    }
    let value = season(client, "ep_id", id).await?;
    let data = value
        .get("result")
        .or_else(|| value.get("data"))
        .ok_or("国际版响应缺少剧集信息")?;
    let items = episodes(data);
    let ep = items
        .iter()
        .find(|ep| number(&ep["id"]).or_else(|| number(&ep["ep_id"])) == Some(id));
    Ok(ep
        .and_then(|ep| ep["subtitles"].as_array())
        .map(|rows| normalize_subtitles(rows))
        .unwrap_or_default())
}
fn normalize_subtitles(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .filter(|s| !string(&s["url"]).is_empty())
        .map(|s| {
            let language = s["lang_key"]
                .as_str()
                .or_else(|| s["key"].as_str())
                .unwrap_or("und");
            json!({"lan":language,"subtitle_url":string(&s["url"]).replace("\\/", "/")})
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn current_web_episode_schema_and_cookie_isolation() {
        use crate::bilibili::test_support::JsonServer;
        let server=JsonServer::new(|url,_,headers|{
            assert!(!headers.to_ascii_lowercase().contains("cookie:"));
            match url.path() {
                "/intl/gateway/v2/ogv/view/app/season"=>json!({"code":0,"result":{"season_id":"123","title":"fixture INTL","cover":"https://example.com/c.jpg","modules":[]}}),
                "/intl/gateway/web/v2/ogv/play/episodes"=>json!({"code":0,"data":{"sections":[{"episodes":[{"episode_id":"456","title_display":"E1 · fixture"},{"episode_id":"457","title_display":"E2"}]}]}}),
                "/intl/gateway/v2/ogv/playurl"=>{
                    let query=url.query_pairs().collect::<std::collections::BTreeMap<_,_>>();
                    let codec=if query["prefer_code_type"]=="0" {7} else {12};
                    json!({"code":0,"data":{"video_info":{"timelength":1000,"stream_list":[{"stream_info":{"quality":80},"dash_video":{"base_url":"https://example.com/v","codecid":codec}}],"dash_audio":[{"id":30280,"base_url":"https://example.com/a"}]}}})
                },
                _=>panic!("unexpected fixture endpoint"),
            }
        }).await;
        let mut config = crate::core::default_config();
        config.auth.api_mode = crate::core::ApiMode::Intl;
        config.auth.cookie = Some("SESSDATA=domestic-fixture".into());
        let mut client = BiliClient::new(&config).unwrap();
        client.api_origin = Some(server.url.clone());
        let content = client
            .resolve("https://www.bilibili.tv/en/play/123/456")
            .await
            .unwrap();
        assert_eq!(content.parts.len(), 1);
        assert_eq!(content.parts[0].title, "E1 · fixture");
        let value = client.play(&content.parts[0]).await.unwrap();
        let streams = crate::bilibili::streams::parse_play_response(&value, 0).unwrap();
        assert_eq!(streams.video.len(), 2);
        assert_eq!(streams.audio.len(), 1);
    }

    #[tokio::test]
    #[ignore = "requires the live international service; playback may be region restricted"]
    async fn live_intl_metadata_and_permission_response() {
        let mut config = crate::core::default_config();
        config.auth.api_mode = crate::core::ApiMode::Intl;
        let mut client = BiliClient::new(&config).unwrap();
        let content = client
            .resolve("https://www.bilibili.tv/en/play/2411030/30656886")
            .await
            .unwrap();
        assert_eq!(content.parts.len(), 1);
        assert_eq!(content.parts[0].episode_id, Some(30656886));
        assert!(!content.parts[0].title.is_empty());
        match client.play(&content.parts[0]).await {
            Ok(value) => {
                crate::bilibili::streams::parse_play_response(&value, 0).unwrap();
            }
            Err(error) => assert!(
                error.contains("版权地区受限") || error.contains("10015001"),
                "{error}"
            ),
        }
    }
    #[test]
    fn parses_international_links_and_module_episodes() {
        assert_eq!(
            identify("https://www.bilibili.tv/en/play/123/456").unwrap(),
            ("ep_id", 456)
        );
        let value = json!({"code":0,"result":{"title":"INTL","modules":[{"data":{"episodes":[{"id":"456","title":"1"},{"id":457,"title":"2"}]}}]}});
        assert_eq!(
            parse_season(&value, Some(456)).unwrap().parts[0].episode_id,
            Some(456)
        );
        assert_eq!(parse_season(&value, None).unwrap().parts.len(), 2);
        assert!(identify("BV17x411w7KC").is_err());
    }
    #[test]
    fn normalizes_intl_streams_and_ass_subtitles() {
        let value = json!({"code":0,"data":{"video_info":{"timelength":1000,"stream_list":[{"stream_info":{"quality":80},"dash_video":{"base_url":"https://example.com/v","codecid":7}}],"dash_audio":[{"id":30280,"base_url":"https://example.com/a"}]}}});
        let streams =
            crate::bilibili::streams::parse_play_response(&normalize_play(&value).unwrap(), 1)
                .unwrap();
        assert_eq!(streams.video[0].id, 80);
        assert_eq!(
            normalize_subtitles(&[json!({"lang_key":"en","url":"https://example.com/sub.ass"})])[0]
                ["lan"],
            "en"
        );
    }
}
