//! Headless mode: `liscan --json [--admin] [--top N] PATH` scans without a
//! window and prints exactly one JSON document to stdout.

use crate::admin::run_admin_scan;
use crate::model::{NodeKind, ScanMode, ScanNode, ScanOptions, ScanRequest, ScanStatus, ScanSummary};
use crate::scanner::{CancelToken, scan_target};
use serde::Serialize;
use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const DEFAULT_TOP: usize = 50;

pub const HELP: &str = "\
LiScan - a clear, complete disk usage scanner

Usage:
  liscan [--admin] [--home] [--new-window] [PATH_OR_URI]
      Open the graphical interface, optionally starting a scan.
  liscan --json [--admin] [--top N] PATH
      Scan without opening a window and print one JSON document to stdout.

Options:
  --json       Headless mode. Only the JSON document goes to stdout;
               diagnostics go to stderr.
  --top N      Number of largest folders and files listed (default 50).
  --admin      Scan as administrator (absolute local paths only).
  --home       Scan the current user's home folder.
  -h, --help   Show this help.

JSON mode exit codes: 0 scan finished (with or without issues), 1 failure
(the document is {\"schema\":1,\"error\":\"...\"}), 130 interrupted by SIGINT.
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonArgs {
    pub target: OsString,
    pub admin: bool,
    pub top: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Cli {
    /// The graphical interface (the arguments are read by the launch request).
    Gui,
    Help,
    Json(JsonArgs),
}

pub fn parse_args(args: &[OsString]) -> Result<Cli, String> {
    let has = |flag: &str| args.iter().any(|arg| arg == flag);
    if has("--help") || has("-h") {
        return Ok(Cli::Help);
    }
    if !has("--json") {
        return Ok(Cli::Gui);
    }
    let mut parsed = JsonArgs {
        target: OsString::new(),
        admin: false,
        top: DEFAULT_TOP,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some("--json" | "--new-window") => {}
            Some("--admin") => parsed.admin = true,
            Some("--home") => {
                parsed.target = dirs::home_dir()
                    .ok_or("the home folder could not be determined")?
                    .into_os_string();
            }
            Some("--top") => {
                parsed.top = iter
                    .next()
                    .and_then(|value| value.to_str())
                    .and_then(|value| value.parse().ok())
                    .ok_or("--top needs a whole number")?;
            }
            Some(option) if option.starts_with('-') => {
                return Err(format!("unknown option {option}"));
            }
            _ => parsed.target = arg.clone(),
        }
    }
    if parsed.target.is_empty() {
        return Err("--json needs a PATH to scan".into());
    }
    Ok(Cli::Json(parsed))
}

#[derive(Debug, Serialize)]
pub struct TopItem {
    pub path: String,
    pub path_lossy: bool,
    pub kind: &'static str,
    pub size_bytes: u64,
    pub apparent_bytes: u64,
    pub items: u64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub target: String,
    pub status: ScanStatus,
    pub summary: ScanSummary,
    pub top: Vec<TopItem>,
    pub issues_count: usize,
}

fn top_item(node: &ScanNode) -> TopItem {
    let (path, path_lossy) = match &node.local_path {
        Some(path) => (path.to_string_lossy().into_owned(), path.to_str().is_none()),
        None => (node.display_path.clone(), false),
    };
    let directory = node.kind == NodeKind::Directory;
    TopItem {
        path,
        path_lossy,
        kind: match node.kind {
            NodeKind::Directory => "dir",
            NodeKind::File => "file",
            NodeKind::Symlink => "symlink",
            NodeKind::SmallFiles => "small_files",
            NodeKind::Other => "other",
        },
        size_bytes: node.allocated_bytes,
        apparent_bytes: node.apparent_bytes,
        items: if directory {
            node.file_count.saturating_add(node.directory_count)
        } else {
            0
        },
    }
}

/// Runs the scan through the same functions the GUI uses and builds the report.
/// `top` lists the largest folders and files below the target, largest first.
pub fn scan_report(
    request: &ScanRequest,
    top: usize,
    cancel: &CancelToken,
) -> Result<Report, String> {
    let tree = if request.mode == ScanMode::Administrator {
        run_admin_scan(request.clone(), cancel, |_| {}).map_err(|error| error.to_string())?
    } else {
        scan_target(request, cancel, |_| {}).map_err(|error| error.to_string())?
    };
    let mut nodes: Vec<&ScanNode> = tree
        .nodes
        .iter()
        .filter(|node| node.id != tree.root_id && node.parent_id.is_some())
        .collect();
    let order = |left: &&ScanNode, right: &&ScanNode| {
        right
            .allocated_bytes
            .cmp(&left.allocated_bytes)
            .then_with(|| left.display_path.cmp(&right.display_path))
    };
    if top < nodes.len() {
        nodes.select_nth_unstable_by(top, order);
        nodes.truncate(top);
    }
    nodes.sort_by(order);
    Ok(Report {
        schema: 1,
        target: request.target.clone(),
        status: tree.summary.status,
        summary: tree.summary.clone(),
        top: nodes.into_iter().map(top_item).collect(),
        issues_count: tree.issues.len(),
    })
}

const SIGINT: i32 = 2;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigint(_: i32) {
    INTERRUPTED.store(true, Ordering::Relaxed);
}

unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

/// Turns SIGINT into a clean scan cancellation. The handler only sets a flag
/// (all it may safely do); a thread forwards it to the cancel token.
fn cancel_on_sigint(cancel: &CancelToken) {
    // SAFETY: `signal` is the C library's; the handler is async-signal-safe.
    unsafe { signal(SIGINT, on_sigint as extern "C" fn(i32) as usize) };
    let cancel = cancel.clone();
    std::thread::spawn(move || {
        while !INTERRUPTED.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(30));
        }
        cancel.cancel();
        // Back to the default action: a second Ctrl+C ends a scan stuck in a syscall.
        // SAFETY: restoring SIG_DFL (0) is always valid.
        unsafe { signal(SIGINT, 0) };
    });
}

