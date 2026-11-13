use crate::model::{
    IssueKind, NodeKind, ScanIssue, ScanMode, ScanNode, ScanProgress, ScanRequest, ScanStatus,
    ScanSummary, ScanTree,
};
use gio::glib;
use gio::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use url::Url;

/// Shared stop signal for one scan: the flag is polled by the walkers, the GIO
/// cancellable interrupts a blocked remote call or mount.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
    gio: gio::Cancellable,
}

impl CancelToken {
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
        self.gio.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
struct MountPolicy {
    path: PathBuf,
    remote: bool,
    removable: bool,
    duplicate_view: bool,
    snapshot_tree: bool,
    pseudo: bool,
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("the target is empty")]
    EmptyTarget,
    #[error("administrator scans accept local paths only")]
    ElevatedRemoteTarget,
    #[error("target must be an absolute path: {0}")]
    RelativePath(String),
    #[error("target does not exist or cannot be read: {0}")]
    InvalidTarget(String),
    #[error("remote scan failed: {0}")]
    Remote(String),
    #[error("scan was cancelled")]
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum ScanSignal {
    /// The privileged helper is authorized and has begun scanning.
    Started,
    Progress(ScanProgress),
    Issue(ScanIssue),
}

pub fn scan_target<F>(
    request: &ScanRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<ScanTree, ScanError>
where
    F: FnMut(ScanSignal),
{
    if request.target.trim().is_empty() {
        return Err(ScanError::EmptyTarget);
    }

    let remote = looks_like_remote_uri(&request.target);
    if remote && request.mode == ScanMode::Administrator {
        return Err(ScanError::ElevatedRemoteTarget);
    }

    if remote {
        scan_remote(request, cancel, &mut emit)
    } else {
        scan_local(request, cancel, &mut emit)
    }
}

fn looks_like_remote_uri(target: &str) -> bool {
    target.contains("://") && !target.starts_with("file://")
}

/// Adds every directory's children into its totals. Children always have a
/// higher id than their parent, so a reverse pass is a post-order roll-up.
fn roll_up(nodes: &mut [ScanNode]) {
    for index in (0..nodes.len()).rev() {
        let Some(parent) = nodes[index].parent_id else {
            continue;
        };
        let (allocated, apparent, files, directories) = {
            let node = &nodes[index];
            (
                node.allocated_bytes,
                node.apparent_bytes,
                node.file_count,
                node.directory_count,
            )
        };
        let parent = &mut nodes[parent as usize];
        parent.allocated_bytes = parent.allocated_bytes.saturating_add(allocated);
        parent.apparent_bytes = parent.apparent_bytes.saturating_add(apparent);
        parent.file_count = parent.file_count.saturating_add(files);
        parent.directory_count = parent.directory_count.saturating_add(directories);
    }
}

fn io_issue_kind(error: &std::io::Error) -> IssueKind {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => IssueKind::PermissionDenied,
        std::io::ErrorKind::NotFound => IssueKind::Changed,
        _ => IssueKind::Io,
    }
}

struct LocalContext<'a, F>
where
    F: FnMut(ScanSignal),
{
    request: &'a ScanRequest,
    cancel: &'a CancelToken,
    emit: &'a mut F,
    nodes: Vec<ScanNode>,
    issues: Vec<ScanIssue>,
    seen_hard_links: HashSet<(u64, u64)>,
    seen_directories: HashSet<(u64, u64)>,
    mounts: Vec<MountPolicy>,
    exclusions: Vec<PathBuf>,
    root_device: u64,
    started: Instant,
    last_progress: Instant,
    files: u64,
    directories: u64,
    bytes: u64,
}

/// One open level of the explicit traversal stack. Entry names are read
/// eagerly so no directory handle stays open while its children are walked.
struct LocalFrame {
    path: PathBuf,
    names: std::vec::IntoIter<OsString>,
    depth: usize,
}

impl<'a, F> LocalContext<'a, F>
where
    F: FnMut(ScanSignal),
{
    fn add_issue(&mut self, path: &Path, kind: IssueKind, message: impl Into<String>) {
        let issue = ScanIssue {
            path: path.to_string_lossy().into_owned(),
            kind,
            message: message.into(),
        };
        self.issues.push(issue.clone());
        (self.emit)(ScanSignal::Issue(issue));
    }

    fn maybe_progress(&mut self, path: &Path, force: bool) {
        if force
            || self.last_progress.elapsed() >= Duration::from_millis(120)
            || (self.files + self.directories).is_multiple_of(2048)
        {
            self.last_progress = Instant::now();
            (self.emit)(ScanSignal::Progress(ScanProgress {
                files_scanned: self.files,
                directories_scanned: self.directories,
                bytes_scanned: self.bytes,
                current_path: path.to_string_lossy().into_owned(),
                issues: self.issues.len(),
            }));
        }
    }

    fn is_excluded(&self, path: &Path) -> bool {
        self.exclusions
            .iter()
            .any(|excluded| path.starts_with(excluded))
    }

    fn blocked_mount_reason(&self, path: &Path, depth: usize) -> Option<&'static str> {
        // mountinfo lists mounts in mount order; the last entry is the visible one.
        let mount = self.mounts.iter().rev().find(|mount| mount.path == path)?;
        if mount.pseudo {
            return Some("virtual or automount filesystem skipped; scan it directly to inspect it");
        }
        if depth == 0 {
            // A chosen root is scanned on purpose (snapshots, removable and remote
            // volumes), except that an administrator scan never reads a remote mount.
            return (mount.remote && self.request.mode == ScanMode::Administrator)
                .then_some("remote filesystem excluded by the active scan policy");
        }
        if mount.snapshot_tree {
            return Some(
                "snapshot filesystem skipped during a broad scan; scan it directly to inspect it",
            );
        }
        if mount.duplicate_view {
            return Some("duplicate container or union mount view skipped");
        }
        if mount.remote && !self.request.options.include_remote_mounts {
            return Some("remote filesystem excluded by the active scan policy");
        }
        if mount.removable && !self.request.options.include_removable {
            return Some("removable filesystem excluded by the active scan policy");
        }
        None
    }

