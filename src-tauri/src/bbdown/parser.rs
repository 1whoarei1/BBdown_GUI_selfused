use crate::core::{ContentKind, ParseResult, PartInfo, StreamBadge, StreamInfo, StreamKind};
use std::collections::HashMap;
use uuid::Uuid;

pub fn parse_bbdown_output(input: String, raw_output: String) -> ParseResult {
    let title = find_after_prefix(&raw_output, "视频标题:")
        .or_else(|| find_after_prefix(&raw_output, "标题:"))
        .unwrap_or_else(|| "未知标题".to_string());
    let owner_name = find_after_prefix(&raw_output, "UP主:");
    let owner_space_url = find_after_prefix(&raw_output, "UP主页:");
    let publish_time =
        find_after_prefix(&raw_output, "发布时间:").map(|value| trim_to_date(&value));
    let aid_or_episode_id = find_after_any_prefix(&raw_output, &["获取aid结果:", "获取aid结束:"]);
    let bvid = extract_bvid(&raw_output).or_else(|| extract_bvid(&input));
    let parts = parse_parts(&raw_output, &title);
    let content_kind = detect_content_kind(&raw_output, &parts);
    let error_message = detect_error_message(&raw_output);
    let warnings = if raw_output.trim().is_empty() {
        vec!["BBDown 没有输出内容。".to_string()]
    } else if parts.is_empty() {
        vec!["未从 BBDown 输出中解析到音视频流。".to_string()]
    } else {
        vec![]
    };

    ParseResult {
        id: Uuid::new_v4().to_string(),
        input,
        content_kind,
        aid_or_episode_id,
        bvid,
        title,
        owner_name,
        owner_space_url,
        publish_time,
        duration: extract_duration(&raw_output),
        part_count: (!parts.is_empty()).then_some(parts.len() as u32),
        cover_path: None,
        save_path: None,
        parts,
        raw_output,
        warnings,
        error_message,
    }
}

fn parse_parts(output: &str, title: &str) -> Vec<PartInfo> {
    let lines = output
        .lines()
        .map(normalize_bbdown_line)
        .collect::<Vec<_>>();
    let part_titles = parse_part_titles(&lines);
    let is_multi_part = output.contains("个分P") || output.contains("分P信息");
    let mut parts = Vec::new();
    let mut current_part = if !is_multi_part && !output.contains("P1") {
        Some(empty_part(1, None, format!("P1: {title}"), None))
    } else {
        None
    };

    for line in &lines {
        if let Some(page_number) = parse_started_part(line) {
            if let Some(part) = current_part.take() {
                parts.push(part);
            }
            let part = part_titles
                .get(&page_number)
                .cloned()
                .unwrap_or_else(|| empty_part(page_number, None, format!("P{page_number}"), None));
            current_part = Some(part);
            continue;
        }

        if let Some(stream) = parse_stream_line(line) {
            if let Some(part) = current_part.as_mut() {
                match stream.kind {
                    StreamKind::Video => part.video_streams.push(stream),
                    StreamKind::Audio => part.audio_streams.push(stream),
                }
            }
        }
    }

    if let Some(part) = current_part.take() {
        parts.push(part);
    }

    if parts.is_empty() && !part_titles.is_empty() {
        parts.extend(part_titles.into_values());
        parts.sort_by_key(|part| part.page_number);
    }

    if parts.is_empty() && title != "未知标题" {
        let mut part = empty_part(1, None, format!("P1: {title}"), None);
        for line in &lines {
            if let Some(stream) = parse_stream_line(line) {
                match stream.kind {
                    StreamKind::Video => part.video_streams.push(stream),
                    StreamKind::Audio => part.audio_streams.push(stream),
                }
            }
        }
        if !part.video_streams.is_empty() || !part.audio_streams.is_empty() {
            parts.push(part);
        }
    }

    parts
}

fn parse_part_titles(lines: &[String]) -> HashMap<u32, PartInfo> {
    let mut titles = HashMap::new();
    for line in lines {
        if let Some((page, cid, title, duration)) = parse_part_info_line(line) {
            titles.insert(page, empty_part(page, cid, title, duration));
        }
    }
    titles
}

