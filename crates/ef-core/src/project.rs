//! The user's data model: a "program" is a named, ordered group of source files.
//!
//! Order is significant — it is both compile order and link order, which matches
//! the object-file-then-link mental model of the DOS-era tool this replaces.

use crate::text;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

/// How permissive the compiler should be about pre-standard code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    /// `-std=legacy`: accepts everything gfortran knows, including the five
    /// features Fortran 90 deleted. The right default for 1980s code.
    #[default]
    Legacy,
    /// `-std=f95`: warn about extensions. For someone checking their code is clean.
    Standard,
}

/// Fixed-form source lines are significant only up to a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineLength {
    /// The standard, and the default. Card-image code keeps sequence numbers in
    /// columns 73-80; widening past 72 turns those into syntax errors.
    #[default]
    Col72,
    /// Extended source, as offered by Lahey/Watcom/MS Fortran.
    Col132,
    /// The whole line is significant. Only safe when there are no sequence numbers.
    None,
}

impl LineLength {
    pub fn flag(self) -> &'static str {
        match self {
            LineLength::Col72 => "-ffixed-line-length-72",
            LineLength::Col132 => "-ffixed-line-length-132",
            LineLength::None => "-ffixed-line-length-none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OptLevel {
    O0,
    #[default]
    O1,
    O2,
}

impl OptLevel {
    pub fn flag(self) -> &'static str {
        match self {
            OptLevel::O0 => "-O0",
            OptLevel::O1 => "-O1",
            OptLevel::O2 => "-O2",
        }
    }
}

/// Whether to run the C preprocessor. Almost always `Never`: an uppercase `.FOR`
/// name would otherwise make gcc preprocess the file, and cpp then chokes on
/// apostrophes in comments and on `#` in column 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Preprocess {
    #[default]
    Never,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildOptions {
    pub dialect: Dialect,
    pub line_length: LineLength,
    /// `-fdec`: STRUCTURE/RECORD/UNION/MAP, `.` member access, `$` in identifiers,
    /// Cray pointers. The VAX/DEC lineage that MS Fortran, Lahey, Watcom and DVF
    /// all inherited.
    pub dec_extensions: bool,
    /// `-fno-automatic -finit-local-zero`: reproduce DOS-era static, zero-initialised
    /// storage. The most common reason old code gives different numbers today.
    pub static_storage: bool,
    /// `-fd-lines-as-code`: treat `D` in column 1 as code rather than a comment.
    pub d_lines_as_code: bool,
    /// `-fdefault-real-8` with `-fdefault-double-8`: promote REAL from four
    /// bytes to eight, and leave DOUBLE PRECISION at eight.
    ///
    /// The second half is not optional decoration. `-fdefault-real-8` alone also
    /// promotes DOUBLE PRECISION, to sixteen bytes of software-emulated quad --
    /// so the option would change the parts of a program that were already
    /// precise, and slow them down, which is the opposite of what anyone turning
    /// it on is reaching for.
    ///
    /// Opt-in regardless. It changes storage sizes, so unformatted files written
    /// by an earlier build stop being readable, and `EQUIVALENCE` overlays see
    /// different bytes.
    pub default_real8: bool,
    /// Large local arrays overflow the 1 MB Windows stack.
    pub big_stack: bool,
    /// `-fcheck=bounds`: stop the program when an array index is outside the
    /// array, instead of reading whatever happens to be next in memory.
    ///
    /// On by default, which is a departure from reproducing a 1980s compiler
    /// exactly, and a deliberate one. Without it, `ARR(7)` on a three-element
    /// array quietly returns a neighbouring value: not a crash, not a NaN, just
    /// a plausible number that flows into a result and is believed. For
    /// engineering arithmetic a program that stops and says "index 7 is outside
    /// 1 to 3" is worth more than one that prints a confident wrong answer.
    ///
    /// It costs nothing at compile time and little at run time for programs of
    /// this size. It can be turned off for old code that reads past an array on
    /// purpose -- which does exist, and used to work.
    pub check_bounds: bool,
    /// Link a small shim that waits for a key before the program exits, so a
    /// console window opened by double-clicking does not vanish before it can be
    /// read. Windows only.
    ///
    /// On by default. Double-clicking the program in Explorer is what a person
    /// who has never used a terminal will do, and without this that is a black
    /// window that flashes and disappears — the results unread, and nothing to
    /// suggest the program worked at all.
    ///
    /// Defaulting it on is safe because the shim asks who owns the console
    /// rather than assuming: run from a Command Prompt, or from a script that
    /// owns the window already, it does not wait. It costs one object on the
    /// link line and nothing at run time.
    pub keep_window_open: bool,
    /// `-s`: drop the symbol table and debug information from the linked
    /// program. Makes the saved file substantially smaller, at the cost of
    /// meaningful locations in a runtime backtrace. Off by default: a smaller
    /// file is worth less than a diagnosable crash.
    pub strip_symbols: bool,
    pub opt_level: OptLevel,
    pub preprocess: Preprocess,
    pub extra_flags: Vec<String>,
}

