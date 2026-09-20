//! How to drive a bundled toolchain.
//!
//! A bundled compiler is not always usable by pointing at its driver and hoping.
//! A relocatable Linux GCC has to be told where its own sysroot and startup files
//! are, or it silently falls back to the host's — which is exactly the "please
//! install glibc-devel" failure this product exists to avoid.
//!
//! Rather than encode that per platform in Rust, each bundle ships a `bundle.toml`
//! describing how to drive it. Windows bundles need almost nothing; Linux bundles
//! need a sysroot. The application code is identical either way, which is the same
//! principle as probing flags at runtime instead of keeping `cfg!` tables.

use crate::error::{EfError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// The file a bundle ships beside its `bin/` directory.
pub const BUNDLE_FILE: &str = "bundle.toml";

/// Substituted with the bundle's absolute root at load time, so the bundle can be
/// installed anywhere — next to the executable, in Program Files, or unpacked into
/// a temporary directory by a test.
const ROOT_TOKEN: &str = "${ROOT}";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BundleDescriptor {
    /// Stable identity of this toolchain, e.g. `winlibs-gcc-16.2.0-ucrt`.
    /// The application embeds the id it was built against so an update can tell
    /// whether the toolchain also has to change.
    pub id: String,
    pub version: String,
    /// Path to the driver, relative to the bundle root.
    pub gfortran: String,
    /// Prepended to every compile invocation.
    pub compile_flags: Vec<String>,
    /// Prepended to every link invocation.
    pub link_flags: Vec<String>,
    /// Extra environment for the child, merged over the scrubbed defaults.
    pub env: BTreeMap<String, String>,
    /// Directories prepended to the child's PATH, relative to the bundle root.
    pub path_dirs: Vec<String>,
    /// Suffix the *bundle's target* gives executables, e.g. `.exe`.
    ///
    /// Not the host's: MinGW's linker appends `.exe` whatever machine it runs on,
    /// so a Windows bundle driven from Linux writes `program.exe` while the host
    /// suffix is empty. Defaults to the host's, which is right whenever the
    /// bundle targets the machine it runs on.
    pub exe_suffix: Option<String>,
    /// Program used to run the bundle's binaries, e.g. `wine`.
    ///
    /// Absent in a shipped bundle. Set when driving a bundle built for another
    /// platform, which is how the Windows bundle is exercised from Linux.
    pub launcher: Option<String>,
}

impl Default for BundleDescriptor {
    fn default() -> Self {
        Self {
            id: String::new(),
            version: String::new(),
            gfortran: default_gfortran_path(),
            compile_flags: Vec::new(),
            link_flags: Vec::new(),
            env: BTreeMap::new(),
            path_dirs: vec!["bin".to_string()],
            exe_suffix: None,
            launcher: None,
        }
    }
}

fn default_gfortran_path() -> String {
    format!("bin/gfortran{}", std::env::consts::EXE_SUFFIX)
}

/// A descriptor with every `${ROOT}` resolved against a real directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub root: PathBuf,
    pub id: String,
    pub version: String,
    pub gfortran: PathBuf,
    pub compile_flags: Vec<OsString>,
    pub link_flags: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub path_dirs: Vec<PathBuf>,
    pub exe_suffix: Option<String>,
    pub launcher: Option<String>,
}

impl Bundle {
    /// Load `bundle.toml` from a bundle root. `Ok(None)` means there is no
    /// descriptor, which is fine: a plain `bin/gfortran` bundle needs no
    /// instructions.
    pub fn load(root: &Path) -> Result<Option<Self>> {
        let file = root.join(BUNDLE_FILE);
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(EfError::io(&file, e)),
        };
        let desc: BundleDescriptor =
            toml::from_str(&text).map_err(|e| EfError::ToolchainInvalid {
                path: file.clone(),
                reason: format!("{BUNDLE_FILE} is malformed: {e}"),
            })?;
        Ok(Some(Self::resolve(root, desc)?))
    }

    pub fn resolve(root: &Path, d: BundleDescriptor) -> Result<Self> {
        // The root must be absolute: flags and the driver path are handed to a
        // child whose working directory is the build tree, not ours, so a
        // relative root would resolve against the wrong place -- and a bad
        // `--sysroot` makes GCC fall back to the host's, silently.
        let root = absolutize(root);
        let sub = |s: &str| -> String { s.replace(ROOT_TOKEN, &root.to_string_lossy()) };

        let gfortran = join_inside(&root, &sub(&d.gfortran), "gfortran")?;
        let mut path_dirs = Vec::new();
        for p in &d.path_dirs {
            path_dirs.push(join_inside(&root, &sub(p), "path_dirs")?);
        }

        Ok(Self {
            id: d.id,
            version: d.version,
            gfortran,
            compile_flags: d.compile_flags.iter().map(|f| sub(f).into()).collect(),
            link_flags: d.link_flags.iter().map(|f| sub(f).into()).collect(),
            env: d
                .env
                .iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(sub(v))))
                .collect(),
            path_dirs,
            exe_suffix: d.exe_suffix,
            // Resolved to an absolute path now, because the child's PATH is
            // scrubbed down to the bundle's own directories: a bare `wine` would
            // not be found there, and every probe would fail for a reason that
            // looks nothing like the cause.
            launcher: d.launcher.map(|l| resolve_launcher(&l)),
            root,
        })
    }
}

