use crate::admin::run_admin_scan;
use crate::model::{
    NodeDetail, NodeKind, NodeSummary, ScanEvent, ScanIssue, ScanMode, ScanNode, ScanRequest,
    ScanStatus, ScanTree, Settings, ViewNode, VolumeInfo,
};
use crate::scanner::{ScanSignal, scan_target};
use crate::settings;
use gio::prelude::*;
use std::collections::HashMap;
use std::process::Command;
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, Ordering},
};
use sysinfo::Disks;
use tauri::{State, ipc::Channel};
use uuid::Uuid;

#[derive(Debug)]
pub struct StoredScan {
    pub request: ScanRequest,
    pub tree: ScanTree,
}

#[derive(Clone)]
pub struct AppState {
    scans: Arc<RwLock<HashMap<String, Arc<RwLock<StoredScan>>>>>,
    active: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    settings: Arc<RwLock<Settings>>,
    launch_request: Arc<Mutex<Option<ScanRequest>>>,
}

impl Default for AppState {
    fn default() -> Self {
        let loaded = settings::load_settings();
        let launch_request = parse_launch_request(&loaded);
        Self {
            scans: Arc::new(RwLock::new(HashMap::new())),
            active: Arc::new(Mutex::new(HashMap::new())),
            settings: Arc::new(RwLock::new(loaded)),
            launch_request: Arc::new(Mutex::new(launch_request)),
        }
    }
}

fn parse_launch_request(settings: &Settings) -> Option<ScanRequest> {
    let mut mode = ScanMode::Standard;
    let mut target = None;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--admin" => mode = ScanMode::Administrator,
            "--home" => target = dirs::home_dir().map(|path| path.to_string_lossy().into_owned()),
            "--new-window" => {}
            value if !value.starts_with('-') => target = Some(value.to_string()),
            _ => {}
        }
    }
    target.map(|target| ScanRequest {
        target,
        mode,
        options: settings.scan_options.clone(),
    })
}

#[tauri::command]
pub fn get_launch_request(state: State<'_, AppState>) -> Option<ScanRequest> {
    state
        .launch_request
        .lock()
        .ok()
        .and_then(|mut request| request.take())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    state
        .settings
        .read()
        .map(|settings| settings.clone())
        .map_err(|_| "Settings are currently unavailable".into())
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    if settings.contrast > 100 || settings.scan_options.exclusions.len() > 256 {
        return Err("Settings are outside the supported range".into());
    }
    settings::save_settings(&settings)
        .map_err(|error| format!("Could not save settings: {error}"))?;
    *state
        .settings
        .write()
        .map_err(|_| "Settings are currently unavailable".to_string())? = settings;
    Ok(())
}

#[tauri::command]
pub fn list_volumes() -> Vec<VolumeInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut volumes = disks
        .list()
        .iter()
        .map(|disk| {
            let mount = disk.mount_point().to_string_lossy().into_owned();
            let file_system = disk.file_system().to_string_lossy().into_owned();
            VolumeInfo {
                name: {
                    let value = disk.name().to_string_lossy();
                    if value.is_empty() {
                        mount.clone()
                    } else {
                        value.into_owned()
                    }
                },
                mount_point: mount,
                remote: is_remote_filesystem(&file_system),
                file_system,
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
                removable: disk.is_removable(),
            }
        })
        .collect::<Vec<_>>();
    volumes.sort_by(|left, right| left.mount_point.cmp(&right.mount_point));
    volumes
}

#[tauri::command]
pub fn get_home_path() -> Result<String, String> {
    dirs::home_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .ok_or_else(|| "The home folder could not be determined".into())
}

fn normalize_locale(value: &str) -> Option<String> {
    let locale = value
        .trim()
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('_', "-");
    let base = locale
        .split('-')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if locale.is_empty() || matches!(base.as_str(), "c" | "posix") {
        None
    } else {
        Some(locale)
    }
}

