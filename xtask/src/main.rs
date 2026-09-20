//! Repository chores.
//!
//! `check-hygiene` is the important one: it turns the project's two structural
//! safety rules into something CI enforces, rather than something reviewers have
//! to remember.

mod fetch;
mod glob;
mod icons;
mod package;

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check-hygiene") => check_hygiene(),
        Some("fetch-toolchain") => fetch::run(&args[1..]),
        Some("fetch-sources") => fetch::sources(&args[1..]),
        Some("package") => package::run(&args[1..]),
        Some("gen-icons") => icons::run(&args[1..]),
        Some("version") => {
            println!("{}", package::workspace_version(&repo_root())?);
            Ok(())
        }
        Some(other) => bail!("unknown task `{other}`\n\n{USAGE}"),
        None => bail!("{USAGE}"),
    }
}

const USAGE: &str = "\
USAGE:
    cargo xtask check-hygiene
    cargo xtask fetch-toolchain [--target <name>] [--out <dir>] [--offline]
    cargo xtask fetch-sources   [--target <name>] [--out <dir>] [--list]
    cargo xtask package         [--target <name>] [--profile <p>] [--no-archive]
    cargo xtask version
    cargo xtask gen-icons
";

struct Violation {
    file: PathBuf,
    line: usize,
    text: String,
    rule: &'static str,
}

/// Every translation key named in the code must exist in both catalogs.
///
/// `i18n::lookup` falls back to returning the key itself, so a mistyped or
/// never-added key does not fail, does not warn, and renders in the interface as
/// `libs.heading` — in front of a user who reads Vietnamese. That silent failure
/// is the whole reason this check exists.
///
/// Only literal keys can be checked. A key chosen at run time (a build error's
/// `reason_key`, say) is invisible here, which is why unused catalog entries are
/// reported as a note rather than a failure.
fn check_i18n_keys(root: &Path, v: &mut Vec<Violation>) -> Result<()> {
    let mut catalogs: Vec<(String, std::collections::BTreeSet<String>)> = Vec::new();
    for lang in ["vi", "en"] {
        let path = root.join(format!("crates/ef-core/assets/i18n/{lang}.toml"));
        let text = std::fs::read_to_string(&path)?;
        let parsed: toml::Value = text.parse()?;
        let keys = parsed
            .as_table()
            .map(|t| t.keys().cloned().collect())
            .unwrap_or_default();
        catalogs.push((lang.to_string(), keys));
    }

    let mut referenced: std::collections::BTreeSet<String> = Default::default();
    for file in rust_sources(root) {
        let rel = file.strip_prefix(root).unwrap_or(&file).to_path_buf();
        let text = blank_comments(&std::fs::read_to_string(&file)?);
        // Scanned over the whole file, not line by line: a `tr!(` call is often
        // wrapped across lines by rustfmt, and its key then sits on a line of its
        // own. Checking single lines silently skipped exactly those calls.
        for (offset, key) in literal_keys_in(&text) {
            referenced.insert(key.clone());
            let line = text[..offset].matches('\n').count() + 1;
            for (lang, keys) in &catalogs {
                if !keys.contains(&key) {
                    v.push(Violation {
                        file: rel.clone(),
                        line,
                        text: format!("key `{key}`"),
                        rule: match lang.as_str() {
                            "vi" => "translation key is missing from vi.toml",
                            _ => "translation key is missing from en.toml",
                        },
                    });
                }
            }
        }
    }

    let unused: Vec<&String> = catalogs[0]
        .1
        .iter()
        .filter(|k| !referenced.contains(*k))
        .collect();
    if !unused.is_empty() {
        eprintln!(
            "note: {} catalog key(s) never named literally in the code \
             (fine if chosen at run time, dead weight otherwise):",
            unused.len()
        );
        for k in unused {
            eprintln!("    {k}");
        }
    }
    Ok(())
}

