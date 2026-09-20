//! Running the compiler.
//!
//! The application does not run the user's programs — it builds them and hands
//! the user the executable. So the only child process here is the compiler, and the
//! only thing we need from it is its interleaved output and its exit code.

use crate::error::{EfError, Result};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Windows: do not flash a console window when a GUI process spawns a child.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const READ_CHUNK: usize = 8192;

/// Apply the platform flags every child of ours needs.
pub fn harden(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group, so a cancelled compile cannot be left attached
        // to our terminal.
        cmd.process_group(0);
    }
}

/// Run a command to completion, capturing stdout and stderr interleaved, with a
/// hard cap.
///
/// One OS pipe is shared by both streams so the order of the output matches the
/// order the compiler wrote it; two separate pipes give no ordering guarantee,
/// which puts error messages in the wrong place. The cap matters because a
/// runaway error cascade can emit hundreds of megabytes.
pub fn run_capture(
    mut cmd: Command,
    cap: usize,
    cancel: &AtomicBool,
) -> Result<(Option<i32>, String)> {
    let (mut reader, writer) = os_pipe::pipe().map_err(|e| EfError::io("<compiler>", e))?;
    let writer2 = writer
        .try_clone()
        .map_err(|e| EfError::io("<compiler>", e))?;
    cmd.stdout(writer2);
    cmd.stderr(writer);
    cmd.stdin(Stdio::null());
    harden(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| EfError::io("<compiler>", e))?;
    // Drop the Command so the parent's copies of the write end are closed.
    // Without this the reader never sees EOF and the build appears to hang.
    drop(cmd);

    let collected = Arc::new(Mutex::new(Vec::<u8>::new()));
    let sink = Arc::clone(&collected);
    let pump = std::thread::spawn(move || {
        let mut buf = [0u8; READ_CHUNK];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut g = sink.lock().unwrap();
                    if g.len() < cap {
                        let room = cap - g.len();
                        g.extend_from_slice(&buf[..n.min(room)]);
                    }
                }
            }
        }
    });

    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(std::time::Duration::from_micros(200)),
            Err(e) => return Err(EfError::io("<compiler>", e)),
        }
    };

    let _ = pump.join();
    let bytes = collected.lock().unwrap().clone();
    let truncated = bytes.len() >= cap;
    let mut text = String::from_utf8_lossy(&bytes).to_string();
    if truncated {
        text.push_str("\n… (output truncated)\n");
    }
    Ok((status.code(), text))
}
