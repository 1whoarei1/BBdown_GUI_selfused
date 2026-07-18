use crate::core::ToolDetectionResult;
use crate::utils::{decode_output, hide_console_window};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tokio::process::Command;

use crate::core::AppConfig;

pub async fn detect_tools(config: AppConfig) -> Result<ToolDetectionResult, String> {
    let bbdown_found = Path::new(&config.tools.bbdown_path).exists();
    let ffmpeg_found = config
        .tools
        .ffmpeg_path
        .as_deref()
        .filter(|path| !path.is_empty())
        .is_some_and(|path| Path::new(path).exists());
    let mp4box_found = config
        .tools
        .mp4box_path
        .as_deref()
        .filter(|path| !path.is_empty())
        .is_some_and(|path| Path::new(path).exists());
    let aria2c_found = config
        .tools
        .aria2c_path
        .as_deref()
        .filter(|path| !path.is_empty())
        .is_some_and(|path| Path::new(path).exists());

    let mut messages = vec![];
    messages.push(format!(
        "BBDown: {}",
        if bbdown_found { "found" } else { "missing" }
    ));
    messages.push(format!(
        "ffmpeg: {}",
        if ffmpeg_found { "found" } else { "missing" }
    ));

    let bbdown_version = if bbdown_found {
        detect_bbdown_version(&config.tools.bbdown_path).await
    } else {
        None
    };

    Ok(ToolDetectionResult {
        bbdown_found,
        ffmpeg_found,
        mp4box_found,
        aria2c_found,
        bbdown_version,
        messages,
    })
}

pub async fn detect_bbdown_version(path: &str) -> Option<String> {
    let mut command = Command::new(path);
    command.arg("--help");
    hide_console_window(&mut command);
    let output = command.output().await.ok()?;
    let raw_output = decode_output(&output.stdout, &output.stderr);
    raw_output
        .lines()
        .find(|line| line.contains("BBDown version"))
        .map(str::trim)
        .map(ToOwned::to_owned)
}

pub fn candidate_roots(app: &AppHandle) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(current_dir) = std::env::current_dir() {
        push_root_chain(&mut roots, current_dir, 4);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            push_root_chain(&mut roots, exe_dir.to_path_buf(), 6);
        }
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        push_root_chain(&mut roots, resource_dir, 4);
    }
    push_root_chain(&mut roots, PathBuf::from(env!("CARGO_MANIFEST_DIR")), 4);

    roots
}

pub fn push_root_chain(roots: &mut Vec<PathBuf>, start: PathBuf, max_depth: usize) {
    let mut current = Some(start);
    for _ in 0..max_depth {
        let Some(path) = current else {
            break;
        };
        if !roots.iter().any(|existing| existing == &path) {
            roots.push(path.clone());
        }
        current = path.parent().map(|parent| parent.to_path_buf());
    }
}
