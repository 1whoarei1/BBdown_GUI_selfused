use super::{client::network_error, engine::TaskContext, transfer::publish};
use reqwest::{header, Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Identity {
    total: Option<u64>,
    etag: Option<String>,
    modified: Option<String>,
    resource: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Chunk {
    start: u64,
    end: u64,
    saved: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    identity: Identity,
    chunks: Vec<Chunk>,
    complete: bool,
}

fn request(http: &Client, url: &str) -> reqwest::RequestBuilder {
    http.get(url)
        .header(header::REFERER, "https://www.bilibili.com/")
        .header(header::ACCEPT_ENCODING, "identity")
}
fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", path.display()))
}
fn chunk_path(path: &Path, index: usize) -> PathBuf {
    sidecar(path, &format!(".chunk-{index}"))
}
fn parse_range(value: &str) -> Option<(u64, u64, u64)> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?, total.parse().ok()?))
}
fn identity_from_response(response: &reqwest::Response, url: &str, total: Option<u64>) -> Identity {
    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|h| h.to_str().ok())
        .filter(|h| !h.starts_with("W/"))
        .map(str::to_string);
    let modified = response
        .headers()
        .get(header::LAST_MODIFIED)
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);
    let resource = reqwest::Url::parse(url)
        .map(|url| format!("{:x}", md5::compute(url.path())))
        .unwrap_or_default();
    Identity {
        total,
        etag,
        modified,
        resource,
    }
}
fn valid_chunks(chunks: &[Chunk], total: u64) -> bool {
    if chunks.is_empty() || chunks.len() > 16 || total == 0 {
        return false;
    }
    let mut next = 0;
    for chunk in chunks {
        if chunk.start != next
            || chunk.end < chunk.start
            || chunk.end >= total
            || chunk.saved > chunk.end - chunk.start + 1
        {
            return false;
        }
        next = chunk.end + 1;
    }
    next == total
}
fn compatible(old: &Identity, new: &Identity) -> bool {
    old.total == new.total
        && old.resource == new.resource
        && match (&old.etag, &new.etag) {
            (Some(a), Some(b)) => a == b,
            (None, None) => old.modified.is_some() && old.modified == new.modified,
            _ => false,
        }
}
fn reset(path: &Path, manifest: &Path, old: Option<&Manifest>) -> Result<(), String> {
    if let Some(old) = old {
        for i in 0..old.chunks.len() {
            let _ = fs::remove_file(chunk_path(path, i));
        }
    }
    let _ = fs::remove_file(manifest);
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(sidecar(path, ".assembling"));
    Ok(())
}

pub async fn download(
    http: &Client,
    urls: &[String],
    path: &Path,
    parallel: bool,
    context: &TaskContext,
) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("缓存目录无效")?).map_err(|e| e.to_string())?;
    let mut last = "没有可用下载地址".to_string();
    for url in urls {
        for attempt in 0..3 {
            context.checkpoint()?;
            match attempt_url(http, url, path, parallel, context).await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last = error;
                    if attempt < 2 {
                        tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt + 1)))
                            .await;
                    }
                }
            }
        }
    }
    Err(last)
}

