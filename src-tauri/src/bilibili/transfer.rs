use super::{client::network_error, engine::TaskContext};
use crate::task::TaskPhase;
use futures_util::future::try_join_all;
use reqwest::{header, Client, StatusCode};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt},
};

pub(crate) struct StagingFile(pub PathBuf);
impl Drop for StagingFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Same-directory, atomic publication without replacing an existing output.
pub(crate) fn publish(source: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
        // Canonical Windows parents retain extended-length prefixes for long paths.
        let source = source.canonicalize()?;
        let target = target
            .parent()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing parent"))?
            .canonicalize()?
            .join(target.file_name().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing filename")
            })?);
        let source = source
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let target = target
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // Both buffers are NUL-terminated and valid for the duration of this call.
        // Omitting REPLACE_EXISTING makes an existing target an error on NTFS and exFAT.
        let moved =
            unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH) };
        if moved == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::hard_link(source, target)
    }
}

/// Downloads to a unique temporary file and publishes only complete, verified data.
pub async fn download(
    http: &Client,
    urls: &[String],
    path: &Path,
    parallel: bool,
    context: &TaskContext,
) -> Result<(), String> {
    if context.manager.cache_root(&context.id).is_some() {
        return super::resume::download(http, urls, path, parallel, context).await;
    }
    if path.exists() {
        return Err(format!(
            "文件已存在，请更换文件名或目录: {}",
            path.display()
        ));
    }
    let parent = path.parent().ok_or("下载路径缺少父目录")?;
    fs::create_dir_all(parent)
        .await
        .map_err(|e| format!("创建下载目录失败: {e}"))?;
    let stage = StagingFile(parent.join(format!(
        ".{}.{}.part",
        path.file_name().unwrap_or_default().to_string_lossy(),
        context.id
    )));
    let mut last_error = "没有可用下载地址".to_string();
    for url in urls {
        for attempt in 0..3 {
            context.checkpoint()?;
            let result = download_url(http, url, &stage.0, parallel, context).await;
            match result {
                Ok(()) => {
                    context.checkpoint()?;
                    publish(&stage.0, path)
                        .map_err(|e| format!("保存下载文件失败 {}: {e}", path.display()))?;
                    return Ok(());
                }
                Err(error) => {
                    last_error = error;
                    if attempt < 2 {
                        tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt + 1)))
                            .await;
                    }
                }
            }
        }
    }
    Err(last_error)
}

fn request(http: &Client, url: &str) -> reqwest::RequestBuilder {
    http.get(url)
        .header(header::REFERER, "https://www.bilibili.com/")
        .header(header::ACCEPT_ENCODING, "identity")
}

