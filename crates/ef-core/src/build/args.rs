//! Building the compiler's argument vector.
//!
//! Pure functions, deliberately: they are the single most test-dense part of the
//! project, and every flag choice here has a reason recorded against it.
//!
//! Paths are relative to the build directory (which is the child's cwd) wherever
//! possible. That keeps the user's possibly-non-ASCII work path out of argv entirely.

use crate::paths::WorkLayout;
use crate::project::{BuildOptions, Dialect, Preprocess};
use crate::toolchain::probe::FlagCapabilities;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::stage::{StagedLibrary, StagedSource};

/// Warnings we always ask for. Surfaced in a details pane, never fatal:
/// `-Werror` on a novice's 1985 code would make the tool unusable.
const WARNINGS: &[&str] = &["-Wall", "-Wsurprising", "-Wtabs", "-Wextra"];

fn dialect_flags(opts: &BuildOptions, caps: &FlagCapabilities, out: &mut Vec<OsString>) {
    match opts.dialect {
        Dialect::Legacy if caps.std_legacy => {
            // Restores the five features Fortran 90 deleted — arithmetic IF, PAUSE,
            // ASSIGN, assigned GOTO, REAL DO variables — and implies
            // -fallow-argument-mismatch, which legacy code needs constantly.
            out.push("-std=legacy".into());
        }
        Dialect::Legacy => {}
        Dialect::Standard => out.push("-std=f95".into()),
    }

    // Always force fixed form: never rely on the file extension for this.
    out.push("-ffixed-form".into());
    out.push(opts.line_length.flag().into());

    if opts.dec_extensions && caps.dec {
        // STRUCTURE/RECORD/UNION/MAP, `.` member access, `$` in identifiers, Cray
        // pointers, IIAND/JIAND variants: the VAX/DEC lineage that Microsoft
        // Fortran, Lahey, Watcom and DVF all inherited.
        out.push("-fdec".into());
    }
    if opts.static_storage {
        // DOS-era compilers used static storage and zeroed it. Code that relies on
        // a local keeping its value between calls, or on an accumulator starting at
        // zero, breaks silently under modern stack allocation. This is the single
        // most common cause of "it gives different numbers than it did in 1987".
        if caps.no_automatic {
            out.push("-fno-automatic".into());
        }
        if caps.init_local_zero {
            out.push("-finit-local-zero".into());
        }
    }
    if opts.d_lines_as_code && caps.d_lines_as_code {
        out.push("-fd-lines-as-code".into());
    }
    if opts.default_real8 && caps.default_real8 {
        out.push("-fdefault-real-8".into());
    }
    if caps.no_range_check {
        // Old code deliberately overflows hex/octal constants in DATA statements.
        out.push("-fno-range-check".into());
    }
    if caps.allow_invalid_boz {
        out.push("-fallow-invalid-boz".into());
    }
    if opts.big_stack && caps.max_stack_var_size {
        // Windows gives a thread 1 MB of stack by default; a 40 MB local array
        // crashes on entry.
        out.push("-fmax-stack-var-size=0".into());
    }
}

/// Arguments to compile one staged source to one object file.
///
/// `bundle_flags` come from the toolchain's own `bundle.toml`. A relocatable
/// Linux bundle uses them to point at its own sysroot; without them it would
/// silently fall back to the host's, which is the "please install glibc-devel"
/// failure this product exists to avoid. They go first so nothing can precede
/// them, and they are never second-guessed here.
pub fn compile_args(
    caps: &FlagCapabilities,
    bundle_flags: &[OsString],
    opts: &BuildOptions,
    src: &StagedSource,
    layout: &WorkLayout,
    include_dirs: &[PathBuf],
) -> Vec<OsString> {
    let mut a: Vec<OsString> = Vec::new();
    a.extend(bundle_flags.iter().cloned());
    a.push("-c".into());

    dialect_flags(opts, caps, &mut a);
    a.push(opts.opt_level.flag().into());

    if caps.max_errors {
        // A runaway error cascade can emit hundreds of megabytes.
        a.push("-fmax-errors=25".into());
    }
    if caps.diagnostics_plain {
        a.push("-fdiagnostics-color=never".into());
    }
    for w in WARNINGS {
        a.push((*w).into());
    }

    // Modules and the object both land inside the work tree, never beside the user's source.
    a.push("-J".into());
    a.push(rel(layout, &layout.module()));

    // Their own directories, read-only, so `INCLUDE 'COMMON.INC'` still resolves.
    for dir in include_dirs {
        let mut inc = OsString::from("-I");
        inc.push(dir.as_os_str());
        a.push(inc);
    }

    for f in &opts.extra_flags {
        if !f.trim().is_empty() {
            a.push(f.into());
        }
    }

    // -x forces the language regardless of the file name; the staged name already
    // guarantees it, but the two together mean no suffix surprise is possible.
    a.push("-x".into());
    a.push(language_name(opts.preprocess).into());
    a.push(rel(layout, &src.staged));
    a.push("-x".into());
    a.push("none".into());

    a.push("-o".into());
    a.push(rel(layout, &src.obj));
    a
}

