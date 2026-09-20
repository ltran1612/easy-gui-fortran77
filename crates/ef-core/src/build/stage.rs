//! Copy the user's sources into the work tree under normalised names.
//!
//! This one step solves three problems at once:
//!
//! 1. **The `.FOR` trap.** gcc's suffix matching is case-sensitive: `.for` is
//!    fixed-form, but `.FOR` runs the C preprocessor, which then chokes on
//!    apostrophes in comments and on `#` in column 1. DOS-era files are almost
//!    always uppercase, and Windows is case-insensitive but case-preserving, so the
//!    driver really does see `.FOR`. (`.f77` is not a recognised suffix at all —
//!    gfortran would treat it as a linker input.)
//! 2. **Non-ASCII argv.** A MinGW-built `gfortran.exe` receives argv converted
//!    through the process ANSI codepage; `Chương trình.for` can arrive mangled at
//!    `f951`. We control the staged names, so we make them boring.
//! 3. **The read-only guarantee.** The compiler only ever sees our copy. The user's
//!    original is opened read-only and never appears as an output path.

use crate::build::objfmt::{self, Expect, LIBRARY_EXTENSIONS};
use crate::build::sourcefmt;
use crate::error::{EfError, Result};
use crate::fs_guard::{FsGuard, MAX_TOTAL_SOURCE_BYTES};
use crate::paths::WorkLayout;
use crate::project::{Preprocess, Program};
use crate::text;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct StagedSource {
    /// The user's file, exactly as the user chose it. Only ever read.
    pub original: PathBuf,
    /// Our copy, inside the work tree.
    pub staged: PathBuf,
    /// The bare staged file name, e.g. `solver.f`.
    pub staged_name: String,
    /// What to show the user, and what to substitute back into compiler messages.
    pub display_name: String,
    /// Object file this source compiles to.
    pub obj: PathBuf,
}

/// One of the user's pre-compiled libraries, copied into the work tree.
///
/// Copied for the same reason sources are: the user's path may hold Vietnamese
/// characters that a MinGW `gfortran.exe` mangles on the way to the linker, and
/// a copy under a boring ASCII name keeps argv free of it. The library itself is
/// never rewritten — the bytes are identical.
#[derive(Debug, Clone)]
pub struct StagedLibrary {
    pub original: PathBuf,
    pub staged: PathBuf,
    pub display_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct Staging {
    pub sources: Vec<StagedSource>,
    /// In the order the user arranged them, which is link order.
    pub libraries: Vec<StagedLibrary>,
    /// Every distinct directory holding one of the user's sources, so `INCLUDE 'X.INC'`
    /// still resolves after staging. Read-only access to directories the user chose.
    pub include_dirs: Vec<PathBuf>,
}

/// Normalise a source file name to something no toolchain can misread.
///
/// Lowercase, ASCII `[a-z0-9_]` only, and an extension that forces fixed form.
/// A pure function, so it can be property-tested without touching a disk.
pub fn staged_name_for(original_stem: &str, preprocess: Preprocess, seq: usize) -> String {
    let ext = match preprocess {
        // Uppercase `.F` is the *only* way we ever ask for preprocessing, and only
        // when the user explicitly turned it on.
        Preprocess::Always => "F",
        Preprocess::Never => "f",
    };
    with_seq(&boring_ascii_stem(original_stem, "src"), ext, seq)
}

/// Reduce a name to something no toolchain and no shell can misread.
///
/// Lowercase ASCII letters, digits and `_`, never empty, and never starting with
/// anything but a letter or `_` — a leading digit or dash is what turns a file
/// name into a compiler option. `fallback` names what it becomes when nothing
/// usable survives.
///
/// Shared, because it was written twice and the copies had already drifted: the
/// library one dropped the leading-character rule, and no test covered the gap.
fn boring_ascii_stem(s: &str, fallback: &str) -> String {
    let mut base: String = s
        .chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    while base.starts_with('_') {
        base.remove(0);
    }
    while base.ends_with('_') {
        base.pop();
    }
    if base.is_empty() || !base.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        base = format!("{fallback}_{base}");
        while base.ends_with('_') {
            base.pop();
        }
    }
    base.truncate(48);
    base
}

