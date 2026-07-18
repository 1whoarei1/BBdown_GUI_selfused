use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use tokio::process::Child;

pub const TASK_STATUS_EVENT: &str = "task-status";
pub const TASK_LOG_EVENT: &str = "task-log";
pub const TASK_PARSE_RESULT_EVENT: &str = "task-parse-result";
pub const LOGIN_QR_EVENT: &str = "login-qr";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskKind {
    Parse,
    Download,
    LoginWeb,
    LoginTv,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Queued,
    Running,
    Stopping,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskPhase {
    Preparing,
    LoggingIn,
    LoadingCookie,
    ResolvingAid,
    FetchingVideoInfo,
    ParsingPart,
    DownloadingPart,
    Finalizing,
    Finished,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub id: String,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub phase: TaskPhase,
    pub input: String,
    pub title: Option<String>,
    pub current_part_index: Option<u32>,
    pub total_parts: Option<u32>,
    pub current_part_title: Option<String>,
    pub latest_message: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskLogEvent {
    pub task_id: String,
    pub stream: TaskLogStream,
    pub line: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginQrEvent {
    pub task_id: String,
    pub mode: String,
    pub image_path: String,
    pub data_path: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskLogStream {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeHint {
    pub phase: Option<TaskPhase>,
    pub title: Option<String>,
    pub current_part_index: Option<u32>,
    pub total_parts: Option<u32>,
    pub current_part_title: Option<String>,
    pub latest_message: Option<String>,
}

#[derive(Debug)]
struct ManagedTask {
    snapshot: TaskSnapshot,
    child: Arc<Mutex<Option<Child>>>,
}

#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<String, ManagedTask>>>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create_task(
        &self,
        kind: TaskKind,
        input: String,
        title: Option<String>,
    ) -> TaskSnapshot {
        let id = uuid::Uuid::new_v4().to_string();
        let snapshot = TaskSnapshot {
            id: id.clone(),
            kind,
            status: TaskStatus::Queued,
            phase: TaskPhase::Preparing,
            input,
            title,
            current_part_index: None,
            total_parts: None,
            current_part_title: None,
            latest_message: None,
            started_at: None,
            finished_at: None,
            exit_code: None,
        };

        let managed = ManagedTask {
            snapshot: snapshot.clone(),
            child: Arc::new(Mutex::new(None)),
        };
        self.tasks
            .lock()
            .expect("task mutex poisoned")
            .insert(id, managed);
        snapshot
    }

    pub fn get_task(&self, id: &str) -> Option<TaskSnapshot> {
        self.tasks
            .lock()
            .expect("task mutex poisoned")
            .get(id)
            .map(|task| task.snapshot.clone())
    }

    pub fn list_tasks(&self) -> Vec<TaskSnapshot> {
        self.tasks
            .lock()
            .expect("task mutex poisoned")
            .values()
            .map(|task| task.snapshot.clone())
            .collect()
    }

    pub fn attach_child(&self, id: &str, child: Child) -> Result<(), String> {
        let child_handle = {
            let mut tasks = self.tasks.lock().expect("task mutex poisoned");
            let managed = tasks
                .get_mut(id)
                .ok_or_else(|| format!("未找到任务: {id}"))?;
            if managed.snapshot.status == TaskStatus::Canceled {
                return Err(format!("任务已取消: {id}"));
            }
            managed.child.clone()
        };

        let mut guard = child_handle.lock().expect("child mutex poisoned");
        *guard = Some(child);
        Ok(())
    }

    pub async fn wait_for_exit(&self, id: &str) -> Result<std::process::ExitStatus, String> {
        let child_handle = {
            let tasks = self.tasks.lock().expect("task mutex poisoned");
            let managed = tasks.get(id).ok_or_else(|| format!("未找到任务: {id}"))?;
            managed.child.clone()
        };

        let mut child = {
            let mut guard = child_handle.lock().expect("child mutex poisoned");
            guard
                .take()
                .ok_or_else(|| format!("任务进程句柄丢失: {id}"))?
        };

        child
            .wait()
            .await
            .map_err(|error| format!("等待任务退出失败: {error}"))
    }

    pub async fn wait_with_output(&self, id: &str) -> Result<std::process::Output, String> {
        let child_handle = {
            let tasks = self.tasks.lock().expect("task mutex poisoned");
            let managed = tasks.get(id).ok_or_else(|| format!("未找到任务: {id}"))?;
            managed.child.clone()
        };

        let child = {
            let mut guard = child_handle.lock().expect("child mutex poisoned");
            guard
                .take()
                .ok_or_else(|| format!("任务进程句柄丢失: {id}"))?
        };

        child
            .wait_with_output()
            .await
            .map_err(|error| format!("等待任务输出失败: {error}"))
    }

    pub fn mark_running(&self, id: &str) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |snapshot| {
            if snapshot.status == TaskStatus::Stopping || snapshot.status == TaskStatus::Canceled {
                return;
            }
            snapshot.status = TaskStatus::Running;
            snapshot.started_at = Some(now_timestamp());
        })
    }

    pub fn apply_runtime_hint(&self, id: &str, hint: RuntimeHint) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |snapshot| {
            if let Some(phase) = hint.phase {
                snapshot.phase = phase;
            }
            if let Some(title) = hint.title {
                snapshot.title = Some(title);
            }
            if let Some(current_part_index) = hint.current_part_index {
                snapshot.current_part_index = Some(current_part_index);
            }
            if let Some(total_parts) = hint.total_parts {
                snapshot.total_parts = Some(total_parts);
            }
            if let Some(current_part_title) = hint.current_part_title {
                snapshot.current_part_title = Some(current_part_title);
            }
            if let Some(latest_message) = hint.latest_message {
                snapshot.latest_message = Some(latest_message);
            }
        })
    }

    pub fn mark_stopping(&self, id: &str) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |snapshot| {
            snapshot.status = TaskStatus::Stopping;
            snapshot.latest_message = Some("正在停止任务...".to_string());
        })
    }

    pub fn mark_completed(&self, id: &str, exit_code: Option<i32>) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |snapshot| {
            snapshot.status = TaskStatus::Completed;
            snapshot.phase = TaskPhase::Finished;
            snapshot.finished_at = Some(now_timestamp());
            snapshot.exit_code = exit_code;
            snapshot.latest_message = Some("任务已完成".to_string());
        })
    }

    pub fn mark_failed(
        &self,
        id: &str,
        exit_code: Option<i32>,
        message: String,
    ) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |snapshot| {
            snapshot.status = TaskStatus::Failed;
            snapshot.phase = TaskPhase::Finished;
            snapshot.finished_at = Some(now_timestamp());
            snapshot.exit_code = exit_code;
            snapshot.latest_message = Some(message);
        })
    }

    pub fn mark_canceled(&self, id: &str) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |snapshot| {
            snapshot.status = TaskStatus::Canceled;
            snapshot.phase = TaskPhase::Finished;
            snapshot.finished_at = Some(now_timestamp());
            snapshot.latest_message = Some("任务已取消".to_string());
        })
    }

    pub async fn stop_task(&self, id: &str) -> Result<TaskSnapshot, String> {
        let child_handle = {
            let tasks = self.tasks.lock().expect("task mutex poisoned");
            let managed = tasks.get(id).ok_or_else(|| format!("未找到任务: {id}"))?;
            if matches!(
                managed.snapshot.status,
                TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Canceled,
            ) {
                return Ok(managed.snapshot.clone());
            }
            managed.child.clone()
        };

        let _snapshot = self.mark_stopping(id)?;
        let mut guard = child_handle.lock().expect("child mutex poisoned");
        if let Some(child) = guard.as_mut() {
            child
                .start_kill()
                .map_err(|error| format!("停止任务失败: {error}"))?;
            drop(guard);
            return self.mark_canceled(id);
        }
        drop(guard);
        self.mark_canceled(id)
    }

    fn update_snapshot<F>(&self, id: &str, updater: F) -> Result<TaskSnapshot, String>
    where
        F: FnOnce(&mut TaskSnapshot),
    {
        let mut tasks = self.tasks.lock().expect("task mutex poisoned");
        let managed = tasks
            .get_mut(id)
            .ok_or_else(|| format!("未找到任务: {id}"))?;
        updater(&mut managed.snapshot);
        Ok(managed.snapshot.clone())
    }
}

