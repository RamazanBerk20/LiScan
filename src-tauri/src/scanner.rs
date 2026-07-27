use crate::model::{
    IssueKind, NodeKind, ScanIssue, ScanMode, ScanNode, ScanProgress, ScanRequest, ScanStatus,
    ScanSummary, ScanTree,
};
use gio::prelude::*;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone)]
struct MountPolicy {
    path: PathBuf,
    remote: bool,
    removable: bool,
    duplicate_view: bool,
    snapshot_tree: bool,
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
    Progress(ScanProgress),
    Issue(ScanIssue),
}

pub fn scan_target<F>(
    request: &ScanRequest,
    cancel: Arc<AtomicBool>,
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

struct LocalContext<'a, F>
where
    F: FnMut(ScanSignal),
{
    request: &'a ScanRequest,
    cancel: Arc<AtomicBool>,
    emit: &'a mut F,
    nodes: Vec<ScanNode>,
    issues: Vec<ScanIssue>,
    seen_hard_links: HashSet<(u64, u64)>,
    seen_directories: HashSet<(u64, u64)>,
    mounts: Vec<MountPolicy>,
    root_device: u64,
    started: Instant,
    last_progress: Instant,
    files: u64,
    directories: u64,
    bytes: u64,
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
        self.request
            .options
            .exclusions
            .iter()
            .map(Path::new)
            .any(|excluded| excluded.is_absolute() && path.starts_with(excluded))
    }

    fn blocked_mount_reason(&self, path: &Path) -> Option<&'static str> {
        let mount = self.mounts.iter().find(|mount| mount.path == path)?;
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

    fn visit(&mut self, path: PathBuf, parent_id: Option<u64>, depth: usize) -> Option<u64> {
        if self.cancel.load(Ordering::Relaxed) {
            return None;
        }
        if depth > 1024 {
            self.add_issue(
                &path,
                IssueKind::Unsupported,
                "directory depth exceeds the safe traversal limit",
            );
            return None;
        }
        if self.is_excluded(&path) {
            self.add_issue(
                &path,
                IssueKind::Excluded,
                "excluded by the active scan policy",
            );
            return None;
        }
        if depth > 0 && is_well_known_snapshot_root(&path) {
            self.add_issue(
                &path,
                IssueKind::Excluded,
                "snapshot tree skipped during a broad scan; scan it directly to inspect it",
            );
            return None;
        }
        if depth > 0
            && let Some(reason) = self.blocked_mount_reason(&path)
        {
            self.add_issue(&path, IssueKind::Excluded, reason);
            return None;
        }

        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                let kind = if error.kind() == std::io::ErrorKind::PermissionDenied {
                    IssueKind::PermissionDenied
                } else if error.kind() == std::io::ErrorKind::NotFound {
                    IssueKind::Changed
                } else {
                    IssueKind::Io
                };
                self.add_issue(&path, kind, error.to_string());
                return None;
            }
        };

        if metadata.is_dir()
            && !should_cross_filesystems(self.request)
            && metadata.dev() != self.root_device
        {
            self.add_issue(
                &path,
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
                    &path,
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
        let apparent_bytes = metadata.len();
        let hard_links = metadata.nlink();
        if kind == NodeKind::File && hard_links > 1 {
            let identity = (metadata.dev(), metadata.ino());
            if !self.seen_hard_links.insert(identity) {
                allocated_bytes = 0;
                flags.push("hardlink_alias".into());
            }
        }

        let id = self.nodes.len() as u64;
        let display_path = path.to_string_lossy().into_owned();
        let name = display_name(&path);
        let uri = Url::from_file_path(&path)
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
            local_path: Some(path.clone()),
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

        match kind {
            NodeKind::Directory => {
                self.directories += 1;
                self.bytes = self.bytes.saturating_add(allocated_bytes);
                self.maybe_progress(&path, false);

                match fs::read_dir(&path) {
                    Ok(entries) => {
                        for entry in entries {
                            if self.cancel.load(Ordering::Relaxed) {
                                break;
                            }
                            match entry {
                                Ok(entry) => {
                                    if let Some(child_id) =
                                        self.visit(entry.path(), Some(id), depth + 1)
                                    {
                                        self.nodes[id as usize].children.push(child_id);
                                    }
                                }
                                Err(error) => self.add_issue(
                                    &path,
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
                        &path,
                        if error.kind() == std::io::ErrorKind::PermissionDenied {
                            IssueKind::PermissionDenied
                        } else {
                            IssueKind::Io
                        },
                        error.to_string(),
                    ),
                }

                let child_ids = self.nodes[id as usize].children.clone();
                let mut total_allocated = allocated_bytes;
                let mut total_apparent = apparent_bytes;
                let mut file_count = 0_u64;
                let mut directory_count = 1_u64;
                for child_id in child_ids {
                    let child = &self.nodes[child_id as usize];
                    total_allocated = total_allocated.saturating_add(child.allocated_bytes);
                    total_apparent = total_apparent.saturating_add(child.apparent_bytes);
                    file_count = file_count.saturating_add(child.file_count);
                    directory_count = directory_count.saturating_add(child.directory_count);
                }
                let node = &mut self.nodes[id as usize];
                node.allocated_bytes = total_allocated;
                node.apparent_bytes = total_apparent;
                node.file_count = file_count;
                node.directory_count = directory_count;
            }
            _ => {
                self.files += 1;
                self.bytes = self.bytes.saturating_add(allocated_bytes);
                self.maybe_progress(&path, false);
            }
        }

        Some(id)
    }
}

fn scan_local<F>(
    request: &ScanRequest,
    cancel: Arc<AtomicBool>,
    emit: &mut F,
) -> Result<ScanTree, ScanError>
where
    F: FnMut(ScanSignal),
{
    let path = if request.target.starts_with("file://") {
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
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| ScanError::InvalidTarget(request.target.clone()))?;
    let started = Instant::now();
    let mut context = LocalContext {
        request,
        cancel: cancel.clone(),
        emit,
        nodes: Vec::new(),
        issues: Vec::new(),
        seen_hard_links: HashSet::new(),
        seen_directories: HashSet::new(),
        mounts: discover_mounts(),
        root_device: metadata.dev(),
        started,
        last_progress: Instant::now(),
        files: 0,
        directories: 0,
        bytes: 0,
    };

    let root_id = context.visit(path.clone(), None, 0).unwrap_or(0);
    context.maybe_progress(&path, true);

    if context.nodes.is_empty() && !cancel.load(Ordering::Relaxed) {
        return Err(ScanError::InvalidTarget(request.target.clone()));
    }

    let cancelled = cancel.load(Ordering::Relaxed);
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
    cancel: Arc<AtomicBool>,
    emit: &'a mut F,
    nodes: Vec<ScanNode>,
    issues: Vec<ScanIssue>,
    files: u64,
    directories: u64,
    bytes: u64,
    started: Instant,
    last_progress: Instant,
}

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

    fn visit(&mut self, file: gio::File, parent_id: Option<u64>, depth: usize) -> Option<u64> {
        if self.cancel.load(Ordering::Relaxed) {
            return None;
        }
        if depth > 512 {
            self.issue(&file.uri(), "remote directory depth exceeds the safe limit");
            return None;
        }

        let attributes = "standard::name,standard::display-name,standard::type,standard::size,standard::allocated-size,time::modified,unix::mode,unix::nlink";
        let info = match file.query_info(
            attributes,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
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

        if kind == NodeKind::Directory {
            self.directories += 1;
            self.progress(&uri, false);
            match file.enumerate_children(
                attributes,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                gio::Cancellable::NONE,
            ) {
                Ok(enumerator) => loop {
                    if self.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    match enumerator.next_file(gio::Cancellable::NONE) {
                        Ok(Some(child_info)) => {
                            let child = file.child(child_info.name());
                            if let Some(child_id) = self.visit(child, Some(id), depth + 1) {
                                self.nodes[id as usize].children.push(child_id);
                            }
                        }
                        Ok(None) => break,
                        Err(error) => {
                            self.issue(&uri, error.to_string());
                            break;
                        }
                    }
                },
                Err(error) => self.issue(&uri, error.to_string()),
            }

            let child_ids = self.nodes[id as usize].children.clone();
            let mut total_allocated = allocated_bytes;
            let mut total_apparent = apparent_bytes;
            let mut file_count = 0_u64;
            let mut directory_count = 1_u64;
            for child_id in child_ids {
                let child = &self.nodes[child_id as usize];
                total_allocated = total_allocated.saturating_add(child.allocated_bytes);
                total_apparent = total_apparent.saturating_add(child.apparent_bytes);
                file_count = file_count.saturating_add(child.file_count);
                directory_count = directory_count.saturating_add(child.directory_count);
            }
            let node = &mut self.nodes[id as usize];
            node.allocated_bytes = total_allocated;
            node.apparent_bytes = total_apparent;
            node.file_count = file_count;
            node.directory_count = directory_count;
        } else {
            self.files += 1;
            self.bytes = self.bytes.saturating_add(allocated_bytes);
            self.progress(&uri, false);
        }
        Some(id)
    }
}

fn scan_remote<F>(
    request: &ScanRequest,
    cancel: Arc<AtomicBool>,
    emit: &mut F,
) -> Result<ScanTree, ScanError>
where
    F: FnMut(ScanSignal),
{
    let started = Instant::now();
    let file = gio::File::for_uri(&request.target);
    let mut context = RemoteContext {
        cancel: cancel.clone(),
        emit,
        nodes: Vec::new(),
        issues: Vec::new(),
        files: 0,
        directories: 0,
        bytes: 0,
        started,
        last_progress: Instant::now(),
    };
    let root_id = context
        .visit(file, None, 0)
        .ok_or_else(|| ScanError::Remote(request.target.clone()))?;
    context.progress(&request.target, true);

    let cancelled = cancel.load(Ordering::Relaxed);
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
    let Ok(contents) = fs::read_to_string("/proc/self/mountinfo") else {
        return Vec::new();
    };

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
                path,
            })
        })
        .collect()
}

