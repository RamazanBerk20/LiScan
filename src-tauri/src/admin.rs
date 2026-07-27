use crate::model::{ScanRequest, ScanTree};
use crate::protocol::{
    AdminControl, AdminMessage, PROTOCOL_VERSION, ScanTreeAssembler, read_frame, write_frame,
};
use crate::scanner::ScanSignal;
use std::env;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

fn helper_path() -> PathBuf {
    if let Ok(path) = env::var("LISCAN_ADMIN_HELPER") {
        return PathBuf::from(path);
    }

    #[cfg(debug_assertions)]
    {
        if let Ok(executable) = env::current_exe()
            && let Some(directory) = executable.parent()
        {
            let candidate = directory.join("liscan-admin-helper");
            if candidate.exists() {
                return candidate;
            }
        }
        let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("admin-helper/target/debug/liscan-admin-helper");
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from("/usr/libexec/liscan/liscan-admin-helper")
}

pub fn run_admin_scan<F>(
    request: ScanRequest,
    cancel: Arc<AtomicBool>,
    mut emit: F,
) -> Result<ScanTree, String>
where
    F: FnMut(ScanSignal),
{
    let helper = helper_path();
    if !helper.exists() {
        return Err(format!(
            "Administrator helper is not installed at {}",
            helper.display()
        ));
    }

    let direct = cfg!(debug_assertions)
        && env::var("LISCAN_ADMIN_DIRECT")
            .ok()
            .is_some_and(|value| value == "1");
    let mut command = if direct {
        Command::new(&helper)
    } else {
        let mut command = Command::new("/usr/bin/pkexec");
        command.arg(&helper);
        command
    };
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start administrator authentication: {error}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Administrator helper has no input pipe".to_string())?;
    write_frame(
        &mut stdin,
        &AdminControl::Start {
            version: PROTOCOL_VERSION,
            request,
        },
    )
    .map_err(|error| format!("Could not send scan request to helper: {error}"))?;

    let finished = Arc::new(AtomicBool::new(false));
    let finished_control = finished.clone();
    let cancel_control = cancel.clone();
    let control = thread::spawn(move || {
        while !finished_control.load(Ordering::Relaxed) {
            if cancel_control.load(Ordering::Relaxed) {
                let _ = write_frame(&mut stdin, &AdminControl::Cancel);
                return;
            }
            thread::sleep(Duration::from_millis(40));
        }
    });

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Administrator helper has no output pipe".to_string())?;
    let mut result = None;
    let mut tree_assembler = ScanTreeAssembler::default();
    let mut cancelled = false;
    let mut helper_error = None;

    loop {
        match read_frame::<AdminMessage>(&mut stdout) {
            Ok(AdminMessage::Started) => {}
            Ok(AdminMessage::Progress(progress)) => emit(ScanSignal::Progress(progress)),
            Ok(AdminMessage::Issue(issue)) => emit(ScanSignal::Issue(issue)),
            Ok(AdminMessage::TreeStart {
                root_id,
                summary,
                node_count,
                issue_count,
            }) => {
                if let Err(error) = tree_assembler.start(root_id, summary, node_count, issue_count)
                {
                    helper_error = Some(format!(
                        "Invalid response from administrator helper: {error}"
                    ));
                    break;
                }
            }
            Ok(AdminMessage::TreeNodes(nodes)) => {
                if let Err(error) = tree_assembler.push_nodes(nodes) {
                    helper_error = Some(format!(
                        "Invalid response from administrator helper: {error}"
                    ));
                    break;
                }
            }
            Ok(AdminMessage::TreeIssues(issues)) => {
                if let Err(error) = tree_assembler.push_issues(issues) {
                    helper_error = Some(format!(
                        "Invalid response from administrator helper: {error}"
                    ));
                    break;
                }
            }
            Ok(AdminMessage::TreeEnd) => match tree_assembler.finish() {
                Ok(tree) => {
                    result = Some(tree);
                    break;
                }
                Err(error) => {
                    helper_error = Some(format!(
                        "Invalid response from administrator helper: {error}"
                    ));
                    break;
                }
            },
            Ok(AdminMessage::Cancelled) => {
                cancelled = true;
                break;
            }
            Ok(AdminMessage::Failed(message)) => {
                helper_error = Some(message);
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                if tree_assembler.is_pending() {
                    helper_error =
                        Some("Administrator helper returned an incomplete scan tree".into());
                }
                break;
            }
            Err(error) => {
                helper_error = Some(format!(
                    "Invalid response from administrator helper: {error}"
                ));
                break;
            }
        }
    }

    finished.store(true, Ordering::Relaxed);
    let _ = control.join();
    let status = child
        .wait()
        .map_err(|error| format!("Could not wait for administrator helper: {error}"))?;

    if let Some(tree) = result {
        return Ok(tree);
    }
    if cancelled {
        return Err("scan was cancelled".into());
    }
    if let Some(error) = helper_error {
        return Err(error);
    }
    match status.code() {
        Some(126) => Err("Administrator authentication was cancelled".into()),
        Some(127) => Err("Administrator authentication was denied or unavailable".into()),
        Some(code) => Err(format!("Administrator helper exited with status {code}")),
        None => Err("Administrator helper stopped unexpectedly".into()),
    }
}
