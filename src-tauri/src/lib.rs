mod admin;
mod commands;
mod model;
pub mod protocol;
pub mod scanner;
mod settings;

pub use model::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_launch_request,
            commands::get_settings,
            commands::update_settings,
            commands::list_volumes,
            commands::get_home_path,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_view,
            commands::get_children,
            commands::get_node,
            commands::get_scan_issues,
            commands::node_scan_target,
            commands::perform_file_action,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LiScan");
}