fn locale_candidates(
    language: Option<&str>,
    lc_all: Option<&str>,
    lc_messages: Option<&str>,
    lang: Option<&str>,
) -> Vec<String> {
    let mut locales = Vec::new();
    let mut add = |value: &str| {
        if let Some(locale) = normalize_locale(value)
            && !locales.contains(&locale)
        {
            locales.push(locale);
        }
    };
    if let Some(language) = language {
        for value in language.split(':') {
            add(value);
        }
    }
    for value in [lc_all, lc_messages, lang].into_iter().flatten() {
        add(value);
    }
    locales
}

#[tauri::command]
pub fn get_system_languages() -> Vec<String> {
    locale_candidates(
        std::env::var("LANGUAGE").ok().as_deref(),
        std::env::var("LC_ALL").ok().as_deref(),
        std::env::var("LC_MESSAGES").ok().as_deref(),
        std::env::var("LANG").ok().as_deref(),
    )
}

fn is_remote_filesystem(file_system: &str) -> bool {
    matches!(
        file_system.to_ascii_lowercase().as_str(),
        "nfs" | "nfs4" | "cifs" | "smbfs" | "sshfs" | "9p" | "afs"
    ) || file_system.to_ascii_lowercase().starts_with("fuse.sshfs")
}

#[tauri::command]
pub async fn start_scan(
    state: State<'_, AppState>,
    request: ScanRequest,
    on_event: Channel<ScanEvent>,
) -> Result<String, String> {
    validate_request(&request)?;
    let scan_id = Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .active
        .lock()
        .map_err(|_| "Scan controller is unavailable".to_string())?
        .insert(scan_id.clone(), cancel.clone());
    let shared = state.inner().clone();
    let task_id = scan_id.clone();
    let _ = on_event.send(ScanEvent::started(scan_id.clone()));

    tauri::async_runtime::spawn_blocking(move || {
        let event_scan_id = task_id.clone();
        let event_channel = &on_event;
        let mut signal = |signal| match signal {
            ScanSignal::Progress(progress) => {
                let _ = event_channel.send(ScanEvent::progress(event_scan_id.clone(), progress));
            }
            ScanSignal::Issue(issue) => {
                let _ = event_channel.send(ScanEvent::issue(event_scan_id.clone(), issue));
            }
        };

        let result = if request.mode == ScanMode::Administrator {
            run_admin_scan(request.clone(), cancel.clone(), &mut signal)
        } else {
            scan_target(&request, cancel.clone(), &mut signal).map_err(|error| error.to_string())
        };

        match result {
            Ok(tree) => {
                let summary = tree.summary.clone();
                if let Ok(mut scans) = shared.scans.write() {
                    scans.insert(
                        task_id.clone(),
                        Arc::new(RwLock::new(StoredScan {
                            request: request.clone(),
                            tree,
                        })),
                    );
                }
                if summary.status == ScanStatus::Cancelled {
                    let _ = on_event.send(ScanEvent::cancelled(task_id.clone(), Some(summary)));
                } else {
                    let _ = on_event.send(ScanEvent::completed(task_id.clone(), summary));
                }
            }
            Err(error) => {
                if cancel.load(Ordering::Relaxed) {
                    let _ = on_event.send(ScanEvent::cancelled(task_id.clone(), None));
                } else {
                    let _ = on_event.send(ScanEvent::failed(task_id.clone(), error));
                }
            }
        }
        if let Ok(mut active) = shared.active.lock() {
            active.remove(&task_id);
        }
    });

    Ok(scan_id)
}

