//! Tier 2: their own pre-compiled libraries, through the real compiler.
//!
//! Two things are being proved here. First, that a library the user adds genuinely
//! reaches the link line and contributes code — the positive case. Second, and
//! the reason the feature exists, that a library from the user's DOS-era compiler is
//! refused with an explanation instead of reaching `ld` and coming back as
//! "file format not recognized", which would tell the user nothing.

use ef_core::build;
use ef_core::fs_guard::FsGuard;
use ef_core::paths::{AppPaths, WorkLayout};
use ef_core::project::{LibraryRef, Program, SourceRef};
use ef_core::toolchain::Toolchain;
use ef_testkit::hash_tree;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

const HELPER: &str = "      SUBROUTINE ADDUP(A,B,C)\n\
                      \x20     REAL A,B,C\n\
                      \x20     C = A + B\n\
                      \x20     RETURN\n\
                      \x20     END\n";

const MAIN: &str = "      PROGRAM MAIN\n\
                    \x20     REAL X\n\
                    \x20     CALL ADDUP(2.0, 40.0, X)\n\
                    \x20     WRITE(*,*) 'ANSWER IS ', X\n\
                    \x20     END\n";

fn toolchain() -> Option<Toolchain> {
    match ef_core::toolchain::discover(None) {
        Ok(tc) => Some(tc),
        Err(e) => {
            if std::env::var("EF77_REQUIRE_TOOLCHAIN").as_deref() == Ok("1") {
                panic!("EF77_REQUIRE_TOOLCHAIN=1 but no compiler was found: {e}");
            }
            eprintln!("skipping: no compiler ({e})");
            None
        }
    }
}

struct Fixture {
    _tmp: tempfile::TempDir,
    user_files: PathBuf,
    paths: AppPaths,
    guard: FsGuard,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let user_files = tmp.path().join("FORTRAN");
        std::fs::create_dir_all(&user_files).unwrap();
        std::fs::write(user_files.join("MAIN.FOR"), MAIN).unwrap();
        std::fs::write(user_files.join("HELPER.FOR"), HELPER).unwrap();
        let paths = AppPaths::under(tmp.path().join("app"));
        let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
        Self {
            _tmp: tmp,
            user_files,
            paths,
            guard,
        }
    }

    /// Compile the helper to a loose object with the toolchain under test, which
    /// is what gives us a genuine, correctly-targeted library to link against.
    fn build_helper_object(&self, tc: &Toolchain) -> PathBuf {
        let layout = WorkLayout::new(self.paths.build_dir(90));
        for d in layout.all_dirs() {
            self.guard.create_dir_all(&d).unwrap();
        }
        let src = self.user_files.join("HELPER.FOR");
        let obj = self.user_files.join("MATHLIB.o");
        let out = tc
            .command(&layout)
            .args(tc.compile_flags())
            .arg("-c")
            .arg("-x")
            .arg("f77")
            .arg(&src)
            .arg("-x")
            .arg("none")
            .arg("-o")
            .arg(&obj)
            .output()
            .expect("could not run the compiler");
        assert!(
            out.status.success() && obj.exists(),
            "could not build the helper object: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        obj
    }

    fn program_with_library(&self, lib: &Path) -> Program {
        let mut p = Program::new("with-library");
        p.sources = vec![SourceRef::new(self.user_files.join("MAIN.FOR"))];
        p.libraries = vec![LibraryRef::new(lib)];
        p
    }
}

#[test]
fn a_real_prebuilt_object_is_linked_and_its_code_runs() {
    let Some(tc) = toolchain() else { return };
    let f = Fixture::new();
    let lib = f.build_helper_object(&tc);

    // MAIN.FOR calls ADDUP, which exists only in the library. If the library
    // never reached the link line this fails with an undefined reference — which
    // is exactly what a wrong link order would also produce.
    let program = f.program_with_library(&lib);
    let before = hash_tree(&f.user_files);

    let layout = WorkLayout::new(f.paths.build_dir(91));
    let outcome = build::build(
        &f.guard,
        &tc,
        &layout,
        &program,
        None,
        &AtomicBool::new(false),
    );

    assert!(
        outcome.success,
        "build failed: {:?}\n{}",
        outcome.internal_error, outcome.raw
    );

    let exe = outcome.exe.expect("no executable");
    let out = ef_testkit::spawn_tolerating_busy(
        tc.run_binary(&exe)
            .current_dir(layout.out())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
    )
    .expect("could not run the built program")
    .wait_with_output()
    .expect("waiting for the built program");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        text.contains("42.0") || text.contains("42."),
        "the library's arithmetic did not run; got {text:?}"
    );

    assert_eq!(
        before,
        hash_tree(&f.user_files),
        "the user's files were modified"
    );
}

