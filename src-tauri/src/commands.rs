use crate::admin::run_admin_scan;
use crate::history::{self, Diff, SnapshotInfo};
use crate::reclaim::{self, Reclaimable};
use crate::model::{
    NodeDetail, NodeKind, NodeSummary, ScanEvent, ScanIssue, ScanMode, ScanNode, ScanRequest,
    ScanStatus, ScanSummary, ScanTree, Settings, ViewNode, VolumeInfo,
};
use crate::scanner::{CancelToken, ScanSignal, scan_target};
use crate::settings;
use gio::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::process::Command;
use std::sync::{Arc, Mutex, RwLock};
use sysinfo::Disks;
use tauri::{State, Window, ipc::Channel};
use uuid::Uuid;

#[derive(Debug)]
pub struct StoredScan {
    pub request: ScanRequest,
    pub tree: ScanTree,
    /// Timestamp of the history snapshot saved for this scan, if any.
    pub snapshot_ts: Option<u64>,
}

/// Lifecycle of one scan as the backend sees it. A window has at most one
/// running scan and at most one ready result once its next scan is ready.
#[derive(Debug)]
enum ScanEntry {
    Running(CancelToken),
    Cancelling(CancelToken),
    Ready(Arc<RwLock<StoredScan>>),
    Failed(String),
}

#[derive(Debug)]
struct ScanSlot {
    window: String,
    entry: ScanEntry,
}

/// How a scan thread ended.
enum ScanOutcome {
    Ready(StoredScan),
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ScanStateView {
    Running,
    Cancelling,
    Ready { summary: ScanSummary },
    Failed { message: String },
}

#[derive(Debug, Default)]
struct ScanRegistry {
    slots: HashMap<String, ScanSlot>,
    /// Newest generation the window's UI has asked for; older requests are stale.
    latest_generation: HashMap<String, u64>,
}

impl ScanRegistry {
    /// Registers a new running scan and stops (and forgets) the window's earlier
    /// running scan and failure. Its ready result stays until this scan is ready.
    /// Returns false (and does nothing) when the UI has already asked for a newer scan.
    fn begin(&mut self, window: &str, scan_id: &str, cancel: CancelToken, generation: u64) -> bool {
        let latest = self.latest_generation.entry(window.to_string()).or_insert(0);
        if generation < *latest {
            return false;
        }
        *latest = generation;
        self.slots.retain(|_, slot| {
            if slot.window != window {
                return true;
            }
            match &slot.entry {
                ScanEntry::Running(token) | ScanEntry::Cancelling(token) => {
                    token.cancel();
                    false
                }
                ScanEntry::Failed(_) => false,
                ScanEntry::Ready(_) => true,
            }
        });
        self.slots.insert(
            scan_id.to_string(),
            ScanSlot {
                window: window.to_string(),
                entry: ScanEntry::Running(cancel),
            },
        );
        true
    }

    /// Drops the window's finished results that no scan on screen refers to any more.
    fn drop_ready(&mut self, window: &str) {
        self.slots
            .retain(|_, slot| slot.window != window || !matches!(slot.entry, ScanEntry::Ready(_)));
    }

    /// Requests cancellation. Anything but a running scan is a no-op.
    fn cancel(&mut self, scan_id: &str) {
        if let Some(slot) = self.slots.get_mut(scan_id)
            && let ScanEntry::Running(token) = &slot.entry
        {
            token.cancel();
            slot.entry = ScanEntry::Cancelling(token.clone());
        }
    }

