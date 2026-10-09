use super::{
    client::{BiliClient, Content},
    streams::parse_play_response,
};
use crate::{
    core::{MetadataSource, ParseRequest, ParseResultV2, PartInfoV2},
    task::{
        emit_task_log, emit_task_status, RuntimeHint, TaskLogEvent, TaskLogStream, TaskManager,
        TaskPhase, TaskStatus,
    },
};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::AppHandle;

#[derive(Clone)]
pub struct TaskContext {
    pub app: Option<AppHandle>,
    pub manager: TaskManager,
    pub id: String,
}

impl TaskContext {
    pub fn checkpoint(&self) -> Result<(), String> {
        match self.manager.get_task(&self.id) {
            Some(task)
                if !matches!(
                    task.status,
                    TaskStatus::Canceled
                        | TaskStatus::Stopping
                        | TaskStatus::Pausing
                        | TaskStatus::Paused
                ) =>
            {
                Ok(())
            }
            _ => Err("任务已取消".to_string()),
        }
    }
    pub fn connections(&self) -> u32 {
        self.manager.connection_limit(&self.id)
    }
    pub fn progress(
        &self,
        key: &str,
        label: &str,
        bytes: u64,
        total: Option<u64>,
        delta: u64,
    ) -> Result<(), String> {
        let snapshot = self
            .manager
            .progress(&self.id, key, label, bytes, total, delta)?;
        if let Some(app) = &self.app {
            emit_task_status(app, &snapshot)?;
        }
        Ok(())
    }
    pub fn completed_part(
        &self,
        completed: u32,
        total: u32,
        files: &[std::path::PathBuf],
    ) -> Result<(), String> {
        let snapshot = self
            .manager
            .part_completed(&self.id, completed, total, files)?;
        if let Some(app) = &self.app {
            emit_task_status(app, &snapshot)?;
        }
        Ok(())
    }
    pub fn report(&self, phase: TaskPhase, message: impl Into<String>) -> Result<(), String> {
        self.checkpoint()?;
        let message = message.into();
        let snapshot = self.manager.apply_runtime_hint(
            &self.id,
            RuntimeHint {
                phase: Some(phase),
                latest_message: Some(message.clone()),
                ..Default::default()
            },
        )?;
        if let Some(app) = &self.app {
            emit_task_status(app, &snapshot)?;
            emit_task_log(
                app,
                &TaskLogEvent {
                    task_id: self.id.clone(),
                    stream: TaskLogStream::System,
                    line: message,
                    timestamp: crate::task::now_timestamp(),
                },
            )?;
        }
        Ok(())
    }
    pub fn part(&self, content: &Content, index: u32, title: &str) -> Result<(), String> {
        self.checkpoint()?;
        let snapshot = self.manager.apply_runtime_hint(
            &self.id,
            RuntimeHint {
                title: Some(content.title.clone()),
                current_part_index: Some(index),
                total_parts: Some(content.parts.len() as u32),
                current_part_title: Some(title.to_string()),
                ..Default::default()
            },
        )?;
        if let Some(app) = &self.app {
            emit_task_status(app, &snapshot)?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct ParseCache(Mutex<HashMap<String, (Instant, ParseResultV2)>>);

impl ParseCache {
    fn key(request: &ParseRequest) -> String {
        // Credentials influence accessible streams, but are never stored as plaintext keys.
        format!(
            "{:x}",
            md5::compute(format!(
                "{}:{:?}:{}:{}:{}:{}:{:?}",
                request.input.trim(),
                request.config.auth.api_mode,
                request.config.auth.cookie.as_deref().unwrap_or(""),
                request.config.auth.user_agent.as_deref().unwrap_or(""),
                request.show_all_parts,
                request.config.auth.access_token.as_deref().unwrap_or(""),
                request.config.default_options.codec_priority
            ))
        )
    }
    pub fn get(&self, request: &ParseRequest) -> Option<ParseResultV2> {
        if !request.use_cache {
            return None;
        }
        self.0
            .lock()
            .ok()?
            .get(&Self::key(request))
            .filter(|(created, _)| created.elapsed() < Duration::from_secs(60))
            .map(|(_, result)| result.clone())
    }
    pub fn insert(&self, request: &ParseRequest, result: &ParseResultV2) {
        if let Ok(mut entries) = self.0.lock() {
            entries.retain(|_, (created, _)| created.elapsed() < Duration::from_secs(60));
            if entries.len() >= 20 {
                entries.clear();
            }
            entries.insert(Self::key(request), (Instant::now(), result.clone()));
        }
    }
    pub fn clear(&self) {
        if let Ok(mut entries) = self.0.lock() {
            entries.clear();
        }
    }
}

pub async fn parse(request: &ParseRequest, context: &TaskContext) -> Result<ParseResultV2, String> {
    let mut client = BiliClient::new(&request.config)?;
    client.context = Some(context.clone());
    context.report(TaskPhase::FetchingVideoInfo, "正在获取视频信息")?;
    let content = client.resolve(&request.input).await?;
    let mut parts = vec![];
    let mut warnings = content.warnings.clone();
    for part in &content.parts {
        context.part(&content, part.number, &part.title)?;
        let mut duration = part.duration;
        let mut video_streams = vec![];
        let mut audio_streams = vec![];
        if request.show_all_parts || part.number == 1 {
            context.report(
                TaskPhase::ParsingPart,
                format!("正在解析 P{}: {}", part.number, part.title),
            )?;
            match client.play(part).await.and_then(|value| {
                let root = value
                    .get("result")
                    .or_else(|| value.get("data"))
                    .unwrap_or(&value);
                let root = root.get("video_info").unwrap_or(root);
                if let Some(ms) = root["timelength"].as_u64() {
                    duration = ms / 1000;
                }
                parse_play_response(&value, duration)
            }) {
                Ok(streams) => {
                    video_streams = streams.video.into_iter().map(|t| t.info).collect();
                    audio_streams = streams.audio.into_iter().map(|t| t.info).collect();
                }
                Err(error) => warnings.push(format!("P{}: {error}", part.number)),
            }
        }
        parts.push(PartInfoV2 {
            watch_url: part.watch_url.clone(),
            page_number: part.number,
            cid: (part.cid > 0).then_some(part.cid),
            aid: (part.aid > 0).then_some(part.aid),
            bvid: (!part.bvid.is_empty()).then(|| part.bvid.clone()),
            title: part.title.clone(),
            duration_seconds: Some(duration),
            video_streams,
            audio_streams,
        });
    }
    context.checkpoint()?;
    let first = content.parts.first().ok_or("没有可用分 P")?;
    let is_list = matches!(content.kind, crate::core::ContentKind::VideoList);
    let duration = parts.iter().filter_map(|p| p.duration_seconds).sum();
    Ok(ParseResultV2 {
        schema_version: 2,
        id: uuid::Uuid::new_v4().to_string(),
        input: request.input.clone(),
        content_kind: content.kind,
        aid: (!is_list && first.aid > 0).then_some(first.aid),
        bvid: (!is_list && !first.bvid.is_empty()).then(|| first.bvid.clone()),
        title: content.title,
        description: Some(content.description),
        owner: content.owner,
        publish_time: content.publish_time,
        duration_seconds: Some(duration),
        cover_url: Some(content.cover),
        parts,
        metadata_source: MetadataSource::Bilibili,
        warnings,
        error_message: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires access to Bilibili's live WEB API"]
    async fn live_web_parse_smoke() {
        let request = ParseRequest {
            input: "BV17x411w7KC".into(),
            config: crate::core::default_config(),
            show_all_parts: true,
            use_cache: false,
        };
        let context = crate::bilibili::test_support::context();
        let result = parse(&request, &context).await.unwrap();
        assert_eq!(result.bvid.as_deref(), Some("BV17x411w7KC"));
        assert!(
            !result.parts[0].video_streams.is_empty(),
            "{:?}",
            result.warnings
        );
    }
}