    fn walk(&mut self, root: PathBuf) -> Option<u64> {
        let (root_id, names) = self.enter(&root, None, 0)?;
        let mut stack = Vec::new();
        if let Some(names) = names {
            stack.push((
                root_id,
                LocalFrame {
                    path: root,
                    names: names.into_iter(),
                    depth: 0,
                },
            ));
        }
        while let Some((parent_id, frame)) = stack.last_mut() {
            if self.cancel.is_cancelled() {
                break;
            }
            let Some(name) = frame.names.next() else {
                stack.pop();
                continue;
            };
            let parent_id = *parent_id;
            let path = frame.path.join(name);
            let depth = frame.depth + 1;
            if let Some((id, names)) = self.enter(&path, Some(parent_id), depth) {
                self.nodes[parent_id as usize].children.push(id);
                if let Some(names) = names {
                    stack.push((
                        id,
                        LocalFrame {
                            path,
                            names: names.into_iter(),
                            depth,
                        },
                    ));
                }
            }
        }
        Some(root_id)
    }

    /// Records one entry. For a readable directory it also returns its entry names.
    fn enter(
        &mut self,
        path: &Path,
        parent_id: Option<u64>,
        depth: usize,
    ) -> Option<(u64, Option<Vec<OsString>>)> {
        if self.cancel.is_cancelled() {
            return None;
        }
        if self.is_excluded(path) {
            self.add_issue(
                path,
                IssueKind::Excluded,
                "excluded by the active scan policy",
            );
            return None;
        }
        if depth > 0 && is_well_known_snapshot_root(path) {
            self.add_issue(
                path,
                IssueKind::Excluded,
                "snapshot tree skipped during a broad scan; scan it directly to inspect it",
            );
            return None;
        }
        if let Some(reason) = self.blocked_mount_reason(path, depth) {
            self.add_issue(path, IssueKind::Excluded, reason);
            return None;
        }

        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.add_issue(path, io_issue_kind(&error), error.to_string());
                return None;
            }
        };

        if metadata.is_dir()
            && !should_cross_filesystems(self.request)
            && metadata.dev() != self.root_device
        {
            self.add_issue(
                path,
                IssueKind::FilesystemBoundary,
                "another filesystem begins here",
            );
            return None;
        }

        let file_type = metadata.file_type();
        let kind = if file_type.is_dir() {
            NodeKind::Directory
        } else if file_type.is_file() {
            NodeKind::File
        } else if file_type.is_symlink() {
            NodeKind::Symlink
        } else {
            NodeKind::Other
        };

        if kind == NodeKind::Directory {
            let identity = (metadata.dev(), metadata.ino());
            if !self.seen_directories.insert(identity) {
                self.add_issue(
                    path,
                    IssueKind::Excluded,
                    "directory already visited through another mount or bind path",
                );
                return None;
            }
        }

        let mut flags = Vec::new();
        if path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with('.'))
        {
            flags.push("hidden".to_string());
        }

        let mut allocated_bytes = metadata.blocks().saturating_mul(512);
        let mut apparent_bytes = metadata.len();
        let hard_links = metadata.nlink();
        if kind == NodeKind::File && hard_links > 1 {
            let identity = (metadata.dev(), metadata.ino());
            if !self.seen_hard_links.insert(identity) {
                // The first-seen link carries both sizes; aliases add nothing to folder totals.
                allocated_bytes = 0;
                apparent_bytes = 0;
                flags.push("hardlink_alias".into());
            }
        }

        let id = self.nodes.len() as u64;
        let display_path = path.to_string_lossy().into_owned();
        let name = display_name(path);
        let uri = Url::from_file_path(path)
            .map(|value| value.to_string())
            .unwrap_or_else(|_| format!("file://{display_path}"));
        let modified_ms = metadata.modified().ok().and_then(system_time_millis);
        let permissions = Some(mode_string(metadata.permissions().mode(), kind));

        self.nodes.push(ScanNode {
            id,
            parent_id,
            name,
            display_path,
            uri,
            local_path: Some(path.to_path_buf()),
            kind,
            allocated_bytes,
            apparent_bytes,
            children: Vec::new(),
            file_count: u64::from(kind == NodeKind::File),
            directory_count: u64::from(kind == NodeKind::Directory),
            modified_ms,
            permissions,
            hard_links,
            flags,
        });

        self.bytes = self.bytes.saturating_add(allocated_bytes);
        if kind != NodeKind::Directory {
            self.files += 1;
            self.maybe_progress(path, false);
            return Some((id, None));
        }

        self.directories += 1;
        self.maybe_progress(path, false);
        let mut names = Vec::new();
        match fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(entry) => names.push(entry.file_name()),
                        Err(error) => self.add_issue(
                            path,
                            if error.kind() == std::io::ErrorKind::PermissionDenied {
                                IssueKind::PermissionDenied
                            } else {
                                IssueKind::Io
                            },
                            error.to_string(),
                        ),
                    }
                }
            }
            Err(error) => self.add_issue(
                path,
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    IssueKind::PermissionDenied
                } else {
                    IssueKind::Io
                },
                error.to_string(),
            ),
        }
        Some((id, Some(names)))
    }
}