/// Append the extension, disambiguating by sequence number after the first.
fn with_seq(base: &str, ext: &str, seq: usize) -> String {
    if seq == 0 {
        format!("{base}.{ext}")
    } else {
        format!("{base}__{}.{ext}", seq + 1)
    }
}

/// Pick the first name the sequence produces that is not already taken.
fn unique_name(used: &mut Vec<String>, make: impl Fn(usize) -> String) -> String {
    let mut seq = 0;
    let name = loop {
        let candidate = make(seq);
        if !used.contains(&candidate) {
            break candidate;
        }
        seq += 1;
    };
    used.push(name.clone());
    name
}

/// Normalise a library file name, keeping an extension the linker recognises.
///
/// The link line names the file by full path, so the extension is cosmetic — but
/// a `.a` that arrives called `lib_1` is confusing in a log, and `ld` does use
/// the suffix when deciding whether to treat an input as an archive.
pub fn staged_library_name_for(original_name: &str, coff: bool, seq: usize) -> String {
    let path = std::path::Path::new(original_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .filter(|e| LIBRARY_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or_else(|| if coff { "lib".into() } else { "a".into() });

    with_seq(&boring_ascii_stem(&stem, "lib"), &ext, seq)
}

/// Copy the user's libraries into the work tree, refusing any the linker cannot read.
///
/// The refusal is the point. A DOS-era `.LIB` handed to GNU `ld` produces
/// "file format not recognized", which tells the user nothing about what to do; we
/// name the actual problem before the build starts.
fn stage_libraries(
    guard: &FsGuard,
    layout: &WorkLayout,
    program: &Program,
    expect: Expect,
    staging: &mut Staging,
) -> Result<()> {
    let mut used: Vec<String> = Vec::new();
    for lib in &program.libraries {
        crate::project::validate_user_path(&lib.path)?;
        let bytes = FsGuard::read_user_library(&lib.path)?;
        let display_name = text::display_file_name(&lib.path);

        let format = objfmt::sniff(&bytes);
        if let Err(reason) = objfmt::assess(&format, expect) {
            return Err(EfError::UnusableFile {
                problem: Box::new(reason.problem(display_name)),
                english: reason.english(),
            });
        }

        let staged_name = unique_name(&mut used, |seq| {
            staged_library_name_for(&display_name, expect.coff, seq)
        });

        let staged = layout.lib().join(&staged_name);
        guard.write_file(&staged, &bytes)?;

        staging.libraries.push(StagedLibrary {
            original: lib.path.clone(),
            staged,
            display_name,
        });
    }
    Ok(())
}

pub fn stage(
    guard: &FsGuard,
    layout: &WorkLayout,
    program: &Program,
    expect: Expect,
) -> Result<Staging> {
    if program.sources.is_empty() {
        return Err(EfError::NoSources);
    }
    for dir in layout.all_dirs() {
        guard.create_dir_all(&dir)?;
    }

    let mut staging = Staging {
        include_dirs: program.include_dirs(),
        ..Default::default()
    };
    let mut used: Vec<String> = Vec::new();
    let mut total: u64 = 0;

    for src in &program.sources {
        crate::project::validate_user_path(&src.path)?;
        let bytes = FsGuard::read_user_source(&src.path)?;

        // Before the compiler ever sees it. Handed a file that is not source,
        // gfortran emits a cascade of character-level errors that never say so.
        if let Err(problem) = sourcefmt::check(&bytes) {
            return Err(EfError::UnusableFile {
                problem: Box::new(problem.problem(text::display_file_name(&src.path))),
                english: problem.english(),
            });
        }

        total += bytes.len() as u64;
        if total > MAX_TOTAL_SOURCE_BYTES {
            return Err(EfError::SourceTooLarge {
                len: total,
                limit: MAX_TOTAL_SOURCE_BYTES,
            });
        }

        let stem = src
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let staged_name = unique_name(&mut used, |seq| {
            staged_name_for(&stem, program.options.preprocess, seq)
        });

        let staged = layout.src().join(&staged_name);
        // Byte-for-byte. Never transcode: the compiler must see what is on disk.
        guard.write_file(&staged, &bytes)?;

        let stem_only = std::path::Path::new(&staged_name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| staged_name.clone());
        let obj = layout.obj().join(format!("{stem_only}.o"));

        staging.sources.push(StagedSource {
            original: src.path.clone(),
            staged,
            staged_name,
            display_name: text::display_file_name(&src.path),
            obj,
        });
    }

    stage_libraries(guard, layout, program, expect, &mut staging)?;

    Ok(staging)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use crate::project::SourceRef;

    /// What the compiler on this machine consumes, so staging tests need not
    /// care which platform they run on.
    fn host_expect() -> Expect {
        Expect::for_exe_suffix(std::env::consts::EXE_SUFFIX)
    }

    #[test]
    fn uppercase_for_becomes_lowercase_f_so_cpp_never_runs() {
        assert_eq!(staged_name_for("SOLVER", Preprocess::Never, 0), "solver.f");
    }

    #[test]
    fn the_f77_suffix_problem_disappears() {
        // `.f77` is not a suffix gfortran recognises at all; after staging the
        // extension is always one it does.
        let n = staged_name_for("model", Preprocess::Never, 0);
        assert!(n.ends_with(".f"));
    }

    #[test]
    fn vietnamese_names_become_ascii() {
        let n = staged_name_for("Chương trình", Preprocess::Never, 0);
        assert!(
            n.bytes().all(|b| b.is_ascii()),
            "staged name must be pure ASCII, got {n:?}"
        );
        assert!(n.ends_with(".f"));
    }

    #[test]
    fn names_never_start_with_a_dash_or_a_digit() {
        for input in ["-lkernel32", "123", "---", "  ", "", "9lives"] {
            let n = staged_name_for(input, Preprocess::Never, 0);
            assert!(!n.starts_with('-'), "{input:?} -> {n:?}");
            assert!(
                n.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'),
                "{input:?} -> {n:?}"
            );
        }
    }

    #[test]
    fn collisions_get_distinct_names() {
        let a = staged_name_for("Solver", Preprocess::Never, 0);
        let b = staged_name_for("SOLVER", Preprocess::Never, 1);
        assert_ne!(a, b);
    }

    #[test]
    fn preprocessing_is_opt_in_only() {
        assert!(staged_name_for("x", Preprocess::Never, 0).ends_with(".f"));
        assert!(staged_name_for("x", Preprocess::Always, 0).ends_with(".F"));
    }

    #[test]
    fn staging_copies_bytes_verbatim_and_leaves_the_original_untouched() {
        let td = tempfile::tempdir().unwrap();
        let his_dir = td.path().join("Documents");
        std::fs::create_dir_all(&his_dir).unwrap();
        let his_file = his_dir.join("SOLVER.FOR");
        // An apostrophe in a comment and a `#` in column 1: both would break if the
        // C preprocessor ever touched this file.
        let content = b"C     Don't touch this\n#define NOPE\n      PROGRAM P\n      END\n";
        std::fs::write(&his_file, content).unwrap();
        let before = std::fs::metadata(&his_file).unwrap().modified().unwrap();

        let paths = AppPaths::under(td.path().join("app"));
        let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
        let layout = WorkLayout::new(paths.build_dir(1));

        let mut program = Program::new("t");
        program.sources = vec![SourceRef::new(&his_file)];

        let staging = stage(&guard, &layout, &program, host_expect()).unwrap();
        assert_eq!(staging.sources.len(), 1);
        let s = &staging.sources[0];
        assert_eq!(s.staged_name, "solver.f");
        assert_eq!(std::fs::read(&s.staged).unwrap(), content);
        assert_eq!(s.display_name, "SOLVER.FOR");
        assert_eq!(staging.include_dirs, vec![his_dir]);

        // The user's file is byte-identical and was not even touched.
        assert_eq!(std::fs::read(&his_file).unwrap(), content);
        assert_eq!(
            std::fs::metadata(&his_file).unwrap().modified().unwrap(),
            before
        );
    }

    #[test]
    fn two_files_with_the_same_stem_do_not_collide() {
        let td = tempfile::tempdir().unwrap();
        let a_dir = td.path().join("a");
        let b_dir = td.path().join("b");
        std::fs::create_dir_all(&a_dir).unwrap();
        std::fs::create_dir_all(&b_dir).unwrap();
        std::fs::write(a_dir.join("MAIN.FOR"), b"      END\n").unwrap();
        std::fs::write(b_dir.join("main.for"), b"      END\n").unwrap();

        let paths = AppPaths::under(td.path().join("app"));
        let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
        let layout = WorkLayout::new(paths.build_dir(1));
        let mut program = Program::new("t");
        program.sources = vec![
            SourceRef::new(a_dir.join("MAIN.FOR")),
            SourceRef::new(b_dir.join("main.for")),
        ];

        let staging = stage(&guard, &layout, &program, host_expect()).unwrap();
        assert_ne!(
            staging.sources[0].staged_name,
            staging.sources[1].staged_name
        );
        assert_ne!(staging.sources[0].obj, staging.sources[1].obj);
    }

    #[test]
    fn a_program_with_no_sources_is_refused() {
        let td = tempfile::tempdir().unwrap();
        let paths = AppPaths::under(td.path());
        let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
        let layout = WorkLayout::new(paths.build_dir(1));
        let err = stage(&guard, &layout, &Program::new("empty"), host_expect()).unwrap_err();
        assert!(matches!(err, EfError::NoSources));
    }
}

#[cfg(test)]
mod library_tests {
    use super::*;
    use crate::paths::AppPaths;
    use crate::project::LibraryRef;

    fn expect() -> Expect {
        Expect::for_exe_suffix(std::env::consts::EXE_SUFFIX)
    }

    fn empty_archive() -> &'static [u8] {
        b"!<arch>\n"
    }

    struct Fx {
        _tmp: tempfile::TempDir,
        user_dir: PathBuf,
        guard: FsGuard,
        layout: WorkLayout,
    }

    fn fx() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let user_dir = tmp.path().join("FORTRAN");
        std::fs::create_dir_all(&user_dir).unwrap();
        let paths = AppPaths::under(tmp.path().join("app"));
        let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
        let layout = WorkLayout::new(paths.build_dir(1));
        Fx {
            _tmp: tmp,
            user_dir,
            guard,
            layout,
        }
    }

    fn program_with(user_dir: &std::path::Path, names: &[&str]) -> Program {
        let mut p = Program::new("t");
        p.sources = vec![crate::project::SourceRef::new(user_dir.join("MAIN.FOR"))];
        p.libraries = names
            .iter()
            .map(|n| LibraryRef::new(user_dir.join(n)))
            .collect();
        p
    }

    #[test]
    fn a_library_is_copied_under_a_boring_ascii_name() {
        // The same trap as sources: a MinGW gfortran.exe receives argv through
        // the ANSI codepage, so a Vietnamese name can arrive mangled at the
        // linker. We control the staged name, so we make it unremarkable.
        let f = fx();
        std::fs::write(f.user_dir.join("MAIN.FOR"), b"      END\n").unwrap();
        std::fs::write(f.user_dir.join("Thư viện Toán.LIB"), empty_archive()).unwrap();

        let p = program_with(&f.user_dir, &["Thư viện Toán.LIB"]);
        let s = stage(&f.guard, &f.layout, &p, expect()).unwrap();

        assert_eq!(s.libraries.len(), 1);
        let name = s.libraries[0]
            .staged
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            name.is_ascii() && !name.starts_with('-'),
            "staged library name is not boring: {name}"
        );
        // and the user still sees their own name
        assert_eq!(s.libraries[0].display_name, "Thư viện Toán.LIB");
    }

    #[test]
    fn the_staged_copy_is_byte_for_byte_and_his_file_is_untouched() {
        let f = fx();
        std::fs::write(f.user_dir.join("MAIN.FOR"), b"      END\n").unwrap();
        let original = f.user_dir.join("MATH.LIB");
        std::fs::write(&original, empty_archive()).unwrap();
        let before = std::fs::read(&original).unwrap();

        let p = program_with(&f.user_dir, &["MATH.LIB"]);
        let s = stage(&f.guard, &f.layout, &p, expect()).unwrap();

        assert_eq!(std::fs::read(&s.libraries[0].staged).unwrap(), before);
        assert_eq!(std::fs::read(&original).unwrap(), before);
    }

    #[test]
    fn a_dos_era_library_stops_staging_before_anything_is_compiled() {
        let f = fx();
        std::fs::write(f.user_dir.join("MAIN.FOR"), b"      END\n").unwrap();
        let mut omf = vec![0xF0u8, 0x0D, 0x00];
        omf.resize(64, 0);
        std::fs::write(f.user_dir.join("OLDMATH.LIB"), &omf).unwrap();

        let p = program_with(&f.user_dir, &["OLDMATH.LIB"]);
        let err = stage(&f.guard, &f.layout, &p, expect()).unwrap_err();
        match err {
            EfError::UnusableFile { problem, .. } => {
                assert_eq!(problem.name, "OLDMATH.LIB");
                assert_eq!(problem.reason_key, "lib.reject.dos_era");
                assert_eq!(problem.title_key, "libs.unusable_title");
            }
            other => panic!("expected UnusableFile, got {other:?}"),
        }
    }

    #[test]
    fn two_libraries_whose_names_normalise_alike_do_not_collide() {
        let f = fx();
        std::fs::write(f.user_dir.join("MAIN.FOR"), b"      END\n").unwrap();
        std::fs::write(f.user_dir.join("math lib.a"), empty_archive()).unwrap();
        std::fs::write(f.user_dir.join("math-lib.a"), empty_archive()).unwrap();

        let p = program_with(&f.user_dir, &["math lib.a", "math-lib.a"]);
        let s = stage(&f.guard, &f.layout, &p, expect()).unwrap();
        assert_ne!(s.libraries[0].staged, s.libraries[1].staged);
    }

    #[test]
    fn the_extension_is_kept_when_it_is_one_the_linker_knows() {
        assert_eq!(staged_library_name_for("MATH.LIB", true, 0), "math.lib");
        assert_eq!(staged_library_name_for("libmath.a", false, 0), "libmath.a");
        assert_eq!(staged_library_name_for("SUB.OBJ", true, 0), "sub.obj");
        // and an unhelpful extension is replaced rather than passed through
        assert!(staged_library_name_for("MATH.XYZ", true, 0).ends_with(".lib"));
        assert!(staged_library_name_for("MATH.XYZ", false, 0).ends_with(".a"));
    }

    #[test]
    fn a_library_name_can_never_look_like_a_compiler_flag() {
        for awkward in ["-lkernel32.a", "--version.a", "-.a", "___.a"] {
            let n = staged_library_name_for(awkward, false, 0);
            assert!(!n.starts_with('-'), "{awkward} -> {n}");
            assert!(!n.is_empty());
        }
    }
}