fn validate_request(request: &ScanRequest) -> Result<(), String> {
    if request.target.trim().is_empty() {
        return Err("Choose a folder or remote location to scan".into());
    }
    if request.options.exclusions.len() > 256
        || request
            .options
            .exclusions
            .iter()
            .any(|path| path.len() > 4096)
    {
        return Err("The exclusion list exceeds the supported bounds".into());
    }
    if request.mode == ScanMode::Administrator && request.target.contains("://") {
        return Err("Administrator scans accept local folders only".into());
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>, scan_id: String) -> Result<(), String> {
    let active = state
        .active
        .lock()
        .map_err(|_| "Scan controller is unavailable".to_string())?;
    let cancel = active
        .get(&scan_id)
        .ok_or_else(|| "This scan is no longer running".to_string())?;
    cancel.store(true, Ordering::Relaxed);
    Ok(())
}

fn with_scan<T>(
    state: &State<'_, AppState>,
    scan_id: &str,
    operation: impl FnOnce(&StoredScan) -> Result<T, String>,
) -> Result<T, String> {
    let scans = state
        .scans
        .read()
        .map_err(|_| "Scan results are unavailable".to_string())?;
    let scan = scans
        .get(scan_id)
        .ok_or_else(|| "Scan results are not ready".to_string())?
        .clone();
    drop(scans);
    let scan = scan
        .read()
        .map_err(|_| "Scan results are unavailable".to_string())?;
    operation(&scan)
}

#[tauri::command]
pub fn get_view(
    state: State<'_, AppState>,
    scan_id: String,
    node_id: Option<u64>,
    depth: usize,
) -> Result<ViewNode, String> {
    with_scan(&state, &scan_id, |stored| {
        let id = node_id.unwrap_or(stored.tree.root_id);
        if id as usize >= stored.tree.nodes.len() {
            return Err("Folder is no longer part of this scan".into());
        }
        Ok(build_view(
            &stored.tree,
            id,
            depth.min(8),
            stored.request.options.show_small_files,
        ))
    })
}

fn build_view(tree: &ScanTree, id: u64, depth: usize, show_small_files: bool) -> ViewNode {
    let node = &tree.nodes[id as usize];
    let mut child_ids = node.children.clone();
    child_ids
        .sort_by_key(|child_id| std::cmp::Reverse(tree.nodes[*child_id as usize].allocated_bytes));
    let limit = if show_small_files { 240 } else { 72 };
    let hidden = if child_ids.len() > limit {
        child_ids.split_off(limit)
    } else {
        Vec::new()
    };
    let mut children = if depth == 0 {
        Vec::new()
    } else {
        child_ids
            .into_iter()
            .map(|child_id| build_view(tree, child_id, depth - 1, show_small_files))
            .collect()
    };
    if depth > 0 && !hidden.is_empty() {
        let mut allocated = 0_u64;
        let mut apparent = 0_u64;
        let mut files = 0_u64;
        let mut directories = 0_u64;
        for child_id in hidden {
            let child = &tree.nodes[child_id as usize];
            allocated = allocated.saturating_add(child.allocated_bytes);
            apparent = apparent.saturating_add(child.apparent_bytes);
            files = files.saturating_add(child.file_count);
            directories = directories.saturating_add(child.directory_count);
        }
        children.push(ViewNode {
            summary: NodeSummary {
                id: u64::MAX - id,
                parent_id: Some(id),
                name: "Small files".into(),
                display_path: node.display_path.clone(),
                kind: NodeKind::SmallFiles,
                allocated_bytes: allocated,
                apparent_bytes: apparent,
                child_count: 0,
                file_count: files,
                directory_count: directories,
                flags: vec!["grouped".into()],
            },
            children: Vec::new(),
        });
    }
    ViewNode {
        summary: NodeSummary::from(node),
        children,
    }
}

#[tauri::command]
pub fn get_children(
    state: State<'_, AppState>,
    scan_id: String,
    node_id: Option<u64>,
) -> Result<Vec<NodeSummary>, String> {
    with_scan(&state, &scan_id, |stored| {
        let id = node_id.unwrap_or(stored.tree.root_id);
        let node = stored
            .tree
            .nodes
            .get(id as usize)
            .ok_or_else(|| "Folder is no longer part of this scan".to_string())?;
        let mut children = node
            .children
            .iter()
            .filter_map(|child_id| stored.tree.nodes.get(*child_id as usize))
            .map(NodeSummary::from)
            .collect::<Vec<_>>();
        children.sort_by_key(|child| std::cmp::Reverse(child.allocated_bytes));
        Ok(children)
    })
}

#[tauri::command]
pub fn get_node(
    state: State<'_, AppState>,
    scan_id: String,
    node_id: u64,
) -> Result<NodeDetail, String> {
    with_scan(&state, &scan_id, |stored| {
        let node = stored
            .tree
            .nodes
            .get(node_id as usize)
            .ok_or_else(|| "Item is no longer part of this scan".to_string())?;
        Ok(NodeDetail {
            summary: NodeSummary::from(node),
            uri: node.uri.clone(),
            modified_ms: node.modified_ms,
            permissions: node.permissions.clone(),
            hard_links: node.hard_links,
        })
    })
}

#[tauri::command]
pub fn get_scan_issues(
    state: State<'_, AppState>,
    scan_id: String,
) -> Result<Vec<ScanIssue>, String> {
    with_scan(&state, &scan_id, |stored| Ok(stored.tree.issues.clone()))
}

#[tauri::command]
pub fn node_scan_target(
    state: State<'_, AppState>,
    scan_id: String,
    node_id: u64,
) -> Result<String, String> {
    with_scan(&state, &scan_id, |stored| {
        stored
            .tree
            .nodes
            .get(node_id as usize)
            .map(|node| node.display_path.clone())
            .ok_or_else(|| "Item is no longer part of this scan".into())
    })
}

#[tauri::command]
pub fn perform_file_action(
    state: State<'_, AppState>,
    scan_id: String,
    node_id: u64,
    action: String,
) -> Result<Option<String>, String> {
    let scan = {
        let scans = state
            .scans
            .read()
            .map_err(|_| "Scan results are unavailable".to_string())?;
        scans
            .get(&scan_id)
            .ok_or_else(|| "Scan results are unavailable".to_string())?
            .clone()
    };
    let node = {
        let stored = scan
            .read()
            .map_err(|_| "Scan results are unavailable".to_string())?;
        stored
            .tree
            .nodes
            .get(node_id as usize)
            .cloned()
            .ok_or_else(|| "Item is no longer part of this scan".to_string())?
    };

    match action.as_str() {
        "copy_path" => return Ok(Some(node.display_path)),
        "open" => open_uri(&node.uri)?,
        "reveal" => reveal_node(&node)?,
        "terminal" => open_terminal(&node)?,
        "trash" => {
            if node.parent_id.is_none() {
                return Err("The root of a scan cannot be moved to Trash".into());
            }
            trash_node(&node)?;
            detach_node(&scan, node_id)?;
        }
        "delete" => {
            if node.parent_id.is_none() {
                return Err("The root of a scan cannot be permanently deleted".into());
            }
            delete_node(&node)?;
            detach_node(&scan, node_id)?;
        }
        _ => return Err("Unknown file action".into()),
    }
    Ok(None)
}

fn open_uri(uri: &str) -> Result<(), String> {
    gio::AppInfo::launch_default_for_uri(uri, gio::AppLaunchContext::NONE)
        .map_err(|error| format!("Could not open item: {error}"))
}

fn reveal_node(node: &ScanNode) -> Result<(), String> {
    let Some(path) = node.local_path.as_ref() else {
        return open_uri(&node.uri);
    };
    let destination = if node.kind == NodeKind::Directory {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let uri = url::Url::from_file_path(destination)
        .map_err(|_| "Could not create a file URI".to_string())?;
    open_uri(uri.as_str())
}

fn open_terminal(node: &ScanNode) -> Result<(), String> {
    let path = node
        .local_path
        .as_ref()
        .ok_or_else(|| "Terminals can only be opened for local folders".to_string())?;
    let directory = if node.kind == NodeKind::Directory {
        path.as_path()
    } else {
        path.parent()
            .ok_or_else(|| "This item has no parent folder".to_string())?
    };
    let candidates: [(&str, &[&str]); 5] = [
        ("xdg-terminal-exec", &["--working-directory"]),
        ("kgx", &["--working-directory"]),
        ("gnome-terminal", &["--working-directory"]),
        ("konsole", &["--workdir"]),
        ("xfce4-terminal", &["--working-directory"]),
    ];
    for (program, arguments) in candidates {
        if command_exists(program) {
            Command::new(program)
                .args(arguments)
                .arg(directory)
                .spawn()
                .map_err(|error| format!("Could not open terminal: {error}"))?;
            return Ok(());
        }
    }
    Err("No supported terminal launcher was found".into())
}

fn command_exists(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|path| path.join(program).is_file()))
}

