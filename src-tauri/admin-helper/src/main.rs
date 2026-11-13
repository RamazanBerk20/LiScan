use liscan_lib::ScanRequest;
use liscan_lib::protocol::{
    AdminControl, AdminMessage, PROTOCOL_VERSION, read_frame, write_frame, write_scan_tree,
};
use liscan_lib::scanner::{CancelToken, ScanSignal, scan_target};
use std::io;
use std::path::Path;
use std::thread;

/// Always excluded, in addition to every pseudo filesystem the scanner skips by type.
const SPECIAL_PATHS: [&str; 4] = ["/proc", "/sys", "/dev", "/run"];

fn main() {
    if let Err(error) = run() {
        let _ = write_frame(
            &mut io::stdout().lock(),
            &AdminMessage::Failed(error.to_string()),
        );
        std::process::exit(1);
    }
}

/// Checks a request from the unprivileged client and applies the fixed policy.
fn validate_request(version: u16, mut request: ScanRequest) -> Result<ScanRequest, String> {
    if version != PROTOCOL_VERSION {
        return Err(format!("unsupported helper protocol version {version}"));
    }
    if request.mode != liscan_lib::ScanMode::Administrator {
        return Err("helper accepts administrator scans only".into());
    }
    if request.target.contains("://") || !Path::new(&request.target).is_absolute() {
        return Err("helper accepts one absolute local path only".into());
    }
    // The canonical form has no "..", "." or symbolic link, so path exclusions
    // such as /proc cannot be sidestepped with "/../proc" or a link to it.
    let canonical = std::fs::canonicalize(&request.target)
        .map_err(|error| format!("target cannot be read: {error}"))?;
    if canonical != Path::new(&request.target) {
        return Err(format!(
            "target must be a canonical path without symbolic links or \"..\" (use {})",
            canonical.display()
        ));
    }
    // Equal by components; keep the exact spelling the walk will build on.
    request.target = canonical.to_string_lossy().into_owned();
    if request.options.exclusions.len() > 256
        || request
            .options
            .exclusions
            .iter()
            .any(|value| value.len() > 4096)
    {
        return Err("scan options exceed safe protocol bounds".into());
    }
    for special in SPECIAL_PATHS {
        if !request
            .options
            .exclusions
            .iter()
            .any(|value| value == special)
        {
            request.options.exclusions.push(special.into());
        }
    }
    request.options.include_remote_mounts = false;
    Ok(request)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let direct_debug = cfg!(debug_assertions)
        && std::env::var("LISCAN_ADMIN_DIRECT")
            .ok()
            .is_some_and(|value| value == "1");
    if unsafe { libc::geteuid() } != 0 && !direct_debug {
        return Err("administrator helper must be launched through Polkit".into());
    }

    unsafe {
        libc::umask(0o077);
        libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
    }

    let mut input = io::stdin();
    let request = match read_frame::<AdminControl>(&mut input)? {
        AdminControl::Start { version, request } => validate_request(version, request)?,
        AdminControl::Cancel => return Ok(()),
    };

    let cancel = CancelToken::default();
    let control_cancel = cancel.clone();
    thread::spawn(move || {
        loop {
            match read_frame::<AdminControl>(&mut input) {
                Ok(AdminControl::Cancel) | Err(_) => {
                    control_cancel.cancel();
                    break;
                }
                Ok(AdminControl::Start { .. }) => {}
            }
        }
    });

    let mut output = io::stdout().lock();
    write_frame(&mut output, &AdminMessage::Started)?;
    let result = scan_target(&request, &cancel, |signal| {
        let message = match signal {
            ScanSignal::Started => AdminMessage::Started,
            ScanSignal::Progress(progress) => AdminMessage::Progress(progress),
            ScanSignal::Issue(issue) => AdminMessage::Issue(issue),
        };
        if write_frame(&mut output, &message).is_err() {
            cancel.cancel();
        }
    });

    match result {
        Ok(tree) => write_scan_tree(&mut output, tree)?,
        Err(_error) if cancel.is_cancelled() => write_frame(&mut output, &AdminMessage::Cancelled)?,
        Err(error) => write_frame(&mut output, &AdminMessage::Failed(error.to_string()))?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use liscan_lib::{ScanMode, ScanOptions};
    use std::io::Cursor;

    fn request(target: &str) -> ScanRequest {
        ScanRequest {
            target: target.into(),
            mode: ScanMode::Administrator,
            options: ScanOptions {
                exclusions: Vec::new(),
                include_remote_mounts: true,
                ..ScanOptions::default()
            },
        }
    }

    #[test]
    fn control_frames_round_trip() {
        let mut wire = Vec::new();
        write_frame(
            &mut wire,
            &AdminControl::Start {
                version: PROTOCOL_VERSION,
                request: request("/var"),
            },
        )
        .unwrap();
        write_frame(&mut wire, &AdminControl::Cancel).unwrap();
        let mut reader = Cursor::new(wire);
        match read_frame::<AdminControl>(&mut reader).unwrap() {
            AdminControl::Start { version, request } => {
                assert_eq!(version, PROTOCOL_VERSION);
                assert_eq!(request.target, "/var");
            }
            other => panic!("unexpected frame {other:?}"),
        }
        assert!(matches!(
            read_frame::<AdminControl>(&mut reader).unwrap(),
            AdminControl::Cancel
        ));
        assert_eq!(
            read_frame::<AdminControl>(&mut reader).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn proc_like_targets_cannot_sidestep_the_exclusions() {
        // Each spelling is either refused or normalised onto an excluded path.
        for target in [
            "/../proc",
            "/./proc",
            "/tmp/../proc",
            "/../sys/kernel",
            "//proc",
        ] {
            if let Ok(accepted) = validate_request(PROTOCOL_VERSION, request(target)) {
                let walked = Path::new(&accepted.target);
                assert!(
                    !accepted.target.contains("..")
                        && accepted
                            .options
                            .exclusions
                            .iter()
                            .any(|excluded| walked.starts_with(excluded)),
                    "{target} escaped the exclusions as {}",
                    accepted.target
                );
            }
        }
        let link = std::env::temp_dir().join(format!("liscan-helper-{}", std::process::id()));
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink("/proc", &link).unwrap();
        let result = validate_request(PROTOCOL_VERSION, request(link.to_str().unwrap()));
        std::fs::remove_file(&link).unwrap();
        assert!(result.is_err(), "a symlink to /proc was accepted");
    }

    #[test]
    fn accepted_targets_always_carry_the_fixed_policy() {
        let accepted = validate_request(PROTOCOL_VERSION, request("/")).unwrap();
        for special in SPECIAL_PATHS {
            assert!(
                accepted
                    .options
                    .exclusions
                    .iter()
                    .any(|value| value == special)
            );
        }
        assert!(!accepted.options.include_remote_mounts);
    }

    #[test]
    fn other_requests_are_rejected() {
        assert!(validate_request(PROTOCOL_VERSION - 1, request("/")).is_err());
        assert!(validate_request(PROTOCOL_VERSION, request("relative/path")).is_err());
        assert!(validate_request(PROTOCOL_VERSION, request("sftp://host/")).is_err());
        let mut standard = request("/");
        standard.mode = ScanMode::Standard;
        assert!(validate_request(PROTOCOL_VERSION, standard).is_err());
    }
}
