use crate::model::{ScanRequest, ScanTree};
use crate::protocol::{
    AdminControl, AdminMessage, PROTOCOL_VERSION, ScanTreeAssembler, read_frame, write_frame,
};
use crate::scanner::{CancelToken, ScanSignal};
use std::env;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdminError {
    #[error("Administrator authentication was cancelled")]
    AuthCancelled,
    #[error("Administrator authentication was denied or unavailable")]
    AuthDenied,
    #[error("Administrator helper is not installed at {0}")]
    HelperMissing(String),
    #[error("scan was cancelled")]
    Cancelled,
    #[error("Invalid response from administrator helper: {0}")]
    Protocol(String),
    #[error("{0}")]
    Helper(String),
}

/// Client side of one helper session. `on_frame` is the only way to move
/// between phases, so out-of-order frames are rejected instead of ignored.
#[derive(Debug)]
pub enum AdminPhase {
    /// pkexec is running; Polkit may be waiting for a password.
    Authorizing,
    Scanning,
    ReceivingTree(ScanTreeAssembler),
    Done(ScanTree),
}

fn protocol(error: impl ToString) -> AdminError {
    AdminError::Protocol(error.to_string())
}

impl AdminPhase {
    pub fn on_frame(
        self,
        message: AdminMessage,
    ) -> Result<(AdminPhase, Option<ScanSignal>), AdminError> {
        use AdminMessage as M;
        use AdminPhase as P;
        match (self, message) {
            (P::Done(_), _) => Err(protocol("message after the scan tree ended")),
            (_, M::Cancelled) => Err(AdminError::Cancelled),
            (_, M::Failed(message)) => Err(AdminError::Helper(message)),
            (P::Authorizing, M::Started) => Ok((P::Scanning, Some(ScanSignal::Started))),
            (P::Scanning, M::Progress(progress)) => {
                Ok((P::Scanning, Some(ScanSignal::Progress(progress))))
            }
            (P::Scanning, M::Issue(issue)) => Ok((P::Scanning, Some(ScanSignal::Issue(issue)))),
            (
                P::Scanning,
                M::TreeStart {
                    root_id,
                    summary,
                    node_count,
                    issue_count,
                },
            ) => {
                let mut assembler = ScanTreeAssembler::default();
                assembler
                    .start(root_id, summary, node_count, issue_count)
                    .map_err(protocol)?;
                Ok((P::ReceivingTree(assembler), None))
            }
            (P::ReceivingTree(mut assembler), M::TreeNodes(nodes)) => {
                assembler.push_nodes(nodes).map_err(protocol)?;
                Ok((P::ReceivingTree(assembler), None))
            }
            (P::ReceivingTree(mut assembler), M::TreeIssues(issues)) => {
                assembler.push_issues(issues).map_err(protocol)?;
                Ok((P::ReceivingTree(assembler), None))
            }
            (P::ReceivingTree(mut assembler), M::TreeEnd) => {
                Ok((P::Done(assembler.finish().map_err(protocol)?), None))
            }
            (P::Authorizing, _) => Err(protocol("scan data arrived before the helper started")),
            (P::Scanning, _) => Err(protocol("unexpected message while scanning")),
            (P::ReceivingTree(_), _) => {
                Err(protocol("unexpected message while receiving the scan tree"))
            }
        }
    }

    /// Maps the end of the helper's output to an error. `exit_code` is None
    /// when the process was killed by a signal.
    pub fn on_eof(&self, exit_code: Option<i32>, stderr: &str) -> AdminError {
        let detail = stderr.trim();
        match (self, exit_code) {
            (AdminPhase::Authorizing, Some(126)) => AdminError::AuthCancelled,
            (AdminPhase::Authorizing, Some(127)) => AdminError::AuthDenied,
            (AdminPhase::Done(_), _) => protocol("unexpected end after a complete tree"),
            (phase, code) => {
                let status = code.map_or_else(
                    || "Administrator helper stopped unexpectedly".to_string(),
                    |code| format!("Administrator helper exited with status {code}"),
                );
                let stage = match phase {
                    AdminPhase::ReceivingTree(_) => " before the scan tree was complete",
                    _ => "",
                };
                if detail.is_empty() {
                    AdminError::Helper(format!("{status}{stage}"))
                } else {
                    AdminError::Helper(format!("{status}{stage}: {detail}"))
                }
            }
        }
    }
}

