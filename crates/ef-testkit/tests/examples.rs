//! The examples in `examples/` must keep doing what their guide says.
//!
//! They are handed to someone as "these work" — and the broken one is handed
//! over as "this fails, and here is the option that fixes it". Both halves are
//! promises, and a default changing underneath them would break both silently.

use ef_core::build;
use ef_core::fs_guard::FsGuard;
use ef_core::paths::{AppPaths, WorkLayout};
use ef_core::project::{LineLength, Program, SourceRef};
use ef_core::toolchain::Toolchain;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

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

#[test]
fn every_example_behaves_the_way_its_guide_says() {
    let Some(tc) = toolchain() else { return };
    let dir = examples_dir();
    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::under(tmp.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();

    // (what the guide calls it, the files in the order it says to add them,
    //  the column setting, whether it should build)
    let cases: &[(&str, &[&str], LineLength, bool)] = &[
        ("basics", &["01-CO-BAN.FOR"], LineLength::Col72, true),
        (
            "loops and branching",
            &["02-VONG-LAP.FOR"],
            LineLength::Col72,
            true,
        ),
        (
            "subroutines across two files",
            &[
                "03-CHUONG-TRINH-CON/CHINH.FOR",
                "03-CHUONG-TRINH-CON/CONGCU.FOR",
            ],
            LineLength::Col72,
            true,
        ),
        (
            "file input and output",
            &["04-DOC-GHI-TEP.FOR"],
            LineLength::Col72,
            true,
        ),
        // The guide's whole point: it fails at 72 columns...
        (
            "broken past column 72",
            &["05-LOI-COT72.FOR"],
            LineLength::Col72,
            false,
        ),
        // ...and the option it names is the fix.
        (
            "...fixed at 132",
            &["05-LOI-COT72.FOR"],
            LineLength::Col132,
            true,
        ),
    ];

    let mut failures = Vec::new();
    for (n, (name, files, line_length, should_build)) in cases.iter().enumerate() {
        let mut program = Program::new(*name);
        program.options.line_length = *line_length;
        for f in *files {
            let path = dir.join(f);
            assert!(path.is_file(), "the guide names {f}, which is not there");
            program.sources.push(SourceRef::new(path));
        }

        let layout = WorkLayout::new(paths.build_dir(n as u64 + 1));
        let outcome = build::build(
            &guard,
            &tc,
            &layout,
            &program,
            None,
            &AtomicBool::new(false),
        );
        if outcome.success != *should_build {
            failures.push(format!(
                "[{name}] expected {}, got {} ({} errors)\n{}",
                if *should_build {
                    "a build"
                } else {
                    "a failure"
                },
                if outcome.success {
                    "a build"
                } else {
                    "a failure"
                },
                outcome.errors,
                outcome.raw.lines().take(4).collect::<Vec<_>>().join("\n")
            ));
            continue;
        }

        // Compiling is not the same as working. An earlier version of the file
        // I/O example built cleanly and died at run time on a FORMAT that did
        // not match what it read back, which is exactly what an example must
        // not do to someone learning from it.
        let Some(exe) = &outcome.exe else { continue };
        let run_in = tmp.path().join(format!("run-{n}"));
        std::fs::create_dir_all(&run_in).unwrap();
        let ran = ef_testkit::spawn_tolerating_busy(
            tc.run_binary(exe)
                .current_dir(&run_in)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped()),
        )
        .and_then(|c| c.wait_with_output());
        match ran {
            Ok(o) if o.status.success() && !o.stdout.is_empty() => {}
            Ok(o) => failures.push(format!(
                "[{name}] built, then exited {:?} with {} bytes of output\n{}",
                o.status.code(),
                o.stdout.len(),
                String::from_utf8_lossy(&o.stderr)
                    .lines()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("\n")
            )),
            Err(e) => failures.push(format!("[{name}] built but would not run: {e}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn the_guide_names_every_example_and_every_example_is_named() {
    let dir = examples_dir();
    let guide = std::fs::read_to_string(dir.join("DOC-TRUOC.txt")).expect("DOC-TRUOC.txt");
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if name == "DOC-TRUOC.txt" {
                continue;
            }
            assert!(
                guide.contains(&name),
                "{name} is in examples/ but the guide never mentions it"
            );
        }
    }
}
