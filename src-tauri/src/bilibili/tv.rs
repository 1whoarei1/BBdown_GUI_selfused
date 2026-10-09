//! Android TV parameter signing, compatible with BBDown's TV requests.
use super::client::{timestamp, BiliClient, ContentPart};
use serde_json::Value;
use std::collections::BTreeMap;

const APP_KEY: &str = "4409e2ce8ffd12b8";
const APP_SECRET: &str = "59b43e04ad6965f34319062b478f83dd";

pub fn signed_params(mut params: BTreeMap<String, String>) -> BTreeMap<String, String> {
    params.remove("sign");
    params.insert("appkey".into(), APP_KEY.into());
    let query = reqwest::Url::parse_with_params("https://api.bilibili.com/", &params)
        .unwrap()
        .query()
        .unwrap()
        .to_string();
    params.insert(
        "sign".into(),
        format!("{:x}", md5::compute(format!("{query}{APP_SECRET}"))),
    );
    params
}

pub async fn play(client: &BiliClient, part: &ContentPart) -> Result<Value, String> {
    if part.cheese {
        return Err("TV 接口不支持课程，请使用 WEB 模式".into());
    }
    let mut params: BTreeMap<String, String> = [
        ("build", "106500"),
        ("device", "android"),
        ("fnval", "4048"),
        ("fnver", "0"),
        ("fourk", "1"),
        ("mid", "0"),
        ("mobi_app", "android_tv_yst"),
        ("platform", "android"),
        ("playurl_type", "1"),
        ("qn", "127"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    params.insert("cid".into(), part.cid.to_string());
    params.insert("object_id".into(), part.aid.to_string());
    params.insert("ts".into(), timestamp().to_string());
    if let Some(token) = &client.token {
        params.insert("access_key".into(), token.clone());
    }
    let path = if let Some(ep) = part.episode_id {
        params.insert("ep_id".into(), ep.to_string());
        params.insert("expire".into(), "0".into());
        "/pgc/player/api/playurltv"
    } else {
        "/x/tv/playurl"
    };
    client
        .api_map(
            &format!("https://api.snm0516.aisee.tv{path}"),
            &signed_params(params),
        )
        .await
}

pub fn login_params() -> BTreeMap<String, String> {
    let device = uuid::Uuid::new_v4().simple().to_string();
    let buvid = uuid::Uuid::new_v4().simple().to_string();
    let fingerprint = format!("{}{}", timestamp(), uuid::Uuid::new_v4().simple());
    let mut params: BTreeMap<String, String> = [
        ("auth_code", ""),
        ("build", "102801"),
        ("channel", "master"),
        ("device", "OnePlus"),
        ("device_name", "OnePlus7TPro"),
        ("device_platform", "Android10OnePlusHD1910"),
        ("mobi_app", "android_tv_yst"),
        ("networkstate", "wifi"),
        ("platform", "android"),
        ("sys_ver", "29"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    for name in ["bili_local_id", "device_id"] {
        params.insert(name.into(), device.clone());
    }
    for name in ["buvid", "guid", "local_id"] {
        params.insert(name.into(), buvid.clone());
    }
    for name in ["fingerprint", "local_fingerprint"] {
        params.insert(name.into(), fingerprint.clone());
    }
    params.insert("ts".into(), timestamp().to_string());
    signed_params(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signing_is_sorted_encoded_and_replaces_old_signature() {
        let params = signed_params(
            [
                ("ts".into(), "1".into()),
                ("access_key".into(), "a+b".into()),
                ("sign".into(), "old".into()),
            ]
            .into(),
        );
        let independent = format!(
            "{:x}",
            md5::compute(
                b"access_key=a%2Bb&appkey=4409e2ce8ffd12b8&ts=159b43e04ad6965f34319062b478f83dd"
            )
        );
        assert_eq!(params["sign"], independent);
        assert!(!params["sign"].contains("old"));
    }
}
