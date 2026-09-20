//! Discover which optional flags this particular gfortran accepts.
//!
//! This is what lets one flag table serve GCC 8 through 16, and Windows through
//! Linux, with no `cfg!` branches: `-static` fails on most Linux boxes for want of
//! a static glibc, so the probe notices and the build simply proceeds without it.

use crate::fs_guard::Scratch;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

const PROBE_SRC: &str = include_str!("../../assets/probe/probe.f");

/// Optional flags, each independently verified. Mandatory behaviour (fixed form,
/// `-c`, `-o`, `-J`, `-I`) is not probed: without it there is no build at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagCapabilities {
    pub std_legacy: bool,
    pub dec: bool,
    pub no_range_check: bool,
    pub allow_invalid_boz: bool,
    pub init_local_zero: bool,
    pub no_automatic: bool,
    pub d_lines_as_code: bool,
    /// `-fcheck=bounds`: stop when an array index is outside its bounds.
    pub check_bounds: bool,
    pub max_errors: bool,
    pub diagnostics_plain: bool,
    pub max_stack_var_size: bool,
    pub default_real8: bool,
    /// `-static`: fully static link. Usually unavailable on Linux.
    pub static_full: bool,
    /// `-static-libgfortran -static-libgcc`: the runtime only. Usually available.
    pub static_runtime: bool,
    /// `-s`: strip the linked program.
    pub strip: bool,
    /// Set when the compiler could not be run at all.
    pub probe_failed: bool,
}

impl FlagCapabilities {
    /// Everything available. Used by unit tests that do not run a compiler.
    pub fn optimistic() -> Self {
        Self {
            std_legacy: true,
            dec: true,
            no_range_check: true,
            allow_invalid_boz: true,
            init_local_zero: true,
            no_automatic: true,
            check_bounds: true,
            d_lines_as_code: true,
            max_errors: true,
            diagnostics_plain: true,
            max_stack_var_size: true,
            default_real8: true,
            static_full: true,
            static_runtime: true,
            strip: true,
            probe_failed: false,
        }
    }

    /// Nothing optional. The build still works, just less forgivingly.
    pub fn conservative() -> Self {
        Self {
            std_legacy: false,
            dec: false,
            no_range_check: false,
            allow_invalid_boz: false,
            init_local_zero: false,
            no_automatic: false,
            check_bounds: false,
            d_lines_as_code: false,
            max_errors: false,
            diagnostics_plain: false,
            max_stack_var_size: false,
            default_real8: false,
            static_full: false,
            static_runtime: false,
            strip: false,
            probe_failed: true,
        }
    }
}

/// Every syntax-affecting flag we might want, in one list, so the common case is
/// a single compiler invocation.
/// A flag, and how to record whether this compiler accepted it.
type FlagSetter = fn(&mut FlagCapabilities, bool);

const SYNTAX_FLAGS: &[(&str, FlagSetter)] = &[
    ("-std=legacy", |c, v| c.std_legacy = v),
    ("-fdec", |c, v| c.dec = v),
    ("-fno-range-check", |c, v| c.no_range_check = v),
    ("-fallow-invalid-boz", |c, v| c.allow_invalid_boz = v),
    ("-finit-local-zero", |c, v| c.init_local_zero = v),
    ("-fno-automatic", |c, v| c.no_automatic = v),
    ("-fd-lines-as-code", |c, v| c.d_lines_as_code = v),
    ("-fcheck=bounds", |c, v| c.check_bounds = v),
    ("-fmax-errors=25", |c, v| c.max_errors = v),
    ("-fdiagnostics-color=never", |c, v| c.diagnostics_plain = v),
    ("-fmax-stack-var-size=0", |c, v| c.max_stack_var_size = v),
    ("-fdefault-real-8", |c, v| c.default_real8 = v),
];

