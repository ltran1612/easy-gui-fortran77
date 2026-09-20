//! Every path in the application derives from `AppPaths`, resolved once at startup.

use crate::error::{EfError, Result};
use directories::ProjectDirs;
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
    /// Root of everything we are allowed to write. All three dirs above live under
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
        let write_roots = vec![config_dir.clone(), data_dir.clone(), cache_dir.clone()];
        Self {
            config_dir,
            data_dir,
            cache_dir,
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
        self.data_dir.join("work")
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
