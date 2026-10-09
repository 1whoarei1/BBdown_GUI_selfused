use super::{engine::TaskContext, test_support::TestDirectory, transfer};
use crate::{core::DownloadRequest, task::TaskManager};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct Server {
    url: String,
    version: Arc<AtomicU64>,
    requests: Arc<Mutex<Vec<(u64, u64)>>>,
    handle: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
impl Server {
    async fn new(size: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let version = Arc::new(AtomicU64::new(1));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let ver = version.clone();
        let reqs = requests.clone();
        let handle = tokio::spawn(async move {
            let mut children = tokio::task::JoinSet::new();
            while let Ok((mut socket, _)) = listener.accept().await {
                let ver = ver.clone();
                let reqs = reqs.clone();
                children.spawn(async move {
                    let mut data=Vec::new();
                    loop {let mut buffer=[0;2048];let Ok(count)=socket.read(&mut buffer).await else{return;};if count==0{return;}data.extend_from_slice(&buffer[..count]);if data.windows(4).any(|x|x==b"\r\n\r\n"){break;}}
                    let request=String::from_utf8_lossy(&data);let version=ver.load(Ordering::Relaxed);
                    let mut range=None;let mut if_range=None;
                    for line in request.lines() {if let Some((name,value))=line.split_once(':') {
                        if name.eq_ignore_ascii_case("range") {let (a,b)=value.trim().strip_prefix("bytes=").unwrap().split_once('-').unwrap();range=Some((a.parse::<u64>().unwrap(),b.parse::<u64>().unwrap()));}
                        if name.eq_ignore_ascii_case("if-range") {if_range=Some(value.trim().to_string());}
                    }}
                    let tag=format!("\"version-{version}\"");
                    let partial=range.is_some() && if_range.is_none_or(|old|old==tag);
                    let (start,end)=if partial {range.unwrap()} else {(0,size as u64-1)};
                    reqs.lock().unwrap().push((start,end));
                    let status=if partial {"206 Partial Content"} else {"200 OK"};
                    let range_header=if partial {format!("Content-Range: bytes {start}-{end}/{size}\r\n")} else {String::new()};
                    let headers=format!("HTTP/1.1 {status}\r\n{range_header}ETag: {tag}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",end-start+1);
                    if socket.write_all(headers.as_bytes()).await.is_err(){return;}
                    let mut offset=start;
                    while offset<=end {let count=(end-offset+1).min(65536) as usize;let bytes=(offset..offset+count as u64).map(|i|((i+version)%251) as u8).collect::<Vec<_>>();if socket.write_all(&bytes).await.is_err(){return;}offset+=count as u64;tokio::time::sleep(std::time::Duration::from_millis(5)).await;}
                });
                while children.try_join_next().is_some() {}
            }
        });
        Self {
            url,
            version,
            requests,
            handle,
        }
    }
}
fn managed(directory: &TestDirectory) -> (TaskManager, TaskContext, std::path::PathBuf) {
    let manager = TaskManager::new();
    let mut config = crate::core::default_config();
    config.work_dir = directory.0.to_string_lossy().into();
    let task = manager
        .enqueue(
            DownloadRequest {
                input: "fixture".into(),
                config,
                parse_id: None,
            },
            Some("fixture".into()),
        )
        .unwrap();
    manager.take_next().unwrap();
    let path = manager.cache_root(&task.id).unwrap().join("video.m4s");
    let context = TaskContext {
        app: None,
        manager: manager.clone(),
        id: task.id,
    };
    (manager, context, path)
}
async fn pause_partial(
    server: &Server,
    manager: &TaskManager,
    context: &TaskContext,
    path: &std::path::Path,
) {
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let urls = [format!("{}/media?token=expired-secret", server.url)];
    let mut signal = manager.cancel_signal(&context.id).unwrap();
    let worker = async {
        tokio::select! {biased;_=signal.changed()=>Err("paused".to_string()),result=transfer::download(&http,&urls,path,true,context)=>result}
    };
    let pause = async {
        loop {
            if manager.get_task(&context.id).unwrap().downloaded_bytes >= 1024 * 1024 {
                manager.control_download(&context.id, "pause").unwrap();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        tokio::join!(worker, pause)
    })
    .await
    .unwrap();
    assert!(result.is_err());
    manager
        .finish_download(&context.id, result.map(|_| String::new()))
        .unwrap();
}
#[tokio::test]
async fn paused_ranges_resume_from_committed_offsets_with_fresh_url() {
    let directory = TestDirectory::new();
    let server = Server::new(9 * 1024 * 1024).await;
    let (manager, context, path) = managed(&directory);
    pause_partial(&server, &manager, &context, &path).await;
    let raw = std::fs::read_to_string(format!("{}.resume.json", path.display())).unwrap();
    assert!(!raw.contains("expired-secret"));
    let before = server.requests.lock().unwrap().len();
    manager.control_download(&context.id, "resume").unwrap();
    manager.take_next().unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    transfer::download(
        &http,
        &[format!("{}/media?token=fresh-secret", server.url)],
        &path,
        true,
        &context,
    )
    .await
    .unwrap();
    let data = std::fs::read(&path).unwrap();
    assert_eq!(
        data,
        (0..9 * 1024 * 1024u64)
            .map(|i| ((i + 1) % 251) as u8)
            .collect::<Vec<_>>()
    );
    let resumed = server.requests.lock().unwrap()[before..].to_vec();
    assert!(resumed
        .iter()
        .any(|(start, end)| *start > 0 && end - start + 1 < 3 * 1024 * 1024));
}
#[tokio::test]
async fn changed_validator_restarts_without_mixing_saved_bytes() {
    let directory = TestDirectory::new();
    let server = Server::new(9 * 1024 * 1024).await;
    let (manager, context, path) = managed(&directory);
    pause_partial(&server, &manager, &context, &path).await;
    server.version.store(2, Ordering::Relaxed);
    manager.control_download(&context.id, "resume").unwrap();
    manager.take_next().unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    transfer::download(
        &http,
        &[format!("{}/media", server.url)],
        &path,
        true,
        &context,
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(path).unwrap(),
        (0..9 * 1024 * 1024u64)
            .map(|i| ((i + 2) % 251) as u8)
            .collect::<Vec<_>>()
    );
}