fn scan_local<F>(
    request: &ScanRequest,
    cancel: &CancelToken,
    emit: &mut F,
) -> Result<ScanTree, ScanError>
where
    F: FnMut(ScanSignal),
{
    let mut path = if request.target.starts_with("file://") {
        Url::parse(&request.target)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .ok_or_else(|| ScanError::InvalidTarget(request.target.clone()))?
    } else {
        PathBuf::from(&request.target)
    };

    if !path.is_absolute() {
        return Err(ScanError::RelativePath(request.target.clone()));
    }
    // A standard scan follows a symlinked target once (for example ~/Data -> /mnt/data);
    // nothing below the root is followed. The privileged helper has already required a
    // canonical target, so it never resolves links here.
    if request.mode == ScanMode::Standard {
        path = fs::canonicalize(&path)
            .map_err(|_| ScanError::InvalidTarget(request.target.clone()))?;
    }
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| ScanError::InvalidTarget(request.target.clone()))?;
    // Walked paths are built from real entry names below a canonical root, so the
    // exclusions must be canonical too for the prefix test to mean what it says.
    let exclusions = request
        .options
        .exclusions
        .iter()
        .map(PathBuf::from)
        .filter(|excluded| excluded.is_absolute())
        .map(|excluded| fs::canonicalize(&excluded).unwrap_or(excluded))
        .collect();
    let started = Instant::now();
    let mut context = LocalContext {
        request,
        cancel,
        emit,
        nodes: Vec::new(),
        issues: Vec::new(),
        seen_hard_links: HashSet::new(),
        seen_directories: HashSet::new(),
        mounts: discover_mounts(),
        exclusions,
        root_device: metadata.dev(),
        started,
        last_progress: Instant::now(),
        files: 0,
        directories: 0,
        bytes: 0,
    };

    let root_id = context.walk(path.clone()).unwrap_or(0);
    roll_up(&mut context.nodes);
    context.maybe_progress(&path, true);

    if context.nodes.is_empty() && !cancel.is_cancelled() {
        return Err(ScanError::InvalidTarget(request.target.clone()));
    }

    let cancelled = cancel.is_cancelled();
    let root = context.nodes.get(root_id as usize);
    let excluded = context
        .issues
        .iter()
        .filter(|issue| {
            matches!(
                issue.kind,
                IssueKind::Excluded | IssueKind::FilesystemBoundary
            )
        })
        .count();
    let status = if cancelled {
        ScanStatus::Cancelled
    } else if context.issues.len() > excluded {
        ScanStatus::CompleteWithIssues
    } else {
        ScanStatus::Complete
    };
    let summary = ScanSummary {
        files: context.files,
        directories: context.directories,
        allocated_bytes: root.map_or(context.bytes, |node| node.allocated_bytes),
        apparent_bytes: root.map_or(0, |node| node.apparent_bytes),
        issues: context.issues.len().saturating_sub(excluded),
        excluded,
        elapsed_ms: context.started.elapsed().as_millis() as u64,
        status,
        elevated: request.mode == ScanMode::Administrator,
    };

    Ok(ScanTree {
        root_id,
        nodes: context.nodes,
        issues: context.issues,
        summary,
    })
}

