use liscan_lib::protocol::{
    AdminControl, AdminMessage, PROTOCOL_VERSION, read_frame, write_frame, write_scan_tree,
};
use liscan_lib::scanner::{ScanSignal, scan_target};
use std::io;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;

fn main() {
    if let Err(error) = run() {
        let _ = write_frame(
            &mut io::stdout().lock(),
            &AdminMessage::Failed(error.to_string()),
        );
        std::process::exit(1);
    }
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
    let (version, mut request) = match read_frame::<AdminControl>(&mut input)? {
        AdminControl::Start { version, request } => (version, request),
        AdminControl::Cancel => return Ok(()),
    };
    if version != PROTOCOL_VERSION {
        return Err(format!("unsupported helper protocol version {version}").into());
    }
    if request.mode != liscan_lib::ScanMode::Administrator {
        return Err("helper accepts administrator scans only".into());
    }
    if request.target.contains("://") || !Path::new(&request.target).is_absolute() {
        return Err("helper accepts one absolute local path only".into());
    }
    if request.options.exclusions.len() > 256
        || request
            .options
            .exclusions
            .iter()
            .any(|value| value.len() > 4096)
    {
        return Err("scan options exceed safe protocol bounds".into());
    }
    for special in ["/proc", "/sys", "/dev", "/run"] {
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

    let cancel = Arc::new(AtomicBool::new(false));
    let control_cancel = cancel.clone();
    thread::spawn(move || {
        loop {
            match read_frame::<AdminControl>(&mut input) {
                Ok(AdminControl::Cancel) | Err(_) => {
                    control_cancel.store(true, Ordering::Relaxed);
                    break;
                }
                Ok(AdminControl::Start { .. }) => {}
            }
        }
    });

    let mut output = io::stdout().lock();
    write_frame(&mut output, &AdminMessage::Started)?;
    let result = scan_target(&request, cancel.clone(), |signal| {
        let message = match signal {
            ScanSignal::Progress(progress) => AdminMessage::Progress(progress),
            ScanSignal::Issue(issue) => AdminMessage::Issue(issue),
        };
        if write_frame(&mut output, &message).is_err() {
            cancel.store(true, Ordering::Relaxed);
        }
    });

    match result {
        Ok(tree) => write_scan_tree(&mut output, tree)?,
        Err(_error) if cancel.load(Ordering::Relaxed) => {
            write_frame(&mut output, &AdminMessage::Cancelled)?
        }
        Err(error) => write_frame(&mut output, &AdminMessage::Failed(error.to_string()))?,
    }
    Ok(())
}
