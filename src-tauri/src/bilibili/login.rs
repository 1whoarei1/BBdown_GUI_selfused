use super::{
    client::{check_api, network_error, string, BiliClient},
    engine::TaskContext,
};
use crate::{
    core::AppConfig,
    task::{emit_login_qr, LoginQrEvent, TaskPhase},
};
use reqwest::header::{HeaderMap, SET_COOKIE};
use std::{collections::BTreeMap, path::Path};
use tokio::fs;

const COOKIE_NAMES: [&str; 5] = [
    "DedeUserID",
    "DedeUserID__ckMd5",
    "Expires",
    "SESSDATA",
    "bili_jct",
];
const WEB_LOGIN_SOURCE: &str = "main-fe-header";

async fn login_web(
    config: &AppConfig,
    directory: &Path,
    context: &TaskContext,
) -> Result<String, String> {
    let client = BiliClient::new(config)?;
    login_web_client(&client, directory, context).await
}

async fn login_web_client(
    client: &BiliClient,
    directory: &Path,
    context: &TaskContext,
) -> Result<String, String> {
    context.report(TaskPhase::LoggingIn, "正在获取登录二维码")?;
    let value = client
        .api(
            "https://passport.bilibili.com/x/passport-login/web/qrcode/generate",
            &[("source", WEB_LOGIN_SOURCE.into())],
        )
        .await?;
    check_api(&value)?;
    let key = string(&value["data"]["qrcode_key"]);
    let url = string(&value["data"]["url"]);
    if key.is_empty() || url.is_empty() {
        return Err("登录接口没有返回二维码".to_string());
    }
    let _cleanup = show_qr(directory, url, "web", context).await?;
    let mut confirmed = false;
    for _ in 0..180 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        context.checkpoint()?;
        let response = client
            .api_response(
                "https://passport.bilibili.com/x/passport-login/web/qrcode/poll",
                &[
                    ("qrcode_key", key.to_string()),
                    ("source", WEB_LOGIN_SOURCE.into()),
                ],
            )
            .await?;
        let headers = response.headers().clone();
        let result: serde_json::Value = response.json().await.map_err(network_error)?;
        check_api(&result)?;
        match result["data"]["code"].as_i64() {
            Some(0) => {
                let cookie = cookie_from_response(&headers, string(&result["data"]["url"]))?;
                context.report(TaskPhase::LoggingIn, "扫码确认成功，正在保存登录凭据")?;
                return Ok(cookie);
            }
            Some(86101) => {}
            Some(86090) if !confirmed => {
                context.report(TaskPhase::LoggingIn, "扫码成功，请在手机上确认登录")?;
                confirmed = true;
            }
            Some(86090) => {}
            Some(86038) => return Err("二维码已过期，请重新生成".to_string()),
            code => return Err(format!("登录失败，状态码: {code:?}")),
        }
    }
    Err("等待扫码超时，请重新生成二维码".to_string())
}

pub enum LoginCredentials {
    Web(String),
    Tv {
        access_token: String,
        refresh_token: Option<String>,
        expires_at: Option<u64>,
    },
}

pub async fn login(
    config: &AppConfig,
    directory: &Path,
    mode: &str,
    context: &TaskContext,
) -> Result<LoginCredentials, String> {
    match mode {
        "web" => login_web(config, directory, context)
            .await
            .map(LoginCredentials::Web),
        "tv" => login_tv(config, directory, context).await,
        _ => Err("请选择 WEB 或 TV 扫码登录".into()),
    }
}

async fn show_qr(
    directory: &Path,
    url: &str,
    mode: &str,
    context: &TaskContext,
) -> Result<super::transfer::StagingFile, String> {
    fs::create_dir_all(directory)
        .await
        .map_err(|e| format!("创建账号目录失败: {e}"))?;
    let qr = qrcode::QrCode::new(url.as_bytes()).map_err(|e| format!("生成二维码失败: {e}"))?;
    let image = qr
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(280, 280)
        .build();
    let qr_path = directory.join(format!("login-{}.svg", context.id));
    let cleanup = super::transfer::StagingFile(qr_path.clone());
    fs::write(&qr_path, image)
        .await
        .map_err(|e| format!("保存二维码失败: {e}"))?;
    context.checkpoint()?;
    if let Some(app) = &context.app {
        emit_login_qr(
            app,
            &LoginQrEvent {
                task_id: context.id.clone(),
                mode: mode.to_string(),
                image_path: qr_path.to_string_lossy().to_string(),
                data_path: directory.join("account.json").to_string_lossy().to_string(),
                timestamp: crate::task::now_timestamp(),
            },
        )?;
    }
    Ok(cleanup)
}

