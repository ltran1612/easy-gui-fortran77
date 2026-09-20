//! Persistence. TOML, because one day I will be on the phone saying "open this file
//! in Notepad and read it to me".
//!
//! Settings and programs live in separate files so that a corrupt settings file can
//! never cost the user the user's program list.

use crate::error::{EfError, Result};
use crate::fs_guard::FsGuard;
use crate::i18n::Lang;
use crate::paths::AppPaths;
use crate::project::Program;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PROGRAMS_SCHEMA: u32 = 1;
pub const SETTINGS_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgramsFile {
    pub schema_version: u32,
    #[serde(rename = "program")]
    pub programs: Vec<Program>,
}

impl Default for ProgramsFile {
    fn default() -> Self {
        Self {
            schema_version: PROGRAMS_SCHEMA,
            programs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub language: Lang,
    pub zoom: f32,
    pub check_updates: bool,
    /// Write a small `.bat`/`.sh` beside a saved program so double-clicking it
    /// keeps the window open. On by default: a console program that vanishes
    /// the instant it finishes is the first thing that goes wrong for someone
    /// running one from Explorer.
    pub create_launcher: bool,
    pub last_check_utc: Option<String>,
    pub toolchain_override: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA,
            language: Lang::from_locale(sys_locale_best().as_deref()),
            zoom: 1.15,
            check_updates: true,
            create_launcher: true,
            last_check_utc: None,
            toolchain_override: None,
        }
    }
}

fn sys_locale_best() -> Option<String> {
    // ef-core does not depend on sys-locale; the GUI passes the locale in when it
    // constructs Settings for the first time. Default here is Vietnamese.
    None
}

/// What happened while loading, so the UI can tell the user plainly rather than
/// silently starting empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadNote {
    /// No file yet — first run.
    Fresh,
    Loaded,
    /// The file would not parse; it was moved aside and the backup was used.
    RecoveredFromBackup {
        corrupt_saved_to: PathBuf,
    },
    /// The file would not parse and there was no usable backup.
    StartedEmpty {
        corrupt_saved_to: PathBuf,
    },
    /// Written by a newer version. We load nothing and refuse to overwrite it.
    TooNew {
        found: u32,
    },
}

pub struct Store {
    paths: AppPaths,
    guard: FsGuard,
    /// Set when the on-disk file came from a newer version: we must never clobber it.
    programs_readonly: bool,
}

impl Store {
    pub fn new(paths: AppPaths) -> Result<Self> {
        let guard = FsGuard::new(paths.write_roots().to_vec())?;
        Ok(Self {
            paths,
            guard,
            programs_readonly: false,
        })
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }
    pub fn guard(&self) -> &FsGuard {
        &self.guard
    }
    pub fn programs_readonly(&self) -> bool {
        self.programs_readonly
    }

    // ------------------------------------------------------------- programs

    pub fn load_programs(&mut self) -> Result<(Vec<Program>, LoadNote)> {
        let path = self.paths.programs_file();
        let Some(bytes) = self.guard.read_app_file(&path)? else {
            return Ok((Vec::new(), LoadNote::Fresh));
        };
        match self.parse_programs(&bytes, &path) {
            Ok(f) => Ok((f.programs, LoadNote::Loaded)),
            Err(EfError::ConfigTooNew { found, .. }) => {
                self.programs_readonly = true;
                Ok((Vec::new(), LoadNote::TooNew { found }))
            }
            Err(_) => {
                // Move the unparseable file aside — never delete it, the user may want it back.
                let stamped = with_stamp(&path, "corrupt");
                let _ = self.guard.rename_within_root(&path, &stamped);
                let bak = append(&path, ".bak");
                if let Some(bbytes) = self.guard.read_app_file(&bak)? {
                    if let Ok(f) = self.parse_programs(&bbytes, &bak) {
                        return Ok((
                            f.programs,
                            LoadNote::RecoveredFromBackup {
                                corrupt_saved_to: stamped,
                            },
                        ));
                    }
                }
                Ok((
                    Vec::new(),
                    LoadNote::StartedEmpty {
                        corrupt_saved_to: stamped,
                    },
                ))
            }
        }
    }

