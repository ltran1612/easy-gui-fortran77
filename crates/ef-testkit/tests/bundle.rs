//! Exercises the *bundled* toolchain path.
//!
//! A real bundle does not exist yet, so we build one around whatever compiler is
//! installed: a `bundle.toml`, a `bin/` directory, and the driver inside it. That
//! is the same shape a shipped bundle has, so this proves the discovery, flag and
//! environment plumbing works before any toolchain is actually vendored.

use ef_core::build;
use ef_core::fs_guard::FsGuard;
use ef_core::paths::{AppPaths, WorkLayout};
use ef_core::project::{Program, SourceRef};
use ef_core::toolchain::{bundle::Bundle, Toolchain, ToolchainKind};
use std::path::{Path, PathBuf};

const FAKE_GFORTRAN: &str = env!("CARGO_BIN_EXE_fake_gfortran");
use std::sync::atomic::AtomicBool;

/// A compiler to build a test bundle around, honouring the same
/// `EF77_REQUIRE_TOOLCHAIN` guard CI uses so these can never silently skip.
fn system_gfortran() -> Option<PathBuf> {
    for name in [
        "gfortran",
        "gfortran-16",
        "gfortran-15",
        "gfortran-14",
        "gfortran-13",
    ] {
        if let Ok(p) = which::which(name) {
            return Some(p);
        }
    }
    if std::env::var("EF77_REQUIRE_TOOLCHAIN").as_deref() == Ok("1") {
        panic!("EF77_REQUIRE_TOOLCHAIN=1 but no gfortran was found to build a test bundle around");
    }
    eprintln!("SKIPPING: no gfortran to build a test bundle around");
    None
}

/// Something that certainly exists on this platform, for tests that only need a
/// file to point at.
fn any_real_file() -> PathBuf {
    std::env::current_exe().expect("the test binary itself always exists")
}

/// Lay out a bundle around an existing compiler.
///
/// A real bundle ships its own assembler and linker, because the child's PATH is
/// scrubbed down to the bundle itself — the host's `as` and `ld` are deliberately
/// out of reach. The fixture has to be equally complete or it is not testing the
/// same thing.
fn make_bundle(root: &Path, driver: &Path, extra_toml: &str) {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();

    let link_in = |src: &Path, name: &str| {
        let dest = bin.join(name);
        // A shipped bundle holds real files; a symlink proves the plumbing and
        // keeps the test fast.
        #[cfg(unix)]
        let _ = std::os::unix::fs::symlink(src, &dest);
        #[cfg(not(unix))]
        let _ = std::fs::copy(src, &dest);
    };
    link_in(driver, &format!("gfortran{}", std::env::consts::EXE_SUFFIX));
    for tool in ["as", "ld"] {
        if let Ok(p) = which::which(tool) {
            link_in(&p, &format!("{tool}{}", std::env::consts::EXE_SUFFIX));
        }
    }

    std::fs::write(
        root.join("bundle.toml"),
        format!(
            "id = \"test-bundle\"\nversion = \"0.0.0\"\ngfortran = \"bin/gfortran{}\"\n{extra_toml}",
            std::env::consts::EXE_SUFFIX
        ),
    )
    .unwrap();
}

#[test]
fn a_bundle_descriptor_round_trips_without_a_compiler() {
    let td = tempfile::tempdir().unwrap();
    make_bundle(
        td.path(),
        &any_real_file(),
        "compile_flags = [\"--sysroot=${ROOT}/sysroot\"]\n",
    );
    let b = Bundle::load(td.path()).unwrap().unwrap();
    assert_eq!(b.id, "test-bundle");
    assert_eq!(
        b.compile_flags[0].to_string_lossy(),
        format!("--sysroot={}/sysroot", b.root.display())
    );
}

