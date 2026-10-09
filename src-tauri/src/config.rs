use crate::core::AppConfig;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};

use super::tools::candidate_roots;
use crate::core::{candidate_existing_file, default_config_for_paths};

pub fn load_config(app: &AppHandle) -> Result<AppConfig, String> {
    let path = config_file_path(app)?;
    if !path.exists() {
        let config = detected_default_config(app);
        write_config_path(&path, &config)?;
        return Ok(config);
    }

    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("读取配置文件失败 {}: {error}", path.display()))?;
    let mut config = serde_json::from_str::<AppConfig>(&raw)
        .map_err(|error| format!("配置文件格式错误 {}: {error}", path.display()))?;
    // Import an existing WEB login once, before dropping the obsolete tool paths.
    let legacy: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if let Some(old_path) = legacy["tools"]["bbdownPath"].as_str() {
        let account_path = account_file_path(app)?;
        if !account_path.exists() {
            if let Some(directory) = Path::new(old_path).parent() {
                if let Ok(cookie) = fs::read_to_string(directory.join("BBDown.data")) {
                    if let Some(cookie) = crate::bilibili::login::normalize_legacy_cookie(&cookie) {
                        write_scan_cookie(app, &cookie)?;
                    }
                }
            }
        }
    }
    if let Some(old_path) = legacy["tools"]["bbdownPath"].as_str() {
        let account = read_scan_account(&account_file_path(app)?)?;
        if account.access_token.is_none() {
            if let Some(directory) = Path::new(old_path).parent() {
                if let Ok(raw) = fs::read_to_string(directory.join("BBDownTV.data")) {
                    if let Some(token) = normalize_token(&raw) {
                        write_tv_token(app, &token, None, None)?;
                    }
                }
            }
        }
    }
    let healed = heal_missing_tool_paths(app, &mut config);
    let changed = healed || serde_json::to_value(&config).map_err(|e| e.to_string())? != legacy;
    if changed {
        write_config_path(&path, &config)?;
    }
    Ok(config)
}

pub fn write_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = config_file_path(app)?;
    write_config_path(&path, config)
}

pub fn write_config_path(path: &Path, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建配置目录失败 {}: {error}", parent.display()))?;
    }

    let json =
        serde_json::to_string_pretty(config).map_err(|error| format!("序列化配置失败: {error}"))?;
    fs::write(path, json).map_err(|error| format!("写入配置文件失败 {}: {error}", path.display()))
}

pub fn config_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("获取应用配置目录失败: {error}"))?;
    Ok(dir.join("config.json"))
}

pub fn detected_default_config(app: &AppHandle) -> AppConfig {
    let roots = candidate_roots(app);
    let ffmpeg_path = candidate_existing_file(
        roots
            .iter()
            .flat_map(|root| [root.join("bin").join("ffmpeg.exe"), root.join("ffmpeg.exe")]),
    );
    let work_dir = app
        .path()
        .download_dir()
        .map(|path| path.join("BBDown Next").to_string_lossy().to_string())
        .unwrap_or_else(|_| "../download".to_string());

    default_config_for_paths(ffmpeg_path.or_else(|| Some("ffmpeg".into())), work_dir)
}

pub fn heal_missing_tool_paths(app: &AppHandle, config: &mut AppConfig) -> bool {
    let detected = detected_default_config(app);
    let mut changed = false;

    let ffmpeg_missing = config
        .tools
        .ffmpeg_path
        .as_deref()
        .is_none_or(|path| path.is_empty() || !Path::new(path).exists());
    if ffmpeg_missing
        && detected
            .tools
            .ffmpeg_path
            .as_deref()
            .is_some_and(|path| Path::new(path).exists())
    {
        config.tools.ffmpeg_path = detected.tools.ffmpeg_path;
        changed = true;
    }

    changed
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ScanAccount {
    cookie: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
}

pub fn account_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_file_path(app)?.with_file_name("account.json"))
}

