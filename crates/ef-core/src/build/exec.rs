//! Running the compiler.
//!
//! The application does not run the user's programs — it builds them and hands
//! the user the executable. So the only child process here is the compiler, and the
//! only thing we need from it is its interleaved output and its exit code.

use crate::error::{EfError, Result};
use std::io::{ErrorKind, Read};
use std::process::{Child, Command, Stdio};
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
        // to our terminal — and so the whole group can be signalled at once,
        // which is what `stop` below depends on.
        cmd.process_group(0);
    }
}

/// Stop a compile and everything it started.
///
/// `Child::kill` is not enough, and the difference is the whole Stop button.
/// `gfortran` is a driver: it forks `f951`, `as` and `ld`, and they inherit the
/// write end of the pipe we are reading. Kill only the driver and those children
/// keep running and keep that pipe open, so the reader never sees EOF and the
/// build blocks until the compile would have finished by itself. Verified: a
/// grandchild held a pipe open for its full lifetime after its parent had exited.
///
/// So the whole tree goes. On unix `harden` already made the child a process
/// group leader, so its pid negated names the group. This shells out to `kill`
/// and `taskkill` rather than calling `killpg` or building a Job object because
/// both of those need `unsafe`, which this workspace forbids — and the tools are
/// part of both systems. `Command::arg` per argument, never a shell.
fn stop(child: &mut Child) {
    let pid = child.id();

    #[cfg(unix)]
    let mut killer = {
        let mut c = Command::new("kill");
        c.args(["-KILL", &format!("-{pid}")]);
        c
    };
    #[cfg(windows)]
    let mut killer = {
        let mut c = Command::new("taskkill");
        c.args(["/T", "/F", "/PID", &pid.to_string()]);
        c
    };

    killer
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    harden(&mut killer);
    let _ = killer.status();

    // Belt and braces: if that could not run at all, at least the driver dies.
    let _ = child.kill();
}

/// Run a command to completion, capturing stdout and stderr interleaved, with a
/// hard cap.
///
/// One OS pipe is shared by both streams so the order of the output matches the
/// order the compiler wrote it; two separate pipes give no ordering guarantee,
/// which puts error messages in the wrong place. The cap matters because a
/// runaway error cascade can emit hundreds of megabytes; `cap` is the room left
/// in the *build's* budget, not a fresh allowance per file.
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
    let dropped_any = Arc::new(AtomicBool::new(false));
    let sink = Arc::clone(&collected);
    let dropped = Arc::clone(&dropped_any);
    let pump = std::thread::spawn(move || {
        let mut buf = [0u8; READ_CHUNK];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                // A signal delivered to this thread mid-read is not end of
                // output. `read` is raw here, so std does not retry for us, and
                // treating it as EOF would silently discard the rest of the
                // compiler's errors.
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
                Ok(n) => {
                    let mut g = sink.lock().unwrap();
                    let room = cap.saturating_sub(g.len());
                    if n > room {
                        dropped.store(true, Ordering::Relaxed);
                    }
                    g.extend_from_slice(&buf[..n.min(room)]);
                }
            }
        }
    });

    let mut stopped = false;
    let status = loop {
        if !stopped && cancel.load(Ordering::Relaxed) {
            stop(&mut child);
            stopped = true;
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(e) => return Err(EfError::io("<compiler>", e)),
        }
    };

    // A panic in the pump must not become a panic here: this runs on the build
    // worker thread, and a panic there drops the channel with no `Finished`
    // event, leaving the interface showing a build that never resolves.
    let pump_failed = pump.join().is_err();
    let bytes = match Arc::try_unwrap(collected) {
        // The pump has been joined, so we are the only owner and the buffer can
        // be taken rather than copied — it may be megabytes.
        Ok(m) => m.into_inner().unwrap_or_else(|e| e.into_inner()),
        Err(arc) => arc.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    };

    // Valid UTF-8 is the overwhelmingly common case and costs no copy at all.
    let mut text = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    };

    if dropped_any.load(Ordering::Relaxed) {
        text.push_str("\n… (output truncated)\n");
    }
    if pump_failed {
        text.push_str("\n… (some compiler output could not be captured)\n");
    }

    let code = status.code();
    // `code()` is `None` when the process died from a signal. Without this the
    // caller sees an ordinary non-zero exit and the user gets "Build failed"
    // with no errors in it — the same screen as a compiler that simply crashed
    // or was killed for running out of memory. Cancelling is the one legitimate
    // way to get here, and says so for itself.
    #[cfg(unix)]
    if code.is_none() && !cancel.load(Ordering::Relaxed) {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            text.push_str(&format!(
                "\n… the compiler itself stopped unexpectedly (signal {sig})\n"
            ));
        }
    }

    Ok((code, text))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Cancelling must stop the whole tree, not just the process we spawned.
    ///
    /// This is the Stop button. A compiler driver forks children that inherit
    /// the pipe we read, so killing only the driver leaves them holding it open
    /// and the read blocks until they finish on their own. `sh -c 'sleep 30 &
    /// wait'` is that shape in miniature: kill only the `sh` and the `sleep`
    /// keeps the pipe open for its full thirty seconds.
    ///
    /// Unix only because it needs a shell that backgrounds a child; the
    /// mechanism it guards is the same on Windows, where `taskkill /T` does the
    /// same job.
    #[test]
    fn cancelling_kills_the_children_the_compiler_started() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30 & wait"]);

        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::SeqCst);
        });

        let start = Instant::now();
        let out = run_capture(cmd, 1024, &cancel);
        let took = start.elapsed();

        assert!(out.is_ok(), "should return, not error: {out:?}");
        assert!(
            took < Duration::from_secs(10),
            "cancelling took {took:?}: the grandchild still held the pipe, so \
             this is the hang the Stop button shows as a build that will not stop"
        );
    }
}