impl BuildOptions {
    /// Should this build carry the shim that waits before the window closes?
    ///
    /// One predicate, because two places act on the answer — `build` compiles
    /// the shim object, `args` emits the `-Wl,--wrap=exit` that reaches it — and
    /// they have to agree. Disagreeing is silent either way: a shim nothing
    /// calls, or a wrap with nothing behind it.
    ///
    /// Gated on the *toolchain's* suffix rather than the host, so a Windows
    /// program cross-built from Linux still gets it.
    pub fn wants_pause_shim(&self, exe_suffix: &str) -> bool {
        self.keep_window_open && exe_suffix.eq_ignore_ascii_case(".exe")
    }
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            dialect: Dialect::Legacy,
            line_length: LineLength::Col72,
            dec_extensions: true,
            static_storage: true,
            d_lines_as_code: false,
            default_real8: false,
            big_stack: false,
            check_bounds: true,
            strip_symbols: false,
            keep_window_open: true,
            opt_level: OptLevel::O1,
            preprocess: Preprocess::Never,
            extra_flags: Vec::new(),
        }
    }
}

/// One of the user's files in one of the program's lists.
///
/// `SourceRef` and `LibraryRef` stay separate types on purpose — a source is
/// copied, renamed and compiled, a library only ever appended to the link line,
/// and keeping them distinct means a library cannot reach the compiler by a slip
/// of the hand. This trait exists so the list *mechanics* need not be written
/// twice for that to hold.
pub trait FileEntry {
    fn path(&self) -> &Path;
    fn from_path(path: PathBuf) -> Self;
}

/// Add a file to a list, refusing a name a compiler would read as an option and
/// silently ignoring an exact duplicate.
fn add_file<T: FileEntry>(list: &mut Vec<T>, path: &Path) -> Result<bool, crate::error::EfError> {
    let path = canonical_user_path(path);
    validate_user_path(&path)?;
    if list.iter().any(|e| e.path() == path) {
        return Ok(false);
    }
    list.push(T::from_path(path));
    Ok(true)
}

/// Point an existing entry at a file the user has found again, validated exactly as
/// adding one is: "Locate file…" must not be a way around the name rules.
fn relocate_file<T: FileEntry>(
    list: &mut [T],
    index: usize,
    path: &Path,
) -> Result<bool, crate::error::EfError> {
    let path = canonical_user_path(path);
    validate_user_path(&path)?;
    match list.get_mut(index) {
        Some(e) => {
            *e = T::from_path(path);
            Ok(true)
        }
        None => Ok(false),
    }
}

fn remove_file<T>(list: &mut Vec<T>, index: usize) -> bool {
    if index < list.len() {
        list.remove(index);
        return true;
    }
    false
}

fn move_file<T>(list: &mut [T], index: usize, delta: isize) -> bool {
    let target = index as isize + delta;
    if index < list.len() && target >= 0 && (target as usize) < list.len() {
        list.swap(index, target as usize);
        return true;
    }
    false
}

/// One of the user's source files. We store the path only; existence and
/// readability are re-checked before every build and never persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub path: PathBuf,
}

impl FileEntry for SourceRef {
    fn path(&self) -> &Path {
        &self.path
    }
    fn from_path(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SourceRef {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn display_name(&self) -> String {
        text::display_file_name(&self.path)
    }
    pub fn display_path(&self) -> String {
        text::display_path(&self.path)
    }
}

/// One of the user's pre-compiled library files.
///
/// Kept in a separate list from the sources rather than as a flag on `SourceRef`,
/// because the two travel completely different paths: a source is copied, renamed
/// and compiled, whereas a library is only ever handed to the linker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryRef {
    pub path: PathBuf,
}

impl FileEntry for LibraryRef {
    fn path(&self) -> &Path {
        &self.path
    }
    fn from_path(path: PathBuf) -> Self {
        Self { path }
    }
}

impl LibraryRef {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn display_name(&self) -> String {
        text::display_file_name(&self.path)
    }
    pub fn display_path(&self) -> String {
        text::display_path(&self.path)
    }
}

/// Transient, never serialized. The config is not a source of truth about what
/// exists on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Present { len: u64 },
    Missing,
    Unreadable(String),
}

