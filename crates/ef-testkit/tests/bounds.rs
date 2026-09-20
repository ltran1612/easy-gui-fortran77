//! Reading past the end of an array must stop the program, not invent a number.
//!
//! This is the one default in the application that departs from reproducing a
//! 1980s compiler exactly, so it is worth showing both halves of why. Without
//! the check, `ARR(7)` on a three-element array returns whatever sits next in
//! memory: not a crash, not a NaN, just a plausible value that flows into a
//! result and is believed. For arithmetic someone builds a bridge on, stopping
//! is the better answer.

use ef_core::build;
use ef_core::fs_guard::FsGuard;
use ef_core::paths::{AppPaths, WorkLayout};
use ef_core::project::{BuildOptions, Program, SourceRef};
use ef_core::toolchain::Toolchain;
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// Fills a three-element array, then reads the seventh.
// A raw string, and not a `\`-continued one: that form strips the leading
// whitespace of each line, and fixed-form Fortran lives in columns 7 onward.
const PAST_THE_END: &str = r"      PROGRAM SPANS
      REAL SPAN(3)
      INTEGER I
      DO 10 I = 1, 3
         SPAN(I) = REAL(I) * 100.0
   10 CONTINUE
      I = 7
      WRITE (*,*) 'VALUE', SPAN(I)
      END
";

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

/// Build and run the program, returning (exit ok, stdout, stderr).
fn run_with(tc: &Toolchain, tmp: &Path, n: u64, options: BuildOptions) -> (bool, String, String) {
    let paths = AppPaths::under(tmp.join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();

    let dir = tmp.join(format!("src-{n}"));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("SPANS.FOR");
    std::fs::write(&file, PAST_THE_END).unwrap();

    let mut program = Program::new("spans");
    program.options = options;
    program.sources.push(SourceRef::new(&file));

    let layout = WorkLayout::new(paths.build_dir(n));
    let outcome = build::build(&guard, tc, &layout, &program, None, &AtomicBool::new(false));
    assert!(
        outcome.success,
        "it must still compile either way: {}",
        outcome.raw.lines().take(5).collect::<Vec<_>>().join("\n")
    );
    let exe = outcome.exe.expect("a built program");

    let run_in = tmp.join(format!("run-{n}"));
    std::fs::create_dir_all(&run_in).unwrap();
    let out = ef_testkit::spawn_tolerating_busy(
        tc.run_binary(&exe)
            .current_dir(&run_in)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
    )
    .and_then(|c| c.wait_with_output())
    .expect("the program must run");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_shipped_settings_stop_a_program_that_reads_past_an_array() {
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();

    let (ok, stdout, stderr) = run_with(&tc, tmp.path(), 1, BuildOptions::default());
    assert!(
        !ok,
        "the program must not be allowed to finish: stdout {stdout:?}"
    );
    let said = format!("{stdout}{stderr}");
    assert!(
        said.contains("SPAN") || said.to_lowercase().contains("bound"),
        "and must say which array and which position; got {said:?}"
    );
}

#[test]
fn turning_it_off_shows_what_it_was_protecting_against() {
    // The other half, and the reason the default is worth its cost: without the
    // check the same program finishes quite happily and prints a number that is
    // simply wrong, with nothing to mark it as wrong.
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();

    let (ok, stdout, _) = run_with(
        &tc,
        tmp.path(),
        1,
        BuildOptions {
            check_bounds: false,
            ..Default::default()
        },
    );
    assert!(ok, "without the check the program runs to completion");
    assert!(
        stdout.contains("VALUE"),
        "and prints a value it had no right to: {stdout:?}"
    );
    // Whatever it printed, it is not one of the three the program actually set.
    for real in ["100.000000", "200.000000", "300.000000"] {
        assert!(
            !stdout.contains(real),
            "the seventh element must not be one of the three that exist: {stdout:?}"
        );
    }
}
