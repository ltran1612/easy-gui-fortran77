//! Tier 1: the whole build and run pipeline, driven by a fake compiler.
//!
//! These tests need no gfortran, no window server and no network, so they are the
//! ones that run everywhere and catch the most.

use ef_core::build::{self, diagnostics::Severity, BuildEvent, BuildPhase};
use ef_core::fs_guard::FsGuard;
use ef_core::paths::{AppPaths, WorkLayout};
use ef_core::project::{Program, SourceRef};
use ef_core::toolchain::{Toolchain, ToolchainKind};
use ef_testkit::hash_tree;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const FAKE_GFORTRAN: &str = env!("CARGO_BIN_EXE_fake_gfortran");
const FAKE_PROGRAM: &str = env!("CARGO_BIN_EXE_fake_program");

struct Fixture {
    _tmp: tempfile::TempDir,
    /// Stands in for the user's Documents folder. Nothing here may ever change.
    user_files: PathBuf,
    paths: AppPaths,
    guard: FsGuard,
    layout: WorkLayout,
}

impl Fixture {
    fn new(sources: &[(&str, &[u8])]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let user_files = tmp.path().join("Documents").join("FORTRAN");
        std::fs::create_dir_all(&user_files).unwrap();
        for (name, body) in sources {
            std::fs::write(user_files.join(name), body).unwrap();
        }
        let paths = AppPaths::under(tmp.path().join("app"));
        let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
        let layout = WorkLayout::new(paths.build_dir(1));
        Self {
            _tmp: tmp,
            user_files,
            paths,
            guard,
            layout,
        }
    }

    fn program(&self, names: &[&str]) -> Program {
        let mut p = Program::new("test");
        p.sources = names
            .iter()
            .map(|n| SourceRef::new(self.user_files.join(n)))
            .collect();
        p
    }

    /// Tell the fake compiler how to behave for this build only.
    fn set_mode(&self, mode: &str) {
        self.guard.create_dir_all(&self.layout.root).unwrap();
        self.guard
            .write_file(&self.layout.root.join("FAKE_MODE"), mode.as_bytes())
            .unwrap();
    }

    fn set_program(&self, p: &str) {
        self.guard
            .write_file(&self.layout.root.join("FAKE_PROGRAM"), p.as_bytes())
            .unwrap();
    }

    fn toolchain(&self) -> Toolchain {
        Toolchain::from_path(PathBuf::from(FAKE_GFORTRAN), ToolchainKind::UserSpecified).unwrap()
    }

    fn build(&self, program: &Program) -> build::BuildOutcome {
        let cancel = AtomicBool::new(false);
        build::build(
            &self.guard,
            &self.toolchain(),
            &self.layout,
            program,
            None,
            &cancel,
        )
    }
}

const SRC: &[u8] = b"      PROGRAM P\n      END\n";

// ---------------------------------------------------------------- file safety

#[test]
fn a_build_never_modifies_his_source_files() {
    // The top-priority requirement, expressed executably.
    let f = Fixture::new(&[
        (
            "SOLVER.FOR",
            b"C     Don't touch this\n#define NOPE\n      END\n",
        ),
        ("MAIN.FOR", SRC),
        ("COMMON.INC", b"      COMMON /BLK/ X\n"),
    ]);
    let before = hash_tree(&f.user_files);
    assert_eq!(before.len(), 3);

    f.set_mode("ok");
    let outcome = f.build(&f.program(&["SOLVER.FOR", "MAIN.FOR"]));
    assert!(outcome.success, "{outcome:?}");

    let after = hash_tree(&f.user_files);
    assert_eq!(before, after, "the source tree changed during a build");
}

#[test]
fn a_failed_build_also_leaves_user_files_alone() {
    let f = Fixture::new(&[("BAD.FOR", SRC)]);
    let before = hash_tree(&f.user_files);
    f.set_mode("error");
    let outcome = f.build(&f.program(&["BAD.FOR"]));
    assert!(!outcome.success);
    assert_eq!(before, hash_tree(&f.user_files));
}

#[test]
fn every_artefact_lands_inside_the_work_tree() {
    let f = Fixture::new(&[("A.FOR", SRC)]);
    f.set_mode("ok");
    assert!(f.build(&f.program(&["A.FOR"])).success);

    // Nothing new appeared next to the user's sources...
    let names: Vec<String> = std::fs::read_dir(&f.user_files)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["A.FOR".to_string()]);

    // ...and the object and executable are inside our own tree.
    assert!(f.layout.obj().join("a.o").exists());
    assert!(f.layout.exe().starts_with(f.paths.data_dir()));
}

// ---------------------------------------------------------------- diagnostics

