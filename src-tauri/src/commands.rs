use crate::bbdown::{command_builder, parser};
use crate::bilibili::{assemble_parse_result_v2, normalize_video_input, MetadataClient};
use crate::config;
use crate::core::{
    AccountInfo, AppConfig, CommandPreview, CommandRunResult, DownloadRequest, ParseRequest,
    ParseResult, ParseResultV2, ToolDetectionResult,
};
use crate::task::{
    emit_login_qr, emit_task_log, emit_task_parse_result, emit_task_status, runtime_hint_from_line,
    LoginQrEvent, TaskKind, TaskLogEvent, TaskLogStream, TaskManager, TaskSnapshot, TaskStatus,
};
use crate::tools;
use crate::utils::{decode_output, hide_console_window};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::{sleep, Duration};

#[tauri::command]
pub async fn get_config(app: AppHandle) -> Result<AppConfig, String> {
    config::load_config(&app)
}

#[tauri::command]
pub async fn save_config(app: AppHandle, config: AppConfig) -> Result<AppConfig, String> {
    config::write_config(&app, &config)?;
    Ok(config)
}

#[tauri::command]
pub async fn detect_tools(config: AppConfig) -> Result<ToolDetectionResult, String> {
    tools::detect_tools(config).await
}

#[tauri::command]
pub async fn get_account_info(config: AppConfig) -> Result<AccountInfo, String> {
    let (cookie, source) = account_cookie(&config).unwrap_or_default();
    if cookie.is_empty() {
        return Ok(AccountInfo {
            is_logged_in: false,
            mid: None,
            name: None,
            avatar_url: None,
            vip_label: None,
            source: "none".to_string(),
        });
    }

    MetadataClient::new(config.auth.user_agent.as_deref())?
        .fetch_account(&cookie, &source)
        .await
}

