//! The ONLY module in the workspace permitted to touch the filesystem.
//!
//! This exists to make the top-priority requirement structural rather than
//! aspirational: **the application never modifies the user's source files.**
//!
//! Two rules, both enforced here and checked by `cargo xtask check-hygiene`:
//!   1. User sources are opened read-only, and never with create/write/truncate.
//!   2. Every write, create or delete asserts that its target resolves inside one
//!      of the application's own write roots.

use crate::error::{EfError, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

/// A single source file may not exceed this. Guards against a mis-selected DVD image.
pub const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
/// All staged sources together may not exceed this.
pub const MAX_TOTAL_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
/// A single library may not exceed this. Larger than the source cap because a
/// static archive legitimately is: a DOS-era `.LIB` is kilobytes, but a modern
/// `.a` full of debug information is not.
pub const MAX_LIBRARY_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct FsGuard {
    write_roots: Vec<PathBuf>,
}

impl FsGuard {
    /// Roots are created if missing, then canonicalized so later comparisons are
    /// made against real paths rather than whatever the caller typed.
    pub fn new(write_roots: impl IntoIterator<Item = PathBuf>) -> Result<Self> {
        let mut roots = Vec::new();
        for r in write_roots {
            fs::create_dir_all(&r).map_err(|e| EfError::io(&r, e))?;
            let c = dunce::canonicalize(&r).map_err(|e| EfError::io(&r, e))?;
            roots.push(c);
        }
        Ok(Self { write_roots: roots })
    }

    // ---------------------------------------------------------------- reading

    /// Open one of the user's own source files. Read-only, always.
    ///
    /// On Windows the share mode is READ|WRITE|DELETE so that we never lock a file
    /// the user has open in Notepad or another editor — taking an exclusive handle on the user's
    /// source would be a different way of interfering with it.
    pub fn open_user_source_readonly(path: &Path) -> Result<File> {
        let mut opts = OpenOptions::new();
        opts.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_SHARE_READ: u32 = 0x0000_0001;
            const FILE_SHARE_WRITE: u32 = 0x0000_0002;
            const FILE_SHARE_DELETE: u32 = 0x0000_0004;
            opts.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
        }
        opts.open(path).map_err(|e| EfError::io(path, e))
    }

    /// Read a user source into memory, byte for byte, with a size cap.
    /// The bytes are never transcoded — the compiler must see exactly what is on disk.
    pub fn read_user_source(path: &Path) -> Result<Vec<u8>> {
        Self::read_user_file(path, MAX_SOURCE_BYTES, |len, limit| {
            EfError::SourceTooLarge { len, limit }
        })
    }

    /// Read one of the user's pre-compiled libraries. A larger cap than a source, because
    /// a static archive legitimately is larger; everything else is identical.
    pub fn read_user_library(path: &Path) -> Result<Vec<u8>> {
        Self::read_user_file(path, MAX_LIBRARY_BYTES, |len, limit| {
            EfError::LibraryTooLarge { len, limit }
        })
    }

    /// The one read path for anything of the user's: open read-only, stat, cap, read.
    ///
    /// Single, because this module exists to be the one audited place that touches
    /// the user's files. A second copy of open/stat/cap/read is where the Windows
    /// share-mode reasoning above quietly stops applying to half the callers.
    fn read_user_file(
        path: &Path,
        limit: u64,
        too_large: impl Fn(u64, u64) -> EfError,
    ) -> Result<Vec<u8>> {
        let mut f = Self::open_user_source_readonly(path)?;
        let len = f.metadata().map_err(|e| EfError::io(path, e))?.len();
        if len > limit {
            return Err(too_large(len, limit));
        }
        let mut buf = Vec::with_capacity(len as usize);
        f.read_to_end(&mut buf).map_err(|e| EfError::io(path, e))?;
        Ok(buf)
    }

    /// Stat a user source without opening it for any kind of write.
    pub fn stat_user_source(path: &Path) -> Result<fs::Metadata> {
        fs::metadata(path).map_err(|e| EfError::io(path, e))
    }

    /// Read one of our own files (config, cache). Missing file yields `Ok(None)`.
    pub fn read_app_file(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        self.assert_under_write_root(path)?;
        match fs::read(path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(EfError::io(path, e)),
        }
    }

    // ---------------------------------------------------------------- writing

    pub fn create_dir_all(&self, path: &Path) -> Result<()> {
        self.assert_under_write_root(path)?;
        fs::create_dir_all(path).map_err(|e| EfError::io(path, e))
    }

    pub fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        self.assert_under_write_root(path)?;
        if let Some(parent) = path.parent() {
            self.create_dir_all(parent)?;
        }
        let mut f = File::create(path).map_err(|e| EfError::io(path, e))?;
        f.write_all(bytes).map_err(|e| EfError::io(path, e))?;
        f.sync_all().map_err(|e| EfError::io(path, e))?;
        Ok(())
    }

    /// Atomic replace: write `.new`, fsync, rotate the current file to `.bak`,
    /// then rename into place. A crash mid-save can cost at most the newest edit,
    /// never the existing file.
    pub fn write_file_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        self.assert_under_write_root(path)?;
        if let Some(parent) = path.parent() {
            self.create_dir_all(parent)?;
        }
        let new = with_suffix(path, ".new");
        let bak = backup_path(path);

        {
            let mut f = File::create(&new).map_err(|e| EfError::io(&new, e))?;
            f.write_all(bytes).map_err(|e| EfError::io(&new, e))?;
            f.sync_all().map_err(|e| EfError::io(&new, e))?;
        }
        if path.exists() {
            let _ = fs::remove_file(&bak);
            let _ = fs::rename(path, &bak);
        }
        fs::rename(&new, path).map_err(|e| EfError::io(path, e))?;
        Ok(())
    }

    pub fn rename_within_root(&self, from: &Path, to: &Path) -> Result<()> {
        self.assert_under_write_root(from)?;
        self.assert_under_write_root(to)?;
        fs::rename(from, to).map_err(|e| EfError::io(from, e))
    }

    pub fn remove_file(&self, path: &Path) -> Result<()> {
        self.assert_under_write_root(path)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(EfError::io(path, e)),
        }
    }

    /// Recursive delete. The containment assertion matters most here: this is the
    /// one call that could do real damage if a path were ever wrong.
    pub fn remove_dir_all(&self, path: &Path) -> Result<()> {
        self.assert_under_write_root(path)?;
        // Belt and braces: never recursively delete a write root itself.
        let resolved = resolve_for_check(path);
        if self.write_roots.contains(&resolved) {
            return Err(EfError::EscapesWriteRoot {
                path: path.to_path_buf(),
            });
        }
        match fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(EfError::io(path, e)),
        }
    }

    pub fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let rd = fs::read_dir(path).map_err(|e| EfError::io(path, e))?;
        let mut out = Vec::new();
        for e in rd {
            let e = e.map_err(|er| EfError::io(path, er))?;
            out.push(e.path());
        }
        out.sort();
        Ok(out)
    }

    /// The rules every write outside the data root must pass: not a folder, not
    /// through a symlink, not over anything that looks like source. Creates the
    /// parent if it is missing.
    ///
    /// Separate from the write it guards because the rules belong to the
    /// *destination* — anywhere the user pointed a save dialog — rather than to
    /// any one thing that lands there. Whatever else this application learns to
    /// hand back goes through here too.
    fn check_export_destination(to: &Path) -> Result<()> {
        if to.is_dir() {
            return Err(EfError::Other(format!(
                "{} is a folder, not a file name",
                to.display()
            )));
        }
        // `fs::copy` follows a symlink, so a link named `out.exe` pointing at the
        // user's source would be written straight through the extension check
        // below. No save dialog produces that, but the check exists for the case
        // it does.
        if fs::symlink_metadata(to)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(EfError::Other(format!(
                "refusing to write through a link: {}",
                to.display()
            )));
        }
        if is_fortran_source(to) {
            return Err(EfError::Other(format!(
                "refusing to write over what looks like a source file: {}",
                to.display()
            )));
        }
        if let Some(parent) = to.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| EfError::io(parent, e))?;
            }
        }
        Ok(())
    }

    /// Copy a built program to a destination the user chose.
    ///
    /// The only place the application writes outside its own data directory, and
    /// deliberately so: the point of the tool is to hand back something to keep.
    /// What keeps it honest is that `from` must be inside our work tree — we
    /// export only what we built, never copy one of the user's own files
    /// somewhere else — and that `to` came from a save dialog, so nothing lands
    /// anywhere unnamed.
    pub fn export_built_program(&self, from: &Path, to: &Path) -> Result<()> {
        self.assert_under_write_root(from)?;
        Self::check_export_destination(to)?;
        fs::copy(from, to).map_err(|e| EfError::io(to, e))?;
        make_runnable(to);
        Ok(())
    }

    // ------------------------------------------------------------- the check

    pub fn assert_under_write_root(&self, path: &Path) -> Result<()> {
        let resolved = resolve_for_check(path);
        if self
            .write_roots
            .iter()
            .any(|root| resolved.starts_with(root))
        {
            Ok(())
        } else {
            Err(EfError::EscapesWriteRoot {
                path: path.to_path_buf(),
            })
        }
    }
}