#[test]
fn errors_are_collected_from_every_file_not_just_the_first() {
    // A single multi-file gfortran command stops at the first failure. Compiling
    // per file is what turns six round-trips into one.
    let f = Fixture::new(&[("ONE.FOR", SRC), ("TWO.FOR", SRC), ("THREE.FOR", SRC)]);
    f.set_mode("error");
    let outcome = f.build(&f.program(&["ONE.FOR", "TWO.FOR", "THREE.FOR"]));

    assert!(!outcome.success);
    assert_eq!(outcome.errors, 3, "expected one error per file");
    let files: Vec<_> = outcome
        .diagnostics
        .iter()
        .filter_map(|d| d.file.clone())
        .collect();
    assert_eq!(files, vec!["ONE.FOR", "TWO.FOR", "THREE.FOR"]);
}

#[test]
fn diagnostics_name_his_file_not_our_staged_copy() {
    let f = Fixture::new(&[("SOLVER.FOR", SRC)]);
    f.set_mode("error");
    let outcome = f.build(&f.program(&["SOLVER.FOR"]));
    let d = &outcome.diagnostics[0];
    assert_eq!(d.file.as_deref(), Some("SOLVER.FOR"));
    assert_eq!(d.line, Some(12));
    assert_eq!(d.col, Some(24));
    assert_eq!(d.severity, Severity::Error);
    assert!(
        !outcome.raw.is_empty(),
        "raw output is kept for the details pane"
    );
}

#[test]
fn warnings_do_not_fail_the_build() {
    let f = Fixture::new(&[("W.FOR", SRC)]);
    f.set_mode("warn");
    let outcome = f.build(&f.program(&["W.FOR"]));
    assert!(outcome.success, "a warning must never block the build");
    assert_eq!(outcome.warnings, 1);
    assert_eq!(outcome.errors, 0);
}

#[test]
fn a_link_failure_is_reported_with_the_missing_symbol() {
    let f = Fixture::new(&[("MAIN.FOR", SRC)]);
    f.set_mode("linkfail");
    let outcome = f.build(&f.program(&["MAIN.FOR"]));
    assert!(!outcome.success);
    assert_eq!(outcome.failed_at, Some(build::FailedAt::Link));
    assert!(outcome
        .diagnostics
        .iter()
        .any(|d| d.undefined_symbol.as_deref() == Some("mysub_")));
}

#[test]
fn a_runaway_error_cascade_is_capped() {
    let f = Fixture::new(&[("FLOOD.FOR", SRC)]);
    f.set_mode("flood");
    let outcome = f.build(&f.program(&["FLOOD.FOR"]));
    assert!(!outcome.success);
    assert!(
        outcome.raw.len() <= build::COMPILER_OUTPUT_CAP + 64,
        "captured {} bytes, cap is {}",
        outcome.raw.len(),
        build::COMPILER_OUTPUT_CAP
    );
}

#[test]
fn a_missing_file_is_refused_before_anything_is_staged() {
    let f = Fixture::new(&[("A.FOR", SRC)]);
    f.set_mode("ok");
    let mut p = f.program(&["A.FOR"]);
    p.sources
        .push(SourceRef::new(f.user_files.join("GONE.FOR")));
    let outcome = f.build(&p);
    assert!(!outcome.success);
    assert!(outcome.internal_error.is_some());
}

// ------------------------------------------------------------------ progress

#[test]
fn the_event_stream_reports_each_file_in_order_then_the_link() {
    let f = Fixture::new(&[("ONE.FOR", SRC), ("TWO.FOR", SRC)]);
    f.set_mode("ok");
    let cancel = Arc::new(AtomicBool::new(false));
    let rx = build::spawn(
        f.guard.clone(),
        Arc::new(f.toolchain()),
        f.layout.clone(),
        f.program(&["ONE.FOR", "TWO.FOR"]),
        cancel,
    );

    let mut phases = Vec::new();
    let mut finished = false;
    while let Ok(ev) = rx.recv_timeout(Duration::from_secs(30)) {
        match ev {
            BuildEvent::Phase(p) => phases.push(p),
            BuildEvent::Finished(o) => {
                assert!(o.success, "{o:?}");
                finished = true;
                break;
            }
            BuildEvent::Diagnostics(_) => {}
        }
    }
    assert!(finished, "the build never reported completion");
    assert_eq!(phases[0], BuildPhase::Preflight);
    assert_eq!(phases[1], BuildPhase::Staging);
    assert_eq!(
        phases[2],
        BuildPhase::Compiling {
            done: 0,
            total: 2,
            file: "ONE.FOR".into()
        }
    );
    assert_eq!(
        phases[3],
        BuildPhase::Compiling {
            done: 1,
            total: 2,
            file: "TWO.FOR".into()
        }
    );
    assert_eq!(phases[4], BuildPhase::Linking);
}