/// Find a launcher on the *parent's* PATH, keeping the name if it cannot be
/// found so the failure names the program the descriptor asked for.
fn resolve_launcher(name: &str) -> String {
    if Path::new(name).is_absolute() {
        return name.to_string();
    }
    which::which(name)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| name.to_string())
}

/// Make a path absolute without requiring it to exist, resolving symlinks where
/// the path is real.
fn absolutize(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    dunce::canonicalize(&abs).unwrap_or(abs)
}

/// Resolve a descriptor-supplied path against the bundle root, refusing anything
/// that ends up outside it.
///
/// The check is on the *resolved* path, not on how it was written: `${ROOT}/bin`
/// is fine even though it substitutes to an absolute path, while `/usr/bin` and
/// `../../usr/bin` are not. A descriptor is our own data, but it is still data,
/// and it must not be able to aim the "bundled" toolchain at the host's
/// compiler — the single outcome this whole mechanism exists to rule out.
fn join_inside(root: &Path, resolved: &str, field: &str) -> Result<PathBuf> {
    let bad = |reason: String| EfError::ToolchainInvalid {
        path: root.to_path_buf(),
        reason: format!("{BUNDLE_FILE}: `{field}` {reason}"),
    };
    if resolved.trim().is_empty() {
        return Err(bad("is empty".into()));
    }
    let p = Path::new(resolved);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    };
    let norm = lexical_normalize(&joined);
    if !norm.starts_with(root) {
        return Err(bad(format!(
            "must stay inside the bundle, got `{resolved}`"
        )));
    }
    Ok(norm)
}