struct RemoteContext<'a, F>
where
    F: FnMut(ScanSignal),
{
    cancel: &'a CancelToken,
    emit: &'a mut F,
    nodes: Vec<ScanNode>,
    issues: Vec<ScanIssue>,
    files: u64,
    directories: u64,
    bytes: u64,
    started: Instant,
    last_progress: Instant,
}

const REMOTE_ATTRIBUTES: &str = "standard::name,standard::display-name,standard::type,standard::size,standard::allocated-size,time::modified,unix::mode,unix::nlink";

impl<'a, F> RemoteContext<'a, F>
where
    F: FnMut(ScanSignal),
{
    fn issue(&mut self, uri: &str, message: impl Into<String>) {
        let issue = ScanIssue {
            path: uri.to_string(),
            kind: IssueKind::Io,
            message: message.into(),
        };
        self.issues.push(issue.clone());
        (self.emit)(ScanSignal::Issue(issue));
    }

    fn progress(&mut self, uri: &str, force: bool) {
        if force || self.last_progress.elapsed() >= Duration::from_millis(150) {
            self.last_progress = Instant::now();
            (self.emit)(ScanSignal::Progress(ScanProgress {
                files_scanned: self.files,
                directories_scanned: self.directories,
                bytes_scanned: self.bytes,
                current_path: uri.to_string(),
                issues: self.issues.len(),
            }));
        }
    }

    fn walk(&mut self, root: gio::File) -> Option<u64> {
        let (root_id, names) = self.enter(&root, None)?;
        let mut stack = Vec::new();
        if let Some(names) = names {
            stack.push((root_id, root, names.into_iter()));
        }
        while let Some((parent_id, directory, names)) = stack.last_mut() {
            if self.cancel.is_cancelled() {
                break;
            }
            let Some(name) = names.next() else {
                stack.pop();
                continue;
            };
            let parent_id = *parent_id;
            let child = directory.child(name);
            if let Some((id, names)) = self.enter(&child, Some(parent_id)) {
                self.nodes[parent_id as usize].children.push(id);
                if let Some(names) = names {
                    stack.push((id, child, names.into_iter()));
                }
            }
        }
        Some(root_id)
    }

    fn enter(
        &mut self,
        file: &gio::File,
        parent_id: Option<u64>,
    ) -> Option<(u64, Option<Vec<PathBuf>>)> {
        if self.cancel.is_cancelled() {
            return None;
        }
        let cancellable = Some(&self.cancel.gio);
        let info = match file.query_info(
            REMOTE_ATTRIBUTES,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            cancellable,
        ) {
            Ok(info) => info,
            Err(error) => {
                self.issue(&file.uri(), error.to_string());
                return None;
            }
        };

        let kind = match info.file_type() {
            gio::FileType::Directory => NodeKind::Directory,
            gio::FileType::Regular => NodeKind::File,
            gio::FileType::SymbolicLink => NodeKind::Symlink,
            _ => NodeKind::Other,
        };
        let apparent_bytes = u64::try_from(info.size()).unwrap_or(0);
        let mut allocated_bytes = info.attribute_uint64("standard::allocated-size");
        if allocated_bytes == 0 {
            allocated_bytes = apparent_bytes;
        }
        let name = info.display_name().to_string();
        let uri = file.uri().to_string();
        let modified_ms = info.attribute_uint64("time::modified").checked_mul(1000);
        let permissions = {
            let mode = info.attribute_uint32("unix::mode");
            (mode > 0).then(|| mode_string(mode, kind))
        };
        let hard_links = u64::from(info.attribute_uint32("unix::nlink"));
        let id = self.nodes.len() as u64;
        self.nodes.push(ScanNode {
            id,
            parent_id,
            name,
            display_path: file.parse_name().to_string(),
            uri: uri.clone(),
            local_path: None,
            kind,
            allocated_bytes,
            apparent_bytes,
            children: Vec::new(),
            file_count: u64::from(kind == NodeKind::File),
            directory_count: u64::from(kind == NodeKind::Directory),
            modified_ms,
            permissions,
            hard_links,
            flags: vec!["remote".into()],
        });

        if kind != NodeKind::Directory {
            self.files += 1;
            self.bytes = self.bytes.saturating_add(allocated_bytes);
            self.progress(&uri, false);
            return Some((id, None));
        }

        self.directories += 1;
        self.progress(&uri, false);
        let mut names = Vec::new();
        match file.enumerate_children(
            "standard::name",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            cancellable,
        ) {
            Ok(enumerator) => loop {
                if self.cancel.is_cancelled() {
                    break;
                }
                match enumerator.next_file(cancellable) {
                    Ok(Some(child_info)) => names.push(child_info.name()),
                    Ok(None) => break,
                    Err(error) => {
                        self.issue(&uri, error.to_string());
                        break;
                    }
                }
            },
            Err(error) => self.issue(&uri, error.to_string()),
        }
        Some((id, Some(names)))
    }
}

