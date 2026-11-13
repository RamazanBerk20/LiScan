mod admin;
mod cli;
mod commands;
mod history;
mod model;
pub mod protocol;
mod reclaim;
pub mod scanner;
mod settings;

pub use model::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Headless mode never creates a window; it prints one JSON document and exits.
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match cli::parse_args(&args) {
        Ok(cli::Cli::Gui) => {}
        Ok(cli::Cli::Help) => {
            print!("{}", cli::HELP);
            return;
        }
        Ok(cli::Cli::Json(json)) => {
            std::process::exit(cli::run_json(json, settings::load_settings().scan_options))
        }
        Err(message) => {
            eprintln!("liscan: {message}");
            println!("{}", cli::error_document(&message));
            std::process::exit(1);
        }
    }
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
            commands::get_system_languages,
            commands::start_scan,
            commands::cancel_scan,
            commands::release_scan,
            commands::get_scan_state,
            commands::get_view,
            commands::get_children,
            commands::get_node,
            commands::get_scan_issues,
            commands::node_scan_target,
            commands::perform_file_action,
            commands::list_snapshots,
            commands::compare_scan,
            commands::reclaimable,
            commands::clear_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LiScan");
}