    /// Records the end of a scan. Returns false when the scan was superseded or released.
    fn finish(&mut self, scan_id: &str, outcome: ScanOutcome) -> bool {
        let Some(slot) = self.slots.get(scan_id) else {
            return false;
        };
        if !matches!(slot.entry, ScanEntry::Running(_) | ScanEntry::Cancelling(_)) {
            return false;
        }
        let window = slot.window.clone();
        match outcome {
            ScanOutcome::Ready(scan) => {
                self.slots
                    .retain(|id, slot| id == scan_id || slot.window != window);
                self.set(scan_id, ScanEntry::Ready(Arc::new(RwLock::new(scan))));
            }
            ScanOutcome::Failed(message) => {
                self.drop_ready(&window);
                self.set(scan_id, ScanEntry::Failed(message));
            }
            ScanOutcome::Cancelled => {
                self.slots.remove(scan_id);
                self.drop_ready(&window);
            }
        }
        true
    }

    fn set(&mut self, scan_id: &str, entry: ScanEntry) {
        if let Some(slot) = self.slots.get_mut(scan_id) {
            slot.entry = entry;
        }
    }

    fn release(&mut self, scan_id: &str) {
        if let Some(slot) = self.slots.remove(scan_id)
            && let ScanEntry::Running(token) | ScanEntry::Cancelling(token) = slot.entry
        {
            token.cancel();
            // The UI dropped the window's earlier result when this scan began.
            self.drop_ready(&slot.window);
        }
    }

    fn ready(&self, scan_id: &str) -> Option<Arc<RwLock<StoredScan>>> {
        match &self.slots.get(scan_id)?.entry {
            ScanEntry::Ready(scan) => Some(scan.clone()),
            _ => None,
        }
    }

    fn state(&self, scan_id: &str) -> Option<ScanStateView> {
        Some(match &self.slots.get(scan_id)?.entry {
            ScanEntry::Running(_) => ScanStateView::Running,
            ScanEntry::Cancelling(_) => ScanStateView::Cancelling,
            ScanEntry::Ready(scan) => ScanStateView::Ready {
                summary: scan.read().ok()?.tree.summary.clone(),
            },
            ScanEntry::Failed(message) => ScanStateView::Failed {
                message: message.clone(),
            },
        })
    }
}

#[derive(Clone)]
pub struct AppState {
    scans: Arc<Mutex<ScanRegistry>>,
    settings: Arc<RwLock<Settings>>,
    launch_request: Arc<Mutex<Option<ScanRequest>>>,
}

impl Default for AppState {
    fn default() -> Self {
        let loaded = settings::load_settings();
        let launch_request = parse_launch_request(&loaded);
        Self {
            scans: Arc::new(Mutex::new(ScanRegistry::default())),
            settings: Arc::new(RwLock::new(loaded)),
            launch_request: Arc::new(Mutex::new(launch_request)),
        }
    }
}

impl AppState {
    fn registry(&self) -> Result<std::sync::MutexGuard<'_, ScanRegistry>, String> {
        self.scans
            .lock()
            .map_err(|_| "Scan controller is unavailable".to_string())
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
                remote: crate::scanner::is_remote_filesystem(&file_system),
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

#[tauri::command]
pub async fn start_scan(
    state: State<'_, AppState>,
    window: Window,
    request: ScanRequest,
    on_event: Channel<ScanEvent>,
    generation: Option<u64>,
) -> Result<String, String> {
    validate_request(&request)?;
    let scan_id = Uuid::new_v4().to_string();
    let cancel = CancelToken::default();
    if !state.registry()?.begin(
        window.label(),
        &scan_id,
        cancel.clone(),
        generation.unwrap_or(u64::MAX),
    ) {
        return Err("A newer scan was requested".into());
    }
    let shared = state.inner().clone();
    let task_id = scan_id.clone();
    let admin = request.mode == ScanMode::Administrator;
    let _ = on_event.send(if admin {
        ScanEvent::authorizing(scan_id.clone())
    } else {
        ScanEvent::started(scan_id.clone())
    });

    tauri::async_runtime::spawn_blocking(move || {
        let event_scan_id = task_id.clone();
        let event_channel = &on_event;
        let mut signal = |signal| {
            let _ = event_channel.send(match signal {
                ScanSignal::Started => ScanEvent::started(event_scan_id.clone()),
                ScanSignal::Progress(progress) => {
                    ScanEvent::progress(event_scan_id.clone(), progress)
                }
                ScanSignal::Issue(issue) => ScanEvent::issue(event_scan_id.clone(), issue),
            });
        };

        // A panic must end in the Failed state, not leave the entry running forever.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if admin {
                run_admin_scan(request.clone(), &cancel, &mut signal)
                    .map_err(|error| error.to_string())
            } else {
                scan_target(&request, &cancel, &mut signal).map_err(|error| error.to_string())
            }
        }))
        .unwrap_or_else(|_| Err("The scan stopped unexpectedly".to_string()));

