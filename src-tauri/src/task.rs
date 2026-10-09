use crate::core::{DownloadManagerOptions, DownloadRequest};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use tokio::sync::{watch, Notify};

pub const TASK_STATUS_EVENT: &str = "task-status";
pub const TASK_LOG_EVENT: &str = "task-log";
pub const TASK_PARSE_RESULT_EVENT: &str = "task-parse-result";
pub const LOGIN_QR_EVENT: &str = "login-qr";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskKind {
    Parse,
    Download,
    LoginWeb,
    LoginTv,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Pausing,
    Paused,
    Queued,
    Running,
    Stopping,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskPhase {
    Preparing,
    LoggingIn,
    FetchingVideoInfo,
    ParsingPart,
    DownloadingPart,
    Finalizing,
    Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
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
    pub created_at: String,
    pub revision: u64,
    pub queue_order: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub current_file: Option<String>,
    pub current_file_downloaded: u64,
    pub current_file_total: Option<u64>,
    pub speed_bytes_per_second: f64,
    pub eta_seconds: Option<u64>,
    pub completed_parts: u32,
    pub selected_parts: u32,
    pub output_files: Vec<String>,
    pub download_dir: Option<String>,
    pub error_message: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskLogStream {
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

impl Default for TaskSnapshot {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: TaskKind::Download,
            status: TaskStatus::Queued,
            phase: TaskPhase::Preparing,
            input: String::new(),
            title: None,
            current_part_index: None,
            total_parts: None,
            current_part_title: None,
            latest_message: None,
            started_at: None,
            finished_at: None,
            exit_code: None,
            created_at: now_timestamp(),
            revision: 1,
            queue_order: 0,
            downloaded_bytes: 0,
            total_bytes: None,
            current_file: None,
            current_file_downloaded: 0,
            current_file_total: None,
            speed_bytes_per_second: 0.0,
            eta_seconds: None,
            completed_parts: 0,
            selected_parts: 0,
            output_files: vec![],
            download_dir: None,
            error_message: None,
        }
    }
}

struct ManagedTask {
    snapshot: TaskSnapshot,
    cancel: watch::Sender<bool>,
    request: Option<DownloadRequest>,
    active: bool,
    transfers: HashMap<String, (u64, Option<u64>)>,
    rate_started: Instant,
    network_bytes: u64,
    rate_key: Option<String>,
    completed_bytes: u64,
}
impl ManagedTask {
    fn new(snapshot: TaskSnapshot, request: Option<DownloadRequest>) -> Self {
        Self {
            snapshot,
            request,
            cancel: watch::channel(false).0,
            active: false,
            transfers: HashMap::new(),
            rate_started: Instant::now(),
            network_bytes: 0,
            rate_key: None,
            completed_bytes: 0,
        }
    }
}
struct Inner {
    tasks: HashMap<String, ManagedTask>,
    settings: DownloadManagerOptions,
    storage: Option<PathBuf>,
    next_order: u64,
    last_saved: Instant,
}
#[derive(Serialize, Deserialize)]
struct StoredTask {
    snapshot: TaskSnapshot,
    request: DownloadRequest,
}
#[derive(Serialize, Deserialize)]
struct TaskStore {
    version: u32,
    tasks: Vec<StoredTask>,
}
#[derive(Clone)]
pub struct TaskManager {
    inner: Arc<Mutex<Inner>>,
    pub wake: Arc<Notify>,
}
impl TaskManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                tasks: HashMap::new(),
                settings: DownloadManagerOptions::default(),
                storage: None,
                next_order: 1,
                last_saved: Instant::now(),
            })),
            wake: Arc::new(Notify::new()),
        }
    }
    pub fn initialize(
        &self,
        path: PathBuf,
        settings: DownloadManagerOptions,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().expect("task mutex poisoned");
        if inner.storage.is_some() {
            return Ok(());
        }
        let store = match std::fs::read(&path) {
            Ok(raw) => Some(
                serde_json::from_slice::<TaskStore>(&raw)
                    .map_err(|e| format!("任务记录格式错误，原文件已保留: {e}"))?,
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("读取任务记录失败: {e}")),
        };
        if let Some(store) = store {
            if store.version != 1 {
                return Err("任务记录版本不支持，原文件已保留".into());
            }
            for stored in store.tasks {
                uuid::Uuid::parse_str(&stored.snapshot.id).map_err(|_| "任务记录包含无效 ID")?;
                let mut snapshot = stored.snapshot;
                snapshot.speed_bytes_per_second = 0.0;
                snapshot.eta_seconds = None;
                if snapshot.status == TaskStatus::Completed {
                    clear_current_transfer(&mut snapshot);
                    snapshot.total_bytes = Some(snapshot.downloaded_bytes);
                }
                match snapshot.status {
                    TaskStatus::Stopping => {
                        snapshot.status = TaskStatus::Canceled;
                        snapshot.latest_message = Some("已取消".into());
                    }
                    TaskStatus::Pausing => {
                        snapshot.status = TaskStatus::Paused;
                        snapshot.latest_message = Some("已暂停".into());
                    }
                    TaskStatus::Queued | TaskStatus::Running => {
                        snapshot.status = if settings.resume_on_start {
                            TaskStatus::Queued
                        } else {
                            TaskStatus::Paused
                        };
                        snapshot.latest_message = Some(
                            if settings.resume_on_start {
                                "等待继续下载"
                            } else {
                                "上次未完成，点击继续"
                            }
                            .into(),
                        );
                    }
                    _ => {}
                }
                snapshot.revision += 1;
                inner.next_order = inner.next_order.max(snapshot.queue_order + 1);
                inner.tasks.insert(
                    snapshot.id.clone(),
                    ManagedTask::new(snapshot, Some(stored.request)),
                );
            }
        }
        inner.settings = settings;
        inner.storage = Some(path);
        Self::save(&mut inner, true)?;
        Ok(())
    }
    pub fn configure(&self, settings: DownloadManagerOptions) -> Result<(), String> {
        if !(1..=8).contains(&settings.max_concurrent_tasks)
            || !(1..=16).contains(&settings.connections_per_task)
        {
            return Err("任务并发范围为 1-8，连接数范围为 1-16".into());
        }
        self.inner.lock().expect("task mutex poisoned").settings = settings;
        self.wake.notify_one();
        Ok(())
    }
    fn save(inner: &mut Inner, force: bool) -> Result<(), String> {
        if !force && inner.last_saved.elapsed().as_secs() < 1 {
            return Ok(());
        }
        if let Some(path) = &inner.storage {
            let mut tasks = inner
                .tasks
                .values()
                .filter_map(|task| {
                    task.request.as_ref().map(|request| StoredTask {
                        snapshot: {
                            let mut snapshot = task.snapshot.clone();
                            snapshot.input = without_credentials(request.clone()).input;
                            snapshot
                        },
                        request: without_credentials(request.clone()),
                    })
                })
                .collect::<Vec<_>>();
            tasks.sort_by_key(|task| task.snapshot.queue_order);
            crate::storage::write_json(path, &TaskStore { version: 1, tasks })?;
        }
        inner.last_saved = Instant::now();
        Ok(())
    }
    pub fn enqueue(
        &self,
        request: DownloadRequest,
        title: Option<String>,
    ) -> Result<TaskSnapshot, String> {
        let mut request = without_credentials(request);
        request.config.work_dir = std::path::absolute(&request.config.work_dir)
            .map_err(|e| format!("下载目录无效: {e}"))?
            .to_string_lossy()
            .into();
        let mut inner = self.inner.lock().expect("task mutex poisoned");
        let id = uuid::Uuid::new_v4().to_string();
        let snapshot = TaskSnapshot {
            id: id.clone(),
            kind: TaskKind::Download,
            input: request.input.clone(),
            title,
            queue_order: inner.next_order,
            download_dir: Some(request.config.work_dir.clone()),
            latest_message: Some("等待下载".into()),
            ..Default::default()
        };
        inner.next_order += 1;
        inner.tasks.insert(
            id.clone(),
            ManagedTask::new(snapshot.clone(), Some(request)),
        );
        if let Err(error) = Self::save(&mut inner, true) {
            inner.tasks.remove(&id);
            return Err(error);
        }
        self.wake.notify_one();
        Ok(snapshot)
    }
    pub fn take_next(&self) -> Result<Option<(TaskSnapshot, DownloadRequest)>, String> {
        let mut inner = self.inner.lock().expect("task mutex poisoned");
        let active = inner
            .tasks
            .values()
            .filter(|t| t.request.is_some() && t.active)
            .count();
        if active >= inner.settings.max_concurrent_tasks as usize {
            return Ok(None);
        }
        let id = inner
            .tasks
            .values()
            .filter(|t| t.request.is_some() && !t.active && t.snapshot.status == TaskStatus::Queued)
            .min_by_key(|t| t.snapshot.queue_order)
            .map(|t| t.snapshot.id.clone());
        let Some(id) = id else {
            return Ok(None);
        };
        let task = inner.tasks.get_mut(&id).unwrap();
        task.active = true;
        task.cancel = watch::channel(false).0;
        task.transfers.clear();
        task.network_bytes = 0;
        task.rate_started = Instant::now();
        task.snapshot.revision += 1;
        task.snapshot.status = TaskStatus::Running;
        task.snapshot.phase = TaskPhase::Preparing;
        task.snapshot.started_at = Some(now_timestamp());
        task.snapshot.finished_at = None;
        task.snapshot.error_message = None;
        task.snapshot.latest_message = Some("准备下载，刷新媒体地址".into());
        task.snapshot.speed_bytes_per_second = 0.0;
        task.snapshot.eta_seconds = None;
        task.rate_key = None;
        let result = (task.snapshot.clone(), task.request.clone().unwrap());
        if let Err(error) = Self::save(&mut inner, true) {
            let task = inner.tasks.get_mut(&id).unwrap();
            task.active = false;
            task.snapshot.status = TaskStatus::Paused;
            task.snapshot.error_message = Some(error.clone());
            return Err(error);
        }
        Ok(Some(result))
    }
    pub fn finish_download(
        &self,
        id: &str,
        result: Result<String, String>,
    ) -> Result<TaskSnapshot, String> {
        let mut inner = self.inner.lock().expect("task mutex poisoned");
        let task = inner.tasks.get_mut(id).ok_or("未找到下载任务")?;
        task.active = false;
        task.snapshot.revision += 1;
        task.snapshot.speed_bytes_per_second = 0.0;
        task.snapshot.eta_seconds = None;
        match task.snapshot.status {
            TaskStatus::Pausing => {
                task.snapshot.status = TaskStatus::Paused;
                task.snapshot.latest_message = Some("已暂停，下载数据已保留".into());
            }
            TaskStatus::Stopping | TaskStatus::Canceled => {
                task.snapshot.status = TaskStatus::Canceled;
                task.snapshot.finished_at = Some(now_timestamp());
                task.snapshot.latest_message = Some("已取消，可重试或移除记录".into());
            }
            _ => match result {
                Ok(message) => {
                    task.snapshot.status = TaskStatus::Completed;
                    clear_current_transfer(&mut task.snapshot);
                    task.snapshot.total_bytes = Some(task.snapshot.downloaded_bytes);
                    task.snapshot.phase = TaskPhase::Finished;
                    task.snapshot.finished_at = Some(now_timestamp());
                    task.snapshot.exit_code = Some(0);
                    task.snapshot.latest_message = Some(message);
                }
                Err(error) => {
                    task.snapshot.status = TaskStatus::Failed;
                    task.snapshot.finished_at = Some(now_timestamp());
                    task.snapshot.error_message = Some(error.clone());
                    task.snapshot.latest_message = Some(error);
                }
            },
        }
        let snapshot = task.snapshot.clone();
        let saved = Self::save(&mut inner, true);
        self.wake.notify_one();
        saved?;
        Ok(snapshot)
    }
    pub fn control_download(&self, id: &str, action: &str) -> Result<Option<TaskSnapshot>, String> {
        let mut inner = self.inner.lock().expect("task mutex poisoned");
        let order = inner.next_order;
        let task = inner.tasks.get_mut(id).ok_or("未找到下载任务")?;
        if task.request.is_none() {
            return Err("此操作仅用于下载任务".into());
        }
        match action {
            "pause" => {
                if task.snapshot.status == TaskStatus::Queued {
                    task.snapshot.status = TaskStatus::Paused;
                } else if task.snapshot.status == TaskStatus::Running {
                    task.cancel.send_replace(true);
                    task.snapshot.status = TaskStatus::Pausing;
                } else {
                    return Ok(Some(task.snapshot.clone()));
                }
                task.snapshot.speed_bytes_per_second = 0.0;
                task.snapshot.eta_seconds = None;
                task.snapshot.latest_message = Some(
                    if task.active {
                        "正在暂停"
                    } else {
                        "已暂停"
                    }
                    .into(),
                );
            }
            "resume" | "retry" => {
                if task.active {
                    return Err("请等待任务暂停或取消完成".into());
                }
                if !matches!(
                    task.snapshot.status,
                    TaskStatus::Paused | TaskStatus::Failed | TaskStatus::Canceled
                ) {
                    return Ok(Some(task.snapshot.clone()));
                }
                task.snapshot.status = TaskStatus::Queued;
                task.snapshot.queue_order = order;
                task.snapshot.finished_at = None;
                task.snapshot.error_message = None;
                task.snapshot.latest_message = Some("等待继续下载".into());
                inner.next_order += 1;
            }
            "cancel" => {
                if matches!(
                    task.snapshot.status,
                    TaskStatus::Completed | TaskStatus::Canceled
                ) {
                    return Ok(Some(task.snapshot.clone()));
                }
                task.cancel.send_replace(true);
                task.snapshot.status = if task.active {
                    TaskStatus::Stopping
                } else {
                    TaskStatus::Canceled
                };
                task.snapshot.speed_bytes_per_second = 0.0;
                task.snapshot.eta_seconds = None;
                task.snapshot.latest_message = Some("正在取消".into());
            }
            "remove" => {
                if task.active || task.snapshot.status == TaskStatus::Queued {
                    return Err("请先暂停或取消任务，再移除记录".into());
                }
                inner.tasks.remove(id);
                Self::save(&mut inner, true)?;
                self.wake.notify_one();
                return Ok(None);
            }
            _ => return Err("未知下载任务操作".into()),
        }
        inner.tasks.get_mut(id).unwrap().snapshot.revision += 1;
        let snapshot = inner.tasks.get(id).unwrap().snapshot.clone();
        Self::save(&mut inner, true)?;
        self.wake.notify_one();
        Ok(Some(snapshot))
    }
    pub fn create_task(
        &self,
        kind: TaskKind,
        input: String,
        title: Option<String>,
    ) -> TaskSnapshot {
        let mut inner = self.inner.lock().expect("task mutex poisoned");
        let snapshot = TaskSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            input,
            title,
            queue_order: inner.next_order,
            ..Default::default()
        };
        inner.next_order += 1;
        inner.tasks.insert(
            snapshot.id.clone(),
            ManagedTask::new(snapshot.clone(), None),
        );
        snapshot
    }
    pub fn get_task(&self, id: &str) -> Option<TaskSnapshot> {
        self.inner
            .lock()
            .expect("task mutex poisoned")
            .tasks
            .get(id)
            .map(|t| t.snapshot.clone())
    }
    pub fn list_tasks(&self) -> Vec<TaskSnapshot> {
        let mut tasks = self
            .inner
            .lock()
            .expect("task mutex poisoned")
            .tasks
            .values()
            .map(|t| t.snapshot.clone())
            .collect::<Vec<_>>();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.queue_order));
        tasks
    }
    pub fn cancel_signal(&self, id: &str) -> Result<watch::Receiver<bool>, String> {
        Ok(self
            .inner
            .lock()
            .expect("task mutex poisoned")
            .tasks
            .get(id)
            .ok_or("未找到任务")?
            .cancel
            .subscribe())
    }
    pub fn while_active<T>(
        &self,
        id: &str,
        action: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let inner = self.inner.lock().expect("task mutex poisoned");
        let task = inner.tasks.get(id).ok_or("未找到任务")?;
        if matches!(
            task.snapshot.status,
            TaskStatus::Canceled | TaskStatus::Stopping | TaskStatus::Paused | TaskStatus::Pausing
        ) {
            return Err("任务已停止".into());
        }
        action()
    }
    pub fn connection_limit(&self, id: &str) -> u32 {
        let inner = self.inner.lock().expect("task mutex poisoned");
        inner
            .tasks
            .get(id)
            .and_then(|t| t.request.as_ref())
            .map(|r| r.config.download_manager.connections_per_task)
            .unwrap_or(4)
    }
    pub fn cache_root(&self, id: &str) -> Option<PathBuf> {
        self.inner
            .lock()
            .ok()?
            .tasks
            .get(id)?
            .request
            .as_ref()
            .map(|r| {
                Path::new(&r.config.work_dir)
                    .join(".bbdown-next-cache")
                    .join(id)
            })
    }
    pub fn progress(
        &self,
        id: &str,
        key: &str,
        label: &str,
        downloaded: u64,
        total: Option<u64>,
        network_delta: u64,
    ) -> Result<TaskSnapshot, String> {
        let mut inner = self.inner.lock().expect("task mutex poisoned");
        let task = inner.tasks.get_mut(id).ok_or("未找到任务")?;
        if task.snapshot.status != TaskStatus::Running {
            return Ok(task.snapshot.clone());
        }
        if task.rate_key.as_deref() != Some(key) {
            task.rate_key = Some(key.into());
            task.rate_started = Instant::now();
            task.network_bytes = 0;
        }
        task.snapshot.revision += 1;
        task.transfers.insert(key.into(), (downloaded, total));
        task.network_bytes = task.network_bytes.saturating_add(network_delta);
        task.snapshot.downloaded_bytes =
            task.completed_bytes + task.transfers.values().map(|p| p.0).sum::<u64>();
        task.snapshot.total_bytes = task
            .transfers
            .values()
            .map(|p| p.1)
            .collect::<Option<Vec<_>>>()
            .map(|v| task.completed_bytes + v.iter().sum::<u64>());
        task.snapshot.current_file = Some(label.into());
        task.snapshot.current_file_downloaded = downloaded;
        task.snapshot.current_file_total = total;
        let elapsed = task.rate_started.elapsed().as_secs_f64();
        task.snapshot.speed_bytes_per_second = if elapsed > 0.3 {
            task.network_bytes as f64 / elapsed
        } else {
            0.0
        };
        task.snapshot.eta_seconds = total
            .filter(|t| *t > downloaded)
            .filter(|_| task.snapshot.speed_bytes_per_second > 0.0)
            .map(|t| {
                ((t - downloaded) as f64 / task.snapshot.speed_bytes_per_second).ceil() as u64
            });
        let snapshot = task.snapshot.clone();
        Self::save(&mut inner, false)?;
        Ok(snapshot)
    }
    pub fn part_completed(
        &self,
        id: &str,
        completed: u32,
        selected: u32,
        files: &[PathBuf],
    ) -> Result<TaskSnapshot, String> {
        let mut inner = self.inner.lock().expect("task mutex poisoned");
        let task = inner.tasks.get_mut(id).ok_or("未找到任务")?;
        task.snapshot.revision += 1;
        if completed == 0 {
            task.completed_bytes = 0;
            task.transfers.clear();
        } else if completed > task.snapshot.completed_parts {
            task.completed_bytes += files
                .iter()
                .filter_map(|path| path.metadata().ok())
                .map(|m| m.len())
                .sum::<u64>();
            task.transfers.clear();
        }
        task.snapshot.downloaded_bytes = task.completed_bytes;
        task.snapshot.total_bytes = (completed > 0).then_some(task.completed_bytes);
        clear_current_transfer(&mut task.snapshot);
        task.rate_key = None;
        task.network_bytes = 0;
        task.snapshot.completed_parts = completed;
        task.snapshot.selected_parts = selected;
        for file in files {
            let path = file.to_string_lossy().into_owned();
            if !task.snapshot.output_files.contains(&path) {
                task.snapshot.output_files.push(path);
            }
        }
        let snapshot = task.snapshot.clone();
        Self::save(&mut inner, true)?;
        Ok(snapshot)
    }
    pub fn mark_running(&self, id: &str) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |s| {
            s.status = TaskStatus::Running;
            s.started_at = Some(now_timestamp());
        })
    }
    pub fn apply_runtime_hint(&self, id: &str, hint: RuntimeHint) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |s| {
            if let Some(v) = hint.phase {
                if v != TaskPhase::DownloadingPart {
                    s.speed_bytes_per_second = 0.0;
                    s.eta_seconds = None;
                }
                s.phase = v;
            }
            if let Some(v) = hint.title {
                s.title = Some(v);
            }
            if let Some(v) = hint.current_part_index {
                s.current_part_index = Some(v);
            }
            if let Some(v) = hint.total_parts {
                s.total_parts = Some(v);
            }
            if let Some(v) = hint.current_part_title {
                s.current_part_title = Some(v);
            }
            if let Some(v) = hint.latest_message {
                s.latest_message = Some(v);
            }
        })
    }
    pub fn mark_completed(&self, id: &str, code: Option<i32>) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |s| {
            s.status = TaskStatus::Completed;
            s.phase = TaskPhase::Finished;
            s.finished_at = Some(now_timestamp());
            s.exit_code = code;
            s.latest_message = Some("任务已完成".into());
        })
    }
    pub fn mark_failed(
        &self,
        id: &str,
        code: Option<i32>,
        error: String,
    ) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |s| {
            s.status = TaskStatus::Failed;
            s.phase = TaskPhase::Finished;
            s.finished_at = Some(now_timestamp());
            s.exit_code = code;
            s.error_message = Some(error.clone());
            s.latest_message = Some(error);
        })
    }
    #[cfg(test)]
    pub fn mark_canceled(&self, id: &str) -> Result<TaskSnapshot, String> {
        self.update_snapshot(id, |s| {
            s.status = TaskStatus::Canceled;
        })
    }
    pub async fn stop_task(&self, id: &str) -> Result<TaskSnapshot, String> {
        if self.cache_root(id).is_some() {
            return self
                .control_download(id, "cancel")?
                .ok_or("未找到任务".into());
        }
        let mut inner = self.inner.lock().expect("task mutex poisoned");
        let task = inner.tasks.get_mut(id).ok_or("未找到任务")?;
        if matches!(
            task.snapshot.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Canceled
        ) {
            return Ok(task.snapshot.clone());
        }
        task.cancel.send_replace(true);
        task.snapshot.status = TaskStatus::Canceled;
        task.snapshot.phase = TaskPhase::Finished;
        task.snapshot.finished_at = Some(now_timestamp());
        task.snapshot.latest_message = Some("任务已取消".into());
        Ok(task.snapshot.clone())
    }
    fn update_snapshot(
        &self,
        id: &str,
        update: impl FnOnce(&mut TaskSnapshot),
    ) -> Result<TaskSnapshot, String> {
        let mut inner = self.inner.lock().expect("task mutex poisoned");
        let task = inner.tasks.get_mut(id).ok_or("未找到任务")?;
        if !matches!(
            task.snapshot.status,
            TaskStatus::Completed
                | TaskStatus::Failed
                | TaskStatus::Canceled
                | TaskStatus::Paused
                | TaskStatus::Pausing
                | TaskStatus::Stopping
        ) {
            task.snapshot.revision += 1;
            update(&mut task.snapshot);
        }
        let snapshot = task.snapshot.clone();
        Self::save(&mut inner, false)?;
        Ok(snapshot)
    }
}
fn clear_current_transfer(snapshot: &mut TaskSnapshot) {
    snapshot.current_file = None;
    snapshot.current_file_downloaded = 0;
    snapshot.current_file_total = None;
    snapshot.speed_bytes_per_second = 0.0;
    snapshot.eta_seconds = None;
}
fn without_credentials(mut request: DownloadRequest) -> DownloadRequest {
    request.config.auth.cookie = None;
    request.config.auth.access_token = None;
    if let Ok(mut url) = reqwest::Url::parse(&request.input) {
        let pairs = url
            .query_pairs()
            .filter(|(k, _)| {
                !matches!(
                    k.as_ref(),
                    "access_key" | "access_token" | "cookie" | "SESSDATA" | "bili_jct"
                )
            })
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>();
        url.set_query(None);
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(pairs);
        }
        request.input = url.to_string();
    }
    request
}