fn trash_node(node: &ScanNode) -> Result<(), String> {
    if let Some(path) = &node.local_path {
        trash::delete(path).map_err(|error| format!("Could not move item to Trash: {error}"))
    } else {
        gio::File::for_uri(&node.uri)
            .trash(gio::Cancellable::NONE)
            .map_err(|error| format!("The remote location could not trash this item: {error}"))
    }
}

fn delete_node(node: &ScanNode) -> Result<(), String> {
    let Some(path) = &node.local_path else {
        return Err("Permanent deletion is available for local items only".into());
    };
    if node.kind == NodeKind::Directory {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
    .map_err(|error| format!("Could not permanently delete item: {error}"))
}

fn detach_node(scan: &Arc<RwLock<StoredScan>>, node_id: u64) -> Result<(), String> {
    let mut stored = scan
        .write()
        .map_err(|_| "Scan results are unavailable".to_string())?;
    let node = stored
        .tree
        .nodes
        .get(node_id as usize)
        .cloned()
        .ok_or_else(|| "Item is no longer part of this scan".to_string())?;
    let mut parent_id = node.parent_id;
    if let Some(parent) = parent_id {
        stored.tree.nodes[parent as usize]
            .children
            .retain(|child| *child != node_id);
    }
    while let Some(id) = parent_id {
        let parent = &mut stored.tree.nodes[id as usize];
        parent.allocated_bytes = parent.allocated_bytes.saturating_sub(node.allocated_bytes);
        parent.apparent_bytes = parent.apparent_bytes.saturating_sub(node.apparent_bytes);
        parent.file_count = parent.file_count.saturating_sub(node.file_count);
        parent.directory_count = parent.directory_count.saturating_sub(node.directory_count);
        parent_id = parent.parent_id;
    }
    let removed = &mut stored.tree.nodes[node_id as usize];
    removed.allocated_bytes = 0;
    removed.apparent_bytes = 0;
    removed.file_count = 0;
    removed.directory_count = 0;
    removed.children.clear();
    removed.flags.push("removed".into());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_filesystems_are_classified() {
        assert!(is_remote_filesystem("nfs4"));
        assert!(is_remote_filesystem("fuse.sshfs"));
        assert!(!is_remote_filesystem("ext4"));
    }

    #[test]
    fn neutral_locale_overrides_fall_back_to_the_configured_language() {
        assert_eq!(
            locale_candidates(None, Some("C.UTF-8"), None, Some("tr_TR.UTF-8")),
            vec!["tr-TR"]
        );
    }

    #[test]
    fn language_preferences_are_normalized_and_deduplicated() {
        assert_eq!(
            locale_candidates(
                Some("es_ES:fr_FR.UTF-8"),
                Some("es_ES.UTF-8"),
                None,
                Some("de_DE.UTF-8")
            ),
            vec!["es-ES", "fr-FR", "de-DE"]
        );
    }
}