/// Resolve `.` and `..` without touching the filesystem, so the containment check
/// works for paths that do not exist yet.
fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_descriptor_is_not_an_error() {
        let td = tempfile::tempdir().unwrap();
        assert_eq!(Bundle::load(td.path()).unwrap(), None);
    }

    #[test]
    fn root_is_substituted_everywhere_it_appears() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::write(
            root.join(BUNDLE_FILE),
            r#"
id = "linux-gcc-16-sysroot"
version = "16.2.0"
gfortran = "bin/gfortran"
compile_flags = ["--sysroot=${ROOT}/sysroot"]
link_flags = ["--sysroot=${ROOT}/sysroot", "-B${ROOT}/lib/gcc/x86_64/16"]
path_dirs = ["bin", "libexec"]

[env]
GCC_EXEC_PREFIX = "${ROOT}/lib/gcc/"
"#,
        )
        .unwrap();

        let b = Bundle::load(root).unwrap().unwrap();
        let root = &b.root; // canonicalized
        assert_eq!(b.id, "linux-gcc-16-sysroot");
        assert_eq!(b.gfortran, root.join("bin/gfortran"));
        assert_eq!(
            b.compile_flags[0],
            OsString::from(format!("--sysroot={}/sysroot", root.display()))
        );
        assert_eq!(
            b.link_flags[1],
            OsString::from(format!("-B{}/lib/gcc/x86_64/16", root.display()))
        );
        assert_eq!(
            b.env[0].1,
            OsString::from(format!("{}/lib/gcc/", root.display()))
        );
        assert_eq!(b.path_dirs, vec![root.join("bin"), root.join("libexec")]);
        // No token may survive into anything we hand a compiler.
        for f in b.compile_flags.iter().chain(b.link_flags.iter()) {
            assert!(!f.to_string_lossy().contains(ROOT_TOKEN));
        }
    }

    /// An absolute path on whichever platform the test is running on.
    ///
    /// `/opt/a` is absolute on Unix and *relative* on Windows, where it has no
    /// drive letter — so `absolutize` prepended the working directory and the
    /// substitution assertions compared against a path that was never produced.
    fn abs(tail: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!("C:\\{}", tail.replace('/', "\\")))
        } else {
            PathBuf::from(format!("/{tail}"))
        }
    }

    #[test]
    fn a_bundle_is_relocatable_because_flags_resolve_at_load_time() {
        let d = BundleDescriptor {
            id: "x".into(),
            compile_flags: vec!["--sysroot=${ROOT}/sysroot".into()],
            ..Default::default()
        };
        let (ra, rb) = (abs("opt/a"), abs("home/user/.local/b"));
        let a = Bundle::resolve(&ra, d.clone()).unwrap();
        let b = Bundle::resolve(&rb, d).unwrap();
        assert_ne!(a.compile_flags, b.compile_flags);
        for (bundle, root) in [(&a, &ra), (&b, &rb)] {
            let want = format!("--sysroot={}", root.display());
            assert!(
                bundle.compile_flags[0].to_string_lossy().starts_with(&want),
                "expected {want:?}, got {:?}",
                bundle.compile_flags[0]
            );
        }
    }

    #[test]
    fn a_windows_style_bundle_needs_no_flags_at_all() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join(BUNDLE_FILE),
            "id = \"winlibs-gcc-16.2.0-ucrt\"\nversion = \"16.2.0\"\n",
        )
        .unwrap();
        let b = Bundle::load(td.path()).unwrap().unwrap();
        assert!(b.compile_flags.is_empty());
        assert!(b.link_flags.is_empty());
        // Against the canonical root, because `resolve` canonicalizes: on Windows
        // a temp dir arrives as `C:\Users\RUNNER~1\...` and comes back with the
        // 8.3 name expanded, so comparing with the raw path compares two spellings
        // of the same directory.
        let root = dunce::canonicalize(td.path()).unwrap();
        assert_eq!(b.path_dirs, vec![root.join("bin")]);
        assert!(b.gfortran.ends_with(default_gfortran_path()));
    }

    #[test]
    fn the_root_token_is_substituted_in_paths_too_not_only_flags() {
        // Following the ${ROOT} convention in `path_dirs` must not produce a
        // literal `${ROOT}` directory that silently is not there.
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join(BUNDLE_FILE),
            "id = \"x\"\npath_dirs = [\"${ROOT}/bin\"]\n",
        )
        .unwrap();
        let b = Bundle::load(td.path()).unwrap().unwrap();
        assert_eq!(b.path_dirs, vec![b.root.join("bin")]);
        assert!(!b.path_dirs[0].to_string_lossy().contains(ROOT_TOKEN));
    }

    #[test]
    fn a_descriptor_cannot_point_outside_the_bundle() {
        // Otherwise a bundle could aim the "bundled" toolchain at the host
        // compiler, which is exactly what bundling exists to prevent.
        let td = tempfile::tempdir().unwrap();
        for bad in [
            "gfortran = \"/usr/bin/gfortran\"",
            "gfortran = \"../../../usr/bin/gfortran\"",
            "path_dirs = [\"/usr/bin\"]",
            "path_dirs = [\"../../usr/bin\"]",
            "gfortran = \"\"",
        ] {
            std::fs::write(td.path().join(BUNDLE_FILE), format!("id = \"x\"\n{bad}\n")).unwrap();
            let err = Bundle::load(td.path()).unwrap_err();
            assert!(
                matches!(err, EfError::ToolchainInvalid { .. }),
                "{bad} should have been refused"
            );
        }
    }

    #[test]
    fn a_relative_root_is_made_absolute() {
        // A relative root would resolve against the child's working directory,
        // which is the build tree, not ours.
        let b = Bundle::resolve(
            Path::new("some/relative/toolchain"),
            BundleDescriptor::default(),
        )
        .unwrap();
        assert!(b.root.is_absolute(), "root was {:?}", b.root);
        assert!(b.gfortran.is_absolute());
    }

    #[test]
    fn a_misspelled_key_is_an_error_rather_than_silently_ignored() {
        // `compile_flag` (singular) would otherwise leave compile_flags empty and
        // the compiler would quietly use the host's headers and libraries.
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join(BUNDLE_FILE),
            "id = \"x\"\ncompile_flag = [\"--sysroot=${ROOT}/sysroot\"]\n",
        )
        .unwrap();
        let err = Bundle::load(td.path()).unwrap_err();
        assert!(matches!(err, EfError::ToolchainInvalid { .. }));
    }

    #[test]
    fn a_bundle_may_declare_its_targets_suffix_and_a_launcher() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join(BUNDLE_FILE),
            "id = \"x\"\nexe_suffix = \".exe\"\nlauncher = \"wine\"\n",
        )
        .unwrap();
        let b = Bundle::load(td.path()).unwrap().unwrap();
        assert_eq!(b.exe_suffix.as_deref(), Some(".exe"));
        // Resolved against the parent's PATH, because the child's is scrubbed.
        let l = b.launcher.as_deref().unwrap();
        assert!(
            l == "wine" || Path::new(l).is_absolute(),
            "launcher should be absolute when findable, got {l:?}"
        );
        assert!(l.ends_with("wine"));
    }

    #[test]
    fn an_unfindable_launcher_keeps_its_name_so_the_error_says_what_was_wanted() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join(BUNDLE_FILE),
            "id = \"x\"\nlauncher = \"definitely-not-installed-xyz\"\n",
        )
        .unwrap();
        let b = Bundle::load(td.path()).unwrap().unwrap();
        assert_eq!(b.launcher.as_deref(), Some("definitely-not-installed-xyz"));
    }

    #[test]
    fn a_bundle_without_them_falls_back_to_the_host() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join(BUNDLE_FILE), "id = \"x\"\n").unwrap();
        let b = Bundle::load(td.path()).unwrap().unwrap();
        assert!(b.exe_suffix.is_none());
        assert!(b.launcher.is_none());
    }

    #[test]
    fn a_malformed_descriptor_is_reported_not_ignored() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join(BUNDLE_FILE), "id = [this is not toml").unwrap();
        let err = Bundle::load(td.path()).unwrap_err();
        assert!(matches!(err, EfError::ToolchainInvalid { .. }));
    }
}
