use crate::bilibili::{
    engine::{self, ParseCache, TaskContext},
    login, MetadataClient,
};
use crate::config;
use crate::core::{
    AccountInfo, AppConfig, CommandPreview, CommandRunResult, DownloadRequest, ParseRequest,
    ParseResult, ParseResultV2, PartInfo, ToolDetectionResult,
};
use crate::task::{
    emit_task_parse_result, emit_task_status, TaskKind, TaskManager, TaskSnapshot, TaskStatus,
};
use crate::tools;
use std::{future::Future, path::PathBuf};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn get_config(app: AppHandle) -> Result<AppConfig, String> {
    config::load_config(&app)
}

#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    mut config: AppConfig,
) -> Result<AppConfig, String> {
    if let Some(token) = config
        .auth
        .access_token
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        config.auth.access_token = Some(config::normalize_token(token).ok_or("Token 格式无效")?);
    }
    manager.configure(config.download_manager.clone())?;
    config::write_config(&app, &config)?;
    Ok(config)
}

#[tauri::command]
pub async fn detect_tools(config: AppConfig) -> Result<ToolDetectionResult, String> {
    tools::detect_tools(config).await
}

#[tauri::command]
pub async fn get_account_info(app: AppHandle, config: AppConfig) -> Result<AccountInfo, String> {
    let token_configured = config::account_token(&app, &config)?.is_some();
    let (cookie, source) = config::account_cookie(&app, &config)?.unwrap_or_default();
    if cookie.is_empty() {
        return Ok(AccountInfo {
            is_logged_in: false,
            mid: None,
            name: None,
            avatar_url: None,
            vip_label: None,
            source: "none".to_string(),
            token_configured,
        });
    }
    let mut account = MetadataClient::new(config.auth.user_agent.as_deref())?
        .fetch_account(&cookie, &source)
        .await?;
    account.token_configured = token_configured;
    Ok(account)
}

#[tauri::command]
pub async fn logout_account(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    cache: State<'_, ParseCache>,
    mut config: AppConfig,
) -> Result<AppConfig, String> {
    // Cancel pending login before deleting credentials, so it cannot log back in later.
    for task in manager
        .list_tasks()
        .into_iter()
        .filter(|task| matches!(task.kind, TaskKind::LoginWeb | TaskKind::LoginTv))
    {
        let stopped = manager.stop_task(&task.id).await?;
        emit_task_status(&app, &stopped)?;
    }
    config.auth.cookie = None;
    config.auth.access_token = None;
    config::write_config(&app, &config)?;
    config::clear_scan_cookie(&app)?;
    cache.clear();
    Ok(config)
}