fn parse_part_info_line(line: &str) -> Option<(u32, Option<String>, String, Option<String>)> {
    let normalized = normalize_bbdown_line(line);
    let text = normalized.trim();
    let text = text.strip_prefix('-').unwrap_or(text).trim();
    let text = text.strip_prefix('P')?;
    let digit_len = text.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_len == 0 {
        return None;
    }

    let page = text[..digit_len].parse::<u32>().ok()?;
    let rest = text[digit_len..].trim_start();
    let rest = rest.strip_prefix(':')?.trim();
    let (cid, after_cid) = extract_bracket_value(rest)
        .map(|(value, remainder)| (Some(value), remainder.trim()))
        .unwrap_or((None, rest));
    let (title_raw, duration) = extract_last_bracket_value(after_cid)
        .map(|(value, remainder)| (remainder.trim().to_string(), Some(value)))
        .unwrap_or((after_cid.to_string(), None));
    let title = strip_leading_bracket(&title_raw).to_string();
    Some((page, cid, format!("P{page}: {title}"), duration))
}

fn strip_leading_bracket(value: &str) -> &str {
    let value = value.trim();
    if !value.starts_with('[') {
        return value;
    }

    match value.find(']') {
        Some(index) => {
            let after = value[index + 1..].trim();
            if after.is_empty() {
                value[1..index].trim()
            } else {
                after
            }
        }
        None => value,
    }
}

fn parse_started_part(line: &str) -> Option<u32> {
    let normalized = normalize_bbdown_line(line);
    let (_, after) = normalized.split_once("开始解析P")?;
    let digits = after
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u32>().ok()
}

fn parse_stream_line(line: &str) -> Option<StreamInfo> {
    let normalized = normalize_bbdown_line(line);
    let trimmed = normalized.trim();
    let digit_len = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_len == 0 || trimmed[digit_len..].chars().next()? != '.' {
        return None;
    }
    if trimmed.contains("http") {
        return None;
    }

    let content = trimmed[digit_len + 1..].trim();
    if content.is_empty() {
        return None;
    }

    let kind = if contains_resolution(content) {
        StreamKind::Video
    } else {
        StreamKind::Audio
    };
    let badges = classify_stream(trimmed);

    Some(StreamInfo {
        kind,
        raw_text: trimmed.to_string(),
        quality_label: extract_first_bracket(content),
        resolution: extract_resolution(content),
        codec: extract_codec(content),
        fps: extract_fps(content),
        bitrate: extract_bitrate(content),
        approx_size: extract_approx_size(content),
        badges,
    })
}

fn contains_resolution(value: &str) -> bool {
    extract_resolution(value).is_some()
}

fn extract_first_bracket(value: &str) -> Option<String> {
    let start = value.find('[')?;
    let end = value[start..].find(']')? + start;
    Some(value[start + 1..end].trim().to_string())
}

fn extract_resolution(value: &str) -> Option<String> {
    let mut rest = value;
    while let Some(start) = rest.find('[') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find(']') else {
            return None;
        };
        let candidate = after_start[..end].trim();
        if candidate.contains('x') && candidate.chars().any(|ch| ch.is_ascii_digit()) {
            return Some(candidate.to_string());
        }
        rest = &after_start[end + 1..];
    }
    None
}

fn extract_codec(value: &str) -> Option<String> {
    for codec in ["avc", "hevc", "av1", "AVC", "HEVC", "AV1"] {
        if value.contains(codec) {
            return Some(codec.to_ascii_lowercase());
        }
    }
    None
}

fn extract_fps(value: &str) -> Option<String> {
    extract_bracket_values(value)
        .into_iter()
        .find(|candidate| candidate.parse::<f32>().is_ok())
}

fn extract_bitrate(value: &str) -> Option<String> {
    extract_bracket_values(value)
        .into_iter()
        .find(|candidate| candidate.contains("kbps"))
}

fn extract_approx_size(value: &str) -> Option<String> {
    extract_bracket_values(value)
        .into_iter()
        .find(|candidate| candidate.starts_with('~'))
}