async fn attempt_url(
    http: &Client,
    url: &str,
    path: &Path,
    parallel: bool,
    context: &TaskContext,
) -> Result<(), String> {
    let metadata_path = sidecar(path, ".resume.json");
    let old = match fs::read(&metadata_path) {
        Ok(raw) => Some(
            serde_json::from_slice::<Manifest>(&raw)
                .map_err(|_| "下载缓存损坏，请移除记录后重新添加")?,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    let probe = request(http, url)
        .header(header::RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?;
    let range = probe
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|h| h.to_str().ok())
        .and_then(parse_range)
        .filter(|(a, b, _)| *a == 0 && *b == 0 && probe.status() == StatusCode::PARTIAL_CONTENT);
    let total = range.map(|r| r.2).or_else(|| probe.content_length());
    let current = identity_from_response(&probe, url, total);
    drop(probe);
    let key = path.to_string_lossy().to_string();
    let filename = path.file_name().unwrap_or_default().to_string_lossy();
    let label = if filename.contains("video") {
        "视频"
    } else if filename.contains("audio") {
        "音频"
    } else {
        "附加文件"
    };
    if let Some(old) = &old {
        if old.complete
            && old.version == 1
            && compatible(&old.identity, &current)
            && path
                .metadata()
                .is_ok_and(|m| Some(m.len()) == current.total)
        {
            context.progress(&key, label, current.total.unwrap_or(0), current.total, 0)?;
            return Ok(());
        }
    }
    let can_resume = range.is_some() && (current.etag.is_some() || current.modified.is_some());
    if !can_resume {
        reset(path, &metadata_path, old.as_ref())?;
        return sequential(http, url, path, current, &metadata_path, context, label).await;
    }
    let total = total.ok_or("服务器没有返回媒体长度")?;
    if total == 0 {
        return Err("媒体内容为空".into());
    }
    if let Some(old) = &old {
        if !old.complete && !valid_chunks(&old.chunks, total) {
            reset(path, &metadata_path, Some(old))?;
        }
    }
    let reuse = old.as_ref().filter(|m| {
        m.version == 1
            && !m.complete
            && valid_chunks(&m.chunks, total)
            && compatible(&m.identity, &current)
    });
    let mut manifest = if let Some(old) = reuse {
        old.clone()
    } else {
        reset(path, &metadata_path, old.as_ref())?;
        let count = if parallel && total >= 8 * 1024 * 1024 {
            context.connections().clamp(1, 16) as u64
        } else {
            1
        };
        let size = total.div_ceil(count);
        Manifest {
            version: 1,
            identity: current.clone(),
            complete: false,
            chunks: (0..count)
                .filter_map(|i| {
                    let start = i * size;
                    (start < total).then(|| Chunk {
                        start,
                        end: (start + size - 1).min(total - 1),
                        saved: 0,
                    })
                })
                .collect(),
        }
    };
    for (index, chunk) in manifest.chunks.iter_mut().enumerate() {
        let file = chunk_path(path, index);
        let expected = chunk.end - chunk.start + 1;
        let available = file.metadata().map(|m| m.len()).unwrap_or(0);
        if chunk.saved > expected || available < chunk.saved {
            chunk.saved = 0;
        }
        if file.exists() {
            fs::OpenOptions::new()
                .write(true)
                .open(&file)
                .and_then(|f| f.set_len(chunk.saved))
                .map_err(|e| e.to_string())?;
        }
    }
    crate::storage::write_json(&metadata_path, &manifest)?;
    let state = Arc::new(Mutex::new(manifest));
    let initial = state.lock().unwrap().chunks.iter().map(|c| c.saved).sum();
    context.progress(&key, label, initial, Some(total), 0)?;
    let count = state.lock().unwrap().chunks.len();
    let result = futures_util::future::try_join_all((0..count).map(|index| {
        range_chunk(
            http,
            url,
            path,
            &metadata_path,
            state.clone(),
            index,
            context,
            label,
        )
    }))
    .await;
    if let Err(error) = result {
        context.checkpoint()?;
        // A changed If-Range validator or a server ignoring ranges cannot be mixed with saved chunks.
        if error.starts_with("Range 响应无效") {
            let old = state.lock().unwrap().clone();
            reset(path, &metadata_path, Some(&old))?;
            return sequential(http, url, path, current, &metadata_path, context, label).await;
        }
        return Err(error);
    }
    let assembly = sidecar(path, ".assembling");
    let mut output = fs::File::create(&assembly).map_err(|e| e.to_string())?;
    let mut buffer = vec![0; 64 * 1024];
    for index in 0..count {
        let mut input = fs::File::open(chunk_path(path, index)).map_err(|e| e.to_string())?;
        loop {
            context.checkpoint()?;
            let size = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if size == 0 {
                break;
            }
            output
                .write_all(&buffer[..size])
                .map_err(|e| e.to_string())?;
        }
    }
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    if assembly.metadata().map_err(|e| e.to_string())?.len() != total {
        return Err("合并分片长度不一致".into());
    }
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    publish(&assembly, path).map_err(|e| e.to_string())?;
    let mut manifest = state.lock().unwrap().clone();
    manifest.complete = true;
    crate::storage::write_json(&metadata_path, &manifest)?;
    for index in 0..count {
        let _ = fs::remove_file(chunk_path(path, index));
    }
    context.progress(&key, label, total, Some(total), 0)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn range_chunk(
    http: &Client,
    url: &str,
    path: &Path,
    metadata: &Path,
    state: Arc<Mutex<Manifest>>,
    index: usize,
    context: &TaskContext,
    label: &str,
) -> Result<(), String> {
    let chunk = state.lock().unwrap().chunks[index].clone();
    let identity = state.lock().unwrap().identity.clone();
    let expected = chunk.end - chunk.start + 1;
    if chunk.saved == expected {
        return Ok(());
    }
    let mut request = request(http, url).header(
        header::RANGE,
        format!("bytes={}-{}", chunk.start + chunk.saved, chunk.end),
    );
    if let Some(validator) = identity.etag.as_ref().or(identity.modified.as_ref()) {
        request = request.header(header::IF_RANGE, validator);
    }
    let mut response = request.send().await.map_err(network_error)?;
    let actual = response
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|h| h.to_str().ok())
        .and_then(parse_range);
    if response.status() != StatusCode::PARTIAL_CONTENT
        || actual
            != Some((
                chunk.start + chunk.saved,
                chunk.end,
                identity.total.unwrap(),
            ))
    {
        return Err("Range 响应无效，重新下载文件".into());
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(chunk_path(path, index))
        .map_err(|e| e.to_string())?;
    let mut received = chunk.saved;
    let mut delta = 0;
    let mut last = Instant::now();
    let mut synced = chunk.saved;
    while let Some(bytes) = response.chunk().await.map_err(network_error)? {
        context.checkpoint()?;
        if received + bytes.len() as u64 > expected {
            return Err("分片响应超过请求长度".into());
        }
        // Synchronous bounded writes leave no background file writer after pause acknowledgement.
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        received += bytes.len() as u64;
        delta += bytes.len() as u64;
        if received - synced >= 1024 * 1024 || last.elapsed().as_millis() >= 500 {
            file.sync_data().map_err(|e| e.to_string())?;
            let mut manifest = state.lock().unwrap();
            manifest.chunks[index].saved = received;
            crate::storage::write_json(metadata, &*manifest)?;
            let downloaded = manifest.chunks.iter().map(|c| c.saved).sum();
            drop(manifest);
            context.progress(
                &path.to_string_lossy(),
                label,
                downloaded,
                identity.total,
                delta,
            )?;
            delta = 0;
            last = Instant::now();
            synced = received;
        }
    }
    if received != expected {
        return Err("分片响应不完整".into());
    }
    file.sync_all().map_err(|e| e.to_string())?;
    let mut manifest = state.lock().unwrap();
    manifest.chunks[index].saved = received;
    crate::storage::write_json(metadata, &*manifest)?;
    let downloaded = manifest.chunks.iter().map(|c| c.saved).sum();
    drop(manifest);
    context.progress(
        &path.to_string_lossy(),
        label,
        downloaded,
        identity.total,
        delta,
    )?;
    Ok(())
}

async fn sequential(
    http: &Client,
    url: &str,
    path: &Path,
    identity: Identity,
    metadata: &Path,
    context: &TaskContext,
    label: &str,
) -> Result<(), String> {
    let mut response = request(http, url)
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?;
    if response.status() != StatusCode::OK {
        return Err("服务器未返回完整媒体内容".into());
    }
    let total = response.content_length().or(identity.total);
    let identity = identity_from_response(&response, url, total);
    let file_path = sidecar(path, ".assembling");
    let mut file = fs::File::create(&file_path).map_err(|e| e.to_string())?;
    let mut received = 0;
    let mut delta = 0;
    let mut last = Instant::now();
    context.progress(&path.to_string_lossy(), label, 0, total, 0)?;
    while let Some(bytes) = response.chunk().await.map_err(network_error)? {
        context.checkpoint()?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        received += bytes.len() as u64;
        delta += bytes.len() as u64;
        if last.elapsed().as_millis() >= 500 {
            context.progress(&path.to_string_lossy(), label, received, total, delta)?;
            delta = 0;
            last = Instant::now();
        }
    }
    if received == 0 || total.is_some_and(|t| t != received) {
        return Err("下载内容不完整".into());
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    publish(&file_path, path).map_err(|e| e.to_string())?;
    crate::storage::write_json(
        metadata,
        &Manifest {
            version: 1,
            identity: Identity {
                total: Some(received),
                ..identity
            },
            chunks: vec![],
            complete: true,
        },
    )?;
    context.progress(
        &path.to_string_lossy(),
        label,
        received,
        Some(received),
        delta,
    )?;
    Ok(())
}