impl FileStatus {
    pub fn is_usable(&self) -> bool {
        matches!(self, FileStatus::Present { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Program {
    pub id: String,
    pub name: String,
    pub created: String,
    pub modified: String,
    #[serde(rename = "source")]
    pub sources: Vec<SourceRef>,
    /// Pre-compiled libraries to link against, in link order. Absent from older
    /// config files, which is why every field here carries `serde(default)`.
    #[serde(rename = "library")]
    pub libraries: Vec<LibraryRef>,
    pub options: BuildOptions,
}

impl Default for Program {
    fn default() -> Self {
        let now = now_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            name: String::new(),
            created: now.clone(),
            modified: now,
            sources: Vec::new(),
            libraries: Vec::new(),
            options: BuildOptions::default(),
        }
    }
}

impl Program {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: text::nfc(&name.into()),
            ..Default::default()
        }
    }

    pub fn touch(&mut self) {
        self.modified = now_rfc3339();
    }

    /// Add a source file. Duplicates are ignored; unusable names are refused.
    pub fn add_source(&mut self, path: impl AsRef<Path>) -> Result<(), crate::error::EfError> {
        if add_file(&mut self.sources, path.as_ref())? {
            self.touch();
        }
        Ok(())
    }

    /// Add a library to link against.
    pub fn add_library(&mut self, path: impl AsRef<Path>) -> Result<(), crate::error::EfError> {
        if add_file(&mut self.libraries, path.as_ref())? {
            self.touch();
        }
        Ok(())
    }

    pub fn remove_source(&mut self, index: usize) {
        if remove_file(&mut self.sources, index) {
            self.touch();
        }
    }

    pub fn remove_library(&mut self, index: usize) {
        if remove_file(&mut self.libraries, index) {
            self.touch();
        }
    }

    /// Source order is compile and link order, so moving one is a real edit.
    pub fn move_source(&mut self, index: usize, delta: isize) {
        if move_file(&mut self.sources, index, delta) {
            self.touch();
        }
    }

    /// Order matters more for libraries than for sources: a static library only
    /// satisfies symbols referenced by something earlier on the link line.
    pub fn move_library(&mut self, index: usize, delta: isize) {
        if move_file(&mut self.libraries, index, delta) {
            self.touch();
        }
    }

    pub fn relocate_source(
        &mut self,
        index: usize,
        path: impl AsRef<Path>,
    ) -> Result<(), crate::error::EfError> {
        if relocate_file(&mut self.sources, index, path.as_ref())? {
            self.touch();
        }
        Ok(())
    }

    pub fn relocate_library(
        &mut self,
        index: usize,
        path: impl AsRef<Path>,
    ) -> Result<(), crate::error::EfError> {
        if relocate_file(&mut self.libraries, index, path.as_ref())? {
            self.touch();
        }
        Ok(())
    }

    /// Every distinct directory holding one of the user's sources, in first-seen order.
    /// These become `-I` arguments so `INCLUDE 'COMMON.INC'` keeps resolving after
    /// the sources are staged elsewhere. Read-only access to dirs the user already chose.
    pub fn include_dirs(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        for s in &self.sources {
            if let Some(dir) = s.path.parent() {
                if !out.iter().any(|d| d == dir) {
                    out.push(dir.to_path_buf());
                }
            }
        }
        out
    }
}

/// gcc has no `--` end-of-options separator, so a file called `-lkernel32.f` would
/// be parsed as a linker flag. We reject such names when they are added rather than
/// trying to escape them later.
/// The argv-safety rules, which apply identically to a source and to a library.
pub fn validate_user_path(path: &Path) -> Result<(), crate::error::EfError> {
    use crate::error::EfError;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.is_empty() {
        return Err(EfError::UnsafeSourceName {
            reason: "the path has no file name".into(),
        });
    }
    if name.starts_with('-') {
        return Err(EfError::UnsafeSourceName {
            reason: format!("`{name}` starts with a dash, which a compiler reads as an option"),
        });
    }
    let s = path.to_string_lossy();
    if s.contains('\0') || s.contains('\n') || s.contains('\r') {
        return Err(EfError::UnsafeSourceName {
            reason: "the path contains a control character".into(),
        });
    }
    Ok(())
}

/// Absolute, and simplified so no `\\?\` verbatim prefix ever reaches argv or the UI.
pub fn canonical_user_path(p: &Path) -> PathBuf {
    match dunce::canonicalize(p) {
        Ok(c) => c,
        Err(_) => {
            // The file may not exist (yet). Make it absolute without resolving.
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_default().join(p)
            }
        }
    }
}