pub fn emit_task_status(app: &AppHandle, snapshot: &TaskSnapshot) -> Result<(), String> {
    app.emit(TASK_STATUS_EVENT, snapshot)
        .map_err(|error| format!("发送任务状态事件失败: {error}"))
}

pub fn emit_task_log(app: &AppHandle, event: &TaskLogEvent) -> Result<(), String> {
    app.emit(TASK_LOG_EVENT, event)
        .map_err(|error| format!("发送任务日志事件失败: {error}"))
}

pub fn emit_task_parse_result(
    app: &AppHandle,
    result: &crate::core::ParseResult,
) -> Result<(), String> {
    app.emit(TASK_PARSE_RESULT_EVENT, result)
        .map_err(|error| format!("发送解析结果事件失败: {error}"))
}

pub fn emit_login_qr(app: &AppHandle, event: &LoginQrEvent) -> Result<(), String> {
    app.emit(LOGIN_QR_EVENT, event)
        .map_err(|error| format!("发送登录二维码事件失败: {error}"))
}

pub fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

pub fn runtime_hint_from_line(line: &str) -> RuntimeHint {
    let trimmed = line.trim();
    let mut hint = RuntimeHint {
        latest_message: (!trimmed.is_empty()).then(|| trimmed.to_string()),
        ..RuntimeHint::default()
    };

    if trimmed.contains("获取aid") {
        hint.phase = Some(TaskPhase::ResolvingAid);
    } else if trimmed.contains("登录") {
        hint.phase = Some(TaskPhase::LoggingIn);
    } else if trimmed.contains("读取本地cookie") || trimmed.contains("加载本地cookie") {
        hint.phase = Some(TaskPhase::LoadingCookie);
    } else if trimmed.contains("视频标题:") || trimmed.contains("获取视频信息") {
        hint.phase = Some(TaskPhase::FetchingVideoInfo);
    } else if trimmed.contains("开始解析P") {
        hint.phase = Some(TaskPhase::ParsingPart);
        if let Some((current, total)) = parse_part_progress(trimmed) {
            hint.current_part_index = Some(current);
            hint.total_parts = Some(total);
        }
    } else if trimmed.contains("解析完毕") || trimmed.contains("任务完成") {
        hint.phase = Some(TaskPhase::Finalizing);
    }

    if let Some(title) = trimmed
        .split_once("视频标题:")
        .map(|(_, value)| value.trim().to_string())
    {
        if !title.is_empty() {
            hint.title = Some(title);
        }
    }

    hint
}

