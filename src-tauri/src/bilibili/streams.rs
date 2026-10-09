use super::client::{check_api, https, string};
use crate::core::{AppConfig, StreamBadge, StreamInfo, StreamKind};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Track {
    pub id: u64,
    pub codec: String,
    pub bandwidth: u64,
    pub urls: Vec<String>,
    pub info: StreamInfo,
}

#[derive(Debug, Clone)]
pub struct Streams {
    pub video: Vec<Track>,
    pub audio: Vec<Track>,
    pub progressive: Vec<Vec<String>>,
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone)]
pub struct Chapter {
    pub start_ms: u64,
    pub end_ms: u64,
    pub title: String,
}

pub(crate) fn parse_chapters(root: &Value, duration: u64) -> Vec<Chapter> {
    let mut chapters = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for field in ["clip_info_list", "view_points"] {
        if let Some(rows) = root[field].as_array() {
            for row in rows {
                let start = row["start"].as_f64().or_else(|| row["from"].as_f64());
                let end = row["end"].as_f64().or_else(|| row["to"].as_f64());
                if let Some((start, end)) = start.zip(end).filter(|(start, end)| {
                    start.is_finite() && end.is_finite() && *start >= 0.0 && end > start
                }) {
                    let start_ms = (start * 1000.0).round() as u64;
                    let mut end_ms = (end * 1000.0).round() as u64;
                    if duration > 0 {
                        end_ms = end_ms.min(duration.saturating_mul(1000));
                    }
                    let title = row["content"]
                        .as_str()
                        .or_else(|| row["toastText"].as_str())
                        .or_else(|| row["title"].as_str())
                        .unwrap_or("章节")
                        .to_string();
                    if end_ms > start_ms && seen.insert((start_ms, end_ms, title.clone())) {
                        chapters.push(Chapter {
                            start_ms,
                            end_ms,
                            title,
                        });
                    }
                }
            }
        }
    }
    chapters.sort_by_key(|c| c.start_ms);
    chapters
}

pub fn parse_play_response(value: &Value, duration: u64) -> Result<Streams, String> {
    check_api(value)?;
    let root = value
        .get("result")
        .or_else(|| value.get("data"))
        .unwrap_or(value);
    let root = root.get("video_info").unwrap_or(root);
    if root["is_preview"].as_bool() == Some(true) || root["is_preview"].as_u64() == Some(1) {
        return Err("当前账号只能获取试看内容，请登录有观看权限的账号".to_string());
    }
    let mut streams = Streams {
        video: vec![],
        audio: vec![],
        progressive: vec![],
        chapters: parse_chapters(root, duration),
    };
    let duration = root["timelength"]
        .as_u64()
        .map(|ms| ms / 1000)
        .unwrap_or(duration);
    if let Some(video) = root["dash"]["video"].as_array() {
        for node in video {
            streams.video.push(track(node, true, duration)?);
        }
    }
    if let Some(audio) = root["dash"]["audio"].as_array() {
        for node in audio {
            streams.audio.push(track(node, false, duration)?);
        }
    }
    if let Some(audio) = root["dash"]["dolby"]["audio"].as_array() {
        for node in audio {
            streams.audio.push(track(node, false, duration)?);
        }
    }
    if root["dash"]["flac"]["audio"].is_object() {
        streams
            .audio
            .push(track(&root["dash"]["flac"]["audio"], false, duration)?);
    }
    if let Some(segments) = root["durl"].as_array() {
        for segment in segments {
            let urls = urls(segment, "url")?;
            streams.progressive.push(urls);
        }
        if !streams.progressive.is_empty() {
            let id = root["quality"].as_u64().unwrap_or(0);
            streams.video.push(Track {
                id,
                codec: "AVC".to_string(),
                bandwidth: 0,
                urls: vec![],
                info: StreamInfo {
                    kind: StreamKind::Video,
                    raw_text: format!("{} [音视频合流]", quality(id)),
                    quality_label: Some(quality(id)),
                    resolution: None,
                    codec: Some("AVC".to_string()),
                    fps: None,
                    bitrate: None,
                    approx_size: None,
                    badges: vec![StreamBadge::Normal],
                },
            });
        }
    }
    streams
        .audio
        .sort_by_key(|t| std::cmp::Reverse((t.id, t.bandwidth)));
    streams.audio.dedup_by_key(|t| t.id);
    if streams.video.is_empty() && streams.audio.is_empty() {
        return Err("B 站没有返回可下载的音视频流，请检查登录状态和观看权限".to_string());
    }
    Ok(streams)
}

