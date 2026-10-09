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
                let rows = value["data"]["subtitle"]["subtitles"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if !rows.is_empty() {
                    return Ok(normalize(rows));
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