#[test]
fn a_dos_era_library_is_refused_with_an_explanation_he_can_act_on() {
    let Some(tc) = toolchain() else { return };
    let f = Fixture::new();

    // A Microsoft/Borland OMF library header, as LIB.EXE wrote it: record type
    // 0xF0, record length, dictionary offset and page count.
    let mut omf = vec![0xF0u8, 0x0D, 0x00];
    omf.extend_from_slice(&1024u32.to_le_bytes());
    omf.extend_from_slice(&2u16.to_le_bytes());
    omf.push(0x00);
    omf.resize(1024, 0);
    let lib = f.user_files.join("OLDMATH.LIB");
    std::fs::write(&lib, &omf).unwrap();

    let program = f.program_with_library(&lib);
    let before = hash_tree(&f.user_files);

    let layout = WorkLayout::new(f.paths.build_dir(92));
    let outcome = build::build(
        &f.guard,
        &tc,
        &layout,
        &program,
        None,
        &AtomicBool::new(false),
    );

    assert!(!outcome.success, "an OMF library must not link");

    let problem = outcome
        .file_problem
        .expect("the failure must be reported as a library problem, not a raw linker error");
    assert_eq!(problem.reason_key, "lib.reject.dos_era");
    assert_eq!(problem.name, "OLDMATH.LIB");

    // And the compiler was never asked: we refuse before spending a build on it,
    // so nothing in the transcript is a linker message about an unknown format.
    assert!(
        !outcome.raw.contains("file format not recognized"),
        "we should have refused before ld ever saw it, but ld ran: {}",
        outcome.raw
    );

    assert_eq!(
        before,
        hash_tree(&f.user_files),
        "the user's files were modified"
    );
}

#[test]
fn a_library_that_is_really_fortran_source_is_named_as_such() {
    let Some(tc) = toolchain() else { return };
    let f = Fixture::new();

    // Picking a .FOR in the library dialog is an easy mistake to make.
    let program = f.program_with_library(&f.user_files.join("HELPER.FOR"));
    let layout = WorkLayout::new(f.paths.build_dir(93));
    let outcome = build::build(
        &f.guard,
        &tc,
        &layout,
        &program,
        None,
        &AtomicBool::new(false),
    );

    assert!(!outcome.success);
    let problem = outcome.file_problem.expect("expected a library problem");
    assert_eq!(problem.reason_key, "lib.reject.not_a_library");
}

/// The negative control for the test above.
///
/// Without this, `a_real_prebuilt_object_is_linked_and_its_code_runs` could pass
/// for the wrong reason — if `ADDUP` were somehow resolved without the library,
/// the test would prove nothing about the link line at all.
#[test]
fn the_same_program_without_the_library_fails_to_link() {
    let Some(tc) = toolchain() else { return };
    let f = Fixture::new();

    let mut program = Program::new("no-library");
    program.sources = vec![SourceRef::new(f.user_files.join("MAIN.FOR"))];
    // deliberately no libraries

    let layout = WorkLayout::new(f.paths.build_dir(94));
    let outcome = build::build(
        &f.guard,
        &tc,
        &layout,
        &program,
        None,
        &AtomicBool::new(false),
    );

    assert!(
        !outcome.success,
        "MAIN.FOR must not link without the library that defines ADDUP"
    );
    let text = outcome.raw.to_lowercase();
    assert!(
        text.contains("addup") || text.contains("undefined"),
        "expected an undefined reference to ADDUP, got: {}",
        outcome.raw
    );
}

/// The mirror of the library case, and the one their own files hit.
///
/// A file with a `.FOR` name can be a FAS4 data file rather than Fortran source.
/// Compiled, it produced twenty-five errors about invalid characters and statement
/// labels, none of which said "this is not a Fortran program".
#[test]
fn a_file_that_is_not_fortran_is_refused_before_the_compiler_sees_it() {
    let Some(tc) = toolchain() else { return };
    let f = Fixture::new();

    let fas4 = f.user_files.join("DATAFILE.FOR");
    let mut body = b"\r\n FAS4-FILE ; Do not change it!\r\n1295\r\n108 $".to_vec();
    body.extend_from_slice(&[0x14, 0x01, 0x01, 0x01, 0x00, 0x09, 0x6a, 0x00]);
    std::fs::write(&fas4, &body).unwrap();

    let mut program = Program::new("not-fortran");
    program.sources = vec![SourceRef::new(&fas4)];
    let before = hash_tree(&f.user_files);

    let layout = WorkLayout::new(f.paths.build_dir(95));
    let outcome = build::build(
        &f.guard,
        &tc,
        &layout,
        &program,
        None,
        &AtomicBool::new(false),
    );

    assert!(!outcome.success);
    let problem = outcome
        .file_problem
        .expect("must be reported as a file problem, not a wall of compiler errors");
    assert_eq!(problem.title_key, "src.unusable_title");
    assert_eq!(problem.reason_key, "src.reject.known");
    assert_eq!(problem.name, "DATAFILE.FOR");

    // The compiler was never run, so none of its character-level noise appears.
    assert!(
        !outcome.raw.contains("Invalid character"),
        "we should have refused first, but the compiler ran: {}",
        outcome.raw
    );
    assert!(outcome.diagnostics.is_empty());

    assert_eq!(
        before,
        hash_tree(&f.user_files),
        "the user's files were modified"
    );
}