fn track(node: &Value, video: bool, duration: u64) -> Result<Track, String> {
    let id = node["id"].as_u64().unwrap_or(0);
    let bandwidth = node["bandwidth"].as_u64().unwrap_or(0);
    let codec = if video {
        match node["codecid"].as_u64() {
            Some(7) => "AVC",
            Some(12) => "HEVC",
            Some(13) => "AV1",
            _ if string(&node["codecs"]).starts_with("hev") => "HEVC",
            _ if string(&node["codecs"]).starts_with("av01") => "AV1",
            _ => "AVC",
        }
    } else {
        match id {
            30250 => "EAC3",
            30251 => "FLAC",
            _ => "M4A",
        }
    }
    .to_string();
    let resolution = (video
        && node["width"].as_u64().unwrap_or(0) > 0
        && node["height"].as_u64().unwrap_or(0) > 0)
        .then(|| format!("{}x{}", node["width"], node["height"]));
    let quality_label = video.then(|| quality(id));
    let approx_size = format!(
        "~{:.2} MB",
        bandwidth as f64 * duration as f64 / 8.0 / 1024.0 / 1024.0
    );
    let bitrate = format!("{} kbps", bandwidth / 1000);
    let info = StreamInfo {
        kind: if video {
            StreamKind::Video
        } else {
            StreamKind::Audio
        },
        raw_text: format!(
            "[{}] [{}] [{}] [{}]",
            quality_label.as_deref().unwrap_or(&codec),
            resolution.as_deref().unwrap_or(&codec),
            bitrate,
            approx_size
        ),
        quality_label,
        resolution,
        codec: Some(codec.clone()),
        fps: node
            .get("frame_rate")
            .or_else(|| node.get("frameRate"))
            .and_then(Value::as_str)
            .map(str::to_string),
        bitrate: Some(bitrate),
        approx_size: Some(approx_size),
        badges: vec![match id {
            127 => StreamBadge::EightK,
            126 | 30250 => StreamBadge::Dolby,
            125 => StreamBadge::Hdr,
            120 => StreamBadge::FourK,
            112 | 116 => StreamBadge::High1080p,
            _ => StreamBadge::Normal,
        }],
    };
    Ok(Track {
        id,
        codec,
        bandwidth,
        urls: urls(node, "base_url")?,
        info,
    })
}

fn urls(node: &Value, primary: &str) -> Result<Vec<String>, String> {
    let first = node
        .get(primary)
        .or_else(|| node.get("baseUrl"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let mut urls = first.into_iter().map(https).collect::<Vec<_>>();
    if let Some(backups) = node
        .get("backup_url")
        .or_else(|| node.get("backupUrl"))
        .and_then(Value::as_array)
    {
        urls.extend(backups.iter().filter_map(Value::as_str).map(https));
    }
    urls.dedup();
    if urls.is_empty() {
        return Err("音视频流缺少下载地址".to_string());
    }
    Ok(urls)
}

pub fn select_video<'a>(tracks: &'a [Track], config: &AppConfig) -> Option<&'a Track> {
    let rank = |values: &[String], value: &str| {
        values
            .iter()
            .position(|p| p.eq_ignore_ascii_case(value))
            .unwrap_or(values.len())
    };
    tracks.iter().min_by_key(|track| {
        (
            rank(
                &config.default_options.dfn_priority,
                track.info.quality_label.as_deref().unwrap_or(""),
            ),
            rank(&config.default_options.codec_priority, &track.codec),
            if config.advanced.video_ascending {
                track.bandwidth
            } else {
                u64::MAX - track.bandwidth
            },
            u64::MAX - track.id,
        )
    })
}

pub fn select_audio<'a>(tracks: &'a [Track], config: &AppConfig) -> Option<&'a Track> {
    tracks.iter().min_by_key(|track| {
        if config.advanced.audio_ascending {
            track.bandwidth
        } else {
            u64::MAX - track.bandwidth
        }
    })
}

pub fn quality(id: u64) -> String {
    match id {
        127 => "8K 超高清",
        126 => "杜比视界",
        125 => "HDR 真彩",
        120 => "4K 超清",
        116 => "1080P 高帧率",
        112 => "1080P 高码率",
        100 => "智能修复",
        80 => "1080P 高清",
        74 => "720P 高帧率",
        64 | 48 => "720P 高清",
        32 => "480P 清晰",
        16 => "360P 流畅",
        6 => "240P 流畅",
        5 => "144P 流畅",
        _ => "未知画质",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::default_config;
    #[test]
    fn parses_dash_aliases_and_optional_dolby_flac() {
        let value = serde_json::json!({"code":0,"data":{"dash":{"video":[{"id":120,"codecid":12,"bandwidth":4000000,"baseUrl":"https://example.com/v","backupUrl":["https://example.com/b"]}],"audio":null,"dolby":{"audio":[{"id":30250,"base_url":"https://example.com/a"}]},"flac":{"audio":{"id":30251,"base_url":"https://example.com/f"}}}}});
        let streams = parse_play_response(&value, 60).unwrap();
        assert_eq!(streams.video[0].urls.len(), 2);
        assert_eq!(streams.video[0].codec, "HEVC");
        assert_eq!(streams.audio.len(), 2);
        let serialized = serde_json::to_string(&streams.video[0].info).unwrap();
        assert!(!serialized.contains("https://"));
    }
    #[test]
    fn selects_quality_before_encoding() {
        let value = serde_json::json!({"code":0,"result":{"video_info":{"dash":{"video":[{"id":80,"codecid":7,"bandwidth":2000000,"base_url":"https://example.com/a"},{"id":120,"codecid":12,"bandwidth":4000000,"base_url":"https://example.com/b"}]}}}});
        let streams = parse_play_response(&value, 60).unwrap();
        assert_eq!(
            select_video(&streams.video, &default_config()).unwrap().id,
            120
        );
    }
    #[test]
    fn rejects_api_errors_and_previews() {
        assert!(
            parse_play_response(&serde_json::json!({"code":-10403,"message":"需要会员"}), 60)
                .is_err()
        );
        assert!(
            parse_play_response(&serde_json::json!({"code":0,"result":{"is_preview":1}}), 60)
                .is_err()
        );
    }
}
