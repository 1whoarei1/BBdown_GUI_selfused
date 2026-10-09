//! WBI request signing, adapted from the BBDown protocol implementation.
use std::collections::BTreeMap;

const MIXIN: [usize; 32] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13,
];

pub fn mixin_key(img_url: &str, sub_url: &str) -> Result<String, String> {
    fn stem(url: &str) -> &str {
        url.rsplit('/')
            .next()
            .unwrap_or("")
            .split('.')
            .next()
            .unwrap_or("")
    }
    let key = format!("{}{}", stem(img_url), stem(sub_url));
    if key.len() < 64 || !key.is_ascii() {
        return Err("B 站返回了无效的 WBI 密钥".to_string());
    }
    Ok(MIXIN
        .iter()
        .map(|&index| key.as_bytes()[index] as char)
        .collect())
}

pub fn sign(mut params: BTreeMap<String, String>, key: &str, timestamp: u64) -> String {
    params.insert("wts".to_string(), timestamp.to_string());
    let query = params
        .iter()
        .map(|(name, value)| {
            let filtered = value
                .chars()
                .filter(|ch| !matches!(ch, '!' | '\'' | '(' | ')' | '*'))
                .collect::<String>();
            format!("{}={}", encode(name), encode(&filtered))
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{query}&w_rid={:x}", md5::compute(format!("{query}{key}")))
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_known_protocol_vector() {
        let key = mixin_key(
            "https://i0.hdslb.com/bfs/wbi/7cd084941338484aae1ad9425b84077c.png",
            "https://i0.hdslb.com/bfs/wbi/4932caff0ff746eab6f01bf08b70ac45.png",
        )
        .unwrap();
        assert_eq!(key, "ea1db124af3c7062474693fa704f4ff8");
        let params = [("foo", "114"), ("bar", "514"), ("baz", "1919810")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(
            sign(params, &key, 1702204169),
            "bar=514&baz=1919810&foo=114&wts=1702204169&w_rid=6149fdadf571698ca7e6a567265cd0ee"
        );
    }

    #[test]
    fn escapes_query_values_and_rejects_short_keys() {
        assert!(mixin_key("a.png", "b.png").is_err());
        let query = sign(
            [("title".to_string(), "中文 !'()*".to_string())].into(),
            "key",
            1,
        );
        assert!(query.starts_with("title=%E4%B8%AD%E6%96%87%20&wts=1&"));
    }
}
