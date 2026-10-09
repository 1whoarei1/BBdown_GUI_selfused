use serde::Serialize;
use std::{fs, io::Write, path::Path};

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("存储路径缺少目录")?;
    fs::create_dir_all(parent).map_err(|e| format!("创建存储目录失败: {e}"))?;
    let temp = parent.join(format!(".state-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let raw = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&raw)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temp, path).map_err(|e| format!("保存任务状态失败: {e}"))
    })();
    let _ = fs::remove_file(temp);
    result
}