fn is_remote_filesystem(filesystem: &str) -> bool {
    let filesystem = filesystem.to_ascii_lowercase();
    matches!(
        filesystem.as_str(),
        "9p" | "afs"
            | "ceph"
            | "cifs"
            | "davfs"
            | "davfs2"
            | "glusterfs"
            | "nfs"
            | "nfs4"
            | "smb3"
            | "smbfs"
            | "sshfs"
    ) || filesystem.starts_with("fuse.sshfs")
        || filesystem.starts_with("fuse.rclone")
        || filesystem.starts_with("fuse.curlftpfs")
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
        let tree = scan_target(&request, Arc::new(AtomicBool::new(false)), |_| {}).unwrap();
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
        let tree = scan_target(&request, Arc::new(AtomicBool::new(false)), |_| {}).unwrap();
        assert!(tree.summary.excluded >= 1);
        assert_eq!(tree.summary.directories, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_is_observed() {
        let root = temp_fixture();
        let cancel = Arc::new(AtomicBool::new(true));
        let request = ScanRequest {
            target: root.to_string_lossy().into_owned(),
            mode: ScanMode::Standard,
            options: ScanOptions::default(),
        };
        let tree = scan_target(&request, cancel, |_| {}).unwrap();
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
        assert!(!is_remote_filesystem("ext4"));
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
}
