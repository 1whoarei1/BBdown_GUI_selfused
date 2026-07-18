use super::VideoMetadata;
use crate::core::{
    ContentKind, MetadataSource, OwnerInfo, ParseResult, ParseResultV2, PartInfo, PartInfoV2,
};

pub fn assemble_parse_result_v2(
    legacy: ParseResult,
    metadata: Option<VideoMetadata>,
    metadata_warning: Option<String>,
) -> ParseResultV2 {
    let mut warnings = legacy.warnings.clone();
    if let Some(warning) = metadata_warning {
        warnings.push(warning);
    }

    match metadata {
        Some(metadata) => assemble_with_metadata(legacy, metadata, warnings),
        None => assemble_bbdown_only(legacy, warnings),
    }
}

fn assemble_with_metadata(
    legacy: ParseResult,
    metadata: VideoMetadata,
    warnings: Vec<String>,
) -> ParseResultV2 {
    let mut legacy_parts = legacy.parts.clone();
    let mut parts = metadata
        .pages
        .iter()
        .map(|page| {
            let legacy_index = legacy_parts
                .iter()
                .position(|part| part.cid.as_deref() == Some(&page.cid.to_string()))
                .or_else(|| {
                    legacy_parts
                        .iter()
                        .position(|part| part.page_number == page.page_number)
                });
            let streams = legacy_index
                .map(|index| legacy_parts.remove(index))
                .unwrap_or_else(|| empty_legacy_part(page.page_number));
            PartInfoV2 {
                page_number: page.page_number,
                cid: Some(page.cid),
                title: page.title.clone(),
                duration_seconds: Some(page.duration_seconds),
                video_streams: streams.video_streams,
                audio_streams: streams.audio_streams,
            }
        })
        .collect::<Vec<_>>();
    parts.extend(legacy_parts.into_iter().map(legacy_part_to_v2));
    parts.sort_by_key(|part| part.page_number);

    ParseResultV2 {
        schema_version: 2,
        id: legacy.id,
        input: legacy.input,
        content_kind: if metadata.pages.len() > 1 {
            ContentKind::MultiPartVideo
        } else {
            ContentKind::SingleVideo
        },
        aid: Some(metadata.aid),
        bvid: Some(metadata.bvid),
        title: metadata.title,
        description: non_empty(metadata.description),
        owner: Some(OwnerInfo {
            mid: Some(metadata.owner.mid),
            name: non_empty(metadata.owner.name),
            face_url: non_empty(metadata.owner.face_url),
            space_url: Some(format!("https://space.bilibili.com/{}", metadata.owner.mid)),
        }),
        publish_time: legacy.publish_time,
        duration_seconds: Some(metadata.duration_seconds),
        cover_url: non_empty(metadata.cover_url),
        parts,
        metadata_source: MetadataSource::BilibiliAndBbdown,
        warnings,
        error_message: legacy.error_message,
    }
}

fn assemble_bbdown_only(legacy: ParseResult, warnings: Vec<String>) -> ParseResultV2 {
    let owner = if legacy.owner_name.is_some() || legacy.owner_space_url.is_some() {
        Some(OwnerInfo {
            mid: legacy
                .owner_space_url
                .as_deref()
                .and_then(|value| value.rsplit('/').next())
                .and_then(|value| value.parse().ok()),
            name: legacy.owner_name.clone(),
            face_url: None,
            space_url: legacy.owner_space_url.clone(),
        })
    } else {
        None
    };

    ParseResultV2 {
        schema_version: 2,
        id: legacy.id,
        input: legacy.input,
        content_kind: legacy.content_kind,
        aid: legacy
            .aid_or_episode_id
            .as_deref()
            .and_then(|value| value.parse().ok()),
        bvid: legacy.bvid,
        title: legacy.title,
        description: None,
        owner,
        publish_time: legacy.publish_time,
        duration_seconds: legacy.duration.as_deref().and_then(parse_duration_seconds),
        cover_url: None,
        parts: legacy.parts.into_iter().map(legacy_part_to_v2).collect(),
        metadata_source: MetadataSource::BbdownOnly,
        warnings,
        error_message: legacy.error_message,
    }
}

fn legacy_part_to_v2(part: PartInfo) -> PartInfoV2 {
    PartInfoV2 {
        page_number: part.page_number,
        cid: part.cid.and_then(|value| value.parse().ok()),
        title: part
            .title
            .strip_prefix(&format!("P{}:", part.page_number))
            .unwrap_or(&part.title)
            .trim()
            .to_string(),
        duration_seconds: part.duration.as_deref().and_then(parse_duration_seconds),
        video_streams: part.video_streams,
        audio_streams: part.audio_streams,
    }
}

fn empty_legacy_part(page_number: u32) -> PartInfo {
    PartInfo {
        page_number,
        cid: None,
        title: String::new(),
        duration: None,
        video_streams: vec![],
        audio_streams: vec![],
    }
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn parse_duration_seconds(value: &str) -> Option<u64> {
    let mut total = 0_u64;
    let mut number = String::new();
    let mut matched = false;
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            number.push(ch);
            continue;
        }
        let multiplier = match ch.to_ascii_lowercase() {
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => continue,
        };
        let component = number.parse::<u64>().ok()?;
        total += component * multiplier;
        number.clear();
        matched = true;
    }
    matched.then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bbdown::parser::parse_bbdown_output;
    use crate::bilibili::metadata::parse_view_response;

    #[test]
    fn merges_metadata_and_streams_by_cid() {
        let bbdown = include_str!("../../tests/fixtures/bbdown_info_real_single_video.txt");
        let view = include_str!("../../tests/fixtures/bilibili_view_single_video.json");
        let legacy = parse_bbdown_output("BV1btKG6PEKU".to_string(), bbdown.to_string());
        let metadata = parse_view_response(view).expect("metadata fixture should parse");

        let result = assemble_parse_result_v2(legacy, Some(metadata), None);

        assert_eq!(result.schema_version, 2);
        assert_eq!(result.title, "世 纪 辐 射【上】");
        assert_eq!(
            result
                .owner
                .as_ref()
                .and_then(|owner| owner.name.as_deref()),
            Some("食贫道")
        );
        assert_eq!(result.parts.len(), 1);
        assert_eq!(result.parts[0].cid, Some(40002191561));
        assert_eq!(result.parts[0].duration_seconds, Some(7444));
        assert_eq!(result.parts[0].video_streams.len(), 2);
        assert_eq!(result.parts[0].audio_streams.len(), 1);
        let serialized = serde_json::to_value(&result).expect("result should serialize");
        assert_eq!(serialized["metadataSource"], "bilibiliAndBbdown");
    }

    #[test]
    fn keeps_bbdown_result_when_metadata_is_unavailable() {
        let bbdown = include_str!("../../tests/fixtures/bbdown_info_real_single_video.txt");
        let legacy = parse_bbdown_output("BV1btKG6PEKU".to_string(), bbdown.to_string());

        let result =
            assemble_parse_result_v2(legacy, None, Some("元数据接口暂时不可用".to_string()));

        assert!(matches!(result.metadata_source, MetadataSource::BbdownOnly));
        assert_eq!(result.parts[0].cid, Some(40002191561));
        assert_eq!(result.parts[0].duration_seconds, Some(7444));
        assert!(result
            .warnings
            .iter()
            .any(|item| item.contains("暂时不可用")));
    }
}
