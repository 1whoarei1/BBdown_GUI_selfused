use super::{
    app,
    client::{check_api, string, BiliClient, ContentPart},
    intl,
};
use crate::core::ApiMode;
use serde_json::Value;

pub struct SubtitleSource {
    pub language: String,
    pub url: String,
    pub ai: bool,
}

pub async fn lookup(
    client: &mut BiliClient,
    part: &ContentPart,
) -> Result<Vec<SubtitleSource>, String> {
    if matches!(client.mode, ApiMode::Intl) {
        return intl::subtitles(client, part).await.map(normalize);
    }
    if matches!(client.mode, ApiMode::App) {
        match app::subtitles(client, part).await {
            Ok(rows) if !rows.is_empty() => return Ok(normalize(rows)),
            Ok(_) => {}
            Err(error) => client.report(format!("APP 字幕接口暂不可用，尝试 WEB 字幕: {error}"))?,
        }
    }
    let params = [("aid", part.aid.to_string()), ("cid", part.cid.to_string())];
    let mut confirmed_empty = false;
    let mut last_error = None;
    for attempt in 0..2 {
        let value = if attempt == 0 {
            client.signed_api("/x/player/wbi/v2", &params).await
        } else {
            client
                .api("https://api.bilibili.com/x/player/v2", &params)
                .await
        };
        match value.and_then(|value| {
            check_api(&value)?;
            Ok(value)
        }) {
            Ok(value) => {
                let Some(rows) = value["data"]["subtitle"]["subtitles"].as_array() else {
                    last_error = Some("WEB 字幕响应缺少列表".into());
                    continue;
                };
                if !rows.is_empty() {
                    return Ok(normalize(rows.clone()));
                }
                if value["data"]["need_login_subtitle"].as_bool() == Some(false) {
                    // A successful, unrestricted empty list is an authoritative absence.
                    return Ok(vec![]);
                }
                confirmed_empty = true;
            }
            Err(error) => last_error = Some(error),
        }
    }
    if !matches!(client.mode, ApiMode::App) {
        match app::subtitles(client, part).await {
            Ok(rows) => return Ok(normalize(rows)),
            Err(error) => {
                client.report(format!("APP 字幕回退不可用: {error}"))?;
                last_error = Some(error);
            }
        }
    }
    if confirmed_empty {
        Ok(vec![])
    } else {
        Err(last_error.unwrap_or_else(|| "字幕接口不可用".into()))
    }
}

fn normalize(rows: Vec<Value>) -> Vec<SubtitleSource> {
    rows.into_iter()
        .filter(|row| !string(&row["subtitle_url"]).is_empty())
        .map(|row| {
            let language = string(&row["lan"]).to_string();
            SubtitleSource {
                ai: language.starts_with("ai-") || row["ai_type"].as_u64().unwrap_or(0) > 0,
                language,
                url: super::client::https(string(&row["subtitle_url"])),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bilibili::test_support::JsonServer;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn confirmed_empty_web_subtitles_do_not_request_app_fallback() {
        let requests = Arc::new(Mutex::new(vec![]));
        let captured = requests.clone();
        let server = JsonServer::new(move |url, _, _| {
            captured.lock().unwrap().push(url.path().to_string());
            match url.path() {
                "/x/web-interface/nav" => serde_json::json!({"data":{"wbi_img":{"img_url":"https://example.com/0123456789abcdef0123456789abcdef.png","sub_url":"https://example.com/fedcba9876543210fedcba9876543210.png"}}}),
                "/x/player/wbi/v2" => serde_json::json!({"code":0,"data":{"need_login_subtitle":false,"subtitle":{"subtitles":[]}}}),
                _ => panic!("unexpected fallback: {}", url.path()),
            }
        }).await;
        let mut client = BiliClient::new(&crate::core::default_config()).unwrap();
        client.api_origin = Some(server.url.clone());
        assert!(lookup(
            &mut client,
            &ContentPart {
                aid: 1,
                cid: 2,
                ..Default::default()
            }
        )
        .await
        .unwrap()
        .is_empty());
        assert_eq!(
            *requests.lock().unwrap(),
            ["/x/web-interface/nav", "/x/player/wbi/v2"]
        );
    }

    #[tokio::test]
    async fn malformed_web_subtitle_response_is_not_reported_as_no_subtitles() {
        let server = JsonServer::new(|url, _, _| {
            match url.path() {
                "/x/web-interface/nav" => serde_json::json!({"data":{"wbi_img":{"img_url":"https://example.com/0123456789abcdef0123456789abcdef.png","sub_url":"https://example.com/fedcba9876543210fedcba9876543210.png"}}}),
                _ => serde_json::json!({"code":0,"data":{}}),
            }
        }).await;
        let mut client = BiliClient::new(&crate::core::default_config()).unwrap();
        client.api_origin = Some(server.url.clone());
        assert!(lookup(
            &mut client,
            &ContentPart {
                aid: 1,
                cid: 2,
                ..Default::default()
            }
        )
        .await
        .is_err());
    }
}
