//! Do the numbers move when the settings move?
//!
//! `corpus/precision` pins what the arithmetic *is* under the settings the
//! application ships, against values worked out from IEEE 754 rather than from
//! this compiler. This file asks the two questions that pinning alone leaves
//! open, and they pull in opposite directions:
//!
//!   * changing something that must not affect arithmetic -- the optimiser --
//!     must not change a single digit, and
//!   * changing something that *is* about precision must change the digits,
//!     because otherwise the corpus assertions could be passing for some reason
//!     that has nothing to do with the arithmetic they claim to describe.
//!
//! The second is the one that keeps the first honest.

use ef_core::build;
use ef_core::fs_guard::FsGuard;
use ef_core::paths::{AppPaths, WorkLayout};
use ef_core::project::{BuildOptions, OptLevel, Program, SourceRef};
use ef_core::toolchain::Toolchain;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn case_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/precision")
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

/// Build `corpus/precision` with the given options and return what it prints.
fn output_with(tc: &Toolchain, tmp: &Path, n: u64, options: BuildOptions) -> String {
    let paths = AppPaths::under(tmp.join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let dir = case_dir();

    let mut program = Program::new("precision");
    program.options = options;
    for f in ["PRECIS.FOR", "OPAQUE.FOR"] {
        program.sources.push(SourceRef::new(dir.join(f)));
    }

    let layout = WorkLayout::new(paths.build_dir(n));
    let outcome = build::build(&guard, tc, &layout, &program, None, &AtomicBool::new(false));
    assert!(
        outcome.success,
        "the precision program must build: {}",
        outcome.raw.lines().take(6).collect::<Vec<_>>().join("\n")
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
    .expect("the precision program must run");
    assert!(
        out.status.success(),
        "it must exit cleanly: {:?}",
        out.status
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_numbers_do_not_move_when_the_optimiser_does() {
    // The optimiser is allowed to make the program faster. It is not allowed to
    // make it give different answers, and GCC only does that when asked -- with
    // -ffast-math and its relatives, which this application never passes. The
    // day one of those arrives by way of the advanced "extra flags" box, or by
    // way of a well-meant default, this is what notices.
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();

    let at = |n, level| {
        output_with(
            &tc,
            tmp.path(),
            n,
            BuildOptions {
                opt_level: level,
                ..Default::default()
            },
        )
    };
    let o0 = at(1, OptLevel::O0);
    let o1 = at(2, OptLevel::O1);
    let o2 = at(3, OptLevel::O2);

    assert_eq!(
        o0, o1,
        "-O0 and -O1 disagree about arithmetic:\n{o0}\nvs\n{o1}"
    );
    assert_eq!(
        o1, o2,
        "-O1 and -O2 disagree about arithmetic:\n{o1}\nvs\n{o2}"
    );
    assert!(o1.contains("A  1036831949"), "sanity: {o1}");
}

#[test]
fn turning_on_double_precision_really_does_change_the_numbers() {
    // This is what gives the corpus assertions their teeth. `default_real8` is
    // the one option in the interface that rewrites what REAL means, and if the
    // pinned bit patterns did not move when it is switched on, they would not
    // be measuring precision at all.
    //
    // It also documents the cost of that switch, which is easy to reach for and
    // not obviously destructive: every REAL in the user's program changes size,
    // so a file written by one build is not readable by the other.
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();

    let shipped = output_with(&tc, tmp.path(), 1, BuildOptions::default());
    let promoted = output_with(
        &tc,
        tmp.path(),
        2,
        BuildOptions {
            default_real8: true,
            ..Default::default()
        },
    );

    assert!(
        shipped.contains("A  1036831949"),
        "the shipped settings must give single-precision 0.1:\n{shipped}"
    );
    assert_ne!(
        shipped, promoted,
        "promoting REAL to eight bytes changed nothing, so these assertions \
         are not actually watching the arithmetic:\n{shipped}"
    );
    assert!(
        !promoted.contains("A  1036831949"),
        "with REAL promoted, the single-precision bit pattern must not survive:\n{promoted}"
    );
}