#[test]
fn a_bundled_toolchain_compiles_and_runs_a_real_program() {
    // On Unix the stand-in bundle symlinks the host's driver, and GCC follows the
    // link back to its real installation to find f951 and its libraries. Windows
    // has no equivalent: the driver derives its prefix from argv[0], so a copy
    // (or a symlink, which GetModuleFileName does not resolve) makes it search
    // the empty test bundle and probe nothing. Standing up a working bundle here
    // would mean copying a whole GCC installation.
    //
    // Little is lost: the real Windows bundle compiles and runs the entire corpus
    // in the `Bundled toolchain (Windows)` job, which is the case this stands in
    // for, and the bundle plumbing either side of it is covered by the other
    // tests in this file, which use the fake compiler and do run here.
    if cfg!(windows) {
        eprintln!("SKIPPING on Windows: a driver copied out of its installation cannot probe");
        return;
    }
    let Some(driver) = system_gfortran() else {
        eprintln!("SKIPPING: no gfortran to build a test bundle around");
        return;
    };
    let td = tempfile::tempdir().unwrap();
    let bundle_root = td.path().join("toolchain");
    make_bundle(&bundle_root, &driver, "");

    let tc = Toolchain::from_bundle(&bundle_root).unwrap();
    assert_eq!(tc.id().kind, ToolchainKind::Bundled);
    // Against the canonical root: a Windows temp dir arrives as
    // `C:\Users\RUNNER~1\...` and the bundle resolves it to the long name, so
    // the raw path is a different spelling of the same directory.
    let canonical_root = dunce::canonicalize(&bundle_root).unwrap();
    assert!(
        tc.gfortran().starts_with(&canonical_root),
        "must use the bundle's own driver, got {}",
        tc.gfortran().display()
    );
    assert!(
        !tc.capabilities().probe_failed,
        "the bundled compiler must probe successfully"
    );

    // The compiler's PATH must point into the bundle, not at the host's compiler.
    let layout = WorkLayout::new(td.path().join("work"));
    let env = tc.compile_env(&layout);
    let path = env
        .iter()
        .find(|(k, _)| k == "PATH")
        .unwrap()
        .1
        .to_string_lossy()
        .to_string();
    assert!(
        path.starts_with(&bundle_root.join("bin").to_string_lossy().to_string()),
        "the bundle's bin must come first on PATH, got {path}"
    );

    // And it must actually build something.
    let his_dir = td.path().join("Documents");
    std::fs::create_dir_all(&his_dir).unwrap();
    let src = his_dir.join("HELLO.FOR");
    std::fs::write(
        &src,
        b"      PROGRAM HELLO\n      WRITE (*,*) 'bundled ok'\n      END\n",
    )
    .unwrap();

    let paths = AppPaths::under(td.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let layout = WorkLayout::new(paths.build_dir(1));
    let mut program = Program::new("bundled");
    program.sources.push(SourceRef::new(&src));

    let outcome = build::build(
        &guard,
        &tc,
        &layout,
        &program,
        None,
        &AtomicBool::new(false),
    );
    assert!(
        outcome.success,
        "the bundled toolchain failed to build:\n{}",
        outcome.raw
    );
    assert!(outcome.exe.unwrap().exists());
}

#[test]
fn a_bundles_flags_actually_reach_the_compiler_on_both_compile_and_link() {
    // The failure this mechanism exists to prevent is a bundle quietly compiling
    // against the *host's* headers and libraries. Whether a bogus `--sysroot`
    // makes a given compiler fail is a property of that compiler -- a stock
    // Fedora gfortran is not configured `--with-sysroot` and simply ignores it --
    // so asserting on build failure would be testing gfortran, not us.
    //
    // What is ours to guarantee is that the bundle's flags are passed, on the
    // compile line *and* the link line. A fake compiler that records its argv
    // makes that exact and deterministic.
    let td = tempfile::tempdir().unwrap();
    let bundle_root = td.path().join("toolchain");
    make_bundle(
        &bundle_root,
        Path::new(FAKE_GFORTRAN),
        "compile_flags = [\"--sysroot=${ROOT}/sysroot\"]\n\
         link_flags = [\"--sysroot=${ROOT}/sysroot\", \"-B${ROOT}/lib\"]\n",
    );

    let tc = Toolchain::from_bundle(&bundle_root).unwrap();
    let his_dir = td.path().join("Documents");
    std::fs::create_dir_all(&his_dir).unwrap();
    std::fs::write(his_dir.join("A.FOR"), b"      END\n").unwrap();

    let paths = AppPaths::under(td.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let layout = WorkLayout::new(paths.build_dir(1));
    let mut program = Program::new("flags");
    program.sources.push(SourceRef::new(his_dir.join("A.FOR")));

    let outcome = build::build(
        &guard,
        &tc,
        &layout,
        &program,
        None,
        &AtomicBool::new(false),
    );
    assert!(outcome.success, "{}", outcome.raw);

    // The compiler ran with its working directory inside the build tree.
    let log = std::fs::read_to_string(layout.root.join("argv.log"))
        .expect("the fake compiler should have recorded its invocations");
    let lines: Vec<&str> = log
        .lines()
        .filter(|l| !l.contains("-fsyntax-only"))
        .collect();
    // `dunce::canonicalize`, not `Path::canonicalize`: on Windows the latter
    // returns a `\\?\` verbatim path, and the bundle does not, so every
    // comparison below would be against a prefix the product never emits.
    let root = dunce::canonicalize(&bundle_root).unwrap();
    let sysroot = format!("--sysroot={}/sysroot", root.display());

    let compile = lines
        .iter()
        .find(|l| l.contains(" -c ") || l.starts_with("-c ") || l.contains("-c "))
        .expect("a compile invocation");
    let link = lines.last().expect("a link invocation");

    assert!(
        compile.contains(&sysroot),
        "the bundle's sysroot must be on the compile line, got:\n{compile}"
    );
    assert!(
        link.contains(&sysroot) && link.contains(&format!("-B{}/lib", root.display())),
        "the bundle's link flags must be on the link line, got:\n{link}"
    );
}

#[test]
fn a_bundle_missing_its_driver_is_reported_as_a_damaged_installation() {
    let td = tempfile::tempdir().unwrap();
    std::fs::write(td.path().join("bundle.toml"), "id = \"x\"\n").unwrap();
    let err = Toolchain::from_bundle(td.path()).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("missing") || msg.contains("antivirus"),
        "a damaged bundle should say so plainly; got {msg}"
    );
}