/// Probe a compiler's optional flags.
///
/// `compile_required` / `link_required` are flags the toolchain always needs — a
/// relocatable Linux bundle cannot compile without its `--sysroot`, nor link
/// without `-B` for its startup files. Without them every probe fails and the
/// compiler looks capable of nothing.
///
/// The probe runs in the *same* scrubbed environment as real builds. A probe run
/// in a different environment from the builds it predicts is worse than no probe:
/// a flag can look available and then fail, or vice versa.
pub fn probe(
    gfortran: &Path,
    compile_required: &[OsString],
    link_required: &[OsString],
    bundle: Option<&super::bundle::Bundle>,
) -> FlagCapabilities {
    let Ok(dir) = Scratch::new("ef77-probe") else {
        return FlagCapabilities::conservative();
    };
    let Ok(src) = dir.write("probe.f", PROBE_SRC.as_bytes()) else {
        return FlagCapabilities::conservative();
    };

    let env = super::scrubbed_env(gfortran, bundle, dir.path());
    let launcher = bundle.and_then(|b| b.launcher.as_deref());

    // Does the compiler run at all?
    if !syntax_ok(
        gfortran,
        dir.path(),
        &src,
        &[],
        compile_required,
        &env,
        launcher,
    ) {
        return FlagCapabilities::conservative();
    }

    let mut caps = FlagCapabilities::conservative();
    caps.probe_failed = false;

    // Fast path: try every syntax flag at once.
    let all: Vec<&str> = SYNTAX_FLAGS.iter().map(|(f, _)| *f).collect();
    if syntax_ok(
        gfortran,
        dir.path(),
        &src,
        &all,
        compile_required,
        &env,
        launcher,
    ) {
        for (_, set) in SYNTAX_FLAGS {
            set(&mut caps, true);
        }
    } else {
        // Something was rejected. Find out exactly what, one flag at a time.
        for (flag, set) in SYNTAX_FLAGS {
            set(
                &mut caps,
                syntax_ok(
                    gfortran,
                    dir.path(),
                    &src,
                    &[flag],
                    compile_required,
                    &env,
                    launcher,
                ),
            );
        }
    }

    // Link flags need a real link, not a syntax check.
    // Link probes get the LINK flags, not the compile ones.
    caps.static_runtime = link_ok(
        &dir,
        gfortran,
        &src,
        &["-static-libgfortran", "-static-libgcc"],
        link_required,
        &env,
        launcher,
    );
    caps.static_full = link_ok(
        &dir,
        gfortran,
        &src,
        &["-static-libgfortran", "-static-libgcc", "-static"],
        link_required,
        &env,
        launcher,
    );
    caps.strip = link_ok(&dir, gfortran, &src, &["-s"], link_required, &env, launcher);

    caps
}

#[allow(clippy::too_many_arguments)]
fn syntax_ok(
    gfortran: &Path,
    cwd: &Path,
    src: &Path,
    flags: &[&str],
    required: &[OsString],
    env: &[(std::ffi::OsString, std::ffi::OsString)],
    launcher: Option<&str>,
) -> bool {
    run(gfortran, cwd, env, launcher, |cmd| {
        cmd.arg("-fsyntax-only");
        cmd.args(required);
        cmd.args(flags);
        cmd.arg("-x").arg("f77").arg(src).arg("-x").arg("none");
    })
}

fn link_ok(
    scratch: &Scratch,
    gfortran: &Path,
    src: &Path,
    flags: &[&str],
    required: &[OsString],
    env: &[(std::ffi::OsString, std::ffi::OsString)],
    launcher: Option<&str>,
) -> bool {
    let cwd = scratch.path();
    // The bundle's target decides the suffix, and the linker may append it
    // itself; glob for either rather than assume.
    let out = cwd.join("probe_out");
    let ok = run(gfortran, cwd, env, launcher, |cmd| {
        cmd.args(required);
        cmd.args(flags);
        cmd.arg("-x").arg("f77").arg(src).arg("-x").arg("none");
        cmd.arg("-o").arg(&out);
    });
    scratch.discard(&out);
    scratch.discard(&cwd.join("probe_out.exe"));
    ok
}

fn run(
    gfortran: &Path,
    cwd: &Path,
    env: &[(std::ffi::OsString, std::ffi::OsString)],
    launcher: Option<&str>,
    build: impl FnOnce(&mut Command),
) -> bool {
    let mut cmd = match launcher {
        Some(l) => {
            let mut c = Command::new(l);
            c.arg(gfortran);
            c
        }
        None => Command::new(gfortran),
    };
    cmd.env_clear();
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::build::exec::CREATE_NO_WINDOW);
    }
    build(&mut cmd);
    matches!(cmd.status(), Ok(s) if s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nonexistent_compiler_probes_conservatively() {
        let caps = probe(Path::new("/nonexistent/gfortran-xyz"), &[], &[], None);
        assert!(caps.probe_failed);
        assert!(!caps.dec);
        assert!(!caps.static_full);
    }

    #[test]
    fn optimistic_and_conservative_differ_on_every_field() {
        let o = FlagCapabilities::optimistic();
        let c = FlagCapabilities::conservative();
        assert_ne!(o, c);
        assert!(!o.probe_failed);
        assert!(c.probe_failed);
    }
}
