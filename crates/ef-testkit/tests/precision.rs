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

/// The low 32 bits of REAL 0.1, read through `EQUIVALENCE` as an INTEGER, in
/// each of the three precisions a user can choose. Each one worked out from the
/// definition of the format rather than taken from the compiler:
///
/// ```text
/// 32-bit single    0.1 rounds to 0x3DCCCCCD                     1036831949
/// 80-bit extended  significand round(2^67 / 10) = 0xCCCCCCCCCCCCCCCD,
///                  low word 0xCCCCCCCD                          -858993459
/// 64-bit double    0.1 rounds to 0x3FB999999999999A,
///                  low word 0x9999999A                         -1717986918
/// ```
const SINGLE: &str = "A  1036831949";
const EXTENDED: &str = "A  -858993459";
const DOUBLE: &str = "A -1717986918";

fn single() -> BuildOptions {
    BuildOptions {
        extended_precision: false,
        ..Default::default()
    }
}

#[test]
fn the_numbers_do_not_move_when_the_optimiser_does() {
    // The optimiser is allowed to make the program faster. It is not allowed to
    // make it give different answers, and GCC only does that when asked -- with
    // -ffast-math and its relatives, which this application never passes. The
    // day one of those arrives by way of the advanced "extra flags" box, or by
    // way of a well-meant default, this is what notices.
    //
    // Checked in both precisions a user can pick, because it is a property of
    // each. It is also the reason extended precision is done by making REAL
    // 80 bits wide rather than by using the x87 registers: the registers give a
    // different answer at -O0 than at -O1.
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let mut n = 0;

    for (name, base, fact) in [
        ("80-bit, the default", BuildOptions::default(), EXTENDED),
        ("32-bit", single(), SINGLE),
    ] {
        let mut at = |level| {
            n += 1;
            output_with(
                &tc,
                tmp.path(),
                n,
                BuildOptions {
                    opt_level: level,
                    ..base.clone()
                },
            )
        };
        let o0 = at(OptLevel::O0);
        let o1 = at(OptLevel::O1);
        let o2 = at(OptLevel::O2);
        assert_eq!(o0, o1, "{name}: -O0 and -O1 disagree:\n{o0}\nvs\n{o1}");
        assert_eq!(o1, o2, "{name}: -O1 and -O2 disagree:\n{o1}\nvs\n{o2}");
        assert!(o1.contains(fact), "{name}: expected {fact:?} in\n{o1}");
    }
}

#[test]
fn each_precision_gives_its_own_answer_and_the_default_is_80_bit() {
    // Three settings, three different bit patterns, each predicted from the
    // format's definition. That does two jobs at once: it pins what the
    // application ships (80-bit), and it shows these assertions are actually
    // watching the arithmetic -- if all three came out the same, they would be
    // passing for some reason that has nothing to do with precision.
    //
    // The last case is the one a user could get wrong by accident: both
    // options redefine REAL, and an explicit choice of 8 bytes has to win over
    // an 80-bit default that nobody chose.
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();

    let shipped = output_with(&tc, tmp.path(), 1, BuildOptions::default());
    let narrow = output_with(&tc, tmp.path(), 2, single());
    let both = output_with(
        &tc,
        tmp.path(),
        3,
        BuildOptions {
            default_real8: true,
            ..Default::default()
        },
    );

    assert!(
        shipped.contains(EXTENDED),
        "the shipped settings must carry REAL at 80 bits:\n{shipped}"
    );
    assert!(
        narrow.contains(SINGLE),
        "turning extended precision off must give 32-bit REAL:\n{narrow}"
    );
    assert!(
        both.contains(DOUBLE),
        "an explicit 8-byte REAL must win over the 80-bit default:\n{both}"
    );
}
