//! Compact scan snapshots and the "what grew" comparison. The tree walk, the
//! retention rule and the diff are pure; only `save`/`list`/`load`/`clear` touch disk.

use crate::model::{NodeKind, ScanMode, ScanNode, ScanRequest, ScanStatus, ScanTree};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Directories deeper than this below the target are not stored.
pub const MAX_DEPTH: usize = 4;
/// Snapshots kept per target.
pub const KEEP: usize = 10;
/// Largest snapshot file `load` accepts.
pub const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;
/// Rows returned by a comparison.
pub const MAX_DIFF_ROWS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    // `timestamp` and `total` come first so `list` can read them from the file's head.
    pub timestamp: u64,
    pub total: u64,
    pub target: String,
    pub dirs: Vec<DirEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub timestamp: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub path: String,
    pub before: u64,
    pub after: u64,
    pub delta: i64,
    /// Node of the current scan, when the directory still exists there.
    pub node_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub before_timestamp: u64,
    pub before_total: u64,
    pub after_total: u64,
    pub delta: i64,
    pub rows: Vec<DiffRow>,
}

/// Directories reachable from the root down to `MAX_DEPTH`. Walking from the
/// root leaves out nodes detached by a deletion.
fn walk_dirs(tree: &ScanTree) -> Vec<&ScanNode> {
    let mut dirs = Vec::new();
    let mut stack = vec![(tree.root_id, 0_usize)];
    while let Some((id, depth)) = stack.pop() {
        let Some(node) = tree.nodes.get(id as usize) else {
            continue;
        };
        if node.kind != NodeKind::Directory {
            continue;
        }
        dirs.push(node);
        if depth < MAX_DEPTH {
            stack.extend(node.children.iter().map(|child| (*child, depth + 1)));
        }
    }
    dirs
}

pub fn build_snapshot(tree: &ScanTree, target: &str, timestamp: u64) -> Snapshot {
    let mut dirs: Vec<DirEntry> = walk_dirs(tree)
        .into_iter()
        .map(|node| DirEntry {
            path: node.display_path.clone(),
            bytes: node.allocated_bytes,
        })
        .collect();
    dirs.sort_by(|left, right| left.path.cmp(&right.path));
    Snapshot {
        target: public_target(target),
        timestamp,
        total: tree
            .nodes
            .get(tree.root_id as usize)
            .map_or(0, |root| root.allocated_bytes),
        dirs,
    }
}

/// Lets a row navigate to its directory in the current scan.
pub fn attach_node_ids(rows: &mut [DiffRow], tree: &ScanTree) {
    let ids: std::collections::HashMap<&str, u64> = walk_dirs(tree)
        .into_iter()
        .map(|node| (node.display_path.as_str(), node.id))
        .collect();
    for row in rows {
        row.node_id = ids.get(row.path.as_str()).copied();
    }
}

fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Per-directory change between two snapshots, largest absolute change first.
pub fn diff(before: &Snapshot, after: &Snapshot, cap: usize) -> Diff {
    let mut sizes: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for entry in &before.dirs {
        sizes.entry(&entry.path).or_default().0 = entry.bytes;
    }
    for entry in &after.dirs {
        sizes.entry(&entry.path).or_default().1 = entry.bytes;
    }
    let mut rows: Vec<DiffRow> = sizes
        .into_iter()
        .map(|(path, (before, after))| DiffRow {
            path: path.to_string(),
            before,
            after,
            delta: signed(after).saturating_sub(signed(before)),
            node_id: None,
        })
        .collect();
    // Stable sort over path-ordered rows keeps ties deterministic.
    rows.sort_by_key(|row| std::cmp::Reverse(row.delta.unsigned_abs()));
    rows.truncate(cap);
    Diff {
        before_timestamp: before.timestamp,
        before_total: before.total,
        after_total: after.total,
        delta: signed(after.total).saturating_sub(signed(before.total)),
        rows,
    }
}