async fn login_tv(
    config: &AppConfig,
    directory: &Path,
    context: &TaskContext,
) -> Result<LoginCredentials, String> {
    let client = BiliClient::new(config)?;
    login_tv_client(&client, directory, context).await
}

async fn login_tv_client(
    client: &BiliClient,
    directory: &Path,
    context: &TaskContext,
) -> Result<LoginCredentials, String> {
    context.report(TaskPhase::LoggingIn, "正在获取 TV / APP 登录二维码")?;
    let mut params = super::tv::login_params();
    let value = client
        .post_form(
            "https://passport.snm0516.aisee.tv/x/passport-tv-login/qrcode/auth_code",
            &params,
        )
        .await?;
    check_api(&value)?;
    let code = string(&value["data"]["auth_code"]);
    let url = string(&value["data"]["url"]);
    if code.is_empty() || url.is_empty() {
        return Err("TV 登录接口没有返回二维码".into());
    }
    let _cleanup = show_qr(directory, url, "tv", context).await?;
    params.insert("auth_code".into(), code.into());
    for _ in 0..180 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        context.checkpoint()?;
        params.insert("ts".into(), super::client::timestamp().to_string());
        params = super::tv::signed_params(params);
        let value = client
            .post_form(
                "https://passport.bilibili.com/x/passport-tv-login/qrcode/poll",
                &params,
            )
            .await?;
        match value["code"].as_i64() {
            Some(86039) | Some(86101) => {}
            Some(86090) => context.report(TaskPhase::LoggingIn, "扫码成功，请在手机上确认")?,
            Some(86038) => return Err("TV 二维码已过期，请重新生成".into()),
            Some(0) => {
                let token = string(&value["data"]["access_token"]);
                if token.is_empty() {
                    return Err("TV 登录响应缺少 access_token".into());
                }
                let refresh = value["data"]["refresh_token"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let expires = value["data"]["expires_in"]
                    .as_u64()
                    .filter(|n| *n > 0)
                    .map(|n| super::client::timestamp().saturating_add(n));
                return Ok(LoginCredentials::Tv {
                    access_token: token.into(),
                    refresh_token: refresh,
                    expires_at: expires,
                });
            }
            _ => {
                check_api(&value)?;
                return Err("TV 登录响应缺少状态码".into());
            }
        }
    }
    Err("TV 扫码等待超时，请重新生成二维码".into())
}

#[cfg(test)]
fn cookie_from_url(url: &str) -> Result<String, String> {
    cookie_from_response(&HeaderMap::new(), url)
}

fn cookie_from_response(headers: &HeaderMap, callback: &str) -> Result<String, String> {
    let mut cookies = url_cookies(callback).unwrap_or_default();
    // HTTP headers carry the new session when the callback URL contains only redirect data.
    // Inspect each Set-Cookie header separately: Expires attributes may contain commas.
    for header in headers.get_all(SET_COOKIE) {
        let Ok(header) = header.to_str() else {
            continue;
        };
        let pair = header.split(';').next().unwrap_or("");
        let Some((name, value)) = pair.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if COOKIE_NAMES.contains(&name) && !value.is_empty() && value != "\"\"" {
            cookies.insert(name.into(), value.replace(',', "%2C"));
        }
    }
    if !cookies.contains_key("SESSDATA") {
        return Err("登录确认成功，但响应头和回调地址均缺少 SESSDATA，请重新扫码".into());
    }
    Ok(COOKIE_NAMES
        .iter()
        .filter_map(|name| cookies.get(*name).map(|value| format!("{name}={value}")))
        .collect::<Vec<_>>()
        .join("; "))
}

