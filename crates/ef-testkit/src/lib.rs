//! Shared helpers for the process-level tests.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Spawn a just-written executable, tolerating a brief `ETXTBSY`.
///
/// Linux refuses to exec a file that any process still holds open for writing.
/// The suite writes a program and runs it immediately while other tests are
/// forking compilers on other threads, and the window between a concurrent
/// `fork` and its `exec` is enough to land in. Nothing is wrong with the file:
/// it is the same bytes a moment later.
///
/// Bounded, so a genuinely unrunnable program still fails the test rather than
/// hanging, and every other error is returned untouched on the first try.
pub fn spawn_tolerating_busy(cmd: &mut Command) -> std::io::Result<Output> {
    use std::io::ErrorKind::ExecutableFileBusy;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match cmd.output() {
            Err(e) if e.kind() == ExecutableFileBusy && std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            other => return other,
        }
    }
}

/// Hash every file under a directory, so a test can assert that a whole tree of
/// the user's sources is byte-for-byte unchanged after a build.
pub fn hash_tree(root: &Path) -> Vec<(PathBuf, String)> {
    use sha2::{Digest, Sha256};
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(bytes) = std::fs::read(&p) {
                out.push((p, format!("{:x}", Sha256::digest(&bytes))));
            }
        }
    }
    out.sort();
    out
}