        let (outcome, event) = match result {
            Ok(tree) => {
                let summary = tree.summary.clone();
                let event = if summary.status == ScanStatus::Cancelled {
                    ScanEvent::cancelled(task_id.clone(), Some(summary))
                } else {
                    ScanEvent::completed(task_id.clone(), summary)
                };
                let keep_history = shared
                    .settings
                    .read()
                    .is_ok_and(|settings| settings.keep_history);
                let snapshot_ts = if keep_history {
                    std::panic::catch_unwind(|| history::record(&request, &tree))
                        .ok()
                        .flatten()
                } else {
                    None
                };
                (
                    ScanOutcome::Ready(StoredScan {
                        request,
                        tree,
                        snapshot_ts,
                    }),
                    event,
                )
            }
            Err(_) if cancel.is_cancelled() => (
                ScanOutcome::Cancelled,
                ScanEvent::cancelled(task_id.clone(), None),
            ),
            Err(error) => (
                ScanOutcome::Failed(error.clone()),
                ScanEvent::failed(task_id.clone(), error),
            ),
        };
        // Store first so the UI can load the result as soon as it hears about it.
        if let Ok(mut scans) = shared.scans.lock() {
            scans.finish(&task_id, outcome);
        }
        let _ = on_event.send(event);
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

/// Stopping a scan that already finished (or was released) is a no-op.
#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>, scan_id: String) -> Result<(), String> {
    state.registry()?.cancel(&scan_id);
    Ok(())
}

/// Frees a scan the UI no longer shows; a running scan is cancelled first.
#[tauri::command]
pub fn release_scan(state: State<'_, AppState>, scan_id: String) -> Result<(), String> {
    state.registry()?.release(&scan_id);
    Ok(())
}

#[tauri::command]
pub fn get_scan_state(
    state: State<'_, AppState>,
    scan_id: String,
) -> Result<ScanStateView, String> {
    state
        .registry()?
        .state(&scan_id)
        .ok_or_else(|| "This scan is unknown or was released".into())
}

fn ready_scan(
    state: &State<'_, AppState>,
    scan_id: &str,
) -> Result<Arc<RwLock<StoredScan>>, String> {
    state
        .registry()?
        .ready(scan_id)
        .ok_or_else(|| "Scan results are not ready".to_string())
}

