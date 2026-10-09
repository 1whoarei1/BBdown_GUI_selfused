use crate::{
    bilibili::{download, engine::TaskContext},
    commands::effective_config,
    config,
    core::DownloadRequest,
    task::{emit_task_status, TaskManager},
};
use tauri::AppHandle;

pub fn start_dispatcher(app: AppHandle, manager: TaskManager) {
    manager.wake.notify_one();
    tauri::async_runtime::spawn(async move {
        loop {
            let wake = manager.wake.notified();
            loop {
                match manager.take_next() {
                    Ok(Some((snapshot, request))) => {
                        let _ = emit_task_status(&app, &snapshot);
                        let app = app.clone();
                        let manager = manager.clone();
                        tauri::async_runtime::spawn(async move {
                            execute(app, manager, snapshot.id, request).await;
                        });
                    }
                    Ok(None) => break,
                    Err(error) => {
                        crate::diagnostics::record(&app, format!("[queue-error] {error}"));
                        eprintln!("下载队列存储错误: {error}");
                        break;
                    }
                }
            }
            wake.await;
        }
    });
}

async fn execute(app: AppHandle, manager: TaskManager, id: String, mut request: DownloadRequest) {
    let context = TaskContext {
        app: Some(app.clone()),
        manager: manager.clone(),
        id: id.clone(),
    };
    let result = async {
        // Use the current saved account, while retaining the task's media settings and API mode.
        let current = config::load_config(&app)?;
        if request
            .config
            .auth
            .cookie
            .as_deref()
            .is_none_or(str::is_empty)
        {
            request.config.auth.cookie = current.auth.cookie;
        }
        if request
            .config
            .auth
            .access_token
            .as_deref()
            .is_none_or(str::is_empty)
        {
            let compatible = matches!(
                (&request.config.auth.api_mode, &current.auth.api_mode),
                (
                    crate::core::ApiMode::Tv | crate::core::ApiMode::App,
                    crate::core::ApiMode::Tv | crate::core::ApiMode::App
                ) | (crate::core::ApiMode::Intl, crate::core::ApiMode::Intl)
            );
            if compatible {
                request.config.auth.access_token = current.auth.access_token;
            }
        }
        request.config = effective_config(&app, request.config)?;
        let mut signal = manager.cancel_signal(&id)?;
        context.checkpoint()?;
        tokio::select! {
            biased;
            _=signal.changed()=>Err("任务已停止".into()),
            output=download::run(&request,&context)=>output,
        }
    }
    .await;
    match manager.finish_download(&id, result) {
        Ok(snapshot) => {
            crate::task::log_task_outcome(&app, &snapshot);
            let _ = emit_task_status(&app, &snapshot);
            if snapshot.status == crate::task::TaskStatus::Completed {
                if let (Some(root), Some(output)) =
                    (manager.cache_root(&id), snapshot.download_dir.as_ref())
                {
                    if let Err(error) =
                        crate::bilibili::session::cleanup(&root, std::path::Path::new(output), &id)
                    {
                        crate::diagnostics::record(&app, format!("[cache-error] [{id}] {error}"));
                    }
                }
            }
        }
        Err(error) => crate::diagnostics::record(
            &app,
            format!("[queue-error] [{id}] 保存下载任务失败: {error}"),
        ),
    }
}