/// Blank out `//` comments, keeping every byte offset and newline in place.
///
/// Without this the doc comment on the `tr!` macro — which spells out
/// `tr!(lang, "key")` — is read as a call site naming a key called `key`.
fn blank_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        match line.find("//") {
            Some(at) => {
                out.push_str(&line[..at]);
                for c in line[at..].chars() {
                    out.push(if c == '\n' { '\n' } else { ' ' });
                }
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Pull the literal key out of `tr!(lang, "k")`, `i18n::lookup(lang, "k")` and
/// `i18n::format(lang, "k", ..)`, anywhere in a file. Returns each key with the
/// byte offset it was found at, so a line number can be recovered.
///
/// A non-literal second argument — a run-time `reason_key`, say — yields nothing,
/// because there is no key to check at this point.
fn literal_keys_in(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for opener in ["tr!(", "i18n::lookup(", "i18n::format("] {
        let mut from = 0;
        while let Some(at) = text[from..].find(opener) {
            let start = from + at + opener.len();
            from = start;
            let Some(comma) = text[start..].find(',') else {
                continue;
            };
            let after = start + comma + 1;
            let rest = text[after..].trim_start();
            let Some(stripped) = rest.strip_prefix('"') else {
                continue;
            };
            if let Some(end) = stripped.find('"') {
                out.push((after, stripped[..end].to_string()));
            }
        }
    }
    out
}

fn check_hygiene() -> Result<()> {
    let root = repo_root();
    let mut v: Vec<Violation> = Vec::new();

    check_no_binaries_tracked(&root, &mut v)?;
    check_manifest_is_a_placeholder(&root, &mut v);
    check_i18n_keys(&root, &mut v)?;

    for file in rust_sources(&root) {
        let rel = file.strip_prefix(&root).unwrap_or(&file).to_path_buf();
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&file)?;

        let is_fs_guard = rel_str.ends_with("ef-core/src/fs_guard.rs");
        let is_test = rel_str.contains("/tests/")
            || rel_str.contains("ef-testkit/")
            || rel_str.contains("xtask/");

        let mut in_test_mod = false;
        let mut brace_depth_at_test = 0usize;
        let mut depth = 0usize;

        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            // Track `#[cfg(test)] mod tests` so unit tests may use anything.
            if line.starts_with("#[cfg(test)]") {
                in_test_mod = true;
                brace_depth_at_test = depth;
            }
            depth += raw.matches('{').count();
            depth = depth.saturating_sub(raw.matches('}').count());
            if in_test_mod && depth <= brace_depth_at_test && line.contains('}') {
                in_test_mod = false;
            }
            if line.starts_with("//") || line.starts_with("///") || line.starts_with("//!") {
                continue;
            }
            let exempt = is_test || in_test_mod;

            // Rule 1: only fs_guard may write, create or delete.
            if !is_fs_guard && !exempt {
                for pat in [
                    "File::create(",
                    "fs::write(",
                    "fs::remove_file(",
                    "fs::remove_dir_all(",
                    "fs::rename(",
                    "fs::create_dir_all(",
                    "OpenOptions::new(",
                ] {
                    if line.contains(pat) {
                        v.push(Violation {
                            file: rel.clone(),
                            line: i + 1,
                            text: line.to_string(),
                            rule: "only fs_guard.rs may create, write, rename or delete files",
                        });
                    }
                }
            }

            // Rule 2: never spawn through a shell.
            for pat in ["cmd /c", "cmd.exe", "\"sh\"", "sh -c", "/bin/sh"] {
                if line.contains(pat) && !exempt {
                    v.push(Violation {
                        file: rel.clone(),
                        line: i + 1,
                        text: line.to_string(),
                        rule: "never spawn a child through a shell; use Command::arg per argument",
                    });
                }
            }

            // Rule 3: the GUI holds no logic.
            if rel_str.contains("ef-gui/src/") && !exempt {
                for pat in ["std::process::Command", "std::fs::"] {
                    if line.contains(pat) {
                        v.push(Violation {
                            file: rel.clone(),
                            line: i + 1,
                            text: line.to_string(),
                            rule:
                                "ef-gui must not touch the filesystem or spawn processes directly",
                        });
                    }
                }
            }

            // Rule 4: flags that would break containment or usability.
            for (pat, why) in [
                (
                    "-save-temps",
                    "-save-temps writes intermediates beside the source",
                ),
                (
                    "-Werror",
                    "-Werror would make a warning block a working program from building",
                ),
            ] {
                if line.contains(pat) && !exempt && !rel_str.ends_with("xtask/src/main.rs") {
                    v.push(Violation {
                        file: rel.clone(),
                        line: i + 1,
                        text: line.to_string(),
                        rule: why,
                    });
                }
            }
        }
    }

    if v.is_empty() {
        println!("hygiene: ok");
        return Ok(());
    }
    for x in &v {
        // A tracked-file violation has no line to point at.
        if x.line == 0 {
            println!("{}: {}", x.file.display(), x.rule);
            if !x.text.is_empty() {
                println!("    {}", x.text);
            }
        } else {
            println!(
                "{}:{}: {}\n    {}",
                x.file.display(),
                x.line,
                x.rule,
                x.text
            );
        }
    }
    bail!("{} hygiene violation(s)", v.len())
}