pub fn emit_task_status(app: &AppHandle, snapshot: &TaskSnapshot) -> Result<(), String> {
    app.emit(TASK_STATUS_EVENT, snapshot)
        .map_err(|e| format!("发送任务状态失败: {e}"))
}
pub fn emit_task_log(app: &AppHandle, event: &TaskLogEvent) -> Result<(), String> {
    crate::diagnostics::record(
        app,
        format!(
            "[{}] {}",
            event.task_id.chars().take(8).collect::<String>(),
            event.line
        ),
    );
    app.emit(TASK_LOG_EVENT, event)
        .map_err(|e| format!("发送任务日志失败: {e}"))
}
pub fn log_task_outcome(app: &AppHandle, snapshot: &TaskSnapshot) {
    crate::diagnostics::record(
        app,
        format!(
            "[{:?}] [{}] {} / P{}: {}",
            snapshot.status,
            snapshot.id.chars().take(8).collect::<String>(),
            snapshot.title.as_deref().unwrap_or(&snapshot.input),
            snapshot.current_part_index.unwrap_or(0),
            snapshot
                .error_message
                .as_deref()
                .or(snapshot.latest_message.as_deref())
                .unwrap_or("任务状态已更新"),
        ),
    );
}
pub fn emit_task_parse_result(
    app: &AppHandle,
    result: &crate::core::ParseResult,
) -> Result<(), String> {
    app.emit(TASK_PARSE_RESULT_EVENT, result)
        .map_err(|e| format!("发送解析结果失败: {e}"))
}
pub fn emit_login_qr(app: &AppHandle, event: &LoginQrEvent) -> Result<(), String> {
    app.emit(LOGIN_QR_EVENT, event)
        .map_err(|e| format!("发送登录二维码失败: {e}"))
}
pub fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request(root: &Path) -> DownloadRequest {
        let mut config = crate::core::default_config();
        config.work_dir = root.to_string_lossy().into();
        DownloadRequest {
            input: "BV17x411w7KC".into(),
            config,
            parse_id: None,
        }
    }
    #[test]
    fn published_parts_reset_transfer_metrics_and_totals_match_saved_files() {
        let directory = crate::bilibili::test_support::TestDirectory::new();
        let path = directory.0.join("tasks.json");
        let manager = TaskManager::new();
        manager
            .initialize(path.clone(), DownloadManagerOptions::default())
            .unwrap();
        let task = manager.enqueue(request(&directory.0), None).unwrap();
        manager.take_next().unwrap();
        manager.part_completed(&task.id, 0, 2, &[]).unwrap();
        manager
            .progress(&task.id, "video", "视频", 50, Some(50), 50)
            .unwrap();
        let first = directory.0.join("first.mp4");
        // Published MP4 and sidecars differ in size from the downloaded streams.
        std::fs::write(&first, [0; 80]).unwrap();
        let snapshot = manager.part_completed(&task.id, 1, 2, &[first]).unwrap();
        assert_eq!(snapshot.downloaded_bytes, 80);
        assert_eq!(snapshot.total_bytes, Some(80));
        assert!(snapshot.current_file.is_none());
        assert_eq!(snapshot.speed_bytes_per_second, 0.0);
        manager
            .progress(&task.id, "audio", "音频", 10, Some(10), 10)
            .unwrap();
        let second = directory.0.join("second.mp4");
        std::fs::write(&second, [0; 30]).unwrap();
        let snapshot = manager.part_completed(&task.id, 2, 2, &[second]).unwrap();
        assert_eq!(snapshot.downloaded_bytes, 110);
        assert_eq!(snapshot.total_bytes, Some(110));
        manager
            .finish_download(&task.id, Ok("done".into()))
            .unwrap();
        let restored = TaskManager::new();
        restored
            .initialize(path, DownloadManagerOptions::default())
            .unwrap();
        let snapshot = restored.get_task(&task.id).unwrap();
        assert_eq!(snapshot.total_bytes, Some(snapshot.downloaded_bytes));
        assert!(snapshot.current_file_total.is_none());
        assert!(snapshot.eta_seconds.is_none());
    }

    #[test]
    fn finalizing_does_not_keep_network_speed_or_eta() {
        let manager = TaskManager::new();
        let task = manager.create_task(TaskKind::Download, "fixture".into(), None);
        manager.mark_running(&task.id).unwrap();
        manager
            .progress(&task.id, "video", "视频", 0, Some(100), 0)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(350));
        let snapshot = manager
            .progress(&task.id, "video", "视频", 50, Some(100), 50)
            .unwrap();
        assert!(snapshot.speed_bytes_per_second > 0.0);
        assert!(snapshot.eta_seconds.is_some());
        let snapshot = manager
            .apply_runtime_hint(
                &task.id,
                RuntimeHint {
                    phase: Some(TaskPhase::Finalizing),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(snapshot.speed_bytes_per_second, 0.0);
        assert!(snapshot.eta_seconds.is_none());
    }
    #[test]
    fn fifo_limit_pause_acknowledgement_and_restart_recovery() {
        let directory = crate::bilibili::test_support::TestDirectory::new();
        let path = directory.0.join("tasks.json");
        let manager = TaskManager::new();
        manager
            .initialize(
                path.clone(),
                DownloadManagerOptions {
                    max_concurrent_tasks: 1,
                    ..Default::default()
                },
            )
            .unwrap();
        let first = manager
            .enqueue(request(&directory.0), Some("first".into()))
            .unwrap();
        let second = manager
            .enqueue(request(&directory.0), Some("second".into()))
            .unwrap();
        assert_eq!(manager.take_next().unwrap().unwrap().0.id, first.id);
        assert!(manager.take_next().unwrap().is_none());
        assert_eq!(
            manager
                .control_download(&first.id, "pause")
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Pausing
        );
        assert!(manager.control_download(&first.id, "resume").is_err());
        assert!(manager.take_next().unwrap().is_none());
        manager
            .finish_download(&first.id, Err("interrupted".into()))
            .unwrap();
        assert_eq!(manager.take_next().unwrap().unwrap().0.id, second.id);
        let restored = TaskManager::new();
        restored
            .initialize(path, DownloadManagerOptions::default())
            .unwrap();
        assert!(restored
            .list_tasks()
            .iter()
            .all(|task| task.status == TaskStatus::Paused));
        assert!(restored.take_next().unwrap().is_none());
        restored.control_download(&second.id, "resume").unwrap();
        assert_eq!(restored.take_next().unwrap().unwrap().0.id, second.id);
    }
    #[test]
    fn history_does_not_store_credentials_and_remove_keeps_output_files() {
        let directory = crate::bilibili::test_support::TestDirectory::new();
        let path = directory.0.join("tasks.json");
        let manager = TaskManager::new();
        manager
            .initialize(path.clone(), DownloadManagerOptions::default())
            .unwrap();
        let mut req = request(&directory.0);
        req.config.auth.cookie = Some("SESSDATA=private-cookie".into());
        req.config.auth.access_token = Some("private-token".into());
        req.input = "https://www.bilibili.com/video/BV17x411w7KC?access_key=url-secret".into();
        let task = manager.enqueue(req, Some("fixture".into())).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        for value in ["private-cookie", "private-token", "url-secret"] {
            assert!(!raw.contains(value));
        }
        manager.take_next().unwrap();
        let output = directory.0.join("video.mp4");
        std::fs::write(&output, "keep").unwrap();
        manager
            .part_completed(&task.id, 1, 1, std::slice::from_ref(&output))
            .unwrap();
        manager
            .finish_download(&task.id, Ok("done".into()))
            .unwrap();
        manager.control_download(&task.id, "remove").unwrap();
        assert_eq!(std::fs::read_to_string(output).unwrap(), "keep");
        assert!(manager.get_task(&task.id).is_none());
    }
    #[test]
    fn startup_preserves_user_pause_and_cancel_even_when_auto_resume_is_enabled() {
        let directory = crate::bilibili::test_support::TestDirectory::new();
        let path = directory.0.join("tasks.json");
        let manager = TaskManager::new();
        manager
            .initialize(path.clone(), DownloadManagerOptions::default())
            .unwrap();
        let paused = manager.enqueue(request(&directory.0), None).unwrap();
        manager.control_download(&paused.id, "pause").unwrap();
        let canceled = manager.enqueue(request(&directory.0), None).unwrap();
        manager.take_next().unwrap();
        manager.control_download(&canceled.id, "cancel").unwrap();
        let restored = TaskManager::new();
        restored
            .initialize(
                path,
                DownloadManagerOptions {
                    resume_on_start: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            restored.get_task(&paused.id).unwrap().status,
            TaskStatus::Paused
        );
        assert_eq!(
            restored.get_task(&canceled.id).unwrap().status,
            TaskStatus::Canceled
        );
    }

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
}