/// A private scratch directory that we own outright and that removes itself.
///
/// Exists so that capability probing — which must write a tiny source file
/// somewhere before a work tree exists — still goes through this module. Keeping
/// the "only `fs_guard` touches the filesystem" rule absolute, with no exemptions,
/// is what makes the hygiene check meaningful.
#[derive(Debug)]
pub struct Scratch {
    dir: tempfile::TempDir,
}

impl Scratch {
    pub fn new(prefix: &str) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .map_err(|e| EfError::io("<scratch>", e))?;
        Ok(Self { dir })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn write(&self, name: &str, bytes: &[u8]) -> Result<PathBuf> {
        let p = self.dir.path().join(name);
        fs::write(&p, bytes).map_err(|e| EfError::io(&p, e))?;
        Ok(p)
    }

    /// Best-effort removal of one file we created here.
    pub fn discard(&self, path: &Path) {
        if path.starts_with(self.dir.path()) {
            let _ = fs::remove_file(path);
        }
    }
}

/// Extensions a Fortran source may plausibly carry, in any case.
/// Extensions a Fortran source file carries.
///
/// One list: the export guard uses it to refuse writing over something that looks
/// like the user's source, and the file dialog uses it to offer the right files. Two
/// lists would disagree the first time either grew.
pub const FORTRAN_EXTENSIONS: &[&str] =
    &["f", "for", "f77", "ftn", "fi", "inc", "f90", "f95", "fpp"];