/// The committed toolchain manifest must stay a placeholder.
///
/// A generated one describes exactly one fetched bundle. Committed, it travels
/// to everyone else, who then has a checkout that reports every file of their
/// bundle as missing and refuses to build. The fetch task overwrites this file
/// as a matter of course, so it is easy to commit by accident -- easy enough
/// that it happened once.
fn check_manifest_is_a_placeholder(root: &Path, v: &mut Vec<Violation>) {
    let rel = PathBuf::from("crates/ef-gui/assets/toolchain-manifest.txt");

    // Read what git has staged, not what is on disk. A generated manifest in the
    // working tree is the normal state in the middle of a release build -- the
    // fetch writes it and the build embeds it -- so checking the file would fail
    // during ordinary work, and a check that cries wolf gets switched off.
    // What must never happen is *committing* one.
    let staged = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", ":crates/ef-gui/assets/toolchain-manifest.txt"])
        .output();
    let text = match staged {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        // Not staged, or not a checkout: fall back to the file itself.
        _ => match std::fs::read_to_string(root.join(&rel)) {
            Ok(t) => t,
            Err(_) => return,
        },
    };
    let hashes = text
        .lines()
        .filter(|l| {
            let l = l.trim();
            l.len() > 64 && l[..64].bytes().all(|b| b.is_ascii_hexdigit())
        })
        .count();
    if hashes > 0 {
        v.push(Violation {
            text: format!(
                "{hashes} hashes staged; run `git checkout {}` before committing",
                rel.display()
            ),
            file: rel,
            line: 0,
            rule: "the committed toolchain manifest must be a placeholder, not a generated one",
        });
    }
}

/// The compiler must never enter git history.
///
/// `.gitignore` is a convenience that a `git add -f` or a stray `--out` can walk
/// straight past. This asks git what is actually tracked, so the guarantee holds
/// however the file got there. It also keeps the repository small enough to
/// clone quickly, which is the other half of why the toolchain is fetched rather
/// than committed.
fn check_no_binaries_tracked(root: &Path, v: &mut Vec<Violation>) -> Result<()> {
    const MAX_TRACKED_BYTES: u64 = 1_000_000;
    const BINARY_EXT: &[&str] = &[
        "conda", "exe", "dll", "so", "a", "o", "obj", "lib", "dylib", "ttf", "otf", "rpm", "7z",
    ];

    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output();
    let Ok(out) = out else {
        // Not a checkout, or no git. Nothing to check rather than a failure.
        return Ok(());
    };
    if !out.status.success() {
        return Ok(());
    }

    for name in String::from_utf8_lossy(&out.stdout).split('\0') {
        if name.is_empty() {
            continue;
        }
        let rel = PathBuf::from(name);
        let full = root.join(&rel);
        let ext = rel
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        if BINARY_EXT.contains(&ext.as_str()) {
            v.push(Violation {
                file: rel.clone(),
                line: 0,
                text: String::new(),
                rule: "binaries are never committed; the toolchain is fetched from its recipe",
            });
            continue;
        }
        if let Ok(md) = std::fs::metadata(&full) {
            if md.is_file() && md.len() > MAX_TRACKED_BYTES {
                v.push(Violation {
                    file: rel,
                    line: 0,
                    text: format!("{:.1} MB", md.len() as f64 / 1e6),
                    rule: "tracked files stay under 1 MB; large artefacts belong in a release",
                });
            }
        }
    }
    Ok(())
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.join("crates")];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}