#[tauri::command]
pub async fn logout_account(app: AppHandle, mut config: AppConfig) -> Result<AppConfig, String> {
    config.auth.cookie = None;
    config.auth.access_token = None;
    config::write_config(&app, &config)?;
    clear_bbdown_account_files(&config.tools.bbdown_path)?;
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
    if parsed.scheme() != "https" || !(host == "bilibili.com" || host.ends_with(".bilibili.com")) {
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
    let args = command_builder::build_parse_args(&request);
    Ok(command_builder::make_command_preview(
        request.config.tools.bbdown_path.clone(),
        args,
    ))
}

#[tauri::command]
pub async fn build_download_preview(request: DownloadRequest) -> Result<CommandPreview, String> {
    let args = command_builder::build_download_args(&request);
    Ok(command_builder::make_command_preview(
        request.config.tools.bbdown_path.clone(),
        args,
    ))
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

#[tauri::command]
pub async fn run_login(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    config: AppConfig,
    mode: String,
) -> Result<CommandRunResult, String> {
    let bbdown_path = config.tools.bbdown_path.clone();
    if !Path::new(&bbdown_path).exists() {
        return Err("未找到 BBDown，可先在设置页配置路径。".to_string());
    }

    let command = match mode.as_str() {
        "web" => "login",
        "tv" => "logintv",
        _ => return Err(format!("未知登录模式: {mode}")),
    };

    let task_kind = if mode == "web" {
        TaskKind::LoginWeb
    } else {
        TaskKind::LoginTv
    };
    let bbdown_dir = Path::new(&bbdown_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let qr_path = bbdown_dir.join("qrcode.png");
    let data_path = bbdown_dir.join(login_data_file_name(mode.as_str()));
    let _ = std::fs::remove_file(&qr_path);
    let snapshot = manager.create_task(
        task_kind,
        command.to_string(),
        Some(format!("login:{mode}")),
    );
    emit_task_status(&app, &snapshot)?;
    spawn_login_qr_watcher(
        app.clone(),
        snapshot.id.clone(),
        mode.clone(),
        qr_path,
        data_path,
    );

    run_bbdown_command(
        &app,
        &manager,
        &snapshot.id,
        &bbdown_path,
        &[command.to_string()],
        Some(&bbdown_dir),
    )
    .await
}

#[tauri::command]
pub async fn run_download(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    request: DownloadRequest,
) -> Result<CommandRunResult, String> {
    let bbdown_path = request.config.tools.bbdown_path.clone();
    if !Path::new(&bbdown_path).exists() {
        return Err("未找到 BBDown，可先在设置页配置路径。".to_string());
    }

    let args = command_builder::build_download_args(&request);
    let snapshot = manager.create_task(
        TaskKind::Download,
        request.input.clone(),
        request.parse_id.clone(),
    );
    emit_task_status(&app, &snapshot)?;

    run_bbdown_command(&app, &manager, &snapshot.id, &bbdown_path, &args, None).await
}

#[tauri::command]
pub async fn parse_video(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    request: ParseRequest,
) -> Result<ParseResult, String> {
    let bbdown_path = request.config.tools.bbdown_path.clone();
    if !Path::new(&bbdown_path).exists() {
        return Ok(ParseResult::placeholder(
            request.input,
            "未找到 BBDown，可先在设置页配置路径。",
        ));
    }

    let args = command_builder::build_parse_args(&request);
    let snapshot = manager.create_task(TaskKind::Parse, request.input.clone(), None);
    emit_task_status(&app, &snapshot)?;
    let output =
        run_bbdown_command(&app, &manager, &snapshot.id, &bbdown_path, &args, None).await?;

    if !output.success {
        return Err(output.output);
    }

    let mut result = parser::parse_bbdown_output(request.input.clone(), output.output);
    if result.title != "未知标题" {
        let cover_dir = cover_work_dir(
            &request.config.work_dir,
            &result.title,
            result.bvid.as_deref(),
        );
        if let Ok(cover_path) = fetch_cover(
            &app,
            &manager,
            &snapshot.id,
            &bbdown_path,
            &request,
            &cover_dir,
        )
        .await
        {
            if result.owner_name.as_deref().is_none_or(str::is_empty) {
                result.owner_name = owner_name_from_cover(&cover_path);
            }
            result.cover_path = Some(cover_path);
            result.save_path = Some(cover_dir.to_string_lossy().to_string());
        }
    }
    let current = manager
        .get_task(&snapshot.id)
        .ok_or_else(|| format!("未找到任务: {}", snapshot.id))?;
    if current.status == TaskStatus::Canceled {
        emit_task_status(&app, &current)?;
        return Err("解析任务已取消".to_string());
    }

    let completed = manager.mark_completed(&snapshot.id, output.exit_code)?;
    emit_task_status(&app, &completed)?;
    emit_task_parse_result(&app, &result)?;
    Ok(result)
}

#[tauri::command]
pub async fn parse_video_v2(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    request: ParseRequest,
) -> Result<ParseResultV2, String> {
    let bbdown_path = request.config.tools.bbdown_path.clone();
    if !Path::new(&bbdown_path).exists() {
        let legacy =
            ParseResult::placeholder(request.input, "未找到 BBDown，可先在设置页配置路径。");
        return Ok(assemble_parse_result_v2(legacy, None, None));
    }

    let args = command_builder::build_parse_args(&request);
    let snapshot = manager.create_task(TaskKind::Parse, request.input.clone(), None);
    emit_task_status(&app, &snapshot)?;
    let output =
        run_bbdown_command(&app, &manager, &snapshot.id, &bbdown_path, &args, None).await?;

    if !output.success {
        return Err(output.output);
    }

    ensure_parse_not_canceled(&app, &manager, &snapshot.id)?;
    let legacy = parser::parse_bbdown_output(request.input.clone(), output.output);
    let (metadata, metadata_warning) = match normalize_video_input(&request.input) {
        Some(video_id) => match MetadataClient::new(request.config.auth.user_agent.as_deref()) {
            Ok(client) => match client
                .fetch_video(&video_id, request.config.auth.cookie.as_deref())
                .await
            {
                Ok(metadata) => (Some(metadata), None),
                Err(error) => (None, Some(format!("{error}；已使用 BBDown 解析结果。"))),
            },
            Err(error) => (None, Some(format!("{error}；已使用 BBDown 解析结果。"))),
        },
        None => (
            None,
            Some("当前输入无法标准化为 BV/AV，已使用 BBDown 解析结果。".to_string()),
        ),
    };

    ensure_parse_not_canceled(&app, &manager, &snapshot.id)?;
    let result = assemble_parse_result_v2(legacy, metadata, metadata_warning);
    let completed = manager.mark_completed(&snapshot.id, output.exit_code)?;
    emit_task_status(&app, &completed)?;
    Ok(result)
}

fn ensure_parse_not_canceled(
    app: &AppHandle,
    manager: &State<'_, TaskManager>,
    task_id: &str,
) -> Result<(), String> {
    let current = manager
        .get_task(task_id)
        .ok_or_else(|| format!("未找到任务: {task_id}"))?;
    if current.status == TaskStatus::Canceled {
        emit_task_status(app, &current)?;
        return Err("解析任务已取消".to_string());
    }
    Ok(())
}

async fn run_bbdown_command(
    app: &AppHandle,
    manager: &State<'_, TaskManager>,
    task_id: &str,
    path: &str,
    args: &[String],
    current_dir: Option<&Path>,
) -> Result<CommandRunResult, String> {
    let mut command = Command::new(path);
    command
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }
    hide_console_window(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| format!("启动 BBDown 失败: {error}"))?;

    if manager
        .get_task(task_id)
        .is_some_and(|task| task.status == TaskStatus::Canceled)
    {
        let _ = child.start_kill();
        return Ok(CommandRunResult {
            success: false,
            exit_code: None,
            output: "任务已取消".to_string(),
        });
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    if let Err(error) = manager.attach_child(task_id, child) {
        return Ok(CommandRunResult {
            success: false,
            exit_code: None,
            output: error,
        });
    }

    let running = manager.mark_running(task_id)?;
    emit_task_status(app, &running)?;

    let stdout_handle = if let Some(stdout) = stdout {
        let app = app.clone();
        let manager = manager.inner().clone();
        let task_id = task_id.to_string();
        Some(tokio::spawn(async move {
            read_stream(stdout, TaskLogStream::Stdout, app, manager, task_id).await
        }))
    } else {
        None
    };

    let stderr_handle = if let Some(stderr) = stderr {
        let app = app.clone();
        let manager = manager.inner().clone();
        let task_id = task_id.to_string();
        Some(tokio::spawn(async move {
            read_stream(stderr, TaskLogStream::Stderr, app, manager, task_id).await
        }))
    } else {
        None
    };

    let collected_stdout = match stdout_handle {
        Some(handle) => handle
            .await
            .map_err(|error| format!("等待 stdout 任务失败: {error}"))??,
        None => String::new(),
    };
    let collected_stderr = match stderr_handle {
        Some(handle) => handle
            .await
            .map_err(|error| format!("等待 stderr 任务失败: {error}"))??,
        None => String::new(),
    };

    let output = decode_output(collected_stdout.as_bytes(), collected_stderr.as_bytes());

    let status = manager.wait_for_exit(task_id).await?;
    let current = manager
        .get_task(task_id)
        .ok_or_else(|| format!("未找到任务: {task_id}"))?;
    let was_canceled = current.status == TaskStatus::Canceled;

    if was_canceled {
        emit_task_status(app, &current)?;
    } else if status.success() {
        if current.kind != TaskKind::Parse {
            let snapshot = manager.mark_completed(task_id, status.code())?;
            emit_task_status(app, &snapshot)?;
        }
    } else {
        let snapshot = manager.mark_failed(
            task_id,
            status.code(),
            format!("任务退出码: {}", status.code().unwrap_or(-1)),
        )?;
        emit_task_status(app, &snapshot)?;
    }

    Ok(CommandRunResult {
        success: status.success() && !was_canceled,
        exit_code: status.code(),
        output,
    })
}

fn login_data_file_name(mode: &str) -> &'static str {
    if mode == "tv" {
        "BBDownTV.data"
    } else {
        "BBDown.data"
    }
}

fn spawn_login_qr_watcher(
    app: AppHandle,
    task_id: String,
    mode: String,
    qr_path: PathBuf,
    data_path: PathBuf,
) {
    tokio::spawn(async move {
        for _ in 0..60 {
            if qr_path.exists() {
                let event = LoginQrEvent {
                    task_id,
                    mode,
                    image_path: qr_path.to_string_lossy().to_string(),
                    data_path: data_path.to_string_lossy().to_string(),
                    timestamp: crate::task::now_timestamp(),
                };
                let _ = emit_login_qr(&app, &event);
                return;
            }
            sleep(Duration::from_millis(500)).await;
        }
    });
}

async fn read_stream<R>(
    mut stream: R,
    log_stream: TaskLogStream,
    app: AppHandle,
    manager: TaskManager,
    task_id: String,
) -> Result<String, String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    stream
        .read_to_end(&mut buffer)
        .await
        .map_err(|error| format!("读取 BBDown 输出失败: {error}"))?;

    let text = match String::from_utf8(buffer.clone()) {
        Ok(text) => text,
        Err(_) => {
            let (decoded, _, _) = encoding_rs::GBK.decode(&buffer);
            decoded.into_owned()
        }
    };

    for line in text.lines() {
        let line = line.to_string();
        let hint = runtime_hint_from_line(&line);
        let snapshot = manager.apply_runtime_hint(&task_id, hint)?;
        emit_task_status(&app, &snapshot)?;
        emit_task_log(
            &app,
            &TaskLogEvent {
                task_id: task_id.clone(),
                stream: log_stream.clone(),
                line,
                timestamp: crate::task::now_timestamp(),
            },
        )?;
    }

    Ok(text)
}

async fn fetch_cover(
    app: &AppHandle,
    manager: &State<'_, TaskManager>,
    task_id: &str,
    bbdown_path: &str,
    request: &ParseRequest,
    cover_dir: &Path,
) -> Result<String, String> {
    std::fs::create_dir_all(cover_dir)
        .map_err(|error| format!("创建封面目录失败 {}: {error}", cover_dir.display()))?;

    let mut args = vec![
        request.input.clone(),
        "--cover-only".to_string(),
        "--work-dir".to_string(),
        cover_dir.to_string_lossy().to_string(),
        "-F".to_string(),
        "<ownerName>".to_string(),
        "-M".to_string(),
        "<ownerName>".to_string(),
        "-p".to_string(),
        "1".to_string(),
    ];
    command_builder::append_api_and_auth(&mut args, &request.config.auth);

    let mut command = Command::new(bbdown_path);
    command
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    hide_console_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("获取封面失败: {error}"))?;

    if manager
        .get_task(task_id)
        .is_some_and(|task| task.status == TaskStatus::Canceled)
    {
        let _ = child.start_kill();
        return Err("解析任务已取消".to_string());
    }

    manager.attach_child(task_id, child)?;
    let output = manager.wait_with_output(task_id).await?;
    let current = manager
        .get_task(task_id)
        .ok_or_else(|| format!("未找到任务: {task_id}"))?;
    if current.status == TaskStatus::Canceled {
        emit_task_status(app, &current)?;
        return Err("解析任务已取消".to_string());
    }

    if !output.status.success() {
        let raw = decode_output(&output.stdout, &output.stderr);
        return Err(format!("获取封面失败: {}", raw.trim()));
    }

    find_cover_file(cover_dir).ok_or_else(|| "BBDown 未生成封面文件".to_string())
}