fn url_cookies(url: &str) -> Result<BTreeMap<String, String>, String> {
    let url = reqwest::Url::parse(url).map_err(|_| "登录接口返回了无效地址".to_string())?;
    // Keep percent encoding, especially SESSDATA's commas and percent escapes.
    let entries = url
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|part| {
            let (name, value) = part.split_once('=')?;
            (COOKIE_NAMES.contains(&name) && !value.is_empty())
                .then(|| (name.into(), value.replace(',', "%2C")))
        })
        .collect();
    Ok(entries)
}

pub fn normalize_legacy_cookie(raw: &str) -> Option<String> {
    let entries = raw
        .split(';')
        .filter_map(|item| {
            let (name, value) = item.trim().split_once('=')?;
            (COOKIE_NAMES.contains(&name) && !value.trim().is_empty())
                .then(|| format!("{name}={}", value.trim()))
        })
        .collect::<Vec<_>>();
    entries
        .iter()
        .any(|v| v.starts_with("SESSDATA="))
        .then(|| entries.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn web_login_reads_success_cookies_from_headers_after_phone_confirmation() {
        use crate::bilibili::test_support::{context, JsonServer, TestDirectory};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let polls = AtomicUsize::new(0);
        let server = JsonServer::new_with_headers(move |url, _, _| {
            let query = url.query_pairs().collect::<BTreeMap<_, _>>();
            assert_eq!(query.get("source").map(|s| s.as_ref()), Some(WEB_LOGIN_SOURCE));
            match url.path() {
                "/x/passport-login/web/qrcode/generate" => (serde_json::json!({"code":0,"data":{"qrcode_key":"fixture-key","url":"https://passport.bilibili.com/?qrcode_key=fixture-key"}}), vec![]),
                "/x/passport-login/web/qrcode/poll" => {
                    assert_eq!(query["qrcode_key"], "fixture-key");
                    match polls.fetch_add(1, Ordering::SeqCst) {
                        0 => (serde_json::json!({"code":0,"data":{"code":86101}}), vec![]),
                        1 => (serde_json::json!({"code":0,"data":{"code":86090}}), vec![]),
                        _ => (serde_json::json!({"code":0,"data":{"code":0,"url":"https://passport.bilibili.com/crossDomain?gourl=https%3A%2F%2Fwww.bilibili.com"}}), vec![
                            ("Set-Cookie".into(), "SESSDATA=header%2Csession%25; Domain=.bilibili.com; Path=/; HttpOnly; Secure; Expires=Fri, 09 Oct 2026 12:00:00 GMT".into()),
                            ("Set-Cookie".into(), "DedeUserID=123; Path=/".into()),
                            ("Set-Cookie".into(), "bili_jct=fixture-csrf; Path=/; SameSite=None".into()),
                        ]),
                    }
                }
                _ => panic!("unexpected fixture endpoint"),
            }
        }).await;
        let mut client = BiliClient::new(&crate::core::default_config()).unwrap();
        client.api_origin = Some(server.url.clone());
        let directory = TestDirectory::new();
        let cookie = login_web_client(&client, &directory.0, &context())
            .await
            .unwrap();
        assert_eq!(
            cookie,
            "DedeUserID=123; SESSDATA=header%2Csession%25; bili_jct=fixture-csrf"
        );
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
    }

    #[test]
    fn web_cookie_headers_override_callback_values_and_preserve_encoding() {
        let mut headers = HeaderMap::new();
        headers.append(
            SET_COOKIE,
            "SESSDATA=new,session%25; Path=/; Expires=Fri, 09 Oct 2026 12:00:00 GMT"
                .parse()
                .unwrap(),
        );
        headers.append(SET_COOKIE, "bili_jct=new-csrf; HttpOnly".parse().unwrap());
        headers.append(SET_COOKIE, "unrelated=ignored; Path=/".parse().unwrap());
        let cookie = cookie_from_response(&headers, "https://passport.bilibili.com/callback?SESSDATA=old-session&DedeUserID=7&bili_jct=old-csrf&gourl=ignored").unwrap();
        assert_eq!(
            cookie,
            "DedeUserID=7; SESSDATA=new%2Csession%25; bili_jct=new-csrf"
        );
        assert!(cookie_from_response(&headers, "").is_ok());
    }

    #[test]
    fn web_login_rejects_missing_or_empty_session_without_exposing_response() {
        let mut headers = HeaderMap::new();
        headers.append(SET_COOKIE, "SESSDATA=; Path=/; Max-Age=0".parse().unwrap());
        headers.append(
            SET_COOKIE,
            "bili_jct=private-fixture; Path=/".parse().unwrap(),
        );
        let error = cookie_from_response(
            &headers,
            "https://passport.bilibili.com/callback?gourl=private-callback",
        )
        .unwrap_err();
        assert!(error.contains("缺少 SESSDATA"));
        assert!(!error.contains("private"));
    }
    #[tokio::test]
    #[ignore = "requires the live TV QR login service; does not confirm a user login"]
    async fn live_tv_qr_generation_and_pending_poll() {
        let client = BiliClient::new(&crate::core::default_config()).unwrap();
        let mut params = super::super::tv::login_params();
        let value = client
            .post_form(
                "https://passport.snm0516.aisee.tv/x/passport-tv-login/qrcode/auth_code",
                &params,
            )
            .await
            .unwrap();
        check_api(&value).unwrap();
        let auth_code = string(&value["data"]["auth_code"]);
        assert!(!auth_code.is_empty());
        assert!(reqwest::Url::parse(string(&value["data"]["url"])).is_ok());
        params.insert("auth_code".into(), auth_code.into());
        params.insert("ts".into(), super::super::client::timestamp().to_string());
        params = super::super::tv::signed_params(params);
        let poll = client
            .post_form(
                "https://passport.bilibili.com/x/passport-tv-login/qrcode/poll",
                &params,
            )
            .await
            .unwrap();
        assert!(
            matches!(poll["code"].as_i64(), Some(86039 | 86101)),
            "unexpected pending login status"
        );
    }
    #[tokio::test]
    async fn tv_login_polls_signed_forms_and_removes_qr_file() {
        use crate::bilibili::test_support::{context, JsonServer, TestDirectory};
        let server=JsonServer::new(|url,body,_|{
            let url_form=reqwest::Url::parse(&format!("https://fixture/?{body}")).unwrap();
            let form=url_form.query_pairs().map(|(k,v)|(k.into_owned(),v.into_owned())).collect::<std::collections::BTreeMap<_,_>>();
            assert_eq!(form["appkey"],"4409e2ce8ffd12b8");
            assert_eq!(form["sign"],super::super::tv::signed_params(form.clone())["sign"]);
            match url.path() {
                "/x/passport-tv-login/qrcode/auth_code"=>serde_json::json!({"code":0,"data":{"url":"https://passport.bilibili.com/?fixture=qr","auth_code":"fixture-code"}}),
                "/x/passport-tv-login/qrcode/poll"=>{assert_eq!(form["auth_code"],"fixture-code");serde_json::json!({"code":0,"data":{"access_token":"fixture-token","refresh_token":"fixture-refresh","expires_in":3600}})},
                _=>panic!("unexpected fixture endpoint"),
            }
        }).await;
        let mut client = BiliClient::new(&crate::core::default_config()).unwrap();
        client.api_origin = Some(server.url.clone());
        let directory = TestDirectory::new();
        let context = context();
        let result = login_tv_client(&client, &directory.0, &context)
            .await
            .unwrap();
        match result {
            LoginCredentials::Tv {
                access_token,
                refresh_token,
                expires_at,
            } => {
                assert_eq!(access_token, "fixture-token");
                assert_eq!(refresh_token.as_deref(), Some("fixture-refresh"));
                assert!(expires_at.unwrap() > super::super::client::timestamp());
            }
            _ => panic!("expected TV credentials"),
        }
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
    }
    #[test]
    fn preserves_cookie_encoding_and_excludes_callback_parameters() {
        let cookie = cookie_from_url("https://passport.bilibili.com/callback?SESSDATA=a%2Cb%25&DedeUserID=1&bili_jct=csrf&gourl=secret").unwrap();
        assert_eq!(cookie, "DedeUserID=1; SESSDATA=a%2Cb%25; bili_jct=csrf");
        assert!(cookie_from_url("https://example.com/?DedeUserID=1").is_err());
        assert!(normalize_legacy_cookie("DedeUserID=1;bili_jct=csrf").is_none());
    }
}