/// Add the executable bit, where there is one to add.
///
/// Best effort on purpose: a saved program that is readable but not marked
/// executable is a nuisance the user can fix, and not a reason to fail a save
/// that has already written the file.
fn make_runnable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(md) = fs::metadata(path) {
            let mut perms = md.permissions();
            perms.set_mode(perms.mode() | 0o111);
            let _ = fs::set_permissions(path, perms);
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn is_fortran_source(p: &Path) -> bool {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| FORTRAN_EXTENSIONS.contains(&e.as_str()))
}

pub(crate) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

/// Where `write_file_atomic` rotates the previous contents.
///
/// One function rather than the literal, because two modules depend on the
/// answer and only one of them writes it: this module rotates the file, and
/// `config` reads it back when the live file will not parse. Spelled twice, a
/// change here would leave recovery looking for a file nobody writes any more —
/// and the recovery test would still pass, because it goes through both halves.
pub(crate) fn backup_path(path: &Path) -> PathBuf {
    with_suffix(path, ".bak")
}

/// Resolve a path for containment checking without requiring it to exist.
///
/// Canonicalizes the longest ancestor that does exist (so a symlinked parent cannot
/// be used to step outside a root), then appends the remaining components
/// lexically, resolving `.` and `..` as we go.
fn resolve_for_check(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };

    // Find the longest existing prefix and canonicalize it.
    let mut existing = absolute.as_path();
    let mut tail: Vec<Component> = Vec::new();
    let base = loop {
        if let Ok(c) = dunce::canonicalize(existing) {
            break c;
        }
        match existing.parent() {
            Some(p) => {
                if let Some(name) = existing.components().next_back() {
                    tail.push(name);
                }
                existing = p;
            }
            None => break absolute.clone(),
        }
    };

    let mut out = base;
    for comp in tail.into_iter().rev() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(n) => out.push(n),
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> (tempfile::TempDir, FsGuard) {
        let td = tempfile::tempdir().unwrap();
        let g = FsGuard::new([td.path().join("data")]).unwrap();
        (td, g)
    }

    #[test]
    fn writes_inside_the_root_are_allowed() {
        let (td, g) = guard();
        let p = td.path().join("data").join("a").join("b.txt");
        g.write_file(&p, b"hi").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"hi");
    }

    #[test]
    fn writes_outside_the_root_are_refused() {
        let (td, g) = guard();
        let outside = td.path().join("user-documents").join("SOLVER.FOR");
        fs::create_dir_all(outside.parent().unwrap()).unwrap();
        fs::write(&outside, b"      PROGRAM P\n      END\n").unwrap();

        let err = g.write_file(&outside, b"clobbered").unwrap_err();
        assert!(matches!(err, EfError::EscapesWriteRoot { .. }));
        // and the original is untouched
        assert_eq!(fs::read(&outside).unwrap(), b"      PROGRAM P\n      END\n");
    }

    #[test]
    fn dotdot_cannot_escape_the_root() {
        let (td, g) = guard();
        let sneaky = td
            .path()
            .join("data")
            .join("..")
            .join("user-documents")
            .join("x.f");
        let err = g.write_file(&sneaky, b"nope").unwrap_err();
        assert!(matches!(err, EfError::EscapesWriteRoot { .. }));
    }

    #[test]
    fn remove_dir_all_refuses_the_root_itself() {
        let (td, g) = guard();
        let root = td.path().join("data");
        let err = g.remove_dir_all(&root).unwrap_err();
        assert!(matches!(err, EfError::EscapesWriteRoot { .. }));
        assert!(root.exists());
    }

    #[test]
    fn atomic_write_keeps_a_backup() {
        let (td, g) = guard();
        let p = td.path().join("data").join("programs.toml");
        g.write_file_atomic(&p, b"first").unwrap();
        g.write_file_atomic(&p, b"second").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"second");
        assert_eq!(fs::read(backup_path(&p)).unwrap(), b"first");
        assert!(!with_suffix(&p, ".new").exists());
    }

    #[test]
    fn reading_a_user_source_never_creates_it() {
        let td = tempfile::tempdir().unwrap();
        let missing = td.path().join("nope.f");
        assert!(FsGuard::read_user_source(&missing).is_err());
        assert!(!missing.exists(), "a failed read must not create the file");
    }

    #[test]
    fn exporting_a_built_program_writes_where_he_asked() {
        let (td, g) = guard();
        let built = td.path().join("data").join("out").join("program");
        g.write_file(&built, b"ELF").unwrap();

        let dest = td.path().join("user-documents").join("TinhDam.exe");
        g.export_built_program(&built, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"ELF");
    }

    #[test]
    fn export_refuses_to_copy_anything_we_did_not_build() {
        // Exporting is a write outside our own tree, so the source must be
        // something we produced -- never one of the user's files.
        let (td, g) = guard();
        let source_file = td.path().join("user-documents").join("SOLVER.FOR");
        fs::create_dir_all(source_file.parent().unwrap()).unwrap();
        fs::write(&source_file, b"      END\n").unwrap();

        let err = g
            .export_built_program(&source_file, &td.path().join("elsewhere").join("copy.exe"))
            .unwrap_err();
        assert!(matches!(err, EfError::EscapesWriteRoot { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn export_refuses_to_write_through_a_symlink() {
        let (td, g) = guard();
        let built = td.path().join("data").join("out").join("program");
        g.write_file(&built, b"ELF").unwrap();

        let source_file = td.path().join("user-documents").join("SOLVER.FOR");
        fs::create_dir_all(source_file.parent().unwrap()).unwrap();
        fs::write(&source_file, b"      END\n").unwrap();
        let trap = td.path().join("user-documents").join("innocent.exe");
        std::os::unix::fs::symlink(&source_file, &trap).unwrap();

        assert!(g.export_built_program(&built, &trap).is_err());
        assert_eq!(
            fs::read(&source_file).unwrap(),
            b"      END\n",
            "the source file survived"
        );
    }

    #[test]
    fn export_refuses_to_overwrite_a_source_file() {
        let (td, g) = guard();
        let built = td.path().join("data").join("out").join("program");
        g.write_file(&built, b"ELF").unwrap();

        let source_file = td.path().join("user-documents").join("SOLVER.FOR");
        fs::create_dir_all(source_file.parent().unwrap()).unwrap();
        fs::write(&source_file, b"      END\n").unwrap();

        assert!(g.export_built_program(&built, &source_file).is_err());
        assert_eq!(
            fs::read(&source_file).unwrap(),
            b"      END\n",
            "the source file survived"
        );
    }

    #[test]
    fn a_scratch_directory_contains_its_files_and_cleans_up() {
        let path;
        {
            let s = Scratch::new("ef77-test").unwrap();
            path = s.path().to_path_buf();
            let f = s.write("probe.f", b"      END\n").unwrap();
            assert!(f.starts_with(s.path()));
            assert_eq!(fs::read(&f).unwrap(), b"      END\n");
            s.discard(&f);
            assert!(!f.exists());
            // discard refuses paths outside the scratch dir
            let outside = std::env::temp_dir().join("ef77-should-not-be-removed");
            fs::write(&outside, b"x").unwrap();
            s.discard(&outside);
            assert!(outside.exists());
            let _ = fs::remove_file(&outside);
        }
        assert!(!path.exists(), "the scratch directory must remove itself");
    }

    #[test]
    fn oversize_sources_are_refused() {
        let td = tempfile::tempdir().unwrap();
        let big = td.path().join("big.f");
        fs::write(&big, vec![b'x'; (MAX_SOURCE_BYTES + 1) as usize]).unwrap();
        assert!(matches!(
            FsGuard::read_user_source(&big),
            Err(EfError::SourceTooLarge { .. })
        ));
    }
}
