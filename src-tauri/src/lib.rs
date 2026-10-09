mod bilibili;
mod commands;
mod config;
mod core;
mod diagnostics;
mod downloads;
mod storage;
mod task;
mod tools;
mod utils;

use commands::{
    build_download_preview, build_preview_command, control_downloads, detect_tools,
    get_account_info, get_config, list_tasks, logout_account, open_bilibili_link,
    open_download_directory, open_task_directory, open_tool_download_page, parse_video,
    parse_video_v2, run_download, run_login, save_config, stop_task,
};
use diagnostics::{read_application_logs, record_application_log, DiagnosticLog};
use task::TaskManager;

pub fn run() {
    tauri::Builder::default()
        .manage(TaskManager::new())
        .manage(DiagnosticLog::default())
        .manage(bilibili::engine::ParseCache::default())
        .setup(|app| {
            use tauri::Manager;
            let handle = app.handle().clone();
            let config_path = config::config_file_path(&handle).map_err(std::io::Error::other)?;
            app.state::<DiagnosticLog>()
                .initialize(config_path.with_file_name("logs").join("application.jsonl"))
                .map_err(std::io::Error::other)?;
            diagnostics::record(&handle, "[startup] 应用启动");
            let manager = app.state::<TaskManager>().inner().clone();
            let settings = config::load_config(&handle).map_err(std::io::Error::other)?;
            let path = config::config_file_path(&handle)
                .map_err(std::io::Error::other)?
                .with_file_name("tasks.json");
            manager
                .initialize(path, settings.download_manager)
                .map_err(std::io::Error::other)?;
            downloads::start_dispatcher(handle, manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            detect_tools,
            get_account_info,
            logout_account,
            open_download_directory,
            open_bilibili_link,
            open_tool_download_page,
            build_preview_command,
            build_download_preview,
            list_tasks,
            read_application_logs,
            record_application_log,
            stop_task,
            control_downloads,
            open_task_directory,
            run_login,
            run_download,
            parse_video,
            parse_video_v2,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BBDown Next");
}
