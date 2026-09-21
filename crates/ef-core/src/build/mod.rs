//! The build state machine.
//!
//! Each source is compiled separately and then the objects are linked. This
//! matches the object-file-then-link model of the DOS tool this replaces, and it
//! is also better behaviour: a single multi-file gfortran command aborts at the
//! first file that fails, so the user would see errors in one file and nothing at all
//! about the other six. Compiling per file collects every error in one pass.

pub mod args;
pub mod diagnostics;
pub mod exec;
pub mod objfmt;
pub mod omf;
pub mod sourcefmt;
pub mod stage;

use crate::error::{EfError, Result};
use crate::fs_guard::FsGuard;
use crate::paths::WorkLayout;
use crate::project::Program;
use crate::toolchain::Toolchain;
use crossbeam_channel::Sender;
use diagnostics::Diagnostic;
use stage::Staging;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A runaway error cascade can emit hundreds of megabytes.
pub const COMPILER_OUTPUT_CAP: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildPhase {
    Preflight,
    Staging,
    Compiling {
        done: usize,
        total: usize,
        file: String,
    },
    Linking,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailedAt {
    Preflight,
    Staging,
    Compile,
    Link,
}

#[derive(Debug, Clone)]
pub struct BuildOutcome {
    pub success: bool,
    pub exe: Option<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    /// Everything the compiler printed, for the technical-details pane.
    pub raw: String,
    pub errors: usize,
    pub warnings: usize,
    pub failed_at: Option<FailedAt>,
    pub cancelled: bool,
    /// Set when the failure was ours rather than the user's code's.
    pub internal_error: Option<String>,
    /// Set when the build stopped because one of the user's files cannot be used.
    ///
    /// Kept as structured data rather than folded into `internal_error`, because
    /// the whole value of checking a file ourselves is being able to explain it
    /// in their own language — and a formatted English string cannot be translated
    /// after the fact.
    pub file_problem: Option<FileProblem>,
}

pub use crate::error::{FileProblem, ProblemArg};

impl BuildOutcome {
    fn failure(at: FailedAt, diagnostics: Vec<Diagnostic>, raw: String) -> Self {
        let errors = diagnostics::count_errors(&diagnostics);
        let warnings = diagnostics::count_warnings(&diagnostics);
        Self {
            success: false,
            exe: None,
            diagnostics,
            raw,
            errors,
            warnings,
            failed_at: Some(at),
            cancelled: false,
            internal_error: None,
            file_problem: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum BuildEvent {
    Phase(BuildPhase),
    /// Streamed as each file finishes, so errors appear while the build continues.
    Diagnostics(Vec<Diagnostic>),
    Finished(Box<BuildOutcome>),
}

fn emit(tx: Option<&Sender<BuildEvent>>, ev: BuildEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(ev);
    }
}

/// Compile and link one program. Blocking; callers run it on a worker thread.
pub fn build(
    guard: &FsGuard,
    toolchain: &Toolchain,
    layout: &WorkLayout,
    program: &Program,
    tx: Option<&Sender<BuildEvent>>,
    cancel: &AtomicBool,
) -> BuildOutcome {
    match build_inner(guard, toolchain, layout, program, tx, cancel) {
        Ok(outcome) => {
            emit(tx, BuildEvent::Finished(Box::new(outcome.clone())));
            outcome
        }
        Err(e) => {
            let mut o = BuildOutcome::failure(FailedAt::Preflight, Vec::new(), String::new());
            if let EfError::UnusableFile { problem, .. } = &e {
                o.file_problem = Some((**problem).clone());
            }
            o.internal_error = Some(e.to_string());
            emit(tx, BuildEvent::Finished(Box::new(o.clone())));
            o
        }
    }
}

fn build_inner(
    guard: &FsGuard,
    toolchain: &Toolchain,
    layout: &WorkLayout,
    program: &Program,
    tx: Option<&Sender<BuildEvent>>,
    cancel: &AtomicBool,
) -> Result<BuildOutcome> {
    emit(tx, BuildEvent::Phase(BuildPhase::Preflight));
    if program.sources.is_empty() {
        return Err(EfError::NoSources);
    }
    // Every file must still be there. The config is not a source of truth about
    // what exists on disk.
    for s in &program.sources {
        if !s.path.exists() {
            return Err(EfError::io(
                &s.path,
                std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
            ));
        }
    }

    emit(tx, BuildEvent::Phase(BuildPhase::Staging));
    let staging: Staging = stage::stage(
        guard,
        layout,
        program,
        objfmt::Expect::for_exe_suffix(toolchain.exe_suffix()),
    )?;
    // What this build actually compiles with -- the program's options, less
    // anything that cannot safely apply to it. See `Program::effective_options`.
    let options = program.effective_options();

    let caps = toolchain.capabilities();
    let total = staging.sources.len();
    let mut all_diags: Vec<Diagnostic> = Vec::new();
    let mut raw = String::new();
    let mut log_full = false;
    let mut compile_failed = false;

    for (i, src) in staging.sources.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(cancelled(all_diags, raw));
        }
        emit(
            tx,
            BuildEvent::Phase(BuildPhase::Compiling {
                done: i,
                total,
                file: src.display_name.clone(),
            }),
        );

        let mut cmd = toolchain.command(layout);
        cmd.args(args::compile_args(
            caps,
            toolchain.compile_flags(),
            &options,
            src,
            layout,
            &staging.include_dirs,
        ));

        let (code, text) = exec::run_capture(cmd, COMPILER_OUTPUT_CAP, cancel)?;
        append_capped(&mut raw, &text, &mut log_full);

        let mut diags = diagnostics::parse(&text);
        diagnostics::rewrite_names(&mut diags, &staging.sources);
        if !diags.is_empty() {
            emit(tx, BuildEvent::Diagnostics(diags.clone()));
        }
        all_diags.extend(diags);

        if code != Some(0) {
            compile_failed = true;
            // Keep going: collect errors from every file, not just the first.
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return Ok(cancelled(all_diags, raw));
    }
    if compile_failed {
        return Ok(BuildOutcome::failure(FailedAt::Compile, all_diags, raw));
    }

    let mut objs: Vec<PathBuf> = staging.sources.iter().map(|s| s.obj.clone()).collect();

    if program.options.wants_pause_shim(toolchain.exe_suffix()) {
        match compile_pause_shim(guard, toolchain, layout, cancel) {
            Ok((obj, text)) => {
                // Its output joins the rest, so a failure here does not throw
                // away what the user's own files had to say.
                append_capped(&mut raw, &text, &mut log_full);
                // Stop pressed while this was compiling. `run_capture` killed the
                // child, so the non-zero exit below is ours to interpret, not a
                // failure to report. Checked here and not in an `Err` arm, because
                // a killed compile comes back as a failed compile, not an error.
                if cancel.load(Ordering::Relaxed) {
                    return Ok(cancelled(all_diags, raw));
                }
                match obj {
                    Some(o) => objs.push(o),
                    None => {
                        let mut out = BuildOutcome::failure(FailedAt::Compile, all_diags, raw);
                        out.internal_error =
                            Some("could not build the part that keeps the window open".into());
                        return Ok(out);
                    }
                }
            }
            // An I/O error raised while Stop was being pressed is not worth
            // reporting as a failure -- and reporting it costs every diagnostic
            // the compile loop had already gathered, because `?` leaves through
            // the catch-all with an empty outcome.
            Err(_) if cancel.load(Ordering::Relaxed) => {
                return Ok(cancelled(all_diags, raw));
            }
            Err(e) => return Err(e),
        }
    }

    emit(tx, BuildEvent::Phase(BuildPhase::Linking));
    let mut cmd = toolchain.command(layout);
    cmd.args(args::link_args(
        caps,
        toolchain.link_flags(),
        &options,
        &objs,
        &staging.libraries,
        layout,
        toolchain.exe_suffix(),
    ));

    let (code, text) = exec::run_capture(cmd, COMPILER_OUTPUT_CAP, cancel)?;
    append_capped(&mut raw, &text, &mut log_full);
    // Stop pressed while linking. Without this the killed linker's non-zero exit
    // reads as a link failure, and the user who asked the build to stop is shown
    // a red "Build failed" with no errors in it.
    if cancel.load(Ordering::Relaxed) {
        return Ok(cancelled(all_diags, raw));
    }
    let mut link_diags = diagnostics::parse(&text);
    diagnostics::rewrite_names(&mut link_diags, &staging.sources);
    if !link_diags.is_empty() {
        emit(tx, BuildEvent::Diagnostics(link_diags.clone()));
    }
    all_diags.extend(link_diags);

    let exe = layout.exe_with_suffix(toolchain.exe_suffix());
    if code != Some(0) || !exe.exists() {
        return Ok(BuildOutcome::failure(FailedAt::Link, all_diags, raw));
    }

    let errors = diagnostics::count_errors(&all_diags);
    let warnings = diagnostics::count_warnings(&all_diags);
    Ok(BuildOutcome {
        success: true,
        exe: Some(exe),
        diagnostics: all_diags,
        raw,
        errors,
        warnings,
        failed_at: None,
        cancelled: false,
        internal_error: None,
        file_problem: None,
    })
}

/// The shim's source, exposed so a test can compile it.
///
/// It is Fortran inside a Rust crate, so nothing about it is checked by building
/// this crate: a typo in the `bind(C)` interface compiles fine here and fails at
/// the user, at link time, on Windows only.
pub const PAUSE_SHIM: &str = include_str!("../../assets/pause-shim.f90");

/// Build the object that keeps the console window open.
///
/// Its source is ours and ships inside the application, so there is nothing to
/// install and nothing of theirs to change. A failure here fails the build
/// rather than quietly dropping the option: they asked for the window to stay,
/// and a program that closed anyway would look like the setting did nothing.
fn compile_pause_shim(
    guard: &FsGuard,
    toolchain: &Toolchain,
    layout: &WorkLayout,
    cancel: &AtomicBool,
) -> Result<(Option<PathBuf>, String)> {
    // Compiled on every build. The same bytes every run -- the source is baked
    // in with `include_str!` -- so it is cacheable, and deliberately is not: the
    // build tree is scratch, and the saving does not pay for a key and an
    // invalidation rule.
    let src = layout.src().join("ef77_pause_shim.f90");
    let obj = layout.obj().join("ef77_pause_shim.o");
    guard.write_file(&src, PAUSE_SHIM.as_bytes())?;

    let mut cmd = toolchain.command(layout);
    cmd.args(args::shim_compile_args(
        toolchain.compile_flags(),
        &src,
        &obj,
        layout,
    ));
    let (code, text) = exec::run_capture(cmd, COMPILER_OUTPUT_CAP, cancel)?;
    // The compiler's own words go back to the caller to join `raw`, rather than
    // into the message the user reads. This is our file, not theirs, so the
    // message says so in one sentence and the English belongs in the details
    // pane with every other line the compiler printed.
    if code != Some(0) || !obj.exists() {
        return Ok((None, text));
    }
    Ok((Some(obj), text))
}

/// Add compiler output to the build's log, bounded.
///
/// The bound is on what the build *keeps*, never on what a file is allowed to
/// print. Capping each invocation separately lets sixty files hold sixty times
/// the cap; spending one shared budget as the build goes is worse still, because
/// the file that exhausts it leaves every later file with nothing to parse and
/// their errors disappear from the panel while the build still reports failure.
///
/// So each file is read and parsed in full, and only this accumulated log is
/// trimmed — once, with one notice, not once per file.
fn append_capped(raw: &mut String, text: &str, full: &mut bool) {
    if *full {
        return;
    }
    let room = COMPILER_OUTPUT_CAP.saturating_sub(raw.len());
    if text.len() <= room {
        raw.push_str(text);
        return;
    }
    let mut end = room;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    raw.push_str(&text[..end]);
    raw.push_str("\n… (rest of the compiler output omitted)\n");
    *full = true;
}

fn cancelled(diagnostics: Vec<Diagnostic>, raw: String) -> BuildOutcome {
    BuildOutcome {
        success: false,
        exe: None,
        errors: diagnostics::count_errors(&diagnostics),
        warnings: diagnostics::count_warnings(&diagnostics),
        diagnostics,
        raw,
        failed_at: None,
        cancelled: true,
        internal_error: None,
        file_problem: None,
    }
}

/// Run a build on a worker thread, returning the event stream immediately.
pub fn spawn(
    guard: FsGuard,
    toolchain: Arc<Toolchain>,
    layout: WorkLayout,
    program: Program,
    cancel: Arc<AtomicBool>,
) -> crossbeam_channel::Receiver<BuildEvent> {
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || {
        build(&guard, &toolchain, &layout, &program, Some(&tx), &cancel);
    });
    rx
}