fn parse_part_progress(line: &str) -> Option<(u32, u32)> {
    let start = line.rfind('(')?;
    let end = line.rfind(')')?;
    let payload = &line[start + 1..end];
    let (current_raw, total_raw) = payload.split_once(" of ")?;
    let current = current_raw.trim().parse::<u32>().ok()?;
    let total = total_raw.trim().parse::<u32>().ok()?;
    Some((current, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_task_with_queued_status() {
        let manager = TaskManager::new();

        let snapshot = manager.create_task(TaskKind::Download, "BV1A3411z7Mf".to_string(), None);

        assert_eq!(snapshot.status, TaskStatus::Queued);
        assert_eq!(snapshot.phase, TaskPhase::Preparing);
        assert_eq!(snapshot.input, "BV1A3411z7Mf");
        assert!(manager.get_task(&snapshot.id).is_some());
    }

    #[test]
    fn lists_created_tasks() {
        let manager = TaskManager::new();
        let first = manager.create_task(
            TaskKind::Parse,
            "input-1".to_string(),
            Some("title".to_string()),
        );
        let second = manager.create_task(TaskKind::Download, "input-2".to_string(), None);

        let tasks = manager.list_tasks();

        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().any(|task| task.id == first.id));
        assert!(tasks.iter().any(|task| task.id == second.id));
    }

    #[test]
    fn updates_running_status() {
        let manager = TaskManager::new();
        let snapshot = manager.create_task(TaskKind::Parse, "input".to_string(), None);

        let running = manager
            .mark_running(&snapshot.id)
            .expect("should mark running");

        assert_eq!(running.status, TaskStatus::Running);
        assert!(running.started_at.is_some());
    }

    #[test]
    fn mark_running_does_not_overwrite_canceled_status() {
        let manager = TaskManager::new();
        let snapshot = manager.create_task(TaskKind::Download, "input".to_string(), None);
        manager
            .mark_canceled(&snapshot.id)
            .expect("should mark canceled");

        let running = manager.mark_running(&snapshot.id).expect("should not fail");

        assert_eq!(running.status, TaskStatus::Canceled);
        assert!(running.started_at.is_none());
    }

    #[test]
    fn stop_task_does_not_overwrite_completed_status() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime should start");
        let manager = TaskManager::new();
        let snapshot = manager.create_task(TaskKind::Download, "input".to_string(), None);
        manager
            .mark_completed(&snapshot.id, Some(0))
            .expect("should mark completed");

        let stopped = runtime
            .block_on(manager.stop_task(&snapshot.id))
            .expect("stop should return existing terminal task");

        assert_eq!(stopped.status, TaskStatus::Completed);
        assert_eq!(stopped.exit_code, Some(0));
    }

    #[test]
    fn mark_completed_sets_finished_state() {
        let manager = TaskManager::new();
        let snapshot = manager.create_task(TaskKind::Download, "input".to_string(), None);

        let completed = manager
            .mark_completed(&snapshot.id, Some(0))
            .expect("should mark completed");

        assert_eq!(completed.status, TaskStatus::Completed);
        assert_eq!(completed.phase, TaskPhase::Finished);
        assert_eq!(completed.exit_code, Some(0));
        assert_eq!(completed.latest_message.as_deref(), Some("任务已完成"));
        assert!(completed.finished_at.is_some());
    }

    #[test]
    fn parses_runtime_hint_for_part_progress() {
        let hint = runtime_hint_from_line("[2026-06-01] - 开始解析P4: xxx... (4 of 13)");

        assert_eq!(hint.phase, Some(TaskPhase::ParsingPart));
        assert_eq!(hint.current_part_index, Some(4));
        assert_eq!(hint.total_parts, Some(13));
    }
}