/// Mounts the GVfs location enclosing `file` when it is not mounted yet. Runs a
/// private main context on this worker thread; `cancellable` aborts the wait.
/// Credentials come from the GVfs keyring; there is no interactive password prompt.
fn ensure_mounted(file: &gio::File, cancellable: &gio::Cancellable) -> Result<(), glib::Error> {
    match file.query_info(
        "standard::type",
        gio::FileQueryInfoFlags::NONE,
        Some(cancellable),
    ) {
        Err(error) if error.matches(gio::IOErrorEnum::NotMounted) => {}
        _ => return Ok(()),
    }
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            let outcome = Rc::new(RefCell::new(None));
            let slot = outcome.clone();
            file.mount_enclosing_volume(
                gio::MountMountFlags::NONE,
                Some(&gio::MountOperation::new()),
                Some(cancellable),
                move |result| *slot.borrow_mut() = Some(result),
            );
            while outcome.borrow().is_none() {
                context.iteration(true);
            }
            outcome.take().unwrap_or(Ok(()))
        })
        .map_err(|error| glib::Error::new(gio::IOErrorEnum::Failed, &error.to_string()))?
}

fn scan_remote<F>(
    request: &ScanRequest,
    cancel: &CancelToken,
    emit: &mut F,
) -> Result<ScanTree, ScanError>
where
    F: FnMut(ScanSignal),
{
    let started = Instant::now();
    let file = gio::File::for_uri(&request.target);
    if let Err(error) = ensure_mounted(&file, &cancel.gio) {
        if cancel.is_cancelled() {
            return Err(ScanError::Cancelled);
        }
        return Err(ScanError::Remote(format!("{}: {error}", request.target)));
    }
    let mut context = RemoteContext {
        cancel,
        emit,
        nodes: Vec::new(),
        issues: Vec::new(),
        files: 0,
        directories: 0,
        bytes: 0,
        started,
        last_progress: Instant::now(),
    };
    let Some(root_id) = context.walk(file) else {
        if cancel.is_cancelled() {
            return Err(ScanError::Cancelled);
        }
        let reason = context
            .issues
            .last()
            .map(|issue| issue.message.clone())
            .unwrap_or_default();
        return Err(ScanError::Remote(format!("{}: {reason}", request.target)));
    };
    roll_up(&mut context.nodes);
    context.progress(&request.target, true);

    let cancelled = cancel.is_cancelled();
    let root = &context.nodes[root_id as usize];
    let status = if cancelled {
        ScanStatus::Cancelled
    } else if context.issues.is_empty() {
        ScanStatus::Complete
    } else {
        ScanStatus::CompleteWithIssues
    };
    let summary = ScanSummary {
        files: context.files,
        directories: context.directories,
        allocated_bytes: root.allocated_bytes,
        apparent_bytes: root.apparent_bytes,
        issues: context.issues.len(),
        excluded: 0,
        elapsed_ms: context.started.elapsed().as_millis() as u64,
        status,
        elevated: false,
    };
    Ok(ScanTree {
        root_id,
        nodes: context.nodes,
        issues: context.issues,
        summary,
    })
}