/// Arguments to compile the pause shim.
///
/// Deliberately not `compile_args`: the shim is ours, written in free-form
/// modern Fortran, and must not inherit the legacy dialect flags their code
/// needs — `-std=legacy` and `-ffixed-form` would reject it on sight.
pub fn shim_compile_args(
    bundle_flags: &[OsString],
    src: &Path,
    obj: &Path,
    layout: &WorkLayout,
) -> Vec<OsString> {
    let mut a: Vec<OsString> = Vec::new();
    a.extend(bundle_flags.iter().cloned());
    a.push("-c".into());
    a.push("-ffree-form".into());
    a.push("-O1".into());
    a.push("-fdiagnostics-color=never".into());
    a.push("-J".into());
    a.push(rel(layout, &layout.module()));
    a.push(rel(layout, src));
    a.push("-o".into());
    a.push(rel(layout, obj));
    a
}

/// Arguments to link the objects into the final executable.
///
/// `libs` are their own pre-compiled libraries, and they go **after** every object
/// file. That is not cosmetic: a static archive contributes only the members that
/// resolve symbols already referenced by something earlier on the line, so a
/// library placed first contributes nothing at all and the link fails with
/// undefined references to the very functions the library defines.
pub fn link_args(
    caps: &FlagCapabilities,
    bundle_flags: &[OsString],
    opts: &BuildOptions,
    objs: &[PathBuf],
    libs: &[StagedLibrary],
    layout: &WorkLayout,
    exe_suffix: &str,
) -> Vec<OsString> {
    let mut a: Vec<OsString> = Vec::new();
    a.extend(bundle_flags.iter().cloned());
    for o in objs {
        a.push(rel(layout, o));
    }
    for l in libs {
        a.push(rel(layout, &l.staged));
    }
    // Name the output with the target's suffix explicitly. MinGW's linker
    // appends `.exe` on its own, so leaving it off would put the program
    // somewhere the build does not look for it.
    a.push("-o".into());
    a.push(rel(layout, &layout.exe_with_suffix(exe_suffix)));

    if caps.static_runtime {
        // So the produced .exe runs on a machine that has never seen gfortran.
        a.push("-static-libgfortran".into());
        a.push("-static-libgcc".into());
    }
    if caps.static_full {
        a.push("-static".into());
    }
    if opts.big_stack {
        // Raise the Windows stack reservation for programs with large local arrays.
        a.push("-Wl,--stack,16777216".into());
    }
    if opts.keep_window_open && exe_suffix.eq_ignore_ascii_case(".exe") {
        // Routes every call to exit() through the shim's __wrap_exit. Without
        // this flag the shim object is linked and never reached, which is what
        // makes the option safe to leave in the link line.
        a.push("-Wl,--wrap=exit".into());
    }
    if opts.strip_symbols && caps.strip {
        // A statically linked Fortran program carries a lot of symbol table and
        // debug information the user will never look at. Dropping it makes the file the user
        // keeps and copies around much smaller.
        a.push("-s".into());
    }
    if caps.diagnostics_plain {
        a.push("-fdiagnostics-color=never".into());
    }
    a
}

fn language_name(p: Preprocess) -> &'static str {
    match p {
        Preprocess::Never => "f77",
        Preprocess::Always => "f77-cpp-input",
    }
}

