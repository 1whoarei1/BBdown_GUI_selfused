use crate::core::{AppConfig, ToolDetectionResult, ToolPaths};
use crate::utils::{decode_output, hide_console_window};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tokio::process::Command;

pub fn resolve_ffmpeg(tools: &ToolPaths) -> Option<PathBuf> {
    let configured = tools
        .ffmpeg_path
        .as_deref()
        .filter(|s| !s.trim().is_empty());
    if let Some(value) = configured {
        let path = Path::new(value);
        if path.is_file() {
            return path.canonicalize().ok();
        }
        // An explicit path must never silently select a different executable.
        if path.components().count() > 1 || path.is_absolute() {
            return None;
        }
    }
    let name = configured.unwrap_or("ffmpeg");
    let candidates = if Path::new(name).extension().is_some() {
        vec![name.to_string()]
    } else if cfg!(windows) {
        vec![format!("{name}.exe")]
    } else {
        vec![name.to_string()]
    };
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .flat_map(|root| candidates.iter().map(move |name| root.join(name)))
        .find(|path| path.is_file())
        .and_then(|path| path.canonicalize().ok())
}

pub async fn detect_tools(config: AppConfig) -> Result<ToolDetectionResult, String> {
    let version = if let Some(path) = resolve_ffmpeg(&config.tools) {
        let mut command = Command::new(path);
        command.arg("-version").kill_on_drop(true);
        hide_console_window(&mut command);
        match tokio::time::timeout(std::time::Duration::from_secs(5), command.output()).await {
            Ok(Ok(output)) if output.status.success() => {
                decode_output(&output.stdout, &output.stderr)
                    .lines()
                    .find(|line| line.starts_with("ffmpeg version"))
                    .map(str::to_string)
            }
            _ => None,
        }
    } else {
        None
    };
    Ok(ToolDetectionResult {
        core_available: true,
        ffmpeg_found: version.is_some(),
        ffmpeg_version: version.clone(),
        messages: vec![
            "内置下载核心已就绪".to_string(),
            version.unwrap_or_else(|| {
                "未找到可用 FFmpeg；解析、登录及单独下载原始流不需要 FFmpeg".to_string()
            }),
        ],
    })
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
