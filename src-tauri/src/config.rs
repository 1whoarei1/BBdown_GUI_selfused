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
    let changed = heal_missing_tool_paths(app, &mut config);
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
    let bbdown_path = candidate_existing_file(
        roots
            .iter()
            .flat_map(|root| [root.join("bin").join("BBDown.exe"), root.join("BBDown.exe")]),
    )
    .unwrap_or_else(|| "../bin/BBDown.exe".to_string());
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

    default_config_for_paths(bbdown_path, ffmpeg_path, work_dir)
}

pub fn heal_missing_tool_paths(app: &AppHandle, config: &mut AppConfig) -> bool {
    let detected = detected_default_config(app);
    let mut changed = false;

    if !Path::new(&config.tools.bbdown_path).exists()
        && Path::new(&detected.tools.bbdown_path).exists()
    {
        config.tools.bbdown_path = detected.tools.bbdown_path;
        changed = true;
    }

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

    if !Path::new(&config.work_dir).exists() && Path::new(&detected.work_dir).exists() {
        config.work_dir = detected.work_dir;
        changed = true;
    }

    changed
}