fn discover_mounts() -> Vec<MountPolicy> {
    let removable_mounts = sysinfo::Disks::new_with_refreshed_list()
        .list()
        .iter()
        .filter(|disk| disk.is_removable())
        .map(|disk| disk.mount_point().to_path_buf())
        .collect::<HashSet<_>>();
    fs::read_to_string("/proc/self/mountinfo")
        .map(|contents| parse_mountinfo(&contents, &removable_mounts))
        .unwrap_or_default()
}

fn parse_mountinfo(contents: &str, removable_mounts: &HashSet<PathBuf>) -> Vec<MountPolicy> {
    contents
        .lines()
        .filter_map(|line| {
            let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
            let separator = fields.iter().position(|field| *field == "-")?;
            if separator < 5 || fields.len() <= separator + 1 {
                return None;
            }
            let path = decode_mount_path(fields[4]);
            let filesystem = fields[separator + 1];
            let mount_root = fields[3];
            let source = fields.get(separator + 2).copied().unwrap_or_default();
            Some(MountPolicy {
                removable: removable_mounts.contains(&path),
                remote: is_remote_filesystem(filesystem),
                duplicate_view: is_duplicate_view_filesystem(filesystem),
                snapshot_tree: is_snapshot_mount(&path, mount_root, source),
                pseudo: is_pseudo_filesystem(filesystem),
                path,
            })
        })
        .collect()
}

/// Kernel virtual filesystems (and autofs triggers) never hold user data and are
/// skipped by type wherever they are mounted, unless one is the scan root.
fn is_pseudo_filesystem(filesystem: &str) -> bool {
    matches!(
        filesystem,
        "proc"
            | "sysfs"
            | "devtmpfs"
            | "devpts"
            | "cgroup"
            | "cgroup2"
            | "debugfs"
            | "tracefs"
            | "securityfs"
            | "bpf"
            | "configfs"
            | "fusectl"
            | "pstore"
            | "efivarfs"
            | "binfmt_misc"
            | "mqueue"
            | "hugetlbfs"
            | "autofs"
    )
}

pub fn is_remote_filesystem(filesystem: &str) -> bool {
    let filesystem = filesystem.to_ascii_lowercase();
    matches!(
        filesystem.as_str(),
        "9p" | "afs"
            | "beegfs"
            | "ceph"
            | "cifs"
            | "davfs"
            | "davfs2"
            | "gfs2"
            | "glusterfs"
            | "lustre"
            | "nfs"
            | "nfs4"
            | "ocfs2"
            | "smb3"
            | "smbfs"
            | "sshfs"
    ) || filesystem.starts_with("fuse.sshfs")
        || filesystem.starts_with("fuse.rclone")
        || filesystem.starts_with("fuse.curlftpfs")
        || [
            "fuse.s3fs",
            "fuse.gcsfuse",
            "fuse.glusterfs",
            "fuse.ceph",
            "fuse.davfs",
            "fuse.goofys",
            "fuse.beegfs",
        ]
        .iter()
        .any(|prefix| filesystem.starts_with(prefix))
}

fn is_duplicate_view_filesystem(filesystem: &str) -> bool {
    matches!(
        filesystem.to_ascii_lowercase().as_str(),
        "aufs" | "overlay" | "unionfs" | "fuse.overlayfs"
    )
}

fn should_cross_filesystems(request: &ScanRequest) -> bool {
    request.mode == ScanMode::Administrator || request.options.cross_filesystems
}

fn is_well_known_snapshot_root(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name == ".snapshots" || name == ".zfs")
        || path == Path::new("/timeshift")
}

fn is_snapshot_mount(path: &Path, mount_root: &str, source: &str) -> bool {
    if is_well_known_snapshot_root(path) {
        return true;
    }
    [mount_root, source].iter().any(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("@snapshots")
            || value.contains("/.snapshots")
            || value.contains("/snapshots/")
            || value.ends_with("/snapshot")
    })
}