fn extract_bracket_values(value: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = value;
    while let Some((current, remainder)) = extract_bracket_value(rest) {
        values.push(current);
        rest = remainder;
    }
    values
}

fn extract_bracket_value(value: &str) -> Option<(String, &str)> {
    let start = value.find('[')?;
    let after_start = &value[start + 1..];
    let end = after_start.find(']')?;
    let current = after_start[..end].trim().to_string();
    let remainder = &after_start[end + 1..];
    Some((current, remainder))
}

fn extract_last_bracket_value(value: &str) -> Option<(String, &str)> {
    let end = value.rfind(']')?;
    let before_end = &value[..end];
    let start = before_end.rfind('[')?;
    let current = value[start + 1..end].trim().to_string();
    let remainder = value[..start].trim_end();
    Some((current, remainder))
}

fn classify_stream(value: &str) -> Vec<StreamBadge> {
    let mut badges = Vec::new();
    if value.contains("8K") {
        badges.push(StreamBadge::EightK);
    }
    if value.contains("杜比") || value.contains("Dolby") || value.contains("Atmos") {
        badges.push(StreamBadge::Dolby);
    }
    if value.contains("HDR") {
        badges.push(StreamBadge::Hdr);
    }
    if value.contains("4K") {
        badges.push(StreamBadge::FourK);
    }
    if value.contains("1080P 高码率") || value.contains("60.0") {
        badges.push(StreamBadge::High1080p);
    }

    if badges.is_empty() {
        badges.push(StreamBadge::Normal);
    }

    badges
}

fn empty_part(
    page_number: u32,
    cid: Option<String>,
    title: String,
    duration: Option<String>,
) -> PartInfo {
    PartInfo {
        page_number,
        cid,
        title,
        duration,
        video_streams: vec![],
        audio_streams: vec![],
    }
}

fn find_after_prefix(output: &str, prefix: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let line = normalize_bbdown_line(line);
        let (_, value) = line.split_once(prefix)?;
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn find_after_any_prefix(output: &str, prefixes: &[&str]) -> Option<String> {
    prefixes
        .iter()
        .find_map(|prefix| find_after_prefix(output, prefix))
}

fn extract_bvid(text: &str) -> Option<String> {
    for (index, _) in text.match_indices("BV") {
        let candidate = text[index..]
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric())
            .take(12)
            .collect::<String>();
        if candidate.len() == 12 {
            return Some(candidate);
        }
    }
    None
}

fn extract_duration(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        extract_bracket_values(&normalize_bbdown_line(line))
            .into_iter()
            .find(|value| value.contains('m') && value.contains('s'))
    })
}

fn detect_content_kind(output: &str, parts: &[PartInfo]) -> ContentKind {
    let is_episode_id = find_after_any_prefix(output, &["获取aid结果:", "获取aid结束:"])
        .is_some_and(|value| value.starts_with("ep:"));
    let is_multi_part = parts.len() > 1;

    match (is_episode_id, is_multi_part) {
        (true, true) => ContentKind::BangumiMultiEpisode,
        (true, false) => ContentKind::BangumiEpisode,
        (false, true) => ContentKind::MultiPartVideo,
        (false, false) if !parts.is_empty() => ContentKind::SingleVideo,
        _ => ContentKind::Unknown,
    }
}

