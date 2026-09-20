//! Locating and describing the Fortran compiler.
//!
//! One code path serves both cases: on Windows a pruned gfortran ships beside the
//! executable; on Linux and macOS (development and testing only) the system
//! gfortran is used. The difference is discovery and verification, not behaviour —
//! which flags are actually usable is discovered at runtime by `probe`, so there
//! are no `cfg!(windows)` flag tables anywhere in the build code.

pub mod bundle;
pub mod manifest;
pub mod probe;

use crate::error::{EfError, Result};
use crate::paths::WorkLayout;
use bundle::Bundle;
use probe::FlagCapabilities;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainKind {
    /// Shipped inside our own installation directory.
    Bundled,
    /// Found on PATH. Development and testing.
    System,
    /// Pointed at explicitly by settings or `EF77_TOOLCHAIN`.
    UserSpecified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainId {
    pub kind: ToolchainKind,
    pub version: String,
    pub path: PathBuf,
}

impl ToolchainId {
    /// A short label for the UI: "gfortran 16.2.0".
    pub fn display(&self) -> String {
        if self.version.is_empty() {
            crate::text::display_path(&self.path)
        } else {
            format!("gfortran {}", self.version)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Toolchain {
    id: ToolchainId,
    gfortran: PathBuf,
    caps: FlagCapabilities,
    /// Present only for a bundled toolchain, which may need to be told where its
    /// own sysroot and startup files live.
    bundle: Option<Bundle>,
}

impl Toolchain {
    pub fn id(&self) -> &ToolchainId {
        &self.id
    }
    pub fn gfortran(&self) -> &Path {
        &self.gfortran
    }
    pub fn capabilities(&self) -> &FlagCapabilities {
        &self.caps
    }
    /// The suffix executables get for the bundle's *target*.
    ///
    /// The host's suffix is only right when the bundle targets the host. A
    /// Windows bundle driven from Linux still produces `program.exe`, and looking
    /// for `program` would report a link failure on a build that worked.
    pub fn exe_suffix(&self) -> &str {
        self.bundle
            .as_ref()
            .and_then(|b| b.exe_suffix.as_deref())
            .unwrap_or(std::env::consts::EXE_SUFFIX)
    }

    /// Program that runs the bundle's binaries, when they are not native.
    pub fn launcher(&self) -> Option<&str> {
        self.bundle.as_ref().and_then(|b| b.launcher.as_deref())
    }

    /// A command that runs one of the bundle's binaries, through the launcher
    /// when there is one.
    pub fn run_binary(&self, exe: &Path) -> Command {
        match self.launcher() {
            Some(l) => {
                let mut c = Command::new(l);
                c.arg(exe);
                c
            }
            None => Command::new(exe),
        }
    }

    /// Flags the bundle requires on every compile, before anything else.
    pub fn compile_flags(&self) -> &[OsString] {
        self.bundle.as_ref().map_or(&[], |b| &b.compile_flags)
    }

    /// Flags the bundle requires on every link.
    pub fn link_flags(&self) -> &[OsString] {
        self.bundle.as_ref().map_or(&[], |b| &b.link_flags)
    }

    /// A command for the *compiler*, with a scrubbed environment.
    ///
    /// `env_clear` then an allowlist, so the user's own `LIBRARY_PATH`, `CPATH`,
    /// `C_INCLUDE_PATH`, `FPATH`, `GCC_EXEC_PREFIX` or `COMPILER_PATH` cannot
    /// inject search paths into the user's build.
    pub fn command(&self, layout: &WorkLayout) -> Command {
        let mut cmd = self.run_binary(&self.gfortran);
        cmd.env_clear();
        for (k, v) in self.compile_env(layout) {
            cmd.env(k, v);
        }
        cmd.current_dir(&layout.root);
        cmd
    }

    /// Check the bundled files against the manifest embedded in the application.
    ///
    /// A system toolchain is not ours and has nothing to attest to, so it always
    /// verifies; so does a bundle built before a manifest was generated, which is
    /// what a development checkout has.
    pub fn verify_integrity(&self, manifest: &manifest::Manifest) -> manifest::VerifyReport {
        match (&self.bundle, manifest.is_empty()) {
            (Some(b), false) => manifest.verify(&b.root),
            _ => manifest::VerifyReport::default(),
        }
    }

    /// Environment for the compiler.
    pub fn compile_env(&self, layout: &WorkLayout) -> Vec<(OsString, OsString)> {
        scrubbed_env(&self.gfortran, self.bundle.as_ref(), &layout.tmp())
    }

    /// Construct directly from a path, probing its capabilities. Used by discovery
    /// and by tests that inject a fake compiler.
    pub fn from_path(path: PathBuf, kind: ToolchainKind) -> Result<Self> {
        if !path.exists() {
            return Err(EfError::ToolchainInvalid {
                path: path.clone(),
                reason: "the file does not exist".into(),
            });
        }
        let version = query_version(&path, None).unwrap_or_default();
        let caps = probe::probe(&path, &[], &[], None);
        Ok(Self {
            id: ToolchainId {
                kind,
                version,
                path: path.clone(),
            },
            gfortran: path,
            caps,
            bundle: None,
        })
    }

    /// Load a bundled toolchain from its root directory, honouring its
    /// `bundle.toml` if it ships one.
    pub fn from_bundle(root: &Path) -> Result<Self> {
        let b = match Bundle::load(root)? {
            Some(b) => b,
            // No descriptor is fine: a plain `bin/gfortran` bundle needs none.
            None => Bundle::resolve(root, bundle::BundleDescriptor::default())?,
        };
        if !b.gfortran.is_file() {
            return Err(EfError::ToolchainInvalid {
                path: b.gfortran.clone(),
                reason: "the bundled compiler is missing (antivirus may have removed it)".into(),
            });
        }
        // Probe with the bundle's own flags, and crucially with its *link* flags
        // for the link probes: a relocatable Linux bundle needs `-B` for its crt
        // startup files, and without them the static-link probe fails, the static
        // flags are dropped, and the produced program needs libgfortran on the
        // user's machine -- the exact failure bundling exists to prevent.
        let caps = probe::probe(&b.gfortran, &b.compile_flags, &b.link_flags, Some(&b));
        let version = if b.version.is_empty() {
            query_version(&b.gfortran, b.launcher.as_deref()).unwrap_or_default()
        } else {
            b.version.clone()
        };
        Ok(Self {
            id: ToolchainId {
                kind: ToolchainKind::Bundled,
                version,
                path: b.gfortran.clone(),
            },
            gfortran: b.gfortran.clone(),
            caps,
            bundle: Some(b),
        })
    }
}

/// Variables that must never reach the compiler, because they inject search
/// paths into a build. They are absent by construction after `env_clear`, and a
/// bundle descriptor is not allowed to put them back.
pub const BANNED_ENV: &[&str] = &[
    "LIBRARY_PATH",
    "CPATH",
    "C_INCLUDE_PATH",
    "CPLUS_INCLUDE_PATH",
    "OBJC_INCLUDE_PATH",
    "FPATH",
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "GFORTRAN_STDIN_UNIT",
    "GFORTRAN_STDOUT_UNIT",
    "GFORTRAN_STDERR_UNIT",
];

/// Build the scrubbed compiler environment.
///
/// One function so real builds and the capability probe see the same
/// environment. A probe run in a different environment from the builds it
/// predicts is worse than no probe at all.
fn scrubbed_env(gfortran: &Path, bundle: Option<&Bundle>, tmp: &Path) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = Vec::new();

    let mut path = OsString::new();
    let push_dir = |p: &Path, path: &mut OsString| {
        path.push(p);
        path.push(PATH_SEP);
    };
    // The driver's own directory always comes first: a bundle that lists extra
    // directories must *add* to it, never replace it, or the driver cannot find
    // its own `as` and `ld`.
    if let Some(bin) = gfortran.parent() {
        push_dir(bin, &mut path);
    }
    if let Some(b) = bundle {
        for dir in &b.path_dirs {
            if Some(dir.as_path()) != gfortran.parent() {
                push_dir(dir, &mut path);
            }
        }
    }
    #[cfg(windows)]
    {
        let sysroot =
            std::env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\Windows"));
        let mut s32 = PathBuf::from(&sysroot);
        s32.push("System32");
        path.push(s32);
        env.push(("SystemRoot".into(), sysroot.clone()));
        env.push(("windir".into(), sysroot));
        env.push(("TMP".into(), tmp.as_os_str().to_os_string()));
        env.push(("TEMP".into(), tmp.as_os_str().to_os_string()));
    }
    #[cfg(not(windows))]
    {
        env.push(("LANG".into(), OsString::from("C.UTF-8")));
        env.push(("LC_ALL".into(), OsString::from("C.UTF-8")));
        if let Some(home) = std::env::var_os("HOME") {
            env.push(("HOME".into(), home));
        }
    }
    env.push(("PATH".into(), path));
    env.push(("TMPDIR".into(), tmp.as_os_str().to_os_string()));

    if let Some(b) = bundle {
        // Everything set above is ours. A bundle may add variables; it may never
        // override them, or it could undo the PATH scrubbing or the
        // temp-directory containment.
        let ours: Vec<String> = env
            .iter()
            .map(|(k, _)| k.to_string_lossy().to_string())
            .collect();
        for (k, v) in &b.env {
            let name = k.to_string_lossy().to_string();
            let banned = BANNED_ENV.iter().any(|b| name.eq_ignore_ascii_case(b));
            let ours_already = ours.iter().any(|o| o.eq_ignore_ascii_case(&name));
            if banned || ours_already {
                tracing::warn!("ignoring `{name}` from bundle.toml: it is not a bundle's to set");
                continue;
            }
            env.push((k.clone(), v.clone()));
        }
    }
    env
}

#[cfg(windows)]
const PATH_SEP: &str = ";";
#[cfg(not(windows))]
const PATH_SEP: &str = ":";

fn query_version(gfortran: &Path, launcher: Option<&str>) -> Option<String> {
    let mut cmd = match launcher {
        Some(l) => {
            let mut c = Command::new(l);
            c.arg(gfortran);
            c
        }
        None => Command::new(gfortran),
    };
    let out = cmd.arg("-dumpversion").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// Discovery order, first success wins.
///
/// 1. an explicit override from settings
/// 2. `EF77_TOOLCHAIN_BUNDLE` — a bundle root, for exercising the bundled path
/// 3. `EF77_TOOLCHAIN` — a bare driver, used by CI and by `cargo run`
/// 4. the bundled toolchain beside our own executable
/// 5. the system gfortran on PATH
pub fn discover(override_path: Option<&Path>) -> Result<Toolchain> {
    if let Some(p) = override_path {
        return Toolchain::from_path(p.to_path_buf(), ToolchainKind::UserSpecified);
    }
    // A bundle root, for testing the bundled path without installing anything.
    if let Some(root) = std::env::var_os("EF77_TOOLCHAIN_BUNDLE") {
        if !root.is_empty() {
            return Toolchain::from_bundle(Path::new(&root));
        }
    }
    if let Some(p) = std::env::var_os("EF77_TOOLCHAIN") {
        if !p.is_empty() {
            return Toolchain::from_path(PathBuf::from(p), ToolchainKind::UserSpecified);
        }
    }
    for root in bundled_roots() {
        if looks_like_a_bundle(&root) {
            match Toolchain::from_bundle(&root) {
                Ok(tc) => return Ok(tc),
                Err(e) => tracing::warn!("ignoring bundle at {}: {e}", root.display()),
            }
        }
    }
    for name in [
        "gfortran",
        "gfortran-16",
        "gfortran-15",
        "gfortran-14",
        "gfortran-13",
    ] {
        if let Ok(p) = which::which(name) {
            return Toolchain::from_path(p, ToolchainKind::System);
        }
    }
    Err(EfError::ToolchainMissing)
}

/// Was a toolchain bundle shipped beside this executable?
///
/// This, not the compile profile, is the question that decides what to tell the
/// user when no compiler is found: a build that shipped a bundle and cannot find
/// one is damaged, while a build that never had one simply needs a system
/// compiler installed. `cfg!(debug_assertions)` answers neither.
pub fn bundle_was_shipped() -> bool {
    bundled_roots().iter().any(|r| looks_like_a_bundle(r))
}

/// A directory is a bundle only if it actually holds a toolchain.
///
/// Merely being called `toolchain` is not enough: the repository has a
/// `toolchain/` directory of *recipes* sitting exactly where the development
/// layout looks for a bundle, and treating that as one would make a developer
/// build report a damaged installation instead of a missing compiler.
fn looks_like_a_bundle(root: &Path) -> bool {
    root.is_dir() && (root.join(bundle::BUNDLE_FILE).is_file() || root.join("bin").is_dir())
}

/// Where a bundled toolchain may live, most specific first.
fn bundled_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("toolchain"));
            // development layout: target/debug/easy-fortran-77 -> ../../toolchain
            out.push(dir.join("..").join("..").join("toolchain"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest as _;

    /// The shared list, plus two a *system* toolchain must never see even though
    /// a bundle may legitimately set them to point at its own tree.
    fn banned_for_system() -> Vec<&'static str> {
        let mut v = BANNED_ENV.to_vec();
        v.extend(["GCC_EXEC_PREFIX", "COMPILER_PATH"]);
        v
    }

    fn dummy() -> Toolchain {
        Toolchain {
            id: ToolchainId {
                kind: ToolchainKind::System,
                version: "16.2.0".into(),
                path: "/usr/bin/gfortran".into(),
            },
            gfortran: "/usr/bin/gfortran".into(),
            caps: FlagCapabilities::optimistic(),
            bundle: None,
        }
    }

    fn bundled(desc: bundle::BundleDescriptor, root: &Path) -> Toolchain {
        let b = Bundle::resolve(root, desc).unwrap();
        Toolchain {
            id: ToolchainId {
                kind: ToolchainKind::Bundled,
                version: b.version.clone(),
                path: b.gfortran.clone(),
            },
            gfortran: b.gfortran.clone(),
            caps: FlagCapabilities::optimistic(),
            bundle: Some(b),
        }
    }

    #[test]
    fn the_child_environment_contains_no_injectable_search_paths() {
        let layout = WorkLayout::new(PathBuf::from("/tmp/work"));
        let env = dummy().compile_env(&layout);
        for (k, _) in &env {
            let k = k.to_string_lossy().to_string();
            assert!(
                !banned_for_system().contains(&k.as_str()),
                "`{k}` must not be passed to the compiler"
            );
        }
    }

    #[test]
    fn the_child_environment_redirects_temporary_files_into_the_work_tree() {
        let layout = WorkLayout::new(PathBuf::from("/tmp/work"));
        let env = dummy().compile_env(&layout);
        let tmpdir = env
            .iter()
            .find(|(k, _)| k == "TMPDIR")
            .map(|(_, v)| PathBuf::from(v));
        assert_eq!(tmpdir, Some(layout.tmp()));
    }

    #[test]
    fn a_bundle_contributes_its_sysroot_flags_and_path_directories() {
        let root = Path::new("/opt/ef77/toolchain");
        let tc = bundled(
            bundle::BundleDescriptor {
                id: "linux-relocatable".into(),
                compile_flags: vec!["--sysroot=${ROOT}/sysroot".into()],
                link_flags: vec!["--sysroot=${ROOT}/sysroot".into(), "-B${ROOT}/lib".into()],
                path_dirs: vec!["bin".into(), "libexec".into()],
                ..Default::default()
            },
            root,
        );
        assert_eq!(
            tc.compile_flags(),
            [OsString::from("--sysroot=/opt/ef77/toolchain/sysroot")]
        );
        assert_eq!(tc.link_flags().len(), 2);

        let layout = WorkLayout::new(PathBuf::from("/tmp/work"));
        let env = tc.compile_env(&layout);
        let path = env
            .iter()
            .find(|(k, _)| k == "PATH")
            .unwrap()
            .1
            .to_string_lossy()
            .to_string();
        assert!(path.contains("/opt/ef77/toolchain/bin"));
        assert!(path.contains("/opt/ef77/toolchain/libexec"));
    }

    #[test]
    fn a_system_toolchain_has_nothing_to_verify() {
        let m = manifest::Manifest::parse(
            "0000000000000000000000000000000000000000000000000000000000000000  bin/nope\n",
        );
        assert!(dummy().verify_integrity(&m).is_ok());
    }

    #[test]
    fn a_bundle_is_verified_against_the_manifest() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path().join("toolchain");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin").join("gfortran"), b"driver").unwrap();
        let hash = format!("{:x}", sha2::Sha256::digest(b"driver"));

        let tc = bundled(bundle::BundleDescriptor::default(), &root);
        assert!(tc
            .verify_integrity(&manifest::Manifest::parse(&format!(
                "{hash}  bin/gfortran\n"
            )))
            .is_ok());

        let bad = manifest::Manifest::parse(
            "0000000000000000000000000000000000000000000000000000000000000000  bin/gfortran\n",
        );
        let r = tc.verify_integrity(&bad);
        assert!(!r.is_ok());
        assert_eq!(r.changed, vec!["bin/gfortran".to_string()]);
    }

    #[test]
    fn an_empty_manifest_verifies_because_there_is_nothing_to_check() {
        let td = tempfile::tempdir().unwrap();
        let tc = bundled(bundle::BundleDescriptor::default(), td.path());
        assert!(tc
            .verify_integrity(&manifest::Manifest::parse("# none\n"))
            .is_ok());
    }

    #[test]
    fn a_system_toolchain_contributes_no_extra_flags() {
        let tc = dummy();
        assert!(tc.compile_flags().is_empty());
        assert!(tc.link_flags().is_empty());
    }

    #[test]
    fn a_bundle_cannot_reintroduce_a_banned_search_path() {
        // A bundle descriptor is ours, but it is data, and data should not be able
        // to undo the environment scrubbing.
        let mut env_map = std::collections::BTreeMap::new();
        env_map.insert("LIBRARY_PATH".to_string(), "/evil".to_string());
        env_map.insert(
            "GCC_EXEC_PREFIX".to_string(),
            "${ROOT}/lib/gcc/".to_string(),
        );
        let tc = bundled(
            bundle::BundleDescriptor {
                env: env_map,
                ..Default::default()
            },
            Path::new("/opt/t"),
        );
        let layout = WorkLayout::new(PathBuf::from("/tmp/work"));
        let env = tc.compile_env(&layout);
        let names: Vec<String> = env
            .iter()
            .map(|(k, _)| k.to_string_lossy().to_string())
            .collect();
        assert!(
            !names.contains(&"LIBRARY_PATH".to_string()),
            "a bundle must not be able to set a banned variable; got {names:?}"
        );
        assert!(names.contains(&"GCC_EXEC_PREFIX".to_string()));
    }

    #[test]
    fn a_directory_named_toolchain_is_not_by_itself_a_bundle() {
        let td = tempfile::tempdir().unwrap();
        let recipes = td.path().join("toolchain");
        std::fs::create_dir_all(&recipes).unwrap();
        std::fs::write(recipes.join("linux-x86_64.toml"), "target = \"x\"\n").unwrap();
        assert!(
            !looks_like_a_bundle(&recipes),
            "a folder of recipes must not be mistaken for a shipped toolchain"
        );

        std::fs::create_dir_all(recipes.join("bin")).unwrap();
        assert!(
            looks_like_a_bundle(&recipes),
            "a bin/ directory makes it one"
        );
    }

    #[test]
    fn a_bundle_is_recognised_by_its_descriptor_alone() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join(bundle::BUNDLE_FILE), "id = \"x\"\n").unwrap();
        assert!(looks_like_a_bundle(td.path()));
    }

    #[test]
    fn a_missing_compiler_is_reported_not_panicked() {
        let err = Toolchain::from_path("/nonexistent/gfortran".into(), ToolchainKind::System)
            .unwrap_err();
        assert!(matches!(err, EfError::ToolchainInvalid { .. }));
    }
}