fn read_scan_account(path: &Path) -> Result<ScanAccount, String> {
    match fs::read(path) {
        Ok(raw) => serde_json::from_slice(&raw).map_err(|e| format!("账号信息格式错误: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ScanAccount::default()),
        Err(e) => Err(format!("读取账号信息失败: {e}")),
    }
}

pub fn account_cookie(
    app: &AppHandle,
    config: &AppConfig,
) -> Result<Option<(String, String)>, String> {
    if let Some(cookie) = config
        .auth
        .cookie
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        return Ok(Some((cookie.trim().into(), "manualCookie".into())));
    }
    let account = read_scan_account(&account_file_path(app)?)?;
    Ok((!account.cookie.is_empty()).then_some((account.cookie, "nativeScan".into())))
}

pub fn account_token(app: &AppHandle, config: &AppConfig) -> Result<Option<String>, String> {
    if let Some(token) = config
        .auth
        .access_token
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        return normalize_token(token)
            .map(Some)
            .ok_or_else(|| "Token 格式无效".into());
    }
    // International access keys belong to Bstar and must not reuse domestic TV credentials.
    if !matches!(
        config.auth.api_mode,
        crate::core::ApiMode::Tv | crate::core::ApiMode::App
    ) {
        return Ok(None);
    }
    let account = read_scan_account(&account_file_path(app)?)?;
    if account.access_token.is_some()
        && account
            .expires_at
            .is_some_and(|t| t <= crate::bilibili::client::timestamp())
    {
        return Err("TV / APP 登录凭据已过期，请重新扫码或填写 Token".into());
    }
    Ok(account.access_token)
}

pub fn normalize_token(value: &str) -> Option<String> {
    let value = value
        .trim()
        .strip_prefix("access_token=")
        .unwrap_or(value.trim());
    (!value.is_empty() && !value.chars().any(|c| c.is_control() || c.is_whitespace()))
        .then(|| value.to_string())
}

fn write_scan_account(path: &Path, account: &ScanAccount) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("账号路径无效")?).map_err(|e| e.to_string())?;
    let raw = serde_json::to_vec(account).map_err(|e| e.to_string())?;
    let temp = crate::bilibili::transfer::StagingFile(
        path.with_file_name(format!(".account-{}.part", uuid::Uuid::new_v4())),
    );
    fs::write(&temp.0, raw).map_err(|e| format!("保存账号信息失败: {e}"))?;
    fs::rename(&temp.0, path).map_err(|e| format!("保存账号信息失败: {e}"))
}

pub fn write_scan_cookie(app: &AppHandle, cookie: &str) -> Result<(), String> {
    let path = account_file_path(app)?;
    let mut account = read_scan_account(&path)?;
    account.cookie = cookie.into();
    write_scan_account(&path, &account)
}

pub fn write_tv_token(
    app: &AppHandle,
    token: &str,
    refresh: Option<String>,
    expires: Option<u64>,
) -> Result<(), String> {
    let path = account_file_path(app)?;
    let mut account = read_scan_account(&path)?;
    account.access_token = Some(token.into());
    account.refresh_token = refresh;
    account.expires_at = expires;
    write_scan_account(&path, &account)
}

pub fn clear_scan_cookie(app: &AppHandle) -> Result<(), String> {
    match fs::remove_file(account_file_path(app)?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("删除账号信息失败: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supports_old_cookie_file_and_preserves_independent_credentials() {
        let directory = crate::bilibili::test_support::TestDirectory::new();
        let path = directory.0.join("account.json");
        fs::write(&path, r#"{"cookie":"SESSDATA=fixture"}"#).unwrap();
        let mut account = read_scan_account(&path).unwrap();
        account.access_token = Some("fixture-tv-token".into());
        write_scan_account(&path, &account).unwrap();
        let account = read_scan_account(&path).unwrap();
        assert_eq!(account.cookie, "SESSDATA=fixture");
        assert_eq!(account.access_token.as_deref(), Some("fixture-tv-token"));
        assert_eq!(
            normalize_token("access_token=fixture"),
            Some("fixture".into())
        );
        assert!(normalize_token("bad\ntoken").is_none());
    }
}