fn helper_path() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        if let Ok(path) = env::var("LISCAN_ADMIN_HELPER") {
            return PathBuf::from(path);
        }
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

/// Keeps the last few KiB of the helper's stderr so a crash can be explained.
fn drain_stderr(mut stderr: impl Read + Send + 'static) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        const KEEP: usize = 4096;
        let mut tail = Vec::new();
        let mut buffer = [0_u8; 4096];
        while let Ok(read) = stderr.read(&mut buffer) {
            if read == 0 {
                break;
            }
            tail.extend_from_slice(&buffer[..read]);
            if tail.len() > KEEP {
                tail.drain(..tail.len() - KEEP);
            }
        }
        String::from_utf8_lossy(&tail).into_owned()
    })
}

pub fn run_admin_scan<F>(
    request: ScanRequest,
    cancel: &CancelToken,
    mut emit: F,
) -> Result<ScanTree, AdminError>
where
    F: FnMut(ScanSignal),
{
    let helper = helper_path();
    if !helper.exists() {
        return Err(AdminError::HelperMissing(helper.display().to_string()));
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
        .map_err(|error| {
            AdminError::Helper(format!(
                "Could not start administrator authentication: {error}"
            ))
        })?;
    let stderr = child.stderr.take().map(drain_stderr);
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AdminError::Helper("Administrator helper has no input pipe".into()))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| AdminError::Helper("Administrator helper has no output pipe".into()))?;
    write_frame(
        &mut stdin,
        &AdminControl::Start {
            version: PROTOCOL_VERSION,
            request,
        },
    )
    .map_err(|error| {
        AdminError::Helper(format!("Could not send scan request to helper: {error}"))
    })?;

    let child = Arc::new(Mutex::new(child));
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let control = {
        let (child, started, finished, cancel) = (
            child.clone(),
            started.clone(),
            finished.clone(),
            cancel.clone(),
        );
        thread::spawn(move || {
            while !finished.load(Ordering::Relaxed) {
                if cancel.is_cancelled() {
                    if started.load(Ordering::Relaxed) {
                        let _ = write_frame(&mut stdin, &AdminControl::Cancel);
                    } else if let Ok(mut child) = child.lock() {
                        // Still in Polkit: stopping pkexec withdraws the authentication request.
                        let _ = child.kill();
                    }
                    return;
                }
                thread::sleep(Duration::from_millis(40));
            }
        })
    };

    let mut phase = AdminPhase::Authorizing;
    let outcome = loop {
        let message = match read_frame::<AdminMessage>(&mut stdout) {
            Ok(message) => message,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break Ok(Err(phase)),
            Err(error) => break Err(protocol(error)),
        };
        match phase.on_frame(message) {
            Ok((AdminPhase::Done(tree), _)) => break Ok(Ok(tree)),
            Ok((next, signal)) => {
                if matches!(next, AdminPhase::Scanning) {
                    started.store(true, Ordering::Relaxed);
                }
                if let Some(signal) = signal {
                    emit(signal);
                }
                phase = next;
            }
            Err(error) => break Err(error),
        }
    };

    finished.store(true, Ordering::Relaxed);
    let _ = control.join();
    if outcome.is_err() {
        // A protocol error leaves the helper running; do not wait on it forever.
        let _ = child.lock().map(|mut child| child.kill());
    }
    let status = child
        .lock()
        .map_err(|_| AdminError::Helper("Administrator helper state is unavailable".into()))?
        .wait()
        .map_err(|error| {
            AdminError::Helper(format!("Could not wait for administrator helper: {error}"))
        })?;
    let stderr = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    match outcome {
        Ok(Ok(tree)) => Ok(tree),
        Err(error) => Err(error),
        Ok(Err(_)) if cancel.is_cancelled() => Err(AdminError::Cancelled),
        Ok(Err(phase)) => Err(phase.on_eof(status.code(), &stderr)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{IssueKind, NodeKind, ScanIssue, ScanNode, ScanProgress, ScanSummary};
    use crate::protocol::write_scan_tree;
    use std::io::Cursor;

    fn node(id: u64, parent_id: Option<u64>) -> ScanNode {
        ScanNode {
            id,
            parent_id,
            name: format!("n{id}"),
            display_path: format!("/n{id}"),
            uri: format!("file:///n{id}"),
            local_path: Some(PathBuf::from(format!("/n{id}"))),
            kind: NodeKind::Directory,
            allocated_bytes: 1,
            apparent_bytes: 1,
            children: Vec::new(),
            file_count: 0,
            directory_count: 1,
            modified_ms: None,
            permissions: None,
            hard_links: 1,
            flags: Vec::new(),
        }
    }

    fn tree() -> ScanTree {
        ScanTree {
            root_id: 0,
            nodes: vec![node(0, None), node(1, Some(0))],
            issues: Vec::new(),
            summary: ScanSummary::default(),
        }
    }

    fn progress() -> AdminMessage {
        AdminMessage::Progress(ScanProgress {
            files_scanned: 1,
            directories_scanned: 1,
            bytes_scanned: 1,
            current_path: "/".into(),
            issues: 0,
        })
    }

    fn issue() -> AdminMessage {
        AdminMessage::Issue(ScanIssue {
            path: "/x".into(),
            kind: IssueKind::Io,
            message: "x".into(),
        })
    }

    fn tree_start(node_count: u64) -> AdminMessage {
        AdminMessage::TreeStart {
            root_id: 0,
            summary: ScanSummary::default(),
            node_count,
            issue_count: 0,
        }
    }

    fn receiving() -> AdminPhase {
        AdminPhase::Scanning.on_frame(tree_start(2)).unwrap().0
    }

    fn phase_name(phase: &AdminPhase) -> &'static str {
        match phase {
            AdminPhase::Authorizing => "authorizing",
            AdminPhase::Scanning => "scanning",
            AdminPhase::ReceivingTree(_) => "receiving",
            AdminPhase::Done(_) => "done",
        }
    }

    /// Every message kind, built fresh for each use.
    fn all_messages() -> Vec<(&'static str, AdminMessage)> {
        vec![
            ("started", AdminMessage::Started),
            ("progress", progress()),
            ("issue", issue()),
            ("tree_start", tree_start(2)),
            ("tree_nodes", AdminMessage::TreeNodes(vec![node(0, None)])),
            ("tree_issues", AdminMessage::TreeIssues(Vec::new())),
            ("tree_end", AdminMessage::TreeEnd),
            ("cancelled", AdminMessage::Cancelled),
            ("failed", AdminMessage::Failed("boom".into())),
        ]
    }

    type Row = (&'static str, Result<&'static str, ()>);

    /// The full transition table: (phase, message) -> next phase name or error.
    #[test]
    fn on_frame_transition_table_is_exhaustive() {
        let expected: &[(&str, &[Row])] = &[
            (
                "authorizing",
                &[
                    ("started", Ok("scanning")),
                    ("progress", Err(())),
                    ("issue", Err(())),
                    ("tree_start", Err(())),
                    ("tree_nodes", Err(())),
                    ("tree_issues", Err(())),
                    ("tree_end", Err(())),
                    ("cancelled", Err(())),
                    ("failed", Err(())),
                ],
            ),
            (
                "scanning",
                &[
                    ("started", Err(())),
                    ("progress", Ok("scanning")),
                    ("issue", Ok("scanning")),
                    ("tree_start", Ok("receiving")),
                    ("tree_nodes", Err(())),
                    ("tree_issues", Err(())),
                    ("tree_end", Err(())),
                    ("cancelled", Err(())),
                    ("failed", Err(())),
                ],
            ),
            (
                "receiving",
                &[
                    ("started", Err(())),
                    ("progress", Err(())),
                    ("issue", Err(())),
                    ("tree_start", Err(())),
                    ("tree_nodes", Ok("receiving")),
                    ("tree_issues", Ok("receiving")),
                    // Ends before both declared nodes arrived.
                    ("tree_end", Err(())),
                    ("cancelled", Err(())),
                    ("failed", Err(())),
                ],
            ),
            (
                "done",
                &[
                    ("started", Err(())),
                    ("progress", Err(())),
                    ("issue", Err(())),
                    ("tree_start", Err(())),
                    ("tree_nodes", Err(())),
                    ("tree_issues", Err(())),
                    ("tree_end", Err(())),
                    ("cancelled", Err(())),
                    ("failed", Err(())),
                ],
            ),
        ];
        let make = |name: &str| match name {
            "authorizing" => AdminPhase::Authorizing,
            "scanning" => AdminPhase::Scanning,
            "receiving" => receiving(),
            _ => AdminPhase::Done(tree()),
        };
        for (phase, row) in expected {
            let messages = all_messages();
            assert_eq!(row.len(), messages.len());
            for ((message_name, message), (expected_name, expected)) in
                messages.into_iter().zip(*row)
            {
                assert_eq!(message_name, *expected_name);
                let result = make(phase).on_frame(message);
                match (result, expected) {
                    (Ok((next, _)), Ok(name)) => assert_eq!(phase_name(&next), *name),
                    (Err(_), Err(())) => {}
                    (result, expected) => panic!(
                        "{phase} + {message_name}: got {:?}, expected {expected:?}",
                        result.map(|(next, _)| phase_name(&next))
                    ),
                }
            }
        }
    }

    #[test]
    fn terminal_messages_map_to_typed_errors() {
        assert_eq!(
            AdminPhase::Scanning
                .on_frame(AdminMessage::Cancelled)
                .unwrap_err(),
            AdminError::Cancelled
        );
        assert_eq!(
            AdminPhase::Authorizing
                .on_frame(AdminMessage::Failed(
                    "unsupported helper protocol version 2".into()
                ))
                .unwrap_err(),
            AdminError::Helper("unsupported helper protocol version 2".into())
        );
    }

    #[test]
    fn signals_are_forwarded_in_order() {
        let (phase, signal) = AdminPhase::Authorizing
            .on_frame(AdminMessage::Started)
            .unwrap();
        assert!(matches!(signal, Some(ScanSignal::Started)));
        let (phase, signal) = phase.on_frame(progress()).unwrap();
        assert!(matches!(signal, Some(ScanSignal::Progress(_))));
        let (_, signal) = phase.on_frame(issue()).unwrap();
        assert!(matches!(signal, Some(ScanSignal::Issue(_))));
    }

    #[test]
    fn a_complete_session_from_the_wire_yields_the_tree() {
        let mut wire = Vec::new();
        write_frame(&mut wire, &AdminMessage::Started).unwrap();
        write_frame(&mut wire, &progress()).unwrap();
        write_scan_tree(&mut wire, tree()).unwrap();
        let mut reader = Cursor::new(wire);
        let mut phase = AdminPhase::Authorizing;
        let tree = loop {
            let message = read_frame::<AdminMessage>(&mut reader).unwrap();
            match phase.on_frame(message).unwrap() {
                (AdminPhase::Done(tree), _) => break tree,
                (next, _) => phase = next,
            }
        };
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[0].children, vec![1]);
    }

    #[test]
    fn end_of_output_is_explained_by_phase_and_exit_code() {
        assert_eq!(
            AdminPhase::Authorizing.on_eof(Some(126), ""),
            AdminError::AuthCancelled
        );
        assert_eq!(
            AdminPhase::Authorizing.on_eof(Some(127), ""),
            AdminError::AuthDenied
        );
        assert_eq!(
            AdminPhase::Scanning.on_eof(Some(101), "thread 'main' panicked\n"),
            AdminError::Helper(
                "Administrator helper exited with status 101: thread 'main' panicked".into()
            )
        );
        assert!(matches!(
            receiving().on_eof(Some(0), ""),
            AdminError::Helper(message) if message.contains("before the scan tree was complete")
        ));
        assert!(matches!(
            AdminPhase::Authorizing.on_eof(None, ""),
            AdminError::Helper(message) if message.contains("stopped unexpectedly")
        ));
    }
}
