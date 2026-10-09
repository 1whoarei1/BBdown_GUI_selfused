use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};

const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ENTRIES: usize = 1000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub line: String,
}

#[derive(Default)]
pub struct DiagnosticLog(Mutex<Option<PathBuf>>);

impl DiagnosticLog {
    pub fn initialize(&self, path: PathBuf) -> Result<(), String> {
        fs::create_dir_all(path.parent().ok_or("日志目录无效")?)
            .map_err(|e| format!("创建日志目录失败: {e}"))?;
        *self.0.lock().map_err(|_| "日志锁不可用")? = Some(path);
        Ok(())
    }

    fn append(&self, mut entry: LogEntry) -> Result<LogEntry, String> {
        entry.line = sanitize(&entry.line).chars().take(16_000).collect();
        let guard = self.0.lock().map_err(|_| "日志锁不可用")?;
        let path = guard.as_ref().ok_or("日志尚未初始化")?;
        let mut bytes = serde_json::to_vec(&entry).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        if path.metadata().map(|m| m.len()).unwrap_or(0) + bytes.len() as u64 > MAX_LOG_BYTES {
            let previous = path.with_extension("previous.jsonl");
            if previous.exists() {
                fs::remove_file(&previous).map_err(|e| format!("轮换日志失败: {e}"))?;
            }
            if path.exists() {
                fs::rename(path, previous).map_err(|e| format!("轮换日志失败: {e}"))?;
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("打开日志失败: {e}"))?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 0 {
            file.seek(SeekFrom::End(-1)).map_err(|e| e.to_string())?;
            let mut last = [0];
            file.read_exact(&mut last).map_err(|e| e.to_string())?;
            if last[0] != b'\n' {
                file.write_all(b"\n").map_err(|e| e.to_string())?;
            }
        }
        file.write_all(&bytes)
            .and_then(|_| file.sync_data())
            .map_err(|e| format!("保存日志失败: {e}"))?;
        Ok(entry)
    }

    fn recent(&self) -> Result<Vec<LogEntry>, String> {
        let guard = self.0.lock().map_err(|_| "日志锁不可用")?;
        let path = guard.as_ref().ok_or("日志尚未初始化")?;
        let mut entries = read_entries(&path.with_extension("previous.jsonl"))?;
        entries.extend(read_entries(path)?);
        if entries.len() > MAX_ENTRIES {
            entries.drain(..entries.len() - MAX_ENTRIES);
        }
        Ok(entries)
    }
}

fn read_entries(path: &Path) -> Result<Vec<LogEntry>, String> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(raw
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(format!("读取日志失败: {e}")),
    }
}

fn sanitize(line: &str) -> String {
    let mut result = line.to_string();
    for scheme in ["https://", "http://"] {
        let mut offset = 0;
        while let Some(found) = result[offset..].find(scheme) {
            let start = offset + found;
            let end = result[start..]
                .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>'))
                .map(|n| start + n)
                .unwrap_or(result.len());
            if let Ok(mut url) = reqwest::Url::parse(&result[start..end]) {
                url.set_query(None);
                url.set_fragment(None);
                let _ = url.set_username("");
                let _ = url.set_password(None);
                let safe = url.to_string();
                result.replace_range(start..end, &safe);
                offset = start + safe.len();
            } else {
                offset = end;
            }
        }
    }
    for name in [
        "cookie",
        "sessdata",
        "bili_jct",
        "access_key",
        "access_token",
        "accesstoken",
        "refresh_token",
        "token",
    ] {
        let mut offset = 0;
        while let Some(found) = result[offset..].to_ascii_lowercase().find(name) {
            let start = offset + found;
            let key_end = start + name.len();
            let tail = result[key_end..].trim_start_matches([' ', '"', '\'']);
            if !tail.starts_with(['=', ':']) {
                offset = key_end;
                continue;
            }
            let separator = result.len() - tail.len();
            let value_tail = result[separator + 1..].trim_start_matches([' ', '"', '\'']);
            let value_start = result.len() - value_tail.len();
            let value_end = if name == "cookie" {
                value_start + value_tail.find(['\r', '\n']).unwrap_or(value_tail.len())
            } else {
                value_start
                    + value_tail
                        .find(|c: char| {
                            c.is_whitespace() || matches!(c, '&' | ';' | ',' | '"' | '\'')
                        })
                        .unwrap_or(value_tail.len())
            };
            result.replace_range(value_start..value_end, "[REDACTED]");
            offset = value_start + "[REDACTED]".len();
        }
    }
    result
}

pub fn record(app: &AppHandle, line: impl Into<String>) {
    let entry = LogEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: crate::task::now_timestamp(),
        line: line.into(),
    };
    if let Err(error) = publish(app, entry) {
        eprintln!("{error}");
    }
}

fn publish(app: &AppHandle, entry: LogEntry) -> Result<LogEntry, String> {
    let entry = app.state::<DiagnosticLog>().append(entry)?;
    let _ = app.emit("application-log", &entry);
    Ok(entry)
}

#[tauri::command]
pub fn read_application_logs(log: State<'_, DiagnosticLog>) -> Result<Vec<LogEntry>, String> {
    log.recent()
}

#[tauri::command]
pub fn record_application_log(app: AppHandle, entry: LogEntry) -> Result<LogEntry, String> {
    publish(&app, entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_survive_restart_without_credentials_and_tolerate_partial_last_entry() {
        let directory = crate::bilibili::test_support::TestDirectory::new();
        let path = directory.0.join("application.jsonl");
        let log = DiagnosticLog::default();
        log.initialize(path.clone()).unwrap();
        let entry = log.append(LogEntry {
            id: "fixture".into(), timestamp: "1".into(),
            line: "[download-error] HTTP 403 https://cdn.example/video?token=url-secret&sign=signature access_token=private-token\nCookie: SESSDATA=private-cookie; bili_jct=private-jct".into(),
        }).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        for secret in [
            "url-secret",
            "signature",
            "private-token",
            "private-cookie",
            "private-jct",
        ] {
            assert!(!raw.contains(secret), "{secret}");
        }
        assert!(entry.line.contains("HTTP 403 https://cdn.example/video"));
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{partial")
            .unwrap();
        let restored = DiagnosticLog::default();
        restored.initialize(path).unwrap();
        assert_eq!(restored.recent().unwrap()[0].id, "fixture");
        restored
            .append(LogEntry {
                id: "after-restart".into(),
                timestamp: "2".into(),
                line: "resumed".into(),
            })
            .unwrap();
        assert_eq!(restored.recent().unwrap().len(), 2);
    }

    #[test]
    fn rotates_logs_and_limits_recent_history() {
        let directory = crate::bilibili::test_support::TestDirectory::new();
        let path = directory.0.join("application.jsonl");
        fs::write(&path, vec![b' '; MAX_LOG_BYTES as usize]).unwrap();
        let log = DiagnosticLog::default();
        log.initialize(path.clone()).unwrap();
        for n in 0..1002 {
            log.append(LogEntry {
                id: n.to_string(),
                timestamp: "1".into(),
                line: "failure".into(),
            })
            .unwrap();
        }
        assert!(path.with_extension("previous.jsonl").exists());
        let entries = log.recent().unwrap();
        assert_eq!(entries.len(), 1000);
        assert_eq!(entries[0].id, "2");
    }
}