fn cover_work_dir(work_dir: &str, title: &str, bvid: Option<&str>) -> PathBuf {
    let suffix = bvid.unwrap_or("Unknown_BV");
    Path::new(work_dir).join(format!("{}_{}", sanitize_path_segment(title), suffix))
}

fn sanitize_path_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter(|ch| !matches!(ch, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect::<String>()
        .trim()
        .to_string();

    if sanitized.is_empty() {
        "untitled".to_string()
    } else {
        sanitized
    }
}

fn find_cover_file(root: &Path) -> Option<String> {
    let valid_exts = ["jpg", "jpeg", "png", "webp", "bmp"];
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
                continue;
            };
            if valid_exts
                .iter()
                .any(|valid| valid.eq_ignore_ascii_case(ext))
            {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }

    None
}

fn owner_name_from_cover(path: &str) -> Option<String> {
    let stem = Path::new(path).file_stem()?.to_str()?.trim();
    let cleaned = stem
        .trim_start_matches(|ch: char| ch == '[' || ch == 'P' || ch.is_ascii_digit() || ch == ']')
        .trim()
        .to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn account_cookie(config: &AppConfig) -> Option<(String, String)> {
    if let Some(cookie) = config
        .auth
        .cookie
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Some((cookie.trim().to_string(), "manualCookie".to_string()));
    }

    let bbdown_dir = Path::new(&config.tools.bbdown_path).parent()?;
    let raw = std::fs::read_to_string(bbdown_dir.join("BBDown.data")).ok()?;
    normalize_bbdown_cookie(&raw).map(|cookie| (cookie, "bbdownScan".to_string()))
}