pub fn error_document(message: &str) -> String {
    let quoted = serde_json::to_string(message).unwrap_or_default();
    format!("{{\"schema\":1,\"error\":{quoted}}}")
}

/// Runs headless mode and returns the process exit code.
pub fn run_json(args: JsonArgs, options: ScanOptions) -> i32 {
    let Ok(target) = args.target.into_string() else {
        println!("{}", error_document("the path is not valid UTF-8"));
        return 1;
    };
    // The scanner wants absolute paths; `liscan --json .` should work.
    let target = match std::path::absolute(&target) {
        Ok(path) if !target.contains("://") => path.to_string_lossy().into_owned(),
        _ => target,
    };
    let request = ScanRequest {
        target,
        mode: if args.admin {
            ScanMode::Administrator
        } else {
            ScanMode::Standard
        },
        options,
    };
    let cancel = CancelToken::default();
    cancel_on_sigint(&cancel);
    let result = scan_report(&request, args.top, &cancel);
    if INTERRUPTED.load(Ordering::Relaxed) {
        // Whatever the scan returned is partial; the document says so.
        let report = result.unwrap_or_else(|_| Report {
            schema: 1,
            target: request.target.clone(),
            status: ScanStatus::Cancelled,
            summary: ScanSummary {
                status: ScanStatus::Cancelled,
                ..ScanSummary::default()
            },
            top: Vec::new(),
            issues_count: 0,
        });
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
        eprintln!("liscan: interrupted");
        return 130;
    }
    match result {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(document) => {
                println!("{document}");
                0
            }
            Err(error) => {
                println!("{}", error_document(&error.to_string()));
                1
            }
        },
        Err(message) => {
            eprintln!("liscan: {message}");
            println!("{}", error_document(&message));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn without_json_the_existing_arguments_open_the_gui() {
        for list in [&[][..], &["--admin", "/"], &["--home"], &["/tmp"], &["--new-window", "/x"]] {
            assert_eq!(parse_args(&args(list)), Ok(Cli::Gui));
        }
        assert_eq!(parse_args(&args(&["--help"])), Ok(Cli::Help));
    }

    #[test]
    fn json_arguments_are_parsed() {
        assert_eq!(
            parse_args(&args(&["--json", "/tmp"])),
            Ok(Cli::Json(JsonArgs { target: "/tmp".into(), admin: false, top: 50 }))
        );
        assert_eq!(
            parse_args(&args(&["/srv", "--top", "5", "--admin", "--json"])),
            Ok(Cli::Json(JsonArgs { target: "/srv".into(), admin: true, top: 5 }))
        );
        assert!(parse_args(&args(&["--json"])).is_err());
        assert!(parse_args(&args(&["--json", "--top", "x", "/"])).is_err());
        assert!(parse_args(&args(&["--json", "--top"])).is_err());
        assert!(parse_args(&args(&["--json", "--bogus", "/"])).is_err());
    }

    fn write(path: &PathBuf, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7_u8; bytes]).unwrap();
    }

    #[test]
    fn report_for_a_temp_directory_has_the_documented_shape_and_order() {
        let root = std::env::temp_dir().join(format!("liscan-json-{}", uuid::Uuid::new_v4()));
        write(&root.join("big/huge.bin"), 300_000);
        write(&root.join("big/small.txt"), 10);
        write(&root.join("mid.bin"), 100_000);
        let odd = root.join(std::ffi::OsStr::from_bytes(b"odd-\xff.bin"));
        write(&odd, 50_000);

        let request = ScanRequest {
            target: root.to_string_lossy().into_owned(),
            mode: ScanMode::Standard,
            options: ScanOptions::default(),
        };
        let report = scan_report(&request, 3, &CancelToken::default()).unwrap();
        let value = serde_json::to_value(&report).unwrap();

        assert_eq!(value["schema"], 1);
        assert_eq!(value["status"], "complete");
        assert_eq!(value["target"], request.target.as_str());
        assert_eq!(value["issues_count"], 0);
        assert!(value["summary"]["allocatedBytes"].as_u64().unwrap() >= 450_000);
        let top = value["top"].as_array().unwrap();
        assert_eq!(top.len(), 3);
        let names: Vec<_> = top.iter().map(|item| item["path"].as_str().unwrap()).collect();
        assert_eq!(names[0], root.join("big").to_str().unwrap());
        assert_eq!(names[1], root.join("big/huge.bin").to_str().unwrap());
        assert_eq!(names[2], root.join("mid.bin").to_str().unwrap());
        assert_eq!(top[0]["kind"], "dir");
        assert_eq!(top[1]["kind"], "file");
        assert_eq!(top[0]["items"], 3);
        assert!(top.iter().all(|item| item["path_lossy"] == false));
        let sizes: Vec<u64> = top.iter().map(|item| item["size_bytes"].as_u64().unwrap()).collect();
        assert!(sizes.windows(2).all(|pair| pair[0] >= pair[1]));

        let all = scan_report(&request, 50, &CancelToken::default()).unwrap();
        let lossy: Vec<_> = all.top.iter().filter(|item| item.path_lossy).collect();
        assert_eq!(lossy.len(), 1);
        assert!(lossy[0].path.contains('\u{fffd}'));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_missing_target_is_an_error() {
        let request = ScanRequest {
            target: "/definitely/not/here".into(),
            mode: ScanMode::Standard,
            options: ScanOptions::default(),
        };
        assert!(scan_report(&request, 5, &CancelToken::default()).is_err());
    }
}