async fn download_url(
    http: &Client,
    url: &str,
    stage: &Path,
    parallel: bool,
    context: &TaskContext,
) -> Result<(), String> {
    if parallel {
        let probe = request(http, url)
            .header(header::RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(network_error)?;
        let total = if probe.status() == StatusCode::PARTIAL_CONTENT {
            probe
                .headers()
                .get(header::CONTENT_RANGE)
                .and_then(|h| h.to_str().ok())
                .and_then(content_range)
                .filter(|(start, end, _)| *start == 0 && *end == 0)
                .map(|(_, _, total)| total)
        } else {
            None
        };
        drop(probe);
        if let Some(total) = total.filter(|total| *total >= 8 * 1024 * 1024) {
            let file = File::create(stage).await.map_err(io_error)?;
            file.set_len(total).await.map_err(io_error)?;
            drop(file);
            let progress = Arc::new(AtomicU64::new(0));
            let reported = Arc::new(AtomicU64::new(0));
            let chunks = ranges(total, 4);
            let result = try_join_all(chunks.iter().map(|&(start, end)| {
                range_download(
                    http,
                    url,
                    stage,
                    start,
                    end,
                    total,
                    progress.clone(),
                    reported.clone(),
                    context,
                )
            }))
            .await;
            if result.is_ok() {
                return Ok(());
            }
            context.checkpoint()?;
            context.report(
                TaskPhase::DownloadingPart,
                "服务器分段下载失败，切换为单连接重试",
            )?;
        }
    }
    let mut response = request(http, url)
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?;
    if response.status() != StatusCode::OK {
        return Err(format!("下载服务器返回非完整响应: {}", response.status()));
    }
    let total = response.content_length();
    let mut file = File::create(stage).await.map_err(io_error)?;
    let mut downloaded = 0;
    let mut last_report = std::time::Instant::now();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        context.checkpoint()?;
        file.write_all(&chunk).await.map_err(io_error)?;
        downloaded += chunk.len() as u64;
        if last_report.elapsed().as_secs() >= 1 {
            context.report(
                TaskPhase::DownloadingPart,
                progress_message(downloaded, total),
            )?;
            last_report = std::time::Instant::now();
        }
    }
    if downloaded == 0 || total.is_some_and(|size| size != downloaded) {
        return Err("下载内容不完整，将重试或切换备用地址".to_string());
    }
    file.flush().await.map_err(io_error)?;
    file.sync_all().await.map_err(io_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn range_download(
    http: &Client,
    url: &str,
    stage: &Path,
    start: u64,
    end: u64,
    total: u64,
    progress: Arc<AtomicU64>,
    reported: Arc<AtomicU64>,
    context: &TaskContext,
) -> Result<(), String> {
    let mut response = request(http, url)
        .header(header::RANGE, format!("bytes={start}-{end}"))
        .send()
        .await
        .map_err(network_error)?;
    let actual = response
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|h| h.to_str().ok())
        .and_then(content_range);
    if response.status() != StatusCode::PARTIAL_CONTENT || actual != Some((start, end, total)) {
        return Err("下载服务器未正确响应 Range 请求".to_string());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .open(stage)
        .await
        .map_err(io_error)?;
    file.seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(io_error)?;
    let expected = end - start + 1;
    let mut received = 0;
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        context.checkpoint()?;
        received += chunk.len() as u64;
        if received > expected {
            return Err("分段下载响应超出请求范围".to_string());
        }
        file.write_all(&chunk).await.map_err(io_error)?;
        let downloaded =
            progress.fetch_add(chunk.len() as u64, Ordering::Relaxed) + chunk.len() as u64;
        let percent = downloaded.saturating_mul(100) / total;
        let previous = reported.load(Ordering::Relaxed);
        if percent >= previous + 5
            && reported
                .compare_exchange(previous, percent, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            context.report(
                TaskPhase::DownloadingPart,
                progress_message(downloaded, Some(total)),
            )?;
        }
    }
    if received != expected {
        return Err("分段下载内容不完整".to_string());
    }
    file.flush().await.map_err(io_error)?;
    file.sync_all().await.map_err(io_error)?;
    Ok(())
}

fn io_error(error: std::io::Error) -> String {
    format!("读写下载文件失败: {error}")
}
fn progress_message(downloaded: u64, total: Option<u64>) -> String {
    match total {
        Some(total) if total > 0 => format!(
            "下载中 {:.1}% ({:.1} / {:.1} MB)",
            downloaded as f64 * 100.0 / total as f64,
            downloaded as f64 / 1048576.0,
            total as f64 / 1048576.0
        ),
        _ => format!("下载中 {:.1} MB", downloaded as f64 / 1048576.0),
    }
}
fn ranges(total: u64, count: u64) -> Vec<(u64, u64)> {
    let size = total.div_ceil(count);
    (0..count)
        .filter_map(|i| {
            let start = i * size;
            (start < total).then(|| (start, (start + size - 1).min(total - 1)))
        })
        .collect()
}
fn content_range(value: &str) -> Option<(u64, u64, u64)> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?, total.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bilibili::test_support::{context, TestDirectory, TestServer};

    #[tokio::test]
    async fn downloads_ranges_and_falls_back_when_server_ignores_them() {
        let data = Arc::new(
            (0..9 * 1024 * 1024)
                .map(|i| (i % 251) as u8)
                .collect::<Vec<_>>(),
        );
        let server = TestServer::new(data.clone()).await;
        let directory = TestDirectory::new();
        let http = Client::builder().no_proxy().build().unwrap();
        let task = context();
        for (index, endpoint) in ["/data", "/ignore-range"].iter().enumerate() {
            let path = directory.0.join(format!("test-{index}.m4s"));
            download(
                &http,
                &[format!("{}{endpoint}", server.url)],
                &path,
                true,
                &task,
            )
            .await
            .unwrap();
            assert_eq!(&fs::read(&path).await.unwrap(), data.as_ref());
        }
        assert!(server.ranges.load(Ordering::Relaxed) >= 5);
    }

    #[tokio::test]
    async fn retries_backup_and_never_publishes_partial_data_or_overwrites() {
        let data = Arc::new(b"test bytes".to_vec());
        let server = TestServer::new(data.clone()).await;
        let directory = TestDirectory::new();
        let http = Client::builder().no_proxy().build().unwrap();
        let task = context();
        let complete = directory.0.join("complete");
        download(
            &http,
            &[
                format!("{}/missing", server.url),
                format!("{}/data", server.url),
            ],
            &complete,
            false,
            &task,
        )
        .await
        .unwrap();
        assert_eq!(&fs::read(&complete).await.unwrap(), data.as_ref());
        assert!(download(
            &http,
            &[format!("{}/data", server.url)],
            &complete,
            false,
            &task
        )
        .await
        .is_err());
        let partial = directory.0.join("partial");
        assert!(download(
            &http,
            &[format!("{}/truncated", server.url)],
            &partial,
            false,
            &task
        )
        .await
        .is_err());
        assert!(!partial.exists());
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn cancellation_stops_a_stalled_response() {
        let server = TestServer::new(Arc::new(b"hello".to_vec())).await;
        let directory = TestDirectory::new();
        let path = directory.0.join("cancelled");
        let task = context();
        let http = Client::builder().no_proxy().build().unwrap();
        let urls = [format!("{}/slow", server.url)];
        let operation =
            crate::commands::run_task(&task, download(&http, &urls, &path, false, &task));
        let cancel = async {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            task.manager.stop_task(&task.id).await.unwrap();
        };
        let (result, ()) = tokio::join!(operation, cancel);
        assert!(result.is_err());
        assert!(!path.exists());
    }
    #[test]
    fn ranges_cover_file_without_overlaps() {
        assert_eq!(ranges(10, 4), vec![(0, 2), (3, 5), (6, 8), (9, 9)]);
        assert_eq!(content_range("bytes 0-0/123"), Some((0, 0, 123)));
        assert_eq!(content_range("bytes */123"), None);
    }
}
