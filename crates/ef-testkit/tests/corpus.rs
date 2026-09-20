//! Tier 2: real gfortran against a corpus of genuine Fortran 77.
//!
//! Skipped automatically when no compiler is installed, so the rest of the suite
//! still runs on a bare machine. Set `EF77_REQUIRE_TOOLCHAIN=1` (CI does) to turn a
//! missing compiler into a failure instead of a skip.

use ef_core::build;
use ef_core::fs_guard::FsGuard;
use ef_core::paths::{AppPaths, WorkLayout};
use ef_core::project::{LineLength, Program, SourceRef};
use ef_core::toolchain::Toolchain;
use ef_testkit::hash_tree;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Expect {
    description: String,
    sources: Vec<String>,
    success: bool,
    run: bool,
    stdin: String,
    exit_code: Option<i32>,
    stdout_contains: Vec<String>,
    error_contains: Vec<String>,
    error_in_file: Option<String>,
    undefined_symbol: bool,
    creates_beside_the_program: Vec<String>,
    line_length: Option<String>,
}

impl Default for Expect {
    fn default() -> Self {
        Self {
            description: String::new(),
            sources: Vec::new(),
            success: true,
            run: false,
            stdin: String::new(),
            exit_code: None,
            stdout_contains: Vec::new(),
            error_contains: Vec::new(),
            error_in_file: None,
            undefined_symbol: false,
            creates_beside_the_program: Vec::new(),
            line_length: None,
        }
    }
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

fn toolchain() -> Option<Toolchain> {
    match ef_core::toolchain::discover(None) {
        Ok(tc) => Some(tc),
        Err(e) => {
            if std::env::var("EF77_REQUIRE_TOOLCHAIN").as_deref() == Ok("1") {
                panic!("EF77_REQUIRE_TOOLCHAIN=1 but no compiler was found: {e}");
            }
            eprintln!("SKIPPING the Fortran corpus: {e}");
            eprintln!("  install one with:  sudo dnf install gcc-gfortran");
            None
        }
    }
}

/// Copy a case into a scratch directory. The read-only assertion is then made
/// against that copy, so a regression cannot damage the repository.
fn stage_case(case: &Path, into: &Path) {
    std::fs::create_dir_all(into).unwrap();
    for e in std::fs::read_dir(case).unwrap().flatten() {
        let p = e.path();
        if p.is_file() && p.file_name().unwrap() != "expect.toml" {
            std::fs::copy(&p, into.join(p.file_name().unwrap())).unwrap();
        }
    }
}

struct Case {
    name: String,
    expect: Expect,
    dir: PathBuf,
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(corpus_root()).unwrap().flatten() {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        let toml_path = dir.join("expect.toml");
        let text = std::fs::read_to_string(&toml_path)
            .unwrap_or_else(|e| panic!("{}: {e}", toml_path.display()));
        let expect: Expect =
            toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", toml_path.display()));
        out.push(Case {
            name: dir.file_name().unwrap().to_string_lossy().to_string(),
            expect,
            dir,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[test]
fn the_corpus_is_well_formed() {
    // Runs with or without a compiler.
    let cases = cases();
    assert!(cases.len() >= 12, "expected a substantial corpus");
    for c in &cases {
        assert!(
            !c.expect.description.trim().is_empty(),
            "{}: no description",
            c.name
        );
        assert!(
            !c.expect.sources.is_empty(),
            "{}: no sources listed",
            c.name
        );
        for s in &c.expect.sources {
            assert!(
                c.dir.join(s).exists(),
                "{}: expect.toml lists {s}, which does not exist",
                c.name
            );
        }
    }
}

#[test]
fn every_corpus_case_behaves_as_documented() {
    let Some(tc) = toolchain() else { return };
    eprintln!("using {}", tc.id().display());

    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::under(tmp.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();

    let mut failures: Vec<String> = Vec::new();
    for (i, case) in cases().iter().enumerate() {
        let sources_dir = tmp.path().join("sources").join(&case.name);
        stage_case(&case.dir, &sources_dir);
        let before = hash_tree(&sources_dir);

        let mut program = Program::new(&case.name);
        for s in &case.expect.sources {
            program.sources.push(SourceRef::new(sources_dir.join(s)));
        }
        if let Some(ll) = &case.expect.line_length {
            program.options.line_length = match ll.as_str() {
                "col132" => LineLength::Col132,
                "none" => LineLength::None,
                _ => LineLength::Col72,
            };
        }

        let layout = WorkLayout::new(paths.build_dir(i as u64 + 1));
        let cancel = AtomicBool::new(false);
        let t0 = std::time::Instant::now();
        let outcome = build::build(&guard, &tc, &layout, &program, None, &cancel);
        let built = t0.elapsed();

        let verdict = check(case, &outcome, &layout, &tc);
        eprintln!(
            "  {:<14} build={:>6.0}ms total={:>6.0}ms  {} errors={} warnings={} -> {}",
            case.name,
            built.as_secs_f64() * 1000.0,
            t0.elapsed().as_secs_f64() * 1000.0,
            if outcome.success { "built " } else { "FAILED" },
            outcome.errors,
            outcome.warnings,
            if verdict.is_ok() { "ok" } else { "MISMATCH" }
        );

        if let Err(e) = verdict {
            failures.push(format!(
                "[{}] {e}\n    {}",
                case.name,
                case.expect.description.trim()
            ));
        }

        // The promise, checked around every single case.
        let after = hash_tree(&sources_dir);
        if before != after {
            failures.push(format!("[{}] THE SOURCE FILES WERE MODIFIED", case.name));
        }
    }

    assert!(
        failures.is_empty(),
        "{} corpus case(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

fn check(
    case: &Case,
    outcome: &build::BuildOutcome,
    layout: &WorkLayout,
    tc: &Toolchain,
) -> Result<(), String> {
    let e = &case.expect;

    if e.success != outcome.success {
        return Err(format!(
            "expected build success={}, got {} ({} errors)\n--- compiler output ---\n{}",
            e.success,
            outcome.success,
            outcome.errors,
            outcome.raw.trim()
        ));
    }

    if !e.success {
        let all: String = outcome
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("\n");
        for needle in &e.error_contains {
            if !all.to_lowercase().contains(&needle.to_lowercase()) {
                return Err(format!(
                    "expected an error mentioning {needle:?}, got:\n{all}"
                ));
            }
        }
        if let Some(f) = &e.error_in_file {
            if !outcome
                .diagnostics
                .iter()
                .any(|d| d.file.as_deref() == Some(f.as_str()))
            {
                return Err(format!(
                    "expected an error attributed to {f:?} (the original name, not the staged one); got {:?}",
                    outcome.diagnostics.iter().map(|d| d.file.clone()).collect::<Vec<_>>()
                ));
            }
        }
        if e.undefined_symbol
            && !outcome
                .diagnostics
                .iter()
                .any(|d| d.undefined_symbol.is_some())
        {
            return Err("expected an undefined-reference diagnostic".into());
        }
        return Ok(());
    }

    if !e.run {
        return Ok(());
    }

    // The application builds programs; it does not run them. The corpus still
    // runs what was built, because that is the only way to prove the dialect
    // flags produce a program that behaves as the user's old compiler's did --
    // `static.f` printing 1, 2, 3 rather than uninitialised rubbish, for one.
    let exe = outcome
        .exe
        .clone()
        .ok_or("build succeeded but produced no executable")?;
    let run_dir = layout.root.join("corpus-run");
    std::fs::create_dir_all(&run_dir).map_err(|err| err.to_string())?;

    let (code, text) = run_built(tc, &exe, &run_dir, &e.stdin)?;

    if let Some(expected) = e.exit_code {
        if code != Some(expected) {
            return Err(format!(
                "expected exit code {expected}, got {code:?}\noutput:\n{text}"
            ));
        }
    }
    for needle in &e.stdout_contains {
        if !text.contains(needle.as_str()) {
            return Err(format!(
                "expected output to contain {needle:?}, got:\n{text}"
            ));
        }
    }
    for name in &e.creates_beside_the_program {
        if !run_dir.join(name).exists() {
            return Err(format!(
                "expected the program to create {name} in {}",
                run_dir.display()
            ));
        }
    }
    Ok(())
}

/// Vietnamese diacritics everywhere a path can carry them.
///
/// On a Vietnamese Windows install the application's own working directory is
/// under `C:\Users\Nguyễn Văn A\AppData\Local\...`, and the user's sources sit in
/// folders the user named himself. Those paths become the compiler's working
/// directory, its `TMPDIR`, and the `-I` it resolves `INCLUDE` through. A MinGW
/// gfortran receives argv converted through the process ANSI codepage, so this
/// is the case most likely to break in a way nobody would guess from the error.
///
/// The `INCLUDE` matters: it forces the compiler to *open a file* through the
/// non-ASCII path rather than merely accept a flag containing one.
#[test]
fn paths_full_of_vietnamese_diacritics_build_and_run() {
    let Some(tc) = toolchain() else { return };

    let tmp = tempfile::tempdir().unwrap();
    let user_dir = tmp
        .path()
        .join("Nguyễn Văn A")
        .join("Tài liệu")
        .join("Dự án Đường sắt");
    std::fs::create_dir_all(&user_dir).unwrap();
    std::fs::write(
        user_dir.join("THAMSO.INC"),
        b"      REAL HESO\n      COMMON /KHOI/ HESO\n",
    )
    .unwrap();
    std::fs::write(
        user_dir.join("CHINH.FOR"),
        "      PROGRAM CHINH\n               INCLUDE 'THAMSO.INC'\n               HESO = 2.5\n               WRITE (*,*) 'HESO =', HESO\n               END\n"
            .as_bytes(),
    )
    .unwrap();

    // The application's own directories carry the diacritics too.
    let paths = AppPaths::under(
        tmp.path()
            .join("Nguyễn Văn A")
            .join("AppData")
            .join("Local"),
    );
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let layout = WorkLayout::new(paths.build_dir(1));

    let mut program = Program::new("Tính dầm bê tông");
    program
        .sources
        .push(SourceRef::new(user_dir.join("CHINH.FOR")));

    let outcome = build::build(
        &guard,
        &tc,
        &layout,
        &program,
        None,
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert!(
        outcome.success,
        "a build under Vietnamese paths failed:\n{}",
        outcome.raw
    );

    let exe = outcome.exe.unwrap();
    let run_dir = layout.root.join("corpus-run");
    std::fs::create_dir_all(&run_dir).unwrap();
    let (code, text) = run_built(&tc, &exe, &run_dir, "").expect("the program should run");
    assert_eq!(code, Some(0), "output was:\n{text}");
    assert!(
        text.contains("2.50"),
        "the INCLUDE was not resolved through the non-ASCII path; output:\n{text}"
    );
}

/// Run a built program with a fixed input and collect what it printed.
fn run_built(
    tc: &Toolchain,
    exe: &Path,
    cwd: &Path,
    stdin: &str,
) -> Result<(Option<i32>, String), String> {
    use std::io::Write;
    use std::process::Stdio;

    // Through the bundle's launcher when there is one, so a Windows bundle can be
    // exercised from Linux under wine.
    let mut child = tc
        .run_binary(exe)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("could not start {}: {err}", exe.display()))?;

    if let Some(mut w) = child.stdin.take() {
        let _ = w.write_all(stdin.as_bytes());
        // Dropping the handle closes stdin, so a program reading until end of
        // file terminates instead of waiting forever.
    }
    let out = child
        .wait_with_output()
        .map_err(|err| format!("waiting for {}: {err}", exe.display()))?;

    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.code(), text))
}
