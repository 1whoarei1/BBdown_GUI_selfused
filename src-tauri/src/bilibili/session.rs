use super::{
    client::Content, download::suffix, engine::TaskContext, selection::select_pages,
    transfer::publish,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
struct Output {
    path: PathBuf,
    size: u64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct PartState {
    base: Option<PathBuf>,
    outputs: Vec<Output>,
    complete: bool,
    #[serde(default)]
    ready: Vec<PathBuf>,
}
#[derive(Serialize, Deserialize)]
struct State {
    version: u32,
    content: Content,
    selection: Vec<u32>,
    parts: HashMap<String, PartState>,
}
pub struct Session {
    root: PathBuf,
    output_root: PathBuf,
    id: String,
    state: State,
}
impl Session {
    pub fn load(root: PathBuf, output_root: PathBuf, id: String) -> Result<Option<Self>, String> {
        let path = root.join("session.json");
        match fs::read(path) {
            Ok(raw) => {
                let state: State = serde_json::from_slice(&raw)
                    .map_err(|_| "任务缓存损坏，请移除记录后重新添加")?;
                if state.version != 1 {
                    return Err("任务缓存版本不支持".into());
                }
                Ok(Some(Self {
                    root,
                    output_root,
                    id,
                    state,
                }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn create(
        root: PathBuf,
        output_root: PathBuf,
        id: String,
        content: Content,
        selection: &str,
    ) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let selected = select_pages(selection, content.parts.len() as u32)?;
        let session = Self {
            root,
            output_root,
            id,
            state: State {
                version: 1,
                content,
                selection: selected,
                parts: HashMap::new(),
            },
        };
        session.save()?;
        Ok(session)
    }
    fn save(&self) -> Result<(), String> {
        crate::storage::write_json(&self.root.join("session.json"), &self.state)
    }
    pub fn content(&self) -> &Content {
        &self.state.content
    }
    pub fn selection(&self) -> Vec<u32> {
        self.state.selection.clone()
    }
    pub fn key(aid: u64, cid: u64, ep: Option<u64>) -> String {
        format!("{aid}-{cid}-{}", ep.unwrap_or(0))
    }
    pub fn cache_base(&self, key: &str) -> Result<PathBuf, String> {
        let folder = self.root.join(key);
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        Ok(folder.join("media"))
    }
    pub fn completed(&self, key: &str) -> Option<Vec<PathBuf>> {
        let part = self.state.parts.get(key)?;
        (part.complete
            && !part.outputs.is_empty()
            && part.outputs.iter().all(|o| {
                o.path
                    .metadata()
                    .is_ok_and(|m| m.is_file() && m.len() == o.size)
            }))
        .then(|| part.outputs.iter().map(|o| o.path.clone()).collect())
    }
    pub fn ready(&self, key: &str) -> Option<Vec<PathBuf>> {
        let part = self.state.parts.get(key)?;
        let base = part.base.as_ref()?;
        let available = part.ready.iter().all(|file| {
            if file.is_file() {
                return true;
            }
            let cached_base = self.root.join(key).join("media");
            let text = file.to_string_lossy();
            let Some(ending) = text.strip_prefix(cached_base.to_string_lossy().as_ref()) else {
                return false;
            };
            let target = suffix(base, ending);
            part.outputs.iter().any(|output| {
                output.path == target && target.metadata().is_ok_and(|m| m.len() == output.size)
            })
        });
        (!part.ready.is_empty() && available).then(|| part.ready.clone())
    }
    pub fn prepare(&mut self, key: &str, files: &[PathBuf]) -> Result<(), String> {
        self.state
            .parts
            .get_mut(key)
            .ok_or("任务没有输出位置")?
            .ready = files.to_vec();
        self.save()
    }
    pub fn allocate(&mut self, key: &str, requested: &Path) -> Result<PathBuf, String> {
        if let Some(base) = self.state.parts.get(key).and_then(|p| p.base.clone()) {
            self.validate(&base)?;
            let lock = lock_path(&base);
            match fs::read_to_string(&lock) {
                Ok(owner) if owner == self.id => {}
                Ok(_) => return Err("输出路径已被其他任务占用".into()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    self.lock(&lock)?;
                }
                Err(e) => return Err(e.to_string()),
            }
            return Ok(base);
        }
        self.validate(requested)?;
        let parent = requested.parent().ok_or("输出目录无效")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let stem = requested
            .file_name()
            .ok_or("输出名称无效")?
            .to_string_lossy();
        for index in 1..1000 {
            let name = if index == 1 {
                stem.to_string()
            } else {
                format!("{stem} ({index})")
            };
            let base = parent.join(&name);
            let lock = lock_path(&base);
            match self.lock(&lock) {
                Ok(()) => {}
                Err(error) if lock.exists() => {
                    let _ = error;
                    continue;
                }
                Err(error) => return Err(error),
            }
            let exists = fs::read_dir(parent)
                .map_err(|e| e.to_string())?
                .flatten()
                .any(|entry| {
                    let filename = entry.file_name().to_string_lossy().into_owned();
                    filename == name || filename.starts_with(&format!("{name}."))
                });
            if exists {
                let _ = fs::remove_file(&lock);
                continue;
            }
            self.state.parts.entry(key.into()).or_default().base = Some(base.clone());
            self.save()?;
            return Ok(base);
        }
        Err("同名输出过多，请修改文件名".into())
    }
    fn lock(&self, path: &Path) -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(self.id.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())
    }
    fn validate(&self, path: &Path) -> Result<(), String> {
        if !path.starts_with(&self.output_root)
            || path
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir))
        {
            return Err("任务输出路径超出下载目录".into());
        }
        Ok(())
    }
    pub fn commit(
        &mut self,
        key: &str,
        cached_base: &Path,
        files: &[PathBuf],
        context: &TaskContext,
    ) -> Result<Vec<PathBuf>, String> {
        let base = self
            .state
            .parts
            .get(key)
            .and_then(|p| p.base.clone())
            .ok_or("任务没有输出位置")?;
        let mut destinations = Vec::new();
        for file in files {
            context.checkpoint()?;
            let ending = file
                .to_string_lossy()
                .strip_prefix(cached_base.to_string_lossy().as_ref())
                .ok_or("缓存文件位置无效")?
                .to_string();
            let target = suffix(&base, &ending);
            self.validate(&target)?;
            let known = self
                .state
                .parts
                .get(key)
                .unwrap()
                .outputs
                .iter()
                .find(|o| o.path == target)
                .cloned();
            if let Some(known) =
                known.filter(|o| o.path.metadata().is_ok_and(|m| m.len() == o.size))
            {
                destinations.push(known.path);
                continue;
            }
            let size = file.metadata().map_err(|e| e.to_string())?.len();
            publish(file, &target)
                .map_err(|e| format!("保存输出失败 {}: {e}", target.display()))?;
            let part = self.state.parts.get_mut(key).unwrap();
            part.outputs.retain(|o| o.path != target);
            part.outputs.push(Output {
                path: target.clone(),
                size,
            });
            // Commit the publication record synchronously before the future can be cancelled.
            self.save()?;
            destinations.push(target);
        }
        self.state.parts.get_mut(key).unwrap().complete = true;
        self.save()?;
        let _ = fs::remove_file(lock_path(&base));
        Ok(destinations)
    }
}
fn lock_path(base: &Path) -> PathBuf {
    base.parent().unwrap_or(Path::new(".")).join(format!(
        ".{}.download-lock",
        base.file_name().unwrap_or_default().to_string_lossy()
    ))
}

pub fn cleanup(root: &Path, output_root: &Path, id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id).map_err(|_| "缓存任务 ID 无效")?;
    if root.file_name().and_then(|s| s.to_str()) != Some(id)
        || root
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            != Some(".bbdown-next-cache")
    {
        return Err("缓存目录无效".into());
    }
    if let Ok(Some(session)) =
        Session::load(root.to_path_buf(), output_root.to_path_buf(), id.into())
    {
        for part in session.state.parts.values() {
            if let Some(base) = &part.base {
                if session.validate(base).is_ok() {
                    let lock = lock_path(base);
                    if fs::read_to_string(&lock).is_ok_and(|owner| owner == id) {
                        let _ = fs::remove_file(lock);
                    }
                }
            }
        }
    }
    match fs::symlink_metadata(root) {
        Ok(meta) if meta.file_type().is_symlink() => Err("缓存目录是链接，已保留".into()),
        Ok(_) => fs::remove_dir_all(root).map_err(|e| format!("清理任务缓存失败: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_reuses_destination_and_publication_records_after_restart() {
        let directory = crate::bilibili::test_support::TestDirectory::new();
        let root = directory.0.join("cache");
        let mut content=crate::bilibili::client::parse_season(&serde_json::json!({"code":0,"result":{"title":"fixture","episodes":[{"id":1,"aid":2,"cid":3,"title":"one"}]}}),Some(1),false).unwrap();
        content.parts[0].duration = 1;
        let mut session = Session::create(
            root.clone(),
            directory.0.clone(),
            "owner".into(),
            content,
            "1",
        )
        .unwrap();
        let base = session
            .allocate("2-3-1", &directory.0.join("movie"))
            .unwrap();
        let cached = session.cache_base("2-3-1").unwrap();
        let file = suffix(&cached, ".mp4");
        fs::write(&file, "complete").unwrap();
        let context = crate::bilibili::test_support::context();
        let files = session.commit("2-3-1", &cached, &[file], &context).unwrap();
        let loaded = Session::load(root, directory.0.clone(), "owner".into())
            .unwrap()
            .unwrap();
        assert_eq!(loaded.completed("2-3-1").unwrap(), files);
        assert_eq!(files[0], suffix(&base, ".mp4"));
    }
}