#[tauri::command]
pub fn open_download_directory(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    std::fs::create_dir_all(&path)
        .map_err(|error| format!("创建下载目录失败 {}: {error}", path.display()))?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("读取下载目录失败 {}: {error}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!("下载路径不是目录: {}", canonical.display()));
    }
    std::process::Command::new("explorer.exe")
        .arg(&canonical)
        .spawn()
        .map_err(|error| format!("打开下载目录失败: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn open_bilibili_link(url: String) -> Result<(), String> {
    let parsed = reqwest::Url::parse(&url).map_err(|error| format!("链接格式错误: {error}"))?;
    let host = parsed.host_str().unwrap_or_default();
    if parsed.scheme() != "https"
        || !(host == "bilibili.com"
            || host.ends_with(".bilibili.com")
            || host == "bilibili.tv"
            || host.ends_with(".bilibili.tv"))
    {
        return Err("只允许打开 B 站 HTTPS 链接".to_string());
    }
    std::process::Command::new("explorer.exe")
        .arg(parsed.as_str())
        .spawn()
        .map_err(|error| format!("打开链接失败: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn open_tool_download_page(tool: String) -> Result<(), String> {
    let url = tool_download_url(&tool)?;
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map_err(|error| format!("打开工具下载页面失败: {error}"))?;
    Ok(())
}

#[tauri::command]
pub async fn build_preview_command(request: ParseRequest) -> Result<CommandPreview, String> {
    Ok(CommandPreview {
        executable: "内置核心".into(),
        args: vec![],
        display: format!("内置核心解析 {}", request.input),
    })
}

#[tauri::command]
pub async fn build_download_preview(request: DownloadRequest) -> Result<CommandPreview, String> {
    Ok(CommandPreview {
        executable: "内置核心".into(),
        args: vec![],
        display: format!(
            "内置核心下载 {}，分 P: {}，目录: {}",
            request.input, request.config.default_options.page_selection, request.config.work_dir
        ),
    })
}

#[tauri::command]
pub fn list_tasks(manager: State<'_, TaskManager>) -> Result<Vec<TaskSnapshot>, String> {
    Ok(manager.list_tasks())
}

#[tauri::command]
pub async fn stop_task(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    task_id: String,
) -> Result<TaskSnapshot, String> {
    let snapshot = manager.stop_task(&task_id).await?;
    emit_task_status(&app, &snapshot)?;
    Ok(snapshot)
}

pub(crate) fn effective_config(
    app: &AppHandle,
    mut config: AppConfig,
) -> Result<AppConfig, String> {
    config.auth.access_token = config::account_token(app, &config)?;
    config.auth.cookie = config::account_cookie(app, &config)?.map(|(cookie, _)| cookie);
    Ok(config)
}

fn start_task(
    app: &AppHandle,
    manager: &TaskManager,
    kind: TaskKind,
    input: String,
    title: Option<String>,
) -> Result<TaskContext, String> {
    let task = manager.create_task(kind, input, title);
    emit_task_status(app, &task)?;
    let task = manager.mark_running(&task.id)?;
    emit_task_status(app, &task)?;
    Ok(TaskContext {
        app: Some(app.clone()),
        manager: manager.clone(),
        id: task.id,
    })
}

pub(crate) async fn run_task<T>(
    context: &TaskContext,
    future: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let mut signal = context.manager.cancel_signal(&context.id)?;
    context.checkpoint()?;
    let result = tokio::select! {
        biased;
        _ = signal.changed() => Err("任务已取消".to_string()),
        result = future => result,
    };
    let result = result.and_then(|value| {
        context.checkpoint()?;
        Ok(value)
    });
    let snapshot = if context
        .manager
        .get_task(&context.id)
        .is_some_and(|task| task.status == TaskStatus::Canceled)
    {
        context.manager.get_task(&context.id).ok_or("任务已取消")?
    } else {
        match &result {
            Ok(_) => context.manager.mark_completed(&context.id, Some(0))?,
            Err(error) => context
                .manager
                .mark_failed(&context.id, None, error.clone())?,
        }
    };
    if let Some(app) = &context.app {
        crate::task::log_task_outcome(app, &snapshot);
        emit_task_status(app, &snapshot)?;
    }
    result
}

#[tauri::command]
pub async fn run_login(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    cache: State<'_, ParseCache>,
    config: AppConfig,
    mode: String,
) -> Result<CommandRunResult, String> {
    if !matches!(mode.as_str(), "web" | "tv") {
        return Err("请选择 WEB 或 TV 登录".into());
    }
    if manager.list_tasks().iter().any(|task| {
        matches!(task.kind, TaskKind::LoginWeb | TaskKind::LoginTv)
            && matches!(task.status, TaskStatus::Queued | TaskStatus::Running)
    }) {
        return Err("已有扫码登录任务，请等待或停止后重试".into());
    }
    let directory = config::account_file_path(&app)?
        .parent()
        .ok_or("账号路径无效")?
        .to_path_buf();
    let kind = if mode == "tv" {
        TaskKind::LoginTv
    } else {
        TaskKind::LoginWeb
    };
    let context = start_task(&app, &manager, kind, "扫码登录".into(), None)?;
    run_task(&context, async {
        let credentials = login::login(&config, &directory, &mode, &context).await?;
        context.checkpoint()?;
        context
            .manager
            .while_active(&context.id, || match credentials {
                login::LoginCredentials::Web(cookie) => config::write_scan_cookie(&app, &cookie),
                login::LoginCredentials::Tv {
                    access_token,
                    refresh_token,
                    expires_at,
                } => config::write_tv_token(&app, &access_token, refresh_token, expires_at),
            })?;
        cache.clear();
        Ok(CommandRunResult {
            success: true,
            exit_code: Some(0),
            output: "登录成功".into(),
        })
    })
    .await
}

#[tauri::command]
pub async fn run_download(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    request: DownloadRequest,
    title: Option<String>,
) -> Result<TaskSnapshot, String> {
    manager.configure(request.config.download_manager.clone())?;
    let snapshot = manager.enqueue(request, title)?;
    emit_task_status(&app, &snapshot)?;
    Ok(snapshot)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadControlResult {
    updated: Vec<TaskSnapshot>,
    removed: Vec<String>,
    errors: Vec<String>,
}

#[tauri::command]
pub fn control_downloads(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    task_ids: Vec<String>,
    action: String,
) -> DownloadControlResult {
    let mut result = DownloadControlResult {
        updated: vec![],
        removed: vec![],
        errors: vec![],
    };
    for id in task_ids {
        let cache = manager.cache_root(&id);
        let directory = manager.get_task(&id).and_then(|task| task.download_dir);
        match manager.control_download(&id, &action) {
            Ok(Some(snapshot)) => {
                crate::task::log_task_outcome(&app, &snapshot);
                let _ = emit_task_status(&app, &snapshot);
                result.updated.push(snapshot);
            }
            Ok(None) => {
                if let (Some(root), Some(directory)) = (cache, directory) {
                    if let Err(error) = crate::bilibili::session::cleanup(
                        &root,
                        std::path::Path::new(&directory),
                        &id,
                    ) {
                        result.errors.push(error);
                    }
                }
                result.removed.push(id)
            }
            Err(error) => result.errors.push(format!(
                "{}: {error}",
                id.chars().take(8).collect::<String>()
            )),
        }
    }
    result
}

#[tauri::command]
pub fn open_task_directory(manager: State<'_, TaskManager>, task_id: String) -> Result<(), String> {
    let task = manager.get_task(&task_id).ok_or("未找到下载任务")?;
    open_download_directory(task.download_dir.ok_or("任务没有下载目录")?)
}

#[tauri::command]
pub async fn parse_video_v2(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    cache: State<'_, ParseCache>,
    mut request: ParseRequest,
) -> Result<ParseResultV2, String> {
    request.config = effective_config(&app, request.config)?;
    let context = start_task(&app, &manager, TaskKind::Parse, request.input.clone(), None)?;
    run_task(&context, async {
        if let Some(result) = cache.get(&request) {
            return Ok(result);
        }
        let result = engine::parse(&request, &context).await?;
        cache.insert(&request, &result);
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn parse_video(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    cache: State<'_, ParseCache>,
    request: ParseRequest,
) -> Result<ParseResult, String> {
    let work_dir = request.config.work_dir.clone();
    let value = parse_video_v2(app.clone(), manager, cache, request).await?;
    let result = ParseResult {
        id: value.id,
        input: value.input,
        content_kind: value.content_kind,
        aid_or_episode_id: value.aid.map(|id| id.to_string()),
        bvid: value.bvid,
        title: value.title,
        owner_name: value.owner.as_ref().and_then(|o| o.name.clone()),
        owner_space_url: value.owner.and_then(|o| o.space_url),
        publish_time: value.publish_time,
        duration: value.duration_seconds.map(|n| format!("{n}s")),
        part_count: Some(value.parts.len() as u32),
        cover_path: None,
        save_path: Some(work_dir),
        parts: value
            .parts
            .into_iter()
            .map(|part| PartInfo {
                page_number: part.page_number,
                cid: part.cid.map(|id| id.to_string()),
                title: part.title,
                duration: part.duration_seconds.map(|n| format!("{n}s")),
                video_streams: part.video_streams,
                audio_streams: part.audio_streams,
            })
            .collect(),
        raw_output: "由内置核心解析".into(),
        warnings: value.warnings,
        error_message: value.error_message,
    };
    emit_task_parse_result(&app, &result)?;
    Ok(result)
}

fn tool_download_url(tool: &str) -> Result<&'static str, String> {
    match tool {
        "ffmpeg" => Ok("https://ffmpeg.org/download.html"),
        _ => Err(format!("未知工具下载页面: {tool}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_interrupts_native_work_and_remains_terminal() {
        let manager = TaskManager::new();
        let task = manager.create_task(TaskKind::Download, "test".into(), None);
        manager.mark_running(&task.id).unwrap();
        let context = TaskContext {
            app: None,
            manager: manager.clone(),
            id: task.id.clone(),
        };
        let stop = async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            manager.stop_task(&task.id).await.unwrap();
        };
        let work = run_task(&context, async {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            Ok(())
        });
        let (result, ()) = tokio::join!(work, stop);
        assert!(result.is_err());
        assert_eq!(
            manager.mark_completed(&task.id, Some(0)).unwrap().status,
            TaskStatus::Canceled
        );
    }
    #[tokio::test]
    async fn failures_are_reported_as_failed_tasks() {
        let manager = TaskManager::new();
        let task = manager.create_task(TaskKind::Parse, "test".into(), None);
        let context = TaskContext {
            app: None,
            manager: manager.clone(),
            id: task.id.clone(),
        };
        assert!(run_task::<()>(&context, async { Err("API error".into()) })
            .await
            .is_err());
        assert_eq!(
            manager.get_task(&task.id).unwrap().status,
            TaskStatus::Failed
        );
    }
}
