//! Is the arithmetic the old compiler's arithmetic, and does it stay put?
//!
//! The default does REAL arithmetic the way the DOS compiler did: on the x87
//! unit, at extended precision inside each expression, rounding to 32 bits
//! whenever a value is stored, with no optimisation. These tests pin the four
//! things that makes true, each one chosen so it fails if the mode stops doing
//! its job:
//!
//!   * REAL stays four bytes -- the arithmetic moves, the storage does not, and
//!     that is what keeps libraries built elsewhere safe to link;
//!   * it keeps a small value that plain 32-bit arithmetic loses;
//!   * the optimisation level asked for does not change a digit; and
//!   * decisions made by comparing computed values go the way the old compiler
//!     sent them -- which widening REAL to 80 bits did not.

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

/// Build and run a program given as source text, and return what it prints.
fn run_source(tc: &Toolchain, tmp: &Path, n: u64, options: BuildOptions, src: &str) -> String {
    let dir = tmp.join(format!("src-{n}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("PROG.FOR"), src).unwrap();

    let paths = AppPaths::under(tmp.join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let mut program = Program::new("arithmetic");
    program.options = options;
    program.sources.push(SourceRef::new(dir.join("PROG.FOR")));

    let layout = WorkLayout::new(paths.build_dir(1000 + n));
    let outcome = build::build(&guard, tc, &layout, &program, None, &AtomicBool::new(false));
    assert!(outcome.success, "it must build: {}", outcome.raw);
    let exe = outcome.exe.expect("a built program");
    let run_in = tmp.join(format!("run-src-{n}"));
    std::fs::create_dir_all(&run_in).unwrap();
    let out = ef_testkit::spawn_tolerating_busy(
        tc.run_binary(&exe)
            .current_dir(&run_in)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
    )
    .and_then(|c| c.wait_with_output())
    .expect("it must run");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn plain_32_bit() -> BuildOptions {
    BuildOptions {
        extended_precision: false,
        ..Default::default()
    }
}

// A raw string, not a `\`-continued one: that form strips each line's leading
// whitespace, and fixed-form Fortran lives in columns 7 onward.

/// A small load added to a large one and taken away again, all in one
/// expression; and a total built up over many statements.
const LARGE_AND_SMALL: &str = r"      PROGRAM LS
      REAL A, B, C, D, T
      INTEGER I
      A = 1.0E7
      B = 0.3
      C = (A + B) - A
      WRITE (*,'(A,F12.8)') ' EXPR', C
      D = 0.1
      T = 0.0
      DO 10 I = 1, 100
         T = T + D * D
   10 CONTINUE
      WRITE (*,'(A,F14.10)') ' LOOP', T
      END
";

/// Two exact-equality decisions that widening REAL to 80 bits got the other
/// way round from the old compiler: of 1600 like them, 73 changed.
const DECISIONS: &str = r"      PROGRAM DC
      REAL A, B
      A = 3.0 / 21.0
      B = A * 21.0
      IF (B .EQ. 3.0) THEN
         WRITE (*,*) 'D1 Y'
      ELSE
         WRITE (*,*) 'D1 N'
      END IF
      A = 5.0 / 37.0
      B = A * 37.0
      IF (B .EQ. 5.0) THEN
         WRITE (*,*) 'D2 Y'
      ELSE
         WRITE (*,*) 'D2 N'
      END IF
      END
";

#[test]
fn the_default_keeps_real_at_four_bytes() {
    // 0.1 stored and read back through `EQUIVALENCE`: 0x3DCCCCCD is single
    // precision's 0.1, worked out from the format, and the default must store
    // exactly that. REAL keeping its size is what lets a library compiled
    // elsewhere receive the values it expects. 8-byte REAL, on top, must still
    // change it (0x3FB999999999999A, low word 0x9999999A) -- which is what shows
    // this assertion is watching storage at all.
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();

    let shipped = output_with(&tc, tmp.path(), 1, BuildOptions::default());
    assert!(
        shipped.contains("A  1036831949"),
        "REAL must stay 4 bytes:\n{shipped}"
    );

    let wide = output_with(
        &tc,
        tmp.path(),
        2,
        BuildOptions {
            default_real8: true,
            ..Default::default()
        },
    );
    assert!(
        wide.contains("A -1717986918"),
        "8-byte REAL must still apply:\n{wide}"
    );
}

#[test]
fn the_default_keeps_what_plain_32_bit_arithmetic_loses() {
    // (1.0E7 + 0.3) - 1.0E7: 32 bits cannot hold both numbers at once, so it
    // rounds the 0.3 away. The old compiler held the whole expression at
    // extended precision and kept it. Across many statements both round every
    // stored value, so the total agrees -- as it did on the old machine.
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();

    let old_way = run_source(&tc, tmp.path(), 1, BuildOptions::default(), LARGE_AND_SMALL);
    let plain = run_source(&tc, tmp.path(), 2, plain_32_bit(), LARGE_AND_SMALL);

    assert!(
        old_way.contains("EXPR  0.30000001"),
        "the 0.3 must survive:\n{old_way}"
    );
    assert!(
        plain.contains("EXPR  0.00000000"),
        "plain 32-bit loses it:\n{plain}"
    );
    for out in [&old_way, &plain] {
        assert!(
            out.contains("LOOP  0.9999993443"),
            "both round what they store:\n{out}"
        );
    }
}

#[test]
fn the_optimisation_level_asked_for_does_not_change_a_digit() {
    // For the old compiler's arithmetic this holds only because the build
    // refuses to optimise it: at -O1 values stay in registers across statements
    // and the 0.3 above is lost again. So asking for -O1 or -O2 must make no
    // difference. Plain 32-bit is checked too; it holds there on its own.
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let mut n = 0;

    for (name, base) in [
        ("the old compiler's way", BuildOptions::default()),
        ("32-bit", plain_32_bit()),
    ] {
        let mut at = |level| {
            n += 1;
            let o = BuildOptions {
                opt_level: level,
                ..base.clone()
            };
            run_source(&tc, tmp.path(), n, o, LARGE_AND_SMALL)
        };
        let (o0, o1, o2) = (at(OptLevel::O0), at(OptLevel::O1), at(OptLevel::O2));
        assert_eq!(o0, o1, "{name}: -O0 and -O1 disagree:\n{o0}\nvs\n{o1}");
        assert_eq!(o1, o2, "{name}: -O1 and -O2 disagree:\n{o1}\nvs\n{o2}");
    }
}

#[test]
fn decisions_go_the_way_the_old_compiler_sent_them() {
    // (3/21)*21 = 3 and (5/37)*37 = 5, compared exactly after storing. The old
    // compiler stored at 32 bits and said no, then yes. Widening REAL to 80 bits
    // said yes, then no -- the kind of change that sends a program down a
    // different branch. The default must say what the old compiler said.
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();

    let out = run_source(&tc, tmp.path(), 1, BuildOptions::default(), DECISIONS);
    assert!(
        out.contains("D1 N"),
        "(3/21)*21 = 3 must be false, as it was:\n{out}"
    );
    assert!(
        out.contains("D2 Y"),
        "(5/37)*37 = 5 must be true, as it was:\n{out}"
    );
}