/// A file name to offer in the save dialog for a program called `name`.
///
/// Keeps their own words, including Vietnamese, and only removes the characters a
/// filesystem will not accept. The platform executable suffix is appended so the
/// saved file is runnable on Windows.
pub fn suggested_program_file_name(name: &str) -> String {
    const ILLEGAL: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let cleaned: String = text::nfc(name)
        .chars()
        .map(|c| {
            if ILLEGAL.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    let stem = if cleaned.is_empty() {
        "program"
    } else {
        cleaned
    };
    format!("{stem}{}", std::env::consts::EXE_SUFFIX)
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| String::from("1970-01-01T00:00:00Z"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dash_prefixed_names_are_refused() {
        let err = validate_user_path(Path::new("/tmp/-lkernel32.f")).unwrap_err();
        assert!(matches!(
            err,
            crate::error::EfError::UnsafeSourceName { .. }
        ));
    }

    #[test]
    fn ordinary_and_vietnamese_names_are_accepted() {
        validate_user_path(Path::new("/tmp/SOLVER.FOR")).unwrap();
        validate_user_path(Path::new("/tmp/Chương trình.for")).unwrap();
    }

    #[test]
    fn include_dirs_are_deduped_in_order() {
        let mut p = Program::new("t");
        p.sources = vec![
            SourceRef::new("/a/one.f"),
            SourceRef::new("/b/two.f"),
            SourceRef::new("/a/three.f"),
        ];
        assert_eq!(
            p.include_dirs(),
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }

    #[test]
    fn relocating_a_file_is_validated_like_adding_one() {
        // "Locate file…" is the second way into the list. A name the build would
        // refuse must not be able to enter through it.
        let mut p = Program::new("t");
        p.sources = vec![SourceRef::new("/a/1.f")];
        p.libraries = vec![LibraryRef::new("/a/lib.a")];

        assert!(p.relocate_source(0, "/tmp/-lkernel32.f").is_err());
        assert_eq!(p.sources[0].path, PathBuf::from("/a/1.f"), "left unchanged");

        assert!(p.relocate_library(0, "/tmp/-lkernel32.a").is_err());
        assert_eq!(p.libraries[0].path, PathBuf::from("/a/lib.a"));

        // and an ordinary path is accepted
        p.relocate_source(0, "/tmp/SOLVER.FOR").unwrap();
        assert!(p.sources[0].path.ends_with("SOLVER.FOR"));
    }

    #[test]
    fn an_out_of_range_relocate_changes_nothing() {
        let mut p = Program::new("t");
        p.relocate_source(7, "/tmp/x.f").unwrap();
        p.relocate_library(7, "/tmp/x.a").unwrap();
        assert!(p.sources.is_empty() && p.libraries.is_empty());
    }

    #[test]
    fn source_order_is_preserved_and_movable() {
        let mut p = Program::new("t");
        p.sources = vec![SourceRef::new("/a/1.f"), SourceRef::new("/a/2.f")];
        p.move_source(0, 1);
        assert_eq!(p.sources[0].path, PathBuf::from("/a/2.f"));
        p.move_source(0, -1); // out of range: no-op
        assert_eq!(p.sources[0].path, PathBuf::from("/a/2.f"));
    }

    #[test]
    fn a_suggested_file_name_keeps_his_words_but_drops_illegal_characters() {
        let n = suggested_program_file_name("Tính dầm bê tông");
        assert!(n.starts_with("Tính dầm bê tông"), "got {n}");
        assert!(n.ends_with(std::env::consts::EXE_SUFFIX));

        let n = suggested_program_file_name("a/b:c*d?");
        assert!(!n.contains('/') && !n.contains(':') && !n.contains('*') && !n.contains('?'));
    }

    #[test]
    fn an_unnamed_program_still_gets_a_usable_file_name() {
        for input in ["", "   ", "...", "///"] {
            let n = suggested_program_file_name(input);
            assert!(!n.is_empty());
            assert!(
                n.starts_with("program") || n.starts_with('_'),
                "{input:?} -> {n:?}"
            );
            assert!(n.ends_with(std::env::consts::EXE_SUFFIX));
        }
    }

    #[test]
    fn defaults_are_permissive_for_legacy_code() {
        let o = BuildOptions::default();
        assert_eq!(o.dialect, Dialect::Legacy);
        assert_eq!(o.line_length, LineLength::Col72, "never default to `none`");
        assert!(o.dec_extensions);
        assert!(o.static_storage);
        assert_eq!(o.preprocess, Preprocess::Never);
        assert!(
            !o.strip_symbols,
            "stripping must be opt-in: a crash nobody can diagnose costs more than a large file"
        );
    }
}