/// Express a path inside the work tree relative to the build directory, with an
/// explicit `./` so it can never be mistaken for an option.
fn rel(layout: &WorkLayout, p: &Path) -> OsString {
    match p.strip_prefix(&layout.root) {
        Ok(r) => {
            let mut s = OsString::from(".");
            s.push(std::path::MAIN_SEPARATOR.to_string());
            s.push(r.as_os_str());
            s
        }
        Err(_) => p.as_os_str().to_os_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{LineLength, OptLevel};

    fn fixture() -> (WorkLayout, StagedSource) {
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let src = StagedSource {
            original: PathBuf::from("/home/user/SOLVER.FOR"),
            staged: layout.src().join("solver.f"),
            staged_name: "solver.f".into(),
            display_name: "SOLVER.FOR".into(),
            obj: layout.obj().join("solver.o"),
            sha256: String::new(),
        };
        (layout, src)
    }

    fn strings(v: &[OsString]) -> Vec<String> {
        v.iter().map(|s| s.to_string_lossy().to_string()).collect()
    }

    #[test]
    fn default_compile_line_is_maximally_permissive() {
        let (layout, src) = fixture();
        let a = strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &src,
            &layout,
            &[PathBuf::from("/home/user")],
        ));
        for expected in [
            "-c",
            "-std=legacy",
            "-ffixed-form",
            "-ffixed-line-length-72",
            "-fdec",
            "-fno-automatic",
            "-finit-local-zero",
            "-fno-range-check",
            "-fallow-invalid-boz",
            "-fmax-errors=25",
            "-O1",
            "-I/home/user",
        ] {
            assert!(
                a.contains(&expected.to_string()),
                "missing {expected} in {a:?}"
            );
        }
        assert!(!a.iter().any(|s| s == "-Werror"), "never -Werror");
        assert!(!a.iter().any(|s| s == "-save-temps"), "never -save-temps");
    }

    #[test]
    fn the_language_is_forced_to_fixed_form_f77_around_the_file_only() {
        let (layout, src) = fixture();
        let a = strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &src,
            &layout,
            &[],
        ));
        let i = a.iter().position(|s| s == "-x").unwrap();
        assert_eq!(a[i + 1], "f77");
        assert!(a[i + 2].ends_with("solver.f"));
        assert_eq!(a[i + 3], "-x");
        assert_eq!(a[i + 4], "none");
    }

    #[test]
    fn preprocessing_is_only_requested_when_asked_for() {
        let (layout, src) = fixture();
        let mut o = BuildOptions::default();
        assert!(!strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &src,
            &layout,
            &[]
        ))
        .contains(&"f77-cpp-input".to_string()));

        o.preprocess = Preprocess::Always;
        assert!(strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &src,
            &layout,
            &[]
        ))
        .contains(&"f77-cpp-input".to_string()));
    }

    #[test]
    fn line_length_never_silently_becomes_unlimited() {
        let (layout, src) = fixture();
        let mut o = BuildOptions::default();
        let a = strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &src,
            &layout,
            &[],
        ));
        assert!(a.contains(&"-ffixed-line-length-72".to_string()));

        o.line_length = LineLength::Col132;
        let a = strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &src,
            &layout,
            &[],
        ));
        assert!(a.contains(&"-ffixed-line-length-132".to_string()));
        assert!(!a.contains(&"-ffixed-line-length-72".to_string()));
    }

    #[test]
    fn unavailable_flags_are_dropped_rather_than_failing_the_build() {
        let (layout, src) = fixture();
        let caps = FlagCapabilities::conservative();
        let a = strings(&compile_args(
            &caps,
            &[],
            &BuildOptions::default(),
            &src,
            &layout,
            &[],
        ));
        assert!(!a.contains(&"-fdec".to_string()));
        assert!(!a.contains(&"-std=legacy".to_string()));
        // but the mandatory parts survive
        assert!(a.contains(&"-c".to_string()));
        assert!(a.contains(&"-ffixed-form".to_string()));
    }

    #[test]
    fn outputs_stay_inside_the_work_tree() {
        let (layout, src) = fixture();
        let a = strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &src,
            &layout,
            &[PathBuf::from("/home/user")],
        ));
        let o = a.iter().position(|s| s == "-o").unwrap();
        assert!(a[o + 1].contains("obj"), "{:?}", a[o + 1]);
        let j = a.iter().position(|s| s == "-J").unwrap();
        assert!(a[j + 1].contains("mod"));
        // the only absolute path is the read-only include dir
        let abs: Vec<_> = a.iter().filter(|s| s.starts_with('/')).collect();
        assert!(abs.is_empty(), "unexpected absolute paths: {abs:?}");
    }

    #[test]
    fn link_line_statically_links_the_fortran_runtime() {
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let objs = vec![layout.obj().join("a.o"), layout.obj().join("b.o")];
        let a = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &objs,
            &[],
            &layout,
            "",
        ));
        assert!(a.contains(&"-static-libgfortran".to_string()));
        assert!(a.contains(&"-static-libgcc".to_string()));
        let o = a.iter().position(|s| s == "-o").unwrap();
        assert!(a[o + 1].contains("out"));
        // link order is the order the user arranged
        let ia = a.iter().position(|s| s.ends_with("a.o")).unwrap();
        let ib = a.iter().position(|s| s.ends_with("b.o")).unwrap();
        assert!(ia < ib);
    }

    fn staged_lib(layout: &WorkLayout, name: &str) -> StagedLibrary {
        StagedLibrary {
            original: PathBuf::from(format!("/home/user/{name}")),
            staged: layout.lib().join(name.to_ascii_lowercase()),
            display_name: name.into(),
        }
    }

    #[test]
    fn libraries_come_after_every_object_file() {
        // A static archive only contributes members that resolve symbols already
        // referenced to its left. Put it first and it contributes nothing, and the
        // link fails with undefined references to the very routines it contains.
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let objs = vec![layout.obj().join("a.o"), layout.obj().join("b.o")];
        let libs = vec![staged_lib(&layout, "mathlib.a")];
        let a = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &objs,
            &libs,
            &layout,
            "",
        ));
        let last_obj = a.iter().rposition(|s| s.ends_with(".o")).unwrap();
        let lib = a.iter().position(|s| s.ends_with("mathlib.a")).unwrap();
        assert!(lib > last_obj, "library must follow the objects, got {a:?}");
    }

    #[test]
    fn libraries_keep_the_order_he_arranged() {
        // Libraries that depend on each other only resolve in one direction, so
        // the user's ordering is preserved exactly as the sources' is.
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let libs = vec![
            staged_lib(&layout, "first.a"),
            staged_lib(&layout, "second.a"),
        ];
        let a = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &[],
            &libs,
            &layout,
            "",
        ));
        let i = a.iter().position(|s| s.ends_with("first.a")).unwrap();
        let j = a.iter().position(|s| s.ends_with("second.a")).unwrap();
        assert!(i < j);
    }

    #[test]
    fn libraries_are_named_relatively_so_his_path_stays_out_of_argv() {
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let libs = vec![staged_lib(&layout, "mathlib.a")];
        let a = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &[],
            &libs,
            &layout,
            "",
        ));
        assert!(
            a.iter().all(|s| !s.starts_with('/')),
            "no absolute paths on the link line: {a:?}"
        );
    }

    #[test]
    fn no_libraries_adds_nothing_at_all_to_the_link_line() {
        // The overwhelmingly common case: the user has no libraries, and the link line
        // must be exactly what it was before the feature existed.
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let objs = vec![layout.obj().join("a.o")];
        let a = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &objs,
            &[],
            &layout,
            "",
        ));
        assert!(
            !a.iter().any(|s| s.contains("lib") && s.starts_with('.')),
            "nothing from the lib directory should appear: {a:?}"
        );
    }

    #[test]
    fn the_window_option_is_off_and_changes_nothing_unless_asked_for() {
        // The overwhelmingly common case. An option that alters the link line
        // by default would put a wrap on every program anyone ever builds.
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let a = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &[],
            &[],
            &layout,
            ".exe",
        ));
        assert!(!a.iter().any(|s| s.contains("--wrap")), "{a:?}");
    }

    #[test]
    fn asking_for_it_wraps_exit_on_windows_only() {
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let o = BuildOptions {
            keep_window_open: true,
            ..Default::default()
        };
        let win = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &[],
            &[],
            &layout,
            ".exe",
        ));
        assert!(win.contains(&"-Wl,--wrap=exit".to_string()), "{win:?}");

        // Everywhere else the program is started from a terminal that stays put,
        // so there is nothing to keep open and nothing to wrap.
        let nix = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &[],
            &[],
            &layout,
            "",
        ));
        assert!(!nix.iter().any(|s| s.contains("--wrap")), "{nix:?}");
    }

    #[test]
    fn the_shim_is_compiled_with_its_own_dialect_not_theirs() {
        // It is modern free-form Fortran; -std=legacy and -ffixed-form would
        // reject it, and their code needs both.
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let a = strings(&shim_compile_args(
            &[],
            &layout.src().join("shim.f90"),
            &layout.obj().join("shim.o"),
            &layout,
        ));
        assert!(a.contains(&"-ffree-form".to_string()));
        for theirs in [
            "-std=legacy",
            "-ffixed-form",
            "-fdec",
            "-ffixed-line-length-72",
        ] {
            assert!(
                !a.contains(&theirs.to_string()),
                "{theirs} leaked into the shim"
            );
        }
    }

    #[test]
    fn stripping_is_off_unless_asked_for() {
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let a = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &[],
            &[],
            &layout,
            "",
        ));
        assert!(!a.contains(&"-s".to_string()));
    }

    #[test]
    fn stripping_is_requested_when_asked_for_and_available() {
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let o = BuildOptions {
            strip_symbols: true,
            ..Default::default()
        };
        let a = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &[],
            &[],
            &layout,
            "",
        ));
        assert!(a.contains(&"-s".to_string()));

        // and dropped, rather than failing the build, where the linker has no -s
        let mut caps = FlagCapabilities::optimistic();
        caps.strip = false;
        let a = strings(&link_args(&caps, &[], &o, &[], &[], &layout, ""));
        assert!(!a.contains(&"-s".to_string()));
    }

    #[test]
    fn stripping_is_a_link_time_choice_only() {
        // -s affects the linked output; putting it on a compile line would be
        // meaningless and would only slow every file down.
        let (layout, src) = fixture();
        let o = BuildOptions {
            strip_symbols: true,
            ..Default::default()
        };
        let a = strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &src,
            &layout,
            &[],
        ));
        assert!(!a.contains(&"-s".to_string()));
    }

    #[test]
    fn the_output_name_uses_the_targets_suffix_not_the_hosts() {
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let a = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &[],
            &[],
            &layout,
            ".exe",
        ));
        let o = a.iter().position(|s| s == "-o").unwrap();
        assert!(a[o + 1].ends_with("program.exe"), "got {:?}", a[o + 1]);
    }

    #[test]
    fn big_stack_raises_the_windows_stack_reservation() {
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let o = BuildOptions {
            big_stack: true,
            ..Default::default()
        };
        let a = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &[],
            &[],
            &layout,
            "",
        ));
        assert!(a.contains(&"-Wl,--stack,16777216".to_string()));
    }

    #[test]
    fn a_static_link_is_skipped_when_the_platform_cannot_do_it() {
        let layout = WorkLayout::new(PathBuf::from("/work/build-1"));
        let mut caps = FlagCapabilities::optimistic();
        caps.static_full = false; // typical Linux: no static glibc
        let a = strings(&link_args(
            &caps,
            &[],
            &BuildOptions::default(),
            &[],
            &[],
            &layout,
            "",
        ));
        assert!(!a.contains(&"-static".to_string()));
        assert!(a.contains(&"-static-libgfortran".to_string()));
    }

    #[test]
    fn extra_flags_are_passed_through_but_blanks_are_not() {
        let (layout, src) = fixture();
        let o = BuildOptions {
            extra_flags: vec!["-ffpe-trap=invalid".into(), "   ".into()],
            ..Default::default()
        };
        let a = strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &src,
            &layout,
            &[],
        ));
        assert!(a.contains(&"-ffpe-trap=invalid".to_string()));
        assert!(!a.iter().any(|s| s.trim().is_empty()));
    }

    #[test]
    fn bundle_flags_come_first_on_both_lines() {
        let (layout, src) = fixture();
        let bf: Vec<OsString> = vec!["--sysroot=/opt/t/sysroot".into(), "-B/opt/t/lib".into()];

        let a = strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &bf,
            &BuildOptions::default(),
            &src,
            &layout,
            &[],
        ));
        assert_eq!(a[0], "--sysroot=/opt/t/sysroot");
        assert_eq!(a[1], "-B/opt/t/lib");
        assert_eq!(a[2], "-c");

        let l = strings(&link_args(
            &FlagCapabilities::optimistic(),
            &bf,
            &BuildOptions::default(),
            &[layout.obj().join("a.o")],
            &[],
            &layout,
            "",
        ));
        assert_eq!(l[0], "--sysroot=/opt/t/sysroot");
        assert!(l.iter().any(|s| s.ends_with("a.o")));
    }

    #[test]
    fn no_bundle_means_no_extra_flags_at_all() {
        // The system-compiler case must be byte-identical to before bundles existed.
        let (layout, src) = fixture();
        let a = compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &BuildOptions::default(),
            &src,
            &layout,
            &[],
        );
        assert_eq!(a[0], OsString::from("-c"));
    }

    #[test]
    fn optimisation_level_is_honoured() {
        let (layout, src) = fixture();
        let o = BuildOptions {
            opt_level: OptLevel::O2,
            ..Default::default()
        };
        let a = strings(&compile_args(
            &FlagCapabilities::optimistic(),
            &[],
            &o,
            &src,
            &layout,
            &[],
        ));
        assert!(a.contains(&"-O2".to_string()));
        assert!(!a.contains(&"-O1".to_string()));
    }
}