/// Timestamps to delete so that only the newest `keep` remain.
pub fn retention_victims(timestamps: &[u64], keep: usize) -> Vec<u64> {
    let mut sorted = timestamps.to_vec();
    sorted.sort_unstable_by_key(|timestamp| std::cmp::Reverse(*timestamp));
    sorted.split_off(keep.min(sorted.len()))
}

pub fn history_root() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("liscan")
        .join("history")
}

/// The target as stored: a URI loses any user name and password.
fn public_target(target: &str) -> String {
    match url::Url::parse(target) {
        Ok(mut uri) if target.contains("://") && !uri.cannot_be_a_base() => {
            let _ = uri.set_username("");
            let _ = uri.set_password(None);
            uri.to_string()
        }
        _ => target.to_string(),
    }
}

/// First 16 hex characters of the SHA-256 of the canonical target path.
pub fn target_key(target: &str) -> String {
    let target = &public_target(target);
    let canonical = fs::canonicalize(target).unwrap_or_else(|_| PathBuf::from(target));
    let digest = Sha256::digest(canonical.as_os_str().as_bytes());
    digest[..8].iter().map(|byte| format!("{byte:02x}")).collect()
}

fn target_dir(root: &Path, target: &str) -> PathBuf {
    root.join(target_key(target))
}

fn private_dir(path: &Path) -> io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

/// Writes the snapshot atomically (temp file, then rename) with mode 0600 and
/// applies the retention rule.
pub fn save(root: &Path, snapshot: &Snapshot) -> io::Result<()> {
    let directory = target_dir(root, &snapshot.target);
    private_dir(&directory)?;
    let temporary = directory.join(format!("{}.json.tmp", snapshot.timestamp));
    let data = serde_json::to_vec(snapshot).map_err(io::Error::other)?;
    let written = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)
        .and_then(|mut file| file.write_all(&data).and_then(|()| file.sync_all()));
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, directory.join(format!("{}.json", snapshot.timestamp)))?;
    let timestamps = timestamps(&directory);
    for victim in retention_victims(&timestamps, KEEP) {
        let _ = fs::remove_file(directory.join(format!("{victim}.json")));
    }
    Ok(())
}

fn timestamps(directory: &Path) -> Vec<u64> {
    fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            name.to_str()?.strip_suffix(".json")?.parse().ok()
        })
        .collect()
}

pub fn load(root: &Path, target: &str, timestamp: u64) -> io::Result<Snapshot> {
    load_limited(root, target, timestamp, MAX_SNAPSHOT_BYTES)
}

fn load_limited(root: &Path, target: &str, timestamp: u64, max: u64) -> io::Result<Snapshot> {
    let path = target_dir(root, target).join(format!("{timestamp}.json"));
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "snapshot file is too large"));
    }
    let mut data = Vec::new();
    io::Read::read_to_end(&mut io::Read::take(file, max), &mut data)?;
    serde_json::from_slice(&data).map_err(io::Error::other)
}

/// Reads `{"timestamp":N,"total":M,` from the head of a snapshot file without
/// parsing the (possibly large) folder list.
fn read_header(path: &Path) -> Option<(u64, u64)> {
    let mut head = [0_u8; 96];
    let mut file = fs::File::open(path).ok()?;
    let read = io::Read::read(&mut file, &mut head).ok()?;
    let text = std::str::from_utf8(&head[..read]).ok()?;
    let rest = text.strip_prefix("{\"timestamp\":")?;
    let (timestamp, rest) = rest.split_once(",\"total\":")?;
    let total: String = rest.chars().take_while(char::is_ascii_digit).collect();
    Some((timestamp.parse().ok()?, total.parse().ok()?))
}

/// Earlier snapshots of this target, newest first.
pub fn list(root: &Path, target: &str) -> Vec<SnapshotInfo> {
    let directory = target_dir(root, target);
    let mut found: Vec<u64> = timestamps(&directory);
    found.sort_unstable_by_key(|timestamp| std::cmp::Reverse(*timestamp));
    found
        .into_iter()
        .filter_map(|stamp| read_header(&directory.join(format!("{stamp}.json"))))
        .map(|(timestamp, total)| SnapshotInfo { timestamp, total })
        .collect()
}