fn detect_error_message(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let normalized = normalize_bbdown_line(line);
        let trimmed = normalized.trim();
        if trimmed.contains("错误") || trimmed.contains("失败") || trimmed.contains("Exception")
        {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn normalize_bbdown_line(line: &str) -> String {
    let trimmed = line.trim();
    if let Some((prefix, value)) = trimmed.split_once("] - ") {
        if prefix.starts_with('[') {
            return value.trim().to_string();
        }
    }

    trimmed.to_string()
}

fn trim_to_date(value: &str) -> String {
    value
        .split_whitespace()
        .next()
        .unwrap_or(value)
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_video_fixture() {
        let raw_output =
            include_str!("../../tests/fixtures/bbdown_info_single_video.txt").to_string();

        let result = parse_bbdown_output(
            "https://www.bilibili.com/video/BV1GJ411x7h7".to_string(),
            raw_output,
        );

        assert_eq!(result.title, "测试视频标题");
        assert_eq!(result.owner_name.as_deref(), Some("测试UP主"));
        assert_eq!(
            result.owner_space_url.as_deref(),
            Some("https://space.bilibili.com/123456"),
        );
        assert_eq!(result.publish_time.as_deref(), Some("2024-01-02"));
        assert_eq!(result.aid_or_episode_id.as_deref(), Some("BV1GJ411x7h7"));
        assert_eq!(result.bvid.as_deref(), Some("BV1GJ411x7h7"));
        assert!(matches!(result.content_kind, ContentKind::SingleVideo));
        assert_eq!(result.part_count, Some(1));
        assert!(result.warnings.is_empty());

        let part = result.parts.first().expect("fixture should parse one part");
        assert_eq!(part.page_number, 1);
        assert_eq!(part.cid.as_deref(), Some("987654321"));
        assert_eq!(part.title, "P1: 第一集标题");
        assert_eq!(part.duration.as_deref(), Some("3m21s"));
        assert_eq!(part.video_streams.len(), 2);
        assert_eq!(part.audio_streams.len(), 1);

        let first_video = &part.video_streams[0];
        assert!(matches!(first_video.kind, StreamKind::Video));
        assert_eq!(first_video.quality_label.as_deref(), Some("1080P 高清"));
        assert_eq!(first_video.resolution.as_deref(), Some("1920x1080"));
        assert_eq!(first_video.codec.as_deref(), Some("avc"));
        assert_eq!(first_video.fps.as_deref(), Some("60.0"));
        assert_eq!(first_video.bitrate.as_deref(), Some("4500kbps"));
        assert_eq!(first_video.approx_size.as_deref(), Some("~120.00MB"));
        assert!(first_video
            .badges
            .iter()
            .any(|badge| matches!(badge, StreamBadge::High1080p)));

        let second_video = &part.video_streams[1];
        assert!(second_video
            .badges
            .iter()
            .any(|badge| matches!(badge, StreamBadge::FourK)));

        let first_audio = &part.audio_streams[0];
        assert!(matches!(first_audio.kind, StreamKind::Audio));
        assert_eq!(first_audio.quality_label.as_deref(), Some("音频"));
        assert_eq!(first_audio.bitrate.as_deref(), Some("192kbps"));
        assert_eq!(first_audio.approx_size.as_deref(), Some("~8.00MB"));
    }

    #[test]
    fn parses_timestamped_single_video_log() {
        let raw_output = r#"BBDown version 1.6.3, Bilibili Downloader.
[2026-06-02 18:22:16.194] - 获取aid...
[2026-06-02 18:22:16.475] - 获取aid结束: 116509493953755
[2026-06-02 18:22:16.524] - 视频标题: 找 不 了 茬
[2026-06-02 18:22:16.524] - 发布时间: 2026-05-03 15:40:30 +08:00
[2026-06-02 18:22:16.524] - UP主页: https://space.bilibili.com/3546755912173945
[2026-06-02 18:22:16.524] - P1: [38057674058] [studio_video_1777793495760] [02m00s]
[2026-06-02 18:22:16.524] - 共计 1 个分P, 已选择：ALL
[2026-06-02 18:22:16.524] - 开始解析P1: 116509493953755... (1 of 1)
[2026-06-02 18:22:16.730] - 共计15条视频流.
                            0. [1080P 高帧率] [1920x1080] [AVC] [60.000] [3051 kbps] [~44.69 MB]
https://example.invalid/video.m4s
[2026-06-02 18:22:16.730] - 共计3条音频流.
                            0. [M4A] [92 kbps] [~1.35 MB]
[2026-06-02 18:22:16.731] - 任务完成"#
            .to_string();

        let result = parse_bbdown_output(
            "https://www.bilibili.com/video/BV1LU9oBsEq1/".to_string(),
            raw_output,
        );

        assert_eq!(result.title, "找 不 了 茬");
        assert_eq!(result.aid_or_episode_id.as_deref(), Some("116509493953755"));
        assert_eq!(result.bvid.as_deref(), Some("BV1LU9oBsEq1"));
        assert_eq!(result.publish_time.as_deref(), Some("2026-05-03"));
        assert_eq!(result.duration.as_deref(), Some("02m00s"));
        assert!(matches!(result.content_kind, ContentKind::SingleVideo));
        assert_eq!(result.part_count, Some(1));

        let part = result
            .parts
            .first()
            .expect("timestamped log should parse one part");
        assert_eq!(part.cid.as_deref(), Some("38057674058"));
        assert_eq!(part.title, "P1: studio_video_1777793495760");
        assert_eq!(part.duration.as_deref(), Some("02m00s"));
        assert_eq!(part.video_streams.len(), 1);
        assert_eq!(part.audio_streams.len(), 1);
        assert_eq!(part.video_streams[0].fps.as_deref(), Some("60.000"));
        assert_eq!(part.video_streams[0].bitrate.as_deref(), Some("3051 kbps"));
        assert!(part.video_streams[0]
            .badges
            .iter()
            .any(|badge| matches!(badge, StreamBadge::High1080p)));
    }

    #[test]
    fn classifies_timestamped_bangumi_episode_log() {
        let raw_output = r#"BBDown version 1.6.3, Bilibili Downloader.
[2026-06-02 18:21:44.172] - 获取aid结束: ep:308153
[2026-06-02 18:21:44.248] - 视频标题: Re：从零开始的异世界生活 Memory Snow(雪之回忆)
[2026-06-02 18:21:44.248] - 发布时间: 2020-01-04 00:00:00 +08:00
[2026-06-02 18:21:44.248] - P1: [140112894] [全片 雪之回忆] [00m00s]
[2026-06-02 18:21:44.248] - 共计 1 个分P, 已选择：ALL
[2026-06-02 18:21:44.248] - 开始解析P1: 81886688... (1 of 1)
[2026-06-02 18:21:44.588] - 共计16条视频流.
                            0. [HDR 真彩] [1920x1080] [HEVC] [23.976] [1164 kbps] [~501.12 MB]
[2026-06-02 18:21:44.588] - 共计3条音频流.
                            0. [M4A] [323 kbps] [~142.34 MB]
[2026-06-02 18:21:44.588] - 任务完成"#
            .to_string();

        let result = parse_bbdown_output(
            "https://www.bilibili.com/bangumi/play/ep308153".to_string(),
            raw_output,
        );

        assert_eq!(result.aid_or_episode_id.as_deref(), Some("ep:308153"));
        assert!(matches!(result.content_kind, ContentKind::BangumiEpisode));
        assert_eq!(result.part_count, Some(1));
        let part = result
            .parts
            .first()
            .expect("bangumi episode should parse one part");
        assert_eq!(part.cid.as_deref(), Some("140112894"));
        assert_eq!(part.title, "P1: 全片 雪之回忆");
        assert!(part.video_streams[0]
            .badges
            .iter()
            .any(|badge| matches!(badge, StreamBadge::Hdr)));
    }

    #[test]
    fn classifies_timestamped_bangumi_multi_episode_log() {
        let raw_output = r#"BBDown version 1.6.3, Bilibili Downloader.
[2026-06-02 18:21:45.563] - 获取aid结束: ep:2009785
[2026-06-02 18:21:45.639] - 视频标题: 薰香花朵凛然绽放
[2026-06-02 18:21:45.639] - 发布时间: 2025-08-06 20:00:00 +08:00
[2026-06-02 18:21:45.639] - P1: [31536253789] [1 凛太郎和薰子] [00m00s]
[2026-06-02 18:21:45.639] - P2: [31708808374] [2 千鸟与桔梗] [00m00s]
[2026-06-02 18:21:45.640] - 共计 2 个分P, 已选择：ALL
[2026-06-02 18:21:45.640] - 开始解析P1: 114975469211392... (1 of 2)
[2026-06-02 18:21:45.908] - 共计19条视频流.
                            0. [4K 超清] [3840x2160] [AV1] [23.973] [1844 kbps] [~317.73 MB]
[2026-06-02 18:21:45.908] - 共计3条音频流.
                            0. [M4A] [174 kbps] [~30.69 MB]
[2026-06-02 18:21:45.908] - 开始解析P2: 114975469211395... (2 of 2)
[2026-06-02 18:21:46.182] - 共计19条视频流.
                            0. [1080P 高码率] [1920x1080] [AVC] [23.973] [1351 kbps] [~232.76 MB]
[2026-06-02 18:21:46.182] - 共计3条音频流.
                            0. [M4A] [176 kbps] [~31.04 MB]
[2026-06-02 18:21:49.005] - 任务完成"#
            .to_string();

        let result = parse_bbdown_output(
            "https://www.bilibili.com/bangumi/play/ep2009785".to_string(),
            raw_output,
        );

        assert!(matches!(
            result.content_kind,
            ContentKind::BangumiMultiEpisode
        ));
        assert_eq!(result.part_count, Some(2));
        assert_eq!(result.parts[0].title, "P1: 1 凛太郎和薰子");
        assert_eq!(result.parts[1].cid.as_deref(), Some("31708808374"));
        assert_eq!(result.parts[1].title, "P2: 2 千鸟与桔梗");
        assert_eq!(
            result.parts[0].video_streams[0].resolution.as_deref(),
            Some("3840x2160")
        );
        assert!(result.parts[0].video_streams[0]
            .badges
            .iter()
            .any(|badge| matches!(badge, StreamBadge::FourK)));
    }

    #[test]
    fn classifies_timestamped_regular_multi_part_log() {
        let raw_output = r#"BBDown version 1.6.3, Bilibili Downloader.
[2026-06-02 18:21:47.691] - 获取aid结束: 249190861
[2026-06-02 18:21:47.754] - 视频标题: 台球零基础入门十三课
[2026-06-02 18:21:47.754] - 发布时间: 2021-07-16 10:19:54 +08:00
[2026-06-02 18:21:47.754] - UP主页: https://space.bilibili.com/450524654
[2026-06-02 18:21:47.754] - P1: [370714642] [第一课：台球基本姿势与动作] [24m43s]
[2026-06-02 18:21:47.754] - P2: [370715712] [第二课：高中低杆] [36m00s]
[2026-06-02 18:21:47.754] - 共计 2 个分P, 已选择：ALL
[2026-06-02 18:21:47.754] - 开始解析P1: 249190861... (1 of 2)
[2026-06-02 18:21:47.978] - 共计9条视频流.
                            0. [720P 高清] [1280x720] [AVC] [25] [761 kbps] [~137.76 MB]
[2026-06-02 18:21:47.978] - 共计3条音频流.
                            0. [M4A] [130 kbps] [~23.53 MB]
[2026-06-02 18:21:47.979] - 开始解析P2: 249190861... (2 of 2)
[2026-06-02 18:21:48.242] - 共计9条视频流.
                            0. [720P 高清] [1280x720] [HEVC] [25] [204 kbps] [~53.79 MB]
[2026-06-02 18:21:48.243] - 共计3条音频流.
                            0. [M4A] [130 kbps] [~34.28 MB]
[2026-06-02 18:21:50.828] - 任务完成"#
            .to_string();

        let result = parse_bbdown_output(
            "https://www.bilibili.com/video/BV1Mv411n7Xr/".to_string(),
            raw_output,
        );

        assert_eq!(result.aid_or_episode_id.as_deref(), Some("249190861"));
        assert!(matches!(result.content_kind, ContentKind::MultiPartVideo));
        assert_eq!(result.bvid.as_deref(), Some("BV1Mv411n7Xr"));
        assert_eq!(result.part_count, Some(2));
        assert_eq!(result.parts[0].duration.as_deref(), Some("24m43s"));
        assert_eq!(result.parts[1].title, "P2: 第二课：高中低杆");
        assert_eq!(
            result.parts[1].video_streams[0].codec.as_deref(),
            Some("hevc")
        );
    }
}