fn normalize_bbdown_cookie(raw: &str) -> Option<String> {
    const ALLOWED: [&str; 5] = [
        "DedeUserID",
        "DedeUserID__ckMd5",
        "Expires",
        "SESSDATA",
        "bili_jct",
    ];
    let cookie = raw
        .split(';')
        .filter_map(|item| {
            let (name, value) = item.trim().split_once('=')?;
            ALLOWED
                .contains(&name)
                .then(|| format!("{name}={}", value.trim()))
        })
        .collect::<Vec<_>>()
        .join("; ");
    (!cookie.is_empty() && cookie.contains("SESSDATA=")).then_some(cookie)
}

fn clear_bbdown_account_files(bbdown_path: &str) -> Result<(), String> {
    let Some(bbdown_dir) = Path::new(bbdown_path).parent() else {
        return Ok(());
    };

    for name in ["BBDown.data", "BBDownTV.data", "qrcode.png"] {
        let path = bbdown_dir.join(name);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|error| format!("删除本机登录信息失败 {}: {error}", path.display()))?;
        }
    }
    Ok(())
}

fn tool_download_url(tool: &str) -> Result<&'static str, String> {
    match tool {
        "bbdown" => Ok("https://github.com/nilaoda/BBDown/releases"),
        "ffmpeg" => Ok("https://ffmpeg.org/download.html"),
        _ => Err(format!("未知工具下载页面: {tool}")),
    }
}