#[test]
fn a_build_can_be_cancelled_while_the_compiler_is_running() {
    let f = Fixture::new(&[("SLOW.FOR", SRC)]);
    f.set_mode("slow");
    let cancel = Arc::new(AtomicBool::new(false));
    let rx = build::spawn(
        f.guard.clone(),
        Arc::new(f.toolchain()),
        f.layout.clone(),
        f.program(&["SLOW.FOR"]),
        Arc::clone(&cancel),
    );

    std::thread::sleep(Duration::from_millis(300));
    cancel.store(true, Ordering::SeqCst);

    let start = std::time::Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(BuildEvent::Finished(_)) => break,
            Ok(_) => {}
            Err(e) => panic!("cancellation did not take effect: {e}"),
        }
    }
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "cancelling should not wait for the 30s compiler"
    );
}

// ---------------------------------------------------- keeping the program

/// Build something that is actually executable, using the fake program as the
/// "compiler output", so the saved file can be run.
fn build_runnable(f: &Fixture) -> PathBuf {
    f.set_mode("interactive");
    f.set_program(FAKE_PROGRAM);
    let outcome = f.build(&f.program(&["PROG.FOR"]));
    assert!(outcome.success, "{outcome:?}");
    outcome.exe.unwrap()
}

#[test]
fn the_built_program_can_be_saved_where_he_chooses_and_still_runs() {
    // The build tree is scratch space that gets cleaned up, so saving is how the user
    // ends up with a program the user can keep. It has to still be a working program
    // on the other side of the copy.
    let f = Fixture::new(&[("PROG.FOR", SRC)]);
    let exe = build_runnable(&f);

    // A name with diacritics and a space, because that is what the user will
    // type. The launcher script used to carry this case; it is the export's
    // to carry now.
    let dest = f
        .user_files
        .parent()
        .unwrap()
        .join(format!("Tính dầm{}", std::env::consts::EXE_SUFFIX));
    f.guard.export_built_program(&exe, &dest).unwrap();
    assert!(dest.exists(), "saved to {}", dest.display());

    let out = ef_testkit::spawn_tolerating_busy(
        std::process::Command::new(&dest)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
    )
    .expect("the saved program must be executable")
    .wait_with_output()
    .expect("waiting for the saved program");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        text.contains("Nhap so N:"),
        "saved program printed: {text:?}"
    );
}

#[test]
fn saving_never_writes_over_one_of_his_source_files() {
    let f = Fixture::new(&[("PROG.FOR", SRC)]);
    let exe = build_runnable(&f);
    let his_source = f.user_files.join("PROG.FOR");
    let before = std::fs::read(&his_source).unwrap();

    assert!(
        f.guard.export_built_program(&exe, &his_source).is_err(),
        "a save dialog should never produce this, but it must be refused if it does"
    );
    assert_eq!(std::fs::read(&his_source).unwrap(), before);
}

#[test]
fn saving_refuses_anything_we_did_not_build() {
    // Export writes outside our own tree, so its source must be something we
    // produced -- never one of the user's files copied somewhere else.
    let f = Fixture::new(&[("PROG.FOR", SRC)]);
    let his_source = f.user_files.join("PROG.FOR");
    let dest = f.user_files.parent().unwrap().join("copy.bin");
    assert!(f.guard.export_built_program(&his_source, &dest).is_err());
    assert!(!dest.exists());
}

// ---------------------------------------------------------------- toolchain

#[test]
fn the_toolchain_reports_its_version_and_probes_its_flags() {
    let tc = Toolchain::from_path(PathBuf::from(FAKE_GFORTRAN), ToolchainKind::System).unwrap();
    assert_eq!(tc.id().version, "16.2.0");
    assert_eq!(tc.id().display(), "gfortran 16.2.0");
    let caps = tc.capabilities();
    assert!(!caps.probe_failed);
    assert!(caps.dec, "the fake compiler accepts every flag");
}

#[test]
fn staging_normalises_uppercase_for_names_so_the_preprocessor_never_runs() {
    let f = Fixture::new(&[("SOLVER.FOR", SRC)]);
    f.set_mode("ok");
    assert!(f.build(&f.program(&["SOLVER.FOR"])).success);
    let staged = f.layout.src().join("solver.f");
    assert!(staged.exists(), "expected a lowercase .f staged copy");
    assert!(!f.layout.src().join("SOLVER.FOR").exists());
    assert_eq!(std::fs::read(&staged).unwrap(), SRC);
}