pub fn clear(root: &Path) -> io::Result<()> {
    match fs::remove_dir_all(root) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Saves a snapshot of a finished scan; a partial scan is not a fair baseline.
/// Returns the timestamp it was saved under.
pub fn record(request: &ScanRequest, tree: &ScanTree) -> Option<u64> {
    // An administrator scan lists folders only root may see; the history lives in
    // the user's home, readable without authentication, so it never keeps them.
    if tree.summary.status == ScanStatus::Cancelled
        || tree.summary.elevated
        || request.mode == ScanMode::Administrator
    {
        return None;
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let snapshot = build_snapshot(tree, &request.target, timestamp);
    match save(&history_root(), &snapshot) {
        Ok(()) => Some(timestamp),
        Err(error) => {
            eprintln!("LiScan: could not save scan history: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ScanSummary;
    use std::os::unix::fs::PermissionsExt;

    fn node(id: u64, parent: Option<u64>, path: &str, kind: NodeKind, bytes: u64) -> ScanNode {
        ScanNode {
            id,
            parent_id: parent,
            name: path.rsplit('/').next().unwrap().into(),
            display_path: path.into(),
            uri: String::new(),
            local_path: None,
            kind,
            allocated_bytes: bytes,
            apparent_bytes: bytes,
            children: Vec::new(),
            file_count: 0,
            directory_count: 0,
            modified_ms: None,
            permissions: None,
            hard_links: 1,
            flags: Vec::new(),
        }
    }

    /// /r with a chain /r/a/b/c/d/e (six levels) plus a file /r/f.
    fn chain() -> ScanTree {
        let paths = ["/r", "/r/a", "/r/a/b", "/r/a/b/c", "/r/a/b/c/d", "/r/a/b/c/d/e"];
        let mut nodes: Vec<ScanNode> = paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let parent = index.checked_sub(1).map(|p| p as u64);
                node(index as u64, parent, path, NodeKind::Directory, 100 - index as u64)
            })
            .collect();
        for index in 1..nodes.len() {
            nodes[index - 1].children.push(index as u64);
        }
        nodes.push(node(6, Some(0), "/r/f", NodeKind::File, 5));
        nodes[0].children.push(6);
        ScanTree {
            root_id: 0,
            nodes,
            issues: Vec::new(),
            summary: ScanSummary::default(),
        }
    }

    fn snap(timestamp: u64, total: u64, dirs: &[(&str, u64)]) -> Snapshot {
        Snapshot {
            target: "/r".into(),
            timestamp,
            total,
            dirs: dirs
                .iter()
                .map(|(path, bytes)| DirEntry {
                    path: (*path).into(),
                    bytes: *bytes,
                })
                .collect(),
        }
    }

    #[test]
    fn snapshot_keeps_directories_down_to_depth_four_only() {
        let snapshot = build_snapshot(&chain(), "/r", 7);
        let paths: Vec<_> = snapshot.dirs.iter().map(|dir| dir.path.as_str()).collect();
        // /r is depth 0, /r/a/b/c/d is depth 4; /r/a/b/c/d/e (5) and the file are cut.
        assert_eq!(paths, ["/r", "/r/a", "/r/a/b", "/r/a/b/c", "/r/a/b/c/d"]);
        assert_eq!(snapshot.total, 100);
        assert_eq!(snapshot.timestamp, 7);
    }

    #[test]
    fn diff_reports_new_deleted_and_unchanged_directories() {
        let before = snap(1, 300, &[("/r", 300), ("/r/old", 200), ("/r/same", 100)]);
        let after = snap(2, 450, &[("/r", 450), ("/r/new", 350), ("/r/same", 100)]);
        let result = diff(&before, &after, 500);
        let row = |path: &str| result.rows.iter().find(|row| row.path == path).unwrap();
        assert_eq!((row("/r/new").before, row("/r/new").after), (0, 350));
        assert_eq!(row("/r/new").delta, 350);
        assert_eq!((row("/r/old").before, row("/r/old").after), (200, 0));
        assert_eq!(row("/r/old").delta, -200);
        assert_eq!(row("/r/same").delta, 0);
        assert_eq!((result.before_total, result.after_total, result.delta), (300, 450, 150));
        let order: Vec<_> = result.rows.iter().map(|row| row.path.as_str()).collect();
        assert_eq!(order, ["/r/new", "/r/old", "/r", "/r/same"]);
    }

    #[test]
    fn diff_is_capped() {
        let dirs: Vec<(String, u64)> = (0..600).map(|n| (format!("/r/d{n:03}"), n)).collect();
        let borrowed: Vec<(&str, u64)> = dirs.iter().map(|(p, b)| (p.as_str(), *b)).collect();
        let result = diff(&snap(1, 0, &[]), &snap(2, 0, &borrowed), MAX_DIFF_ROWS);
        assert_eq!(result.rows.len(), 500);
        assert_eq!(result.rows[0].path, "/r/d599");
    }

    #[test]
    fn retention_drops_the_oldest_beyond_the_limit() {
        let all: Vec<u64> = (1..=12).collect();
        assert_eq!(retention_victims(&all, 10), vec![2, 1]);
        assert!(retention_victims(&all[..10], 10).is_empty());
    }

    #[test]
    fn administrator_scans_are_never_recorded() {
        let mut tree = chain();
        let request = ScanRequest {
            target: "/r".into(),
            mode: ScanMode::Administrator,
            options: Default::default(),
        };
        assert_eq!(record(&request, &tree), None);
        tree.summary.elevated = true;
        let standard = ScanRequest { mode: ScanMode::Standard, ..request };
        assert_eq!(record(&standard, &tree), None);
    }

    #[test]
    fn uri_credentials_are_not_stored_or_hashed() {
        let with = build_snapshot(&chain(), "sftp://ann:secret@host/srv", 1);
        assert!(!with.target.contains("secret") && !with.target.contains("ann"));
        assert_eq!(
            target_key("sftp://ann:secret@host/srv"),
            target_key("sftp://host/srv")
        );
    }

    #[test]
    fn oversized_or_malformed_snapshots_are_rejected() {
        let root = std::env::temp_dir().join(format!("liscan-history-{}", uuid::Uuid::new_v4()));
        save(&root, &snap(5, 9, &[("/r", 9)])).unwrap();
        assert!(load_limited(&root, "/r", 5, 10).is_err());
        assert_eq!(load(&root, "/r", 5).unwrap().total, 9);
        let path = target_dir(&root, "/r");
        fs::write(path.join("6.json"), b"not json").unwrap();
        assert!(load(&root, "/r", 6).is_err());
        let listed: Vec<_> = list(&root, "/r").iter().map(|i| (i.timestamp, i.total)).collect();
        assert_eq!(listed, [(5, 9)]); // the malformed file has no valid header
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn save_is_private_lists_newest_first_and_prunes() {
        let root = std::env::temp_dir().join(format!("liscan-history-{}", uuid::Uuid::new_v4()));
        for timestamp in 100..113 {
            save(&root, &snap(timestamp, timestamp, &[("/r", timestamp)])).unwrap();
        }
        let listed = list(&root, "/r");
        assert_eq!(listed.len(), KEEP);
        assert_eq!(listed[0].timestamp, 112);
        assert_eq!(listed[9].timestamp, 103);
        let directory = target_dir(&root, "/r");
        let mode = fs::metadata(directory.join("112.json")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(target_key("/r").len(), 16);
        assert_eq!(load(&root, "/r", 105).unwrap().total, 105);
        assert!(fs::read_dir(&directory).unwrap().all(|entry| {
            !entry.unwrap().file_name().to_string_lossy().ends_with(".tmp")
        }));
        clear(&root).unwrap();
        assert!(list(&root, "/r").is_empty());
        clear(&root).unwrap();
        fs::remove_dir_all(&root).ok();
    }
}
