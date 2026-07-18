#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoId {
    Bvid(String),
    Aid(u64),
}

pub fn normalize_video_input(input: &str) -> Option<VideoId> {
    extract_bvid(input)
        .map(VideoId::Bvid)
        .or_else(|| extract_aid(input).map(VideoId::Aid))
}

fn extract_bvid(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    for index in 0..bytes.len().saturating_sub(1) {
        if !bytes[index].eq_ignore_ascii_case(&b'b')
            || !bytes[index + 1].eq_ignore_ascii_case(&b'v')
        {
            continue;
        }
        if index > 0 && bytes[index - 1].is_ascii_alphanumeric() {
            continue;
        }

        let end = bytes[index..]
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric())
            .count()
            + index;
        let candidate = &input[index..end];
        if candidate.len() == 12 {
            return Some(format!("BV{}", &candidate[2..]));
        }
    }
    None
}

fn extract_aid(input: &str) -> Option<u64> {
    let bytes = input.as_bytes();
    for index in 0..bytes.len().saturating_sub(2) {
        if !bytes[index].eq_ignore_ascii_case(&b'a')
            || !bytes[index + 1].eq_ignore_ascii_case(&b'v')
            || !bytes[index + 2].is_ascii_digit()
        {
            continue;
        }
        if index > 0 && bytes[index - 1].is_ascii_alphanumeric() {
            continue;
        }

        let digits = bytes[index + 2..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        return input[index + 2..index + 2 + digits].parse().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bvid_and_discards_tracking_query() {
        let input = "https://www.bilibili.com/video/BV1btKG6PEKU?spm_id_from=333&vd_source=secret";

        assert_eq!(
            normalize_video_input(input),
            Some(VideoId::Bvid("BV1btKG6PEKU".to_string()))
        );
    }

    #[test]
    fn normalizes_lowercase_bvid_prefix() {
        assert_eq!(
            normalize_video_input("bv1btKG6PEKU"),
            Some(VideoId::Bvid("BV1btKG6PEKU".to_string()))
        );
    }

    #[test]
    fn extracts_aid_from_video_url() {
        assert_eq!(
            normalize_video_input("https://www.bilibili.com/video/av170001"),
            Some(VideoId::Aid(170001))
        );
    }

    #[test]
    fn rejects_unrelated_text() {
        assert_eq!(normalize_video_input("search words"), None);
    }
}