fn decode_mount_path(value: &str) -> PathBuf {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && index + 3 < bytes.len()
            && bytes[index + 1..=index + 3]
                .iter()
                .all(|byte| matches!(byte, b'0'..=b'7'))
        {
            let value = (bytes[index + 1] - b'0') * 64
                + (bytes[index + 2] - b'0') * 8
                + (bytes[index + 3] - b'0');
            decoded.push(value);
            index += 4;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    PathBuf::from(OsString::from_vec(decoded))
}

fn display_name(path: &Path) -> String {
    if path == Path::new("/") {
        return "/".into();
    }
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn system_time_millis(value: SystemTime) -> Option<u64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn mode_string(mode: u32, kind: NodeKind) -> String {
    let mut output = String::with_capacity(10);
    output.push(match kind {
        NodeKind::Directory => 'd',
        NodeKind::Symlink => 'l',
        _ => '-',
    });
    for (read, write, execute) in [
        (0o400, 0o200, 0o100),
        (0o040, 0o020, 0o010),
        (0o004, 0o002, 0o001),
    ] {
        output.push(if mode & read != 0 { 'r' } else { '-' });
        output.push(if mode & write != 0 { 'w' } else { '-' });
        output.push(if mode & execute != 0 { 'x' } else { '-' });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ScanOptions;
    use std::io::Write;

    fn temp_fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("liscan-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("alpha.txt"), vec![1_u8; 4096]).unwrap();
        fs::write(root.join("nested/beta.txt"), vec![2_u8; 8192]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&root, root.join("nested/loop")).unwrap();
        root
    }

    #[test]
    fn scans_tree_without_following_symlink() {
        let root = temp_fixture();
        let request = ScanRequest {
            target: root.to_string_lossy().into_owned(),
            mode: ScanMode::Standard,
            options: ScanOptions::default(),
        };
        let tree = scan_target(&request, &CancelToken::default(), |_| {}).unwrap();
        assert_eq!(tree.summary.files, 3);
        assert_eq!(tree.summary.directories, 2);
        assert!(tree.nodes.iter().any(|node| node.kind == NodeKind::Symlink));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_excluded_folder() {
        let root = temp_fixture();
        let mut options = ScanOptions::default();
        options
            .exclusions
            .push(root.join("nested").to_string_lossy().into_owned());
        let request = ScanRequest {
            target: root.to_string_lossy().into_owned(),
            mode: ScanMode::Standard,
            options,
        };
        let tree = scan_target(&request, &CancelToken::default(), |_| {}).unwrap();
        assert!(tree.summary.excluded >= 1);
        assert_eq!(tree.summary.directories, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_is_observed() {
        let root = temp_fixture();
        let cancel = CancelToken::default();
        cancel.cancel();
        let request = ScanRequest {
            target: root.to_string_lossy().into_owned(),
            mode: ScanMode::Standard,
            options: ScanOptions::default(),
        };
        let tree = scan_target(&request, &cancel, |_| {}).unwrap();
        assert_eq!(tree.summary.status, ScanStatus::Cancelled);
        assert!(tree.nodes.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mode_string_is_readable() {
        let mut path = std::env::temp_dir().join(format!("liscan-mode-{}", uuid::Uuid::new_v4()));
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(b"x").unwrap();
        let metadata = fs::metadata(&path).unwrap();
        assert_eq!(
            mode_string(metadata.permissions().mode(), NodeKind::File).len(),
            10
        );
        fs::remove_file(&path).unwrap();
        path.clear();
    }

    #[test]
    fn decodes_mountinfo_paths() {
        assert_eq!(
            decode_mount_path("/media/My\\040Drive"),
            PathBuf::from("/media/My Drive")
        );
        assert_eq!(
            decode_mount_path("/media/Backslash\\134Name"),
            PathBuf::from("/media/Backslash\\Name")
        );
    }

    #[test]
    fn recognizes_remote_filesystems() {
        assert!(is_remote_filesystem("nfs4"));
        assert!(is_remote_filesystem("fuse.sshfs"));
        for name in ["fuse.s3fs", "fuse.gcsfuse", "glusterfs", "lustre", "ceph", "fuse.rclone", "9p", "afs", "beegfs", "gfs2"] {
            assert!(is_remote_filesystem(name), "{name}");
        }
        assert!(!is_remote_filesystem("ext4"));
        assert!(!is_remote_filesystem("fuse.portal"));
    }

    #[test]
    fn recognizes_duplicate_mount_views() {
        assert!(is_duplicate_view_filesystem("overlay"));
        assert!(is_duplicate_view_filesystem("fuse.overlayfs"));
        assert!(!is_duplicate_view_filesystem("btrfs"));
    }

    #[test]
    fn administrator_scans_cross_local_filesystem_boundaries() {
        let mut request = ScanRequest {
            target: "/".into(),
            mode: ScanMode::Standard,
            options: ScanOptions::default(),
        };
        assert!(!should_cross_filesystems(&request));

        request.options.cross_filesystems = true;
        assert!(should_cross_filesystems(&request));

        request.options.cross_filesystems = false;
        request.mode = ScanMode::Administrator;
        assert!(should_cross_filesystems(&request));
    }

    #[test]
    fn recognizes_snapshot_trees() {
        assert!(is_well_known_snapshot_root(Path::new("/.snapshots")));
        assert!(is_well_known_snapshot_root(Path::new(
            "/home/alex/.snapshots"
        )));
        assert!(is_snapshot_mount(
            Path::new("/backup/history"),
            "/@snapshots",
            "/dev/sda2"
        ));
        assert!(!is_snapshot_mount(
            Path::new("/home"),
            "/@home",
            "/dev/sda2"
        ));
    }

    fn standard_request(root: &Path) -> ScanRequest {
        ScanRequest {
            target: root.to_string_lossy().into_owned(),
            mode: ScanMode::Standard,
            options: ScanOptions::default(),
        }
    }

    #[test]
    fn non_utf8_names_keep_their_raw_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let root = std::env::temp_dir().join(format!("liscan-bytes-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let name = OsString::from_vec(b"caf\xe9-\xff.txt".to_vec());
        fs::write(root.join(&name), b"x").unwrap();

        let tree = scan_target(&standard_request(&root), &CancelToken::default(), |_| {}).unwrap();
        let node = tree
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::File)
            .unwrap();
        let local = node.local_path.as_ref().unwrap();
        assert_eq!(local.file_name().unwrap().as_bytes(), name.as_bytes());
        assert!(local.exists());
        assert!(node.name.contains('\u{fffd}'));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn very_deep_tree_scans_on_a_small_stack() {
        let root = std::env::temp_dir().join(format!("liscan-deep-{}", uuid::Uuid::new_v4()));
        let mut deepest = root.clone();
        for _ in 0..1500 {
            deepest.push("a");
        }
        fs::create_dir_all(&deepest).unwrap();
        fs::write(deepest.join("leaf"), b"x").unwrap();

        let request = standard_request(&root);
        // 128 KiB would overflow within a few hundred levels of recursion.
        let tree = std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(move || scan_target(&request, &CancelToken::default(), |_| {}).unwrap())
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(tree.summary.directories, 1501);
        assert_eq!(tree.summary.files, 1);
        assert_eq!(tree.nodes[tree.root_id as usize].file_count, 1);
        assert_eq!(tree.summary.status, ScanStatus::Complete);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn symlinked_target_is_followed_once() {
        let root = temp_fixture();
        let link = std::env::temp_dir().join(format!("liscan-link-{}", uuid::Uuid::new_v4()));
        std::os::unix::fs::symlink(&root, &link).unwrap();
        let tree = scan_target(&standard_request(&link), &CancelToken::default(), |_| {}).unwrap();
        assert_eq!(tree.summary.directories, 2);
        assert!(tree.nodes.iter().any(|node| node.kind == NodeKind::Symlink));
        fs::remove_file(link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hard_link_aliases_count_once_in_both_sizes() {
        let root = std::env::temp_dir().join(format!("liscan-links-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("one"), vec![1_u8; 10_000]).unwrap();
        fs::hard_link(root.join("one"), root.join("two")).unwrap();
        let tree = scan_target(&standard_request(&root), &CancelToken::default(), |_| {}).unwrap();
        let root_node = &tree.nodes[tree.root_id as usize];
        let directory_apparent = fs::metadata(&root).unwrap().len();
        assert_eq!(root_node.apparent_bytes, directory_apparent + 10_000);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mount_policy_uses_type_and_the_topmost_entry() {
        let mountinfo = "\
22 1 0:21 / /proc rw,nosuid shared:5 - proc proc rw
40 1 0:35 / /net/share rw shared:20 - autofs systemd-1 rw,fd=50
90 40 0:60 / /net/share rw shared:40 - nfs4 server:/export rw
95 1 0:70 / /var/lib/machines/box/proc rw - proc proc rw
96 1 0:71 / /home rw - btrfs /dev/sda2 rw,subvol=/@home
97 1 0:72 / /mnt/auto rw - autofs systemd-1 rw
";
        let mounts = parse_mountinfo(mountinfo, &HashSet::new());
        let topmost = |path: &str| {
            mounts
                .iter()
                .rev()
                .find(|mount| mount.path == Path::new(path))
                .unwrap()
        };
        assert!(topmost("/proc").pseudo);
        assert!(topmost("/var/lib/machines/box/proc").pseudo);
        assert!(topmost("/net/share").remote);
        assert!(!topmost("/net/share").pseudo);
        assert!(topmost("/mnt/auto").pseudo);
        assert!(!topmost("/home").pseudo && !topmost("/home").remote);
    }
}