#[cfg(test)]
mod account_tests {
    use super::{clear_bbdown_account_files, normalize_bbdown_cookie};
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn keeps_only_required_bbdown_cookie_fields() {
        let raw = "DedeUserID=1;SESSDATA=abc==;bili_jct=csrf;gourl=https://example.com";

        let cookie = normalize_bbdown_cookie(raw).expect("cookie should normalize");

        assert_eq!(cookie, "DedeUserID=1; SESSDATA=abc==; bili_jct=csrf");
        assert!(!cookie.contains("gourl"));
    }

    #[test]
    fn rejects_data_without_session_cookie() {
        assert!(normalize_bbdown_cookie("DedeUserID=1;bili_jct=csrf").is_none());
    }

    #[test]
    fn clears_bbdown_account_files() {
        let root = std::env::temp_dir().join(format!("bbdown-next-logout-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp directory should be created");
        for name in ["BBDown.data", "BBDownTV.data", "qrcode.png"] {
            fs::write(root.join(name), "test credential").expect("fixture should be written");
        }

        clear_bbdown_account_files(root.join("BBDown.exe").to_string_lossy().as_ref())
            .expect("logout files should be removed");

        for name in ["BBDown.data", "BBDownTV.data", "qrcode.png"] {
            assert!(!root.join(name).exists());
        }
        fs::remove_dir(root).expect("temp directory should be removed");
    }
}

#[cfg(test)]
mod tool_download_tests {
    use super::tool_download_url;

    #[test]
    fn maps_known_tools_to_official_download_pages() {
        assert_eq!(
            tool_download_url("bbdown").unwrap(),
            "https://github.com/nilaoda/BBDown/releases"
        );
        assert_eq!(
            tool_download_url("ffmpeg").unwrap(),
            "https://ffmpeg.org/download.html"
        );
    }

    #[test]
    fn rejects_unknown_tool_download_page() {
        assert!(tool_download_url("other").is_err());
    }
}
