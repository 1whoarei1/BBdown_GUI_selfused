mod bbdown;
mod bilibili;
mod commands;
mod config;
mod core;
mod task;
mod tools;
mod utils;

use commands::{
    build_download_preview, build_preview_command, detect_tools, get_account_info, get_config,
    list_tasks, logout_account, open_bilibili_link, open_download_directory,
    open_tool_download_page, parse_video, parse_video_v2, run_download, run_login, save_config,
    stop_task,
};
use task::TaskManager;

pub fn run() {
    tauri::Builder::default()
        .manage(TaskManager::new())
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
            stop_task,
            run_login,
            run_download,
            parse_video,
            parse_video_v2,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BBDown Next");
}