fn with_scan<T>(
    state: &State<'_, AppState>,
    scan_id: &str,
    operation: impl FnOnce(&StoredScan) -> Result<T, String>,
) -> Result<T, String> {
    let scan = ready_scan(state, scan_id)?;
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

/// Upper bound on nodes in one map view sent to the webview.
const VIEW_NODE_BUDGET: usize = 5_000;
/// Children smaller than this share of the view root are invisible on the map
/// and are folded into the "Small files" segment instead of being sent.
const VIEW_MIN_SHARE: f64 = 1.0 / 30_000.0;

/// Builds the map view breadth-first so the node budget is spent on the
/// shallow, visible rings first; whatever does not fit is folded into one
/// "Small files" segment per folder or left collapsed (its size still counts).
fn build_view(tree: &ScanTree, id: u64, depth: usize, show_small_files: bool) -> ViewNode {
    let limit = if show_small_files { 240 } else { 72 };
    let root_bytes = tree.nodes[id as usize].allocated_bytes as f64;
    let mut budget = VIEW_NODE_BUDGET.saturating_sub(1);
    let mut expanded: HashMap<u64, (Vec<u64>, Vec<u64>)> = HashMap::new();
    let mut queue = std::collections::VecDeque::from([(id, depth)]);
    while let Some((node_id, depth)) = queue.pop_front() {
        let node = &tree.nodes[node_id as usize];
        if depth == 0 || node.children.is_empty() || budget == 0 {
            continue;
        }
        let mut shown = node.children.clone();
        shown.sort_by_key(|child| std::cmp::Reverse(tree.nodes[*child as usize].allocated_bytes));
        let visible = shown
            .iter()
            .take(limit)
            .take_while(|child| {
                root_bytes == 0.0
                    || tree.nodes[**child as usize].allocated_bytes as f64 / root_bytes
                        >= VIEW_MIN_SHARE
            })
            .count();
        let mut hidden = shown.split_off(visible);
        let reserve = usize::from(!hidden.is_empty());
        if shown.len() + reserve > budget {
            hidden.splice(0..0, shown.drain(budget.saturating_sub(1)..));
        }
        budget = budget.saturating_sub(shown.len() + usize::from(!hidden.is_empty()));
        queue.extend(shown.iter().map(|child| (*child, depth - 1)));
        expanded.insert(node_id, (shown, hidden));
    }
    assemble_view(tree, id, &mut expanded)
}

fn assemble_view(
    tree: &ScanTree,
    id: u64,
    expanded: &mut HashMap<u64, (Vec<u64>, Vec<u64>)>,
) -> ViewNode {
    let node = &tree.nodes[id as usize];
    let Some((shown, hidden)) = expanded.remove(&id) else {
        return ViewNode {
            summary: NodeSummary::from(node),
            children: Vec::new(),
        };
    };
    let mut children = shown
        .into_iter()
        .map(|child_id| assemble_view(tree, child_id, expanded))
        .collect::<Vec<_>>();
    if !hidden.is_empty() {
        let mut allocated = 0_u64;
        let mut apparent = 0_u64;
        let mut files = 0_u64;
        let mut directories = 0_u64;
        for child_id in &hidden {
            let child = &tree.nodes[*child_id as usize];
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
                child_count: hidden.len(),
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildrenPage {
    pub items: Vec<NodeSummary>,
    pub total: usize,
}

const MAX_CHILDREN_PAGE: usize = 1_000;

#[tauri::command]
pub fn get_children(
    state: State<'_, AppState>,
    scan_id: String,
    node_id: Option<u64>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<ChildrenPage, String> {
    with_scan(&state, &scan_id, |stored| {
        let id = node_id.unwrap_or(stored.tree.root_id);
        let node = stored
            .tree
            .nodes
            .get(id as usize)
            .ok_or_else(|| "Folder is no longer part of this scan".to_string())?;
        let nodes = &stored.tree.nodes;
        let mut children = node
            .children
            .iter()
            .filter(|child_id| (**child_id as usize) < nodes.len())
            .copied()
            .collect::<Vec<_>>();
        children.sort_by_key(|child| std::cmp::Reverse(nodes[*child as usize].allocated_bytes));
        let items = children
            .iter()
            .skip(offset.unwrap_or(0))
            .take(limit.unwrap_or(240).min(MAX_CHILDREN_PAGE))
            .map(|child| NodeSummary::from(&nodes[*child as usize]))
            .collect();
        Ok(ChildrenPage {
            items,
            total: children.len(),
        })
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

const MAX_ISSUES: usize = 5_000;

#[tauri::command]
pub fn get_scan_issues(
    state: State<'_, AppState>,
    scan_id: String,
) -> Result<Vec<ScanIssue>, String> {
    // The coverage dialog shows totals from the summary; the list itself is bounded.
    with_scan(&state, &scan_id, |stored| {
        Ok(stored
            .tree
            .issues
            .iter()
            .take(MAX_ISSUES)
            .cloned()
            .collect())
    })
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

/// Earlier snapshots of the scan's target, newest first; this scan's own is left out.
#[tauri::command]
pub fn list_snapshots(
    state: State<'_, AppState>,
    scan_id: String,
) -> Result<Vec<SnapshotInfo>, String> {
    with_scan(&state, &scan_id, |stored| {
        // Administrator scans are never recorded, so there is nothing to compare with.
        if stored.tree.summary.elevated {
            return Ok(Vec::new());
        }
        let mut earlier = history::list(&history::history_root(), &stored.request.target);
        earlier.retain(|info| Some(info.timestamp) != stored.snapshot_ts);
        Ok(earlier)
    })
}

#[tauri::command]
pub fn compare_scan(
    state: State<'_, AppState>,
    scan_id: String,
    snapshot: u64,
) -> Result<Diff, String> {
    with_scan(&state, &scan_id, |stored| {
        if stored.tree.summary.elevated {
            return Err("Administrator scans cannot be compared".to_string());
        }
        let target = &stored.request.target;
        let before = history::load(&history::history_root(), target, snapshot)
            .map_err(|error| format!("The earlier scan could not be loaded: {error}"))?;
        let after = history::build_snapshot(&stored.tree, target, 0);
        let mut diff = history::diff(&before, &after, history::MAX_DIFF_ROWS);
        history::attach_node_ids(&mut diff.rows, &stored.tree);
        Ok(diff)
    })
}

#[tauri::command]
pub fn clear_history() -> Result<(), String> {
    history::clear(&history::history_root())
        .map_err(|error| format!("Could not clear scan history: {error}"))
}

/// Read-only: names what could be reclaimed; nothing is ever deleted or run.
#[tauri::command]
pub fn reclaimable(state: State<'_, AppState>, scan_id: String) -> Result<Reclaimable, String> {
    with_scan(&state, &scan_id, |stored| {
        Ok(reclaim::find(&stored.tree, dirs::home_dir().as_deref()))
    })
}

#[tauri::command]
pub fn perform_file_action(
    state: State<'_, AppState>,
    scan_id: String,
    node_id: u64,
    action: String,
) -> Result<Option<String>, String> {
    let scan = ready_scan(&state, &scan_id)?;
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

#[tauri::command]
pub fn open_sponsor_page() -> Result<(), String> {
    open_uri("https://github.com/sponsors/RamazanBerk20")
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
    // xdg-terminal-exec only understands --dir=; every launcher also inherits the cwd.
    // Raw bytes, so folders whose names are not UTF-8 work.
    let mut directory_flag = OsString::from("--dir=");
    directory_flag.push(directory.as_os_str());
    let candidates: [(&str, Vec<&OsStr>); 5] = [
        ("xdg-terminal-exec", vec![OsStr::new(&directory_flag)]),
        (
            "kgx",
            vec![OsStr::new("--working-directory"), directory.as_os_str()],
        ),
        (
            "gnome-terminal",
            vec![OsStr::new("--working-directory"), directory.as_os_str()],
        ),
        (
            "konsole",
            vec![OsStr::new("--workdir"), directory.as_os_str()],
        ),
        (
            "xfce4-terminal",
            vec![OsStr::new("--working-directory"), directory.as_os_str()],
        ),
    ];
    for (program, arguments) in candidates {
        if command_exists(program) {
            let mut child = Command::new(program)
                .args(arguments)
                .current_dir(directory)
                .spawn()
                .map_err(|error| format!("Could not open terminal: {error}"))?;
            // Reap the launcher so it does not linger as a zombie.
            std::thread::spawn(move || child.wait());
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
    // Another link may still hold (or now solely hold) the blocks of a removed
    // hard-linked file, so ancestor totals become an estimate.
    let mut pending = vec![node_id];
    let mut shares_blocks = false;
    while let Some(id) = pending.pop() {
        let item = &stored.tree.nodes[id as usize];
        shares_blocks |= item.kind == NodeKind::File && item.hard_links > 1;
        pending.extend(&item.children);
    }
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
        if shares_blocks && !parent.flags.iter().any(|flag| flag == "approximate") {
            parent.flags.push("approximate".into());
        }
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

    fn stored(children: u64) -> StoredScan {
        let node = |id: u64, parent_id: Option<u64>, bytes: u64| ScanNode {
            id,
            parent_id,
            name: format!("n{id}"),
            display_path: format!("/n{id}"),
            uri: String::new(),
            local_path: None,
            kind: if id == 0 {
                NodeKind::Directory
            } else {
                NodeKind::File
            },
            allocated_bytes: bytes,
            apparent_bytes: bytes,
            children: if id == 0 {
                (1..=children).collect()
            } else {
                Vec::new()
            },
            file_count: 1,
            directory_count: 0,
            modified_ms: None,
            permissions: None,
            hard_links: 1,
            flags: Vec::new(),
        };
        let mut nodes = vec![node(0, None, 0)];
        nodes.extend((1..=children).map(|id| node(id, Some(0), 1_000)));
        nodes[0].allocated_bytes = children * 1_000;
        StoredScan {
            snapshot_ts: None,
            request: ScanRequest {
                target: "/".into(),
                mode: ScanMode::Standard,
                options: Default::default(),
            },
            tree: ScanTree {
                root_id: 0,
                nodes,
                issues: Vec::new(),
                summary: ScanSummary::default(),
            },
        }
    }

    fn kind(registry: &ScanRegistry, id: &str) -> Option<&'static str> {
        registry.state(id).map(|state| match state {
            ScanStateView::Running => "running",
            ScanStateView::Cancelling => "cancelling",
            ScanStateView::Ready { .. } => "ready",
            ScanStateView::Failed { .. } => "failed",
        })
    }

    #[test]
    fn registry_keeps_one_scan_per_window_and_releases_the_old_result() {
        let mut registry = ScanRegistry::default();
        let first = CancelToken::default();
        registry.begin("main", "a", first.clone(), u64::MAX);
        assert_eq!(kind(&registry, "a"), Some("running"));
        assert!(registry.finish("a", ScanOutcome::Ready(stored(1))));

        // A new scan keeps the old result until it is ready itself.
        registry.begin("main", "b", CancelToken::default(), u64::MAX);
        registry.begin("other", "c", CancelToken::default(), u64::MAX);
        assert_eq!(kind(&registry, "a"), Some("ready"));
        assert!(registry.finish("b", ScanOutcome::Ready(stored(1))));
        assert_eq!(kind(&registry, "a"), None);
        assert_eq!(kind(&registry, "b"), Some("ready"));
        assert_eq!(kind(&registry, "c"), Some("running"));

        // Stop after completion is a no-op; release frees the result.
        registry.cancel("b");
        assert_eq!(kind(&registry, "b"), Some("ready"));
        registry.release("b");
        assert!(registry.ready("b").is_none());
    }

    #[test]
    fn superseded_and_cancelled_scans_are_not_stored() {
        let mut registry = ScanRegistry::default();
        let first = CancelToken::default();
        registry.begin("main", "a", first.clone(), u64::MAX);
        registry.begin("main", "b", CancelToken::default(), u64::MAX);
        assert!(first.is_cancelled());
        assert!(!registry.finish("a", ScanOutcome::Ready(stored(1))));
        assert_eq!(kind(&registry, "a"), None);

        registry.cancel("b");
        assert_eq!(kind(&registry, "b"), Some("cancelling"));
        assert!(registry.finish("b", ScanOutcome::Cancelled));
        assert_eq!(kind(&registry, "b"), None);

        registry.begin("main", "c", CancelToken::default(), u64::MAX);
        assert!(registry.finish("c", ScanOutcome::Failed("boom".into())));
        assert_eq!(kind(&registry, "c"), Some("failed"));
        registry.begin("main", "d", CancelToken::default(), u64::MAX);
        assert_eq!(kind(&registry, "c"), None);
    }

    #[test]
    fn a_stale_start_cannot_cancel_a_newer_scan() {
        let mut registry = ScanRegistry::default();
        let newer = CancelToken::default();
        assert!(registry.begin("main", "new", newer.clone(), 2));
        assert!(!registry.begin("main", "old", CancelToken::default(), 1));
        assert!(!newer.is_cancelled());
        assert_eq!(kind(&registry, "new"), Some("running"));
        assert_eq!(kind(&registry, "old"), None);
    }

    #[test]
    fn a_shown_result_is_dropped_when_the_next_scan_ends_without_one() {
        for ending in ["cancelled", "failed", "released"] {
            let mut registry = ScanRegistry::default();
            registry.begin("main", "a", CancelToken::default(), 1);
            registry.finish("a", ScanOutcome::Ready(stored(1)));
            registry.begin("main", "b", CancelToken::default(), 2);
            match ending {
                "cancelled" => {
                    registry.finish("b", ScanOutcome::Cancelled);
                }
                "failed" => {
                    registry.finish("b", ScanOutcome::Failed("x".into()));
                }
                _ => registry.release("b"),
            }
            assert_eq!(kind(&registry, "a"), None, "{ending}");
        }
    }

    fn count(view: &ViewNode) -> usize {
        1 + view.children.iter().map(count).sum::<usize>()
    }

    #[test]
    fn view_is_bounded_and_folds_the_rest() {
        let scan = stored(20_000);
        let view = build_view(&scan.tree, 0, 8, true);
        assert!(count(&view) <= VIEW_NODE_BUDGET);
        let small = view.children.last().unwrap();
        assert_eq!(small.summary.kind, NodeKind::SmallFiles);
        let total: u64 = view
            .children
            .iter()
            .map(|child| child.summary.allocated_bytes)
            .sum();
        assert_eq!(total, scan.tree.nodes[0].allocated_bytes);
    }

    #[test]
    fn wide_and_deep_view_stays_within_budget_and_keeps_sizes() {
        // 240 folders x 240 files = 57,841 nodes at depth 2.
        let mut scan = stored(0);
        let nodes = &mut scan.tree.nodes;
        for folder in 0..240_u64 {
            let id = nodes.len() as u64;
            let mut directory = nodes[0].clone();
            directory.id = id;
            directory.parent_id = Some(0);
            directory.children = Vec::new();
            nodes.push(directory);
            nodes[0].children.push(id);
            for file in 0..240_u64 {
                let child = nodes.len() as u64;
                let mut leaf = nodes[0].clone();
                leaf.id = child;
                leaf.parent_id = Some(id);
                leaf.kind = NodeKind::File;
                leaf.children = Vec::new();
                leaf.allocated_bytes = 1_000 + folder + file;
                nodes.push(leaf);
                nodes[id as usize].children.push(child);
            }
            let size = nodes[id as usize]
                .children
                .iter()
                .map(|child| nodes[*child as usize].allocated_bytes)
                .sum();
            nodes[id as usize].allocated_bytes = size;
        }
        nodes[0].allocated_bytes = nodes[0]
            .children
            .iter()
            .map(|child| nodes[*child as usize].allocated_bytes)
            .sum();
        let view = build_view(&scan.tree, 0, 8, true);
        assert!(count(&view) <= VIEW_NODE_BUDGET, "{} nodes", count(&view));
        fn sizes_add_up(view: &ViewNode) -> bool {
            view.children.is_empty()
                || (view
                    .children
                    .iter()
                    .map(|child| child.summary.allocated_bytes)
                    .sum::<u64>()
                    == view.summary.allocated_bytes
                    && view.children.iter().all(sizes_add_up))
        }
        assert!(sizes_add_up(&view));
    }
}
