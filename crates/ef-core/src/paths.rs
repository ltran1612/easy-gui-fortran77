//! Every path in the application derives from `AppPaths`, resolved once at startup.

use crate::error::{EfError, Result};
use directories::ProjectDirs;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Reverse-DNS qualifier. Changing this moves every user's saved data, so it is
/// fixed for the life of the product. See "Open items" in the plan.
pub const QUALIFIER: &str = "io.github";
pub const ORGANIZATION: &str = "easy-fortran-77";
pub const APPLICATION: &str = "Easy Fortran 77";

#[derive(Debug, Clone)]
pub struct AppPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    /// Scratch space for builds. Usually under `data_dir`, but not always — see
    /// `choose_work_root`.
    work_root: PathBuf,
    /// Root of everything we are allowed to write. All the dirs above live under
    /// their platform locations, so the write root is checked per-directory.
    write_roots: Vec<PathBuf>,
    session_id: String,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let pd = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
            .ok_or(EfError::NoProjectDirs)?;
        Ok(Self::from_dirs(
            pd.config_dir().to_path_buf(),
            pd.data_dir().to_path_buf(),
            pd.cache_dir().to_path_buf(),
        ))
    }

    /// Used by tests and by `EF77_HOME` so an integration test can point the whole
    /// application at a scratch directory.
    pub fn under(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self::from_dirs(root.join("config"), root.join("data"), root.join("cache"))
    }

    /// Honours `EF77_HOME` when set; otherwise the platform directories.
    pub fn resolve() -> Result<Self> {
        match std::env::var_os("EF77_HOME") {
            Some(h) if !h.is_empty() => Ok(Self::under(PathBuf::from(h))),
            _ => Self::discover(),
        }
    }

    fn from_dirs(config_dir: PathBuf, data_dir: PathBuf, cache_dir: PathBuf) -> Self {
        let work_root = choose_work_root(&data_dir, system_scratch_base().as_deref());
        let write_roots = vec![
            config_dir.clone(),
            data_dir.clone(),
            cache_dir.clone(),
            work_root.clone(),
        ];
        Self {
            config_dir,
            data_dir,
            cache_dir,
            work_root,
            write_roots,
            session_id: uuid::Uuid::new_v4().simple().to_string(),
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
    pub fn write_roots(&self) -> &[PathBuf] {
        &self.write_roots
    }

    pub fn programs_file(&self) -> PathBuf {
        self.config_dir.join("programs.toml")
    }
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.toml")
    }
    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
    pub fn work_root(&self) -> PathBuf {
        self.work_root.clone()
    }
    pub fn session_work_dir(&self) -> PathBuf {
        self.work_root().join(&self.session_id)
    }
    pub fn build_dir(&self, n: u64) -> PathBuf {
        self.session_work_dir().join(format!("build-{n}"))
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Where a build's scratch tree goes.
///
/// Normally under the application's own data directory. On Windows that sits in
/// the user's profile, and MinGW's driver hands paths to `as.exe` through the
/// system ANSI codepage — so a profile named `Nguyễn Văn A` arrives at the
/// assembler as `Nguy?n Van A` and it reports `Invalid argument` trying to open
/// its own temp file. Nothing about the build is wrong; the path cannot survive
/// the trip.
///
/// So when the natural location is not pure ASCII, the scratch tree moves
/// somewhere that is. Only the scratch tree: the program list, the user's sources
/// and the saved program are read and written by us rather than by the compiler,
/// and Rust handles Windows paths as UTF-16 throughout, so those keep their real
/// locations and their real names.
///
/// Taking `base` as an argument rather than reading the environment keeps this a
/// pure function, so both branches are testable on a machine that is neither.
fn choose_work_root(data_dir: &Path, base: Option<&Path>) -> PathBuf {
    let natural = data_dir.join("work");
    if natural.to_string_lossy().is_ascii() {
        return natural;
    }
    let Some(base) = base else {
        // Nowhere better to go. On Unix this is the normal answer: paths are
        // bytes there and the toolchain passes them through unharmed.
        return natural;
    };
    // Named from a hash of the data directory, so two accounts on one machine
    // never share a scratch tree, and hex so the name is ASCII by construction.
    let digest = format!(
        "{:x}",
        Sha256::digest(data_dir.to_string_lossy().as_bytes())
    );
    base.join("EasyFortran77").join("work").join(&digest[..16])
}

/// A machine-wide location whose path is ASCII, or `None` where the problem does
/// not arise.
fn system_scratch_base() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        // %ProgramData% is ASCII on every Windows install and writable by a
        // standard user for directories it creates itself, so this needs no
        // elevation.
        std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .filter(|p| p.to_string_lossy().is_ascii())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Layout of one build's working tree. Nothing outside this is ever written.
#[derive(Debug, Clone)]
pub struct WorkLayout {
    pub root: PathBuf,
}

impl WorkLayout {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn src(&self) -> PathBuf {
        self.root.join("src")
    }
    pub fn obj(&self) -> PathBuf {
        self.root.join("obj")
    }
    pub fn module(&self) -> PathBuf {
        self.root.join("mod")
    }
    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }
    pub fn out(&self) -> PathBuf {
        self.root.join("out")
    }
    /// Staged copies of the user's pre-compiled libraries.
    pub fn lib(&self) -> PathBuf {
        self.root.join("lib")
    }
    /// The built program, for a target whose executables carry `suffix`.
    ///
    /// The suffix comes from the toolchain's bundle, not from the host: a
    /// Windows bundle produces `program.exe` wherever it is driven from.
    pub fn exe_with_suffix(&self, suffix: &str) -> PathBuf {
        self.out().join(format!("program{suffix}"))
    }

    /// The built program for a toolchain targeting this machine.
    pub fn exe(&self) -> PathBuf {
        self.exe_with_suffix(std::env::consts::EXE_SUFFIX)
    }
    pub fn all_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.src(),
            self.obj(),
            self.module(),
            self.tmp(),
            self.out(),
            self.lib(),
        ]
    }
}

