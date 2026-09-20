//! Shared helpers for the process-level tests.

use std::path::{Path, PathBuf};

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
