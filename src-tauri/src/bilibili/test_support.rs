use super::engine::TaskContext;
use crate::task::{TaskKind, TaskManager};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

pub struct TestDirectory(pub PathBuf);
impl TestDirectory {
    pub fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("bbdown-native-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn context() -> TaskContext {
    let manager = TaskManager::new();
    let task = manager.create_task(TaskKind::Download, "fixture".into(), None);
    manager.mark_running(&task.id).unwrap();
    TaskContext {
        app: None,
        manager,
        id: task.id,
    }
}

pub struct TestServer {
    pub url: String,
    pub ranges: Arc<AtomicUsize>,
    handle: JoinHandle<()>,
}
impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
impl TestServer {
    pub async fn new(bytes: Arc<Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let ranges = Arc::new(AtomicUsize::new(0));
        let range_count = ranges.clone();
        let handle = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            while let Ok((mut socket, _)) = listener.accept().await {
                let bytes = bytes.clone();
                let range_count = range_count.clone();
                connections.spawn(async move {
                    let mut request = Vec::new();
                    loop {
                        let mut buffer = [0u8; 1024];
                        let Ok(count) = socket.read(&mut buffer).await else { return; };
                        if count == 0 { return; }
                        request.extend_from_slice(&buffer[..count]);
                        if request.windows(4).any(|s| s == b"\r\n\r\n") { break; }
                    }
                    let request = String::from_utf8_lossy(&request);
                    let path = request.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("");
                    if path == "/missing" {
                        let _ = socket.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                        return;
                    }
                    let range = request.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if !name.eq_ignore_ascii_case("range") { return None; }
                        let (start, end) = value.trim().strip_prefix("bytes=")?.split_once('-')?;
                        Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()?))
                    });
                    if let Some((start, end)) = range.filter(|r| path != "/ignore-range" || *r == (0, 0)) {
                        range_count.fetch_add(1, Ordering::Relaxed);
                        let response = format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len(), end - start + 1);
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.write_all(&bytes[start..=end]).await;
                        return;
                    }
                    let size = bytes.len() + if path == "/truncated" { 2 } else { 0 };
                    let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n");
                    let _ = socket.write_all(response.as_bytes()).await;
                    if path == "/slow" {
                        let _ = socket.write_all(&bytes[..1]).await;
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    } else { let _ = socket.write_all(&bytes).await; }
                });
                while connections.try_join_next().is_some() {}
            }
        });
        Self {
            url,
            ranges,
            handle,
        }
    }
}

pub struct JsonServer {
    pub url: String,
    handle: JoinHandle<()>,
}
impl Drop for JsonServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
impl JsonServer {
    pub async fn new(
        route: impl Fn(&reqwest::Url, &str, &str) -> serde_json::Value + Send + Sync + 'static,
    ) -> Self {
        Self::new_with_headers(move |url, body, headers| (route(url, body, headers), vec![])).await
    }

    pub async fn new_with_headers(
        route: impl Fn(&reqwest::Url, &str, &str) -> (serde_json::Value, Vec<(String, String)>)
            + Send
            + Sync
            + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let route = Arc::new(route);
        let handle = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            while let Ok((mut socket, _)) = listener.accept().await {
                let route = route.clone();
                connections.spawn(async move {
                    let mut bytes=Vec::new();
                    let header_end=loop {
                        let mut buffer=[0u8;4096];
                        let Ok(count)=socket.read(&mut buffer).await else { return; };
                        if count==0 { return; }
                        bytes.extend_from_slice(&buffer[..count]);
                        if let Some(index)=bytes.windows(4).position(|s|s==b"\r\n\r\n") { break index+4; }
                    };
                    let headers=String::from_utf8_lossy(&bytes[..header_end]).to_string();
                    let length=headers.lines().find_map(|line|{
                        let (name,value)=line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length").then(||value.trim().parse::<usize>().ok()).flatten()
                    }).unwrap_or(0);
                    while bytes.len()<header_end+length {
                        let mut buffer=[0u8;4096];
                        let Ok(count)=socket.read(&mut buffer).await else { return; };
                        if count==0 { return; }
                        bytes.extend_from_slice(&buffer[..count]);
                    }
                    let request=headers.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("/");
                    let url=reqwest::Url::parse(&format!("http://fixture{request}")).unwrap();
                    let body=String::from_utf8_lossy(&bytes[header_end..header_end+length]);
                    let (value, extra_headers) = route(&url,&body,&headers);
                    let response=serde_json::to_vec(&value).unwrap();
                    let extra_headers = extra_headers.into_iter().map(|(name,value)| {
                        assert!(!name.contains(['\r','\n']) && !value.contains(['\r','\n']));
                        format!("{name}: {value}\r\n")
                    }).collect::<String>();
                    let head=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n",response.len());
                    let _=socket.write_all(head.as_bytes()).await;
                    let _=socket.write_all(&response).await;
                });
                while connections.try_join_next().is_some() {}
            }
        });
        Self { url, handle }
    }
}

pub fn view_fixture(aid: u64) -> serde_json::Value {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/bilibili_view_single_video.json"
    ))
    .unwrap();
    let data = &mut value["data"];
    data["aid"] = aid.into();
    data["bvid"] = format!("BV{aid:010}").into();
    data["title"] = format!("video {aid}").into();
    data["pic"] = format!("https://example.com/{aid}.jpg").into();
    data["desc"] = format!("description {aid}").into();
    data["owner"]["name"] = format!("author {aid}").into();
    data["owner"]["mid"] = (aid * 10).into();
    let mut pages = vec![serde_json::json!({"page":1,"cid":aid*10,"part":"part one","duration":5})];
    if aid == 1 {
        pages.push(serde_json::json!({"page":2,"cid":11,"part":"part two","duration":6}));
    }
    data["pages"] = pages.into();
    value
}