#[cfg(test)]
mod work_root_tests {
    use super::*;

    const ASCII_BASE: &str = "/ProgramData";

    #[test]
    fn an_ascii_data_directory_keeps_its_work_tree_where_it_is() {
        let d = PathBuf::from("/home/user/.local/share/easyfortran77");
        assert_eq!(
            choose_work_root(&d, Some(Path::new(ASCII_BASE))),
            d.join("work")
        );
    }

    #[test]
    fn a_non_ascii_data_directory_moves_its_work_tree_somewhere_ascii() {
        // The real case: a Windows profile the compiler's ANSI round-trip mangles.
        let d = PathBuf::from(r"C:\Users\Nguyễn Văn A\AppData\Local\easyfortran77");
        let w = choose_work_root(&d, Some(Path::new(ASCII_BASE)));

        assert!(
            w.to_string_lossy().is_ascii(),
            "the whole scratch path must survive the codepage, got {}",
            w.display()
        );
        assert!(!w.starts_with(&d), "it has to leave the profile entirely");
        assert!(w.starts_with(ASCII_BASE));
    }

    #[test]
    fn two_accounts_on_one_machine_do_not_share_a_scratch_tree() {
        let base = Path::new(ASCII_BASE);
        let a = choose_work_root(
            Path::new(r"C:\Users\Nguyễn Văn A\AppData\Local\ef"),
            Some(base),
        );
        let b = choose_work_root(
            Path::new(r"C:\Users\Trần Thị B\AppData\Local\ef"),
            Some(base),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn the_same_account_gets_the_same_scratch_tree_every_time() {
        // Otherwise every launch would strand the last one's build directories.
        let d = Path::new(r"C:\Users\Nguyễn Văn A\AppData\Local\ef");
        let base = Some(Path::new(ASCII_BASE));
        assert_eq!(choose_work_root(d, base), choose_work_root(d, base));
    }

    #[test]
    fn with_nowhere_ascii_to_go_it_stays_put() {
        // Unix: paths are bytes and the toolchain passes them through, so this is
        // the normal answer rather than a degraded one.
        let d = PathBuf::from("/home/Nguyễn Văn A/.local/share/ef");
        assert_eq!(choose_work_root(&d, None), d.join("work"));
    }

    #[test]
    fn the_work_root_is_writable_by_the_guard() {
        // It is not always under data_dir any more, so it has to be a write root
        // in its own right or every build would be refused by fs_guard.
        let p = AppPaths::under("/tmp/ef-test");
        assert!(
            p.write_roots().iter().any(|r| p.work_root().starts_with(r)),
            "work root {} is not under any write root {:?}",
            p.work_root().display(),
            p.write_roots()
        );
    }
}
