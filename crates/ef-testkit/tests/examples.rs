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
        ("one file", &["DAMBTCT.FOR"], LineLength::Col72, true),
        (
            "several files and an INCLUDE",
            &["NHIEUTEP/CHINH.FOR", "NHIEUTEP/TINHTOAN.FOR"],
            LineLength::Col72,
            true,
        ),
        // The guide's whole point: it fails at 72 columns...
        (
            "broken past column 72",
            &["LOI-COT72.FOR"],
            LineLength::Col72,
            false,
        ),
        // ...and the option it names is the fix.
        (
            "...fixed at 132",
            &["LOI-COT72.FOR"],
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