    fn parse_programs(&self, bytes: &[u8], path: &Path) -> Result<ProgramsFile> {
        let text = String::from_utf8_lossy(bytes);
        // Read the version before anything else, so a newer schema never reaches serde.
        let raw: toml::Value = toml::from_str(&text).map_err(|e| EfError::ConfigParse {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
        let found = raw
            .get("schema_version")
            .and_then(|v| v.as_integer())
            .unwrap_or(PROGRAMS_SCHEMA as i64) as u32;
        if found > PROGRAMS_SCHEMA {
            return Err(EfError::ConfigTooNew {
                path: path.to_path_buf(),
                found,
                supported: PROGRAMS_SCHEMA,
            });
        }
        let file: ProgramsFile = raw.try_into().map_err(|e| EfError::ConfigParse {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
        Ok(migrate_programs(file, found))
    }

    pub fn save_programs(&self, programs: &[Program]) -> Result<()> {
        if self.programs_readonly {
            return Ok(()); // refuse to overwrite a newer-schema file
        }
        let file = ProgramsFile {
            schema_version: PROGRAMS_SCHEMA,
            programs: programs.to_vec(),
        };
        let text = toml::to_string_pretty(&file)?;
        self.guard
            .write_file_atomic(&self.paths.programs_file(), text.as_bytes())
    }

    // ------------------------------------------------------------- settings

    pub fn load_settings(&self) -> Result<Settings> {
        let path = self.paths.settings_file();
        let Some(bytes) = self.guard.read_app_file(&path)? else {
            return Ok(Settings::default());
        };
        let text = String::from_utf8_lossy(bytes.as_slice());
        Ok(toml::from_str(&text).unwrap_or_default())
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let text = toml::to_string_pretty(s)?;
        self.guard
            .write_file_atomic(&self.paths.settings_file(), text.as_bytes())
    }
}

fn migrate_programs(f: ProgramsFile, _from: u32) -> ProgramsFile {
    // Only schema 1 exists so far. Future migrations chain here.
    ProgramsFile {
        schema_version: PROGRAMS_SCHEMA,
        ..f
    }
}

fn append(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

fn with_stamp(p: &Path, what: &str) -> PathBuf {
    let stamp = crate::project::now_rfc3339().replace([':', '-'], "");
    append(p, &format!(".{what}-{stamp}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{BuildOptions, LineLength, SourceRef};

    fn store() -> (tempfile::TempDir, Store) {
        let td = tempfile::tempdir().unwrap();
        let s = Store::new(AppPaths::under(td.path())).unwrap();
        (td, s)
    }

    #[test]
    fn first_run_is_fresh_and_empty() {
        let (_td, mut s) = store();
        let (p, note) = s.load_programs().unwrap();
        assert!(p.is_empty());
        assert_eq!(note, LoadNote::Fresh);
    }

    #[test]
    fn programs_round_trip_with_vietnamese_names_and_paths() {
        let (_td, mut s) = store();
        let mut p = Program::new("Tính dầm bê tông");
        p.sources = vec![SourceRef::new("/home/Nguyễn Văn A/FORTRAN/SOLVER.FOR")];
        p.options.line_length = LineLength::Col132;
        s.save_programs(&[p.clone()]).unwrap();

        let (back, note) = s.load_programs().unwrap();
        assert_eq!(note, LoadNote::Loaded);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "Tính dầm bê tông");
        assert_eq!(back[0].sources[0].path, p.sources[0].path);
        assert_eq!(back[0].options.line_length, LineLength::Col132);
    }

    #[test]
    fn unknown_fields_are_tolerated_for_forward_compatibility() {
        let (td, mut s) = store();
        let path = AppPaths::under(td.path()).programs_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "schema_version = 1\n\n[[program]]\nid = \"abc\"\nname = \"X\"\nfuture_field = 42\n",
        )
        .unwrap();
        let (back, note) = s.load_programs().unwrap();
        assert_eq!(note, LoadNote::Loaded);
        assert_eq!(back[0].name, "X");
    }

    #[test]
    fn a_newer_schema_is_never_overwritten() {
        let (td, mut s) = store();
        let path = AppPaths::under(td.path()).programs_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "schema_version = 99\n").unwrap();

        let (back, note) = s.load_programs().unwrap();
        assert!(back.is_empty());
        assert_eq!(note, LoadNote::TooNew { found: 99 });
        assert!(s.programs_readonly());

        s.save_programs(&[Program::new("nope")]).unwrap();
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("99"), "the newer file was clobbered");
    }

    #[test]
    fn a_corrupt_file_is_moved_aside_and_the_backup_is_used() {
        let (_td, mut s) = store();
        s.save_programs(&[Program::new("good")]).unwrap();
        s.save_programs(&[Program::new("newer good")]).unwrap(); // creates .bak

        let path = s.paths().programs_file();
        std::fs::write(&path, "this is not toml {{{").unwrap();

        let (back, note) = s.load_programs().unwrap();
        match note {
            LoadNote::RecoveredFromBackup { corrupt_saved_to } => {
                assert!(corrupt_saved_to.exists(), "the corrupt file must be kept");
                assert_eq!(back.len(), 1);
                assert_eq!(back[0].name, "good");
            }
            other => panic!("expected recovery from backup, got {other:?}"),
        }
    }

    #[test]
    fn settings_round_trip() {
        let (_td, s) = store();
        let st = Settings {
            language: Lang::En,
            zoom: 1.5,
            ..Default::default()
        };
        s.save_settings(&st).unwrap();
        let back = s.load_settings().unwrap();
        assert_eq!(back.language, Lang::En);
        assert!((back.zoom - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn build_options_defaults_survive_an_empty_table() {
        let f: ProgramsFile =
            toml::from_str("schema_version = 1\n[[program]]\nname = \"x\"\n").unwrap();
        assert_eq!(f.programs[0].options, BuildOptions::default());
    }
}
