//! Headless driver.
//!
//! Two jobs: it is how CI exercises the whole build pipeline without a window
//! server, and it is how you diagnose an installation over the phone
//! ("run this one command and read me what it says").

use anyhow::{bail, Context, Result};
use ef_core::build::{self, diagnostics::Severity};
use ef_core::config::Store;
use ef_core::fs_guard::FsGuard;
use ef_core::paths::{AppPaths, WorkLayout};
use ef_core::project::Program;
use ef_core::toolchain::manifest::Manifest;
use ef_core::toolchain::{self, Toolchain};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

const USAGE: &str = "\
Easy Fortran 77 — headless driver

USAGE:
    ef-cli doctor
    ef-cli build <file.f>... [--lib <file>]... [--out <path>] [--json]
                             [--132] [--no-dec] [--no-static] [--preprocess]
    ef-cli inspect-library <file>...

OPTIONS:
    --lib P          link against the pre-compiled library P
    --out P          save the built program to P
    --json           machine-readable result on stdout
    --132            treat source as 132-column instead of 72
    --no-dec         disable DEC/Microsoft extensions
    --no-static      disable static local storage and zero-init
    --preprocess     run the C preprocessor (rarely wanted)
    --strip          strip the saved program (smaller file, no debug info)
    --keep-open      wait for a key before the program exits (Windows only)
";

struct Opts {
    files: Vec<PathBuf>,
    json: bool,
    out: Option<PathBuf>,
    col132: bool,
    no_dec: bool,
    no_static: bool,
    preprocess: bool,
    strip: bool,
    keep_open: bool,
    libs: Vec<PathBuf>,
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e:#}");
        std::process::exit(2);
    }
}

fn real_main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        print!("{USAGE}");
        std::process::exit(1);
    };
    let rest: Vec<String> = args.collect();

    match cmd.as_str() {
        "doctor" => doctor(),
        "inspect-library" => inspect_library(&rest),
        "build" => {
            let o = parse(&rest)?;
            let (paths, outcome, _) = do_build(&o)?;
            report_build(&o, &outcome);
            if !outcome.success {
                std::process::exit(1);
            }
            if let (Some(dest), Some(built)) = (&o.out, &outcome.exe) {
                let guard = FsGuard::new(paths.write_roots().to_vec())?;
                guard
                    .export_built_program(built, dest)
                    .with_context(|| format!("saving to {}", dest.display()))?;
                if !o.json {
                    println!("saved: {}", dest.display());
                }
            }
            Ok(())
        }
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => bail!("unknown command `{other}`\n\n{USAGE}"),
    }
}

fn parse(args: &[String]) -> Result<Opts> {
    let mut o = Opts {
        files: Vec::new(),
        json: false,
        out: None,
        col132: false,
        no_dec: false,
        no_static: false,
        preprocess: false,
        strip: false,
        keep_open: false,
        libs: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => o.json = true,
            "--132" => o.col132 = true,
            "--no-dec" => o.no_dec = true,
            "--no-static" => o.no_static = true,
            "--preprocess" => o.preprocess = true,
            "--strip" => o.strip = true,
            "--keep-open" => o.keep_open = true,
            "--out" => o.out = Some(PathBuf::from(it.next().context("--out needs a path")?)),
            "--lib" => o
                .libs
                .push(PathBuf::from(it.next().context("--lib needs a path")?)),
            other if other.starts_with("--") => bail!("unknown option `{other}`"),
            other => o.files.push(PathBuf::from(other)),
        }
    }
    if o.files.is_empty() {
        bail!("no source files given\n\n{USAGE}");
    }
    Ok(o)
}

/// Say what a library file actually is.
///
/// This exists for the conversation that starts "I have some .LIB files from the old compiler":
/// it answers, without building anything, whether they can be used at all. A
/// DOS-era library cannot, and finding that out early saves a great deal of time.
fn inspect_library(args: &[String]) -> Result<()> {
    use ef_core::build::objfmt::{assess, sniff, Expect, Format};

    if args.is_empty() {
        bail!("no files given\n\n{USAGE}");
    }
    let want = match find_toolchain() {
        Ok(tc) => Expect::for_exe_suffix(tc.exe_suffix()),
        // Worth answering even with no compiler installed: the DOS verdict does
        // not depend on which toolchain would have been used.
        Err(_) => Expect::for_exe_suffix(std::env::consts::EXE_SUFFIX),
    };

    let mut usable = true;
    for a in args {
        let path = PathBuf::from(a);
        // Through the guard, like every other file of the user's we touch: read-only,
        // and on Windows without taking a lock on a file the user may have open.
        let bytes = FsGuard::read_user_library(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let fmt = sniff(&bytes);
        let verdict = assess(&fmt, want);

        let what = match &fmt {
            Format::OmfLibrary { runtime: true } => {
                "OMF library (DOS-era), the old compiler's own runtime".to_string()
            }
            Format::OmfLibrary { runtime: false } => "OMF library (DOS-era)".to_string(),
            Format::OmfObject => "OMF object (DOS-era)".to_string(),
            Format::CoffArchive { machine } => match machine {
                Some(m) => format!("COFF/PE archive for {}", m.label()),
                None => "COFF/PE archive".into(),
            },
            Format::ElfArchive { machine } => match machine {
                Some(m) => format!("ELF archive for {}", m.label()),
                None => "ELF archive".into(),
            },
            Format::CoffObject(m) => format!("COFF/PE object for {}", m.label()),
            Format::ElfObject(m) => format!("ELF object for {}", m.label()),
            Format::EmptyArchive => "empty archive (a stub, links to nothing)".into(),
            Format::Bitcode => "LLVM bitcode".into(),
            Format::Empty => "empty file".into(),
            Format::Unknown => "not an object file".into(),
        };

        println!("{}\n    {what}", path.display());
        match verdict {
            Ok(()) => println!("    usable: yes"),
            Err(reason) => {
                usable = false;
                println!("    usable: NO");
                println!("    why:    {}", reason.english());
                describe_omf(&fmt, &bytes);
            }
        }
    }
    if !usable {
        std::process::exit(1);
    }
    Ok(())
}

/// For an OMF file, say what is actually inside it.
///
/// Knowing a library cannot be linked is only half an answer. The module names
/// are usually the source files it was built from, and the public symbols are the
/// routines it provides — which is what tells the user whether the user needs it at all.
fn describe_omf(fmt: &ef_core::build::objfmt::Format, bytes: &[u8]) {
    use ef_core::build::objfmt::Format;
    use ef_core::build::omf;

    let lib = match fmt {
        Format::OmfLibrary { .. } => match omf::read_library(bytes) {
            Some(l) => l,
            None => return,
        },
        Format::OmfObject => match omf::read_object(bytes) {
            Some(m) => omf::Library::of_object(m),
            None => return,
        },
        _ => return,
    };

    println!(
        "    inside: {} module(s), {}-bit{}",
        lib.modules.len(),
        if lib.any_32_bit() { 32 } else { 16 },
        if lib.truncated {
            " (stopped early: the record stream did not parse to the end)"
        } else {
            ""
        }
    );
    if let Some(t) = lib.modules.iter().find_map(|m| m.translator.as_deref()) {
        println!("    built by: {t}");
    }
    for m in &lib.modules {
        let name = if m.name.is_empty() { "?" } else { &m.name };
        println!("      {name}");
        if !m.publics.is_empty() {
            println!("          defines: {}", m.publics.join(", "));
        }
    }
}

fn find_toolchain() -> Result<Toolchain> {
    let store_override = AppPaths::resolve()
        .ok()
        .and_then(|p| Store::new(p).ok())
        .and_then(|s| s.load_settings().ok())
        .and_then(|s| s.toolchain_override);
    toolchain::discover(store_override.as_deref())
        .context("no Fortran compiler found (install gfortran, or set EF77_TOOLCHAIN)")
}

fn program_from(o: &Opts) -> Result<Program> {
    use ef_core::project::{LineLength, Preprocess};
    let mut p = Program::new("cli");
    for f in &o.files {
        p.add_source(f)
            .with_context(|| format!("adding {}", f.display()))?;
    }
    if o.col132 {
        p.options.line_length = LineLength::Col132;
    }
    if o.no_dec {
        p.options.dec_extensions = false;
    }
    if o.no_static {
        p.options.static_storage = false;
    }
    if o.preprocess {
        p.options.preprocess = Preprocess::Always;
    }
    p.options.strip_symbols = o.strip;
    p.options.keep_window_open = o.keep_open;
    for l in &o.libs {
        p.add_library(l)
            .with_context(|| format!("adding library {}", l.display()))?;
    }
    Ok(p)
}

fn do_build(o: &Opts) -> Result<(AppPaths, build::BuildOutcome, WorkLayout)> {
    let paths = AppPaths::resolve()?;
    let guard = FsGuard::new(paths.write_roots().to_vec())?;
    let tc = find_toolchain()?;
    let layout = WorkLayout::new(paths.build_dir(1));
    let cancel = AtomicBool::new(false);
    let program = program_from(o)?;
    let outcome = build::build(&guard, &tc, &layout, &program, None, &cancel);
    Ok((paths, outcome, layout))
}

fn report_build(o: &Opts, outcome: &build::BuildOutcome) {
    if o.json {
        let mut s = String::from("{\n");
        s.push_str(&format!("  \"success\": {},\n", outcome.success));
        s.push_str(&format!("  \"errors\": {},\n", outcome.errors));
        s.push_str(&format!("  \"warnings\": {},\n", outcome.warnings));
        s.push_str(&format!(
            "  \"exe\": {},\n",
            match &outcome.exe {
                Some(p) => format!("{:?}", p.display().to_string()),
                None => "null".into(),
            }
        ));
        if let Some(ie) = &outcome.internal_error {
            s.push_str(&format!("  \"internal_error\": {:?},\n", ie));
        }
        s.push_str("  \"diagnostics\": [\n");
        for (i, d) in outcome.diagnostics.iter().enumerate() {
            s.push_str(&format!(
                "    {{\"severity\": {:?}, \"file\": {}, \"line\": {}, \"col\": {}, \"message\": {:?}}}{}\n",
                match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Note => "note",
                },
                d.file.as_ref().map(|f| format!("{f:?}")).unwrap_or_else(|| "null".into()),
                d.line.map(|l| l.to_string()).unwrap_or_else(|| "null".into()),
                d.col.map(|c| c.to_string()).unwrap_or_else(|| "null".into()),
                d.message,
                if i + 1 == outcome.diagnostics.len() { "" } else { "," }
            ));
        }
        s.push_str("  ]\n}\n");
        print!("{s}");
        return;
    }

    for d in &outcome.diagnostics {
        let sev = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };
        match (&d.file, d.line, d.col) {
            (Some(f), Some(l), Some(c)) => println!("{f}:{l}:{c}: {sev}: {}", d.message),
            _ => println!("{sev}: {}", d.message),
        }
    }
    if let Some(ie) = &outcome.internal_error {
        println!("internal error: {ie}");
    }
    println!(
        "{}: {} errors, {} warnings",
        if outcome.success { "ok" } else { "FAILED" },
        outcome.errors,
        outcome.warnings
    );
}

/// The same manifest the application embeds, so `doctor` reports what the
/// application would actually do.
const TOOLCHAIN_MANIFEST: &str = include_str!("../../ef-gui/assets/toolchain-manifest.txt");

fn doctor() -> Result<()> {
    let paths = AppPaths::resolve()?;
    println!("config dir : {}", paths.config_dir().display());
    println!("data dir   : {}", paths.data_dir().display());
    println!("work root  : {}", paths.work_root().display());

    match find_toolchain() {
        Ok(tc) => {
            println!("compiler   : {}", tc.gfortran().display());
            println!("version    : {}", tc.id().display());
            println!("kind       : {:?}", tc.id().kind);

            let manifest = Manifest::parse(TOOLCHAIN_MANIFEST);
            let report = tc.verify_integrity(&manifest);
            if manifest.is_empty() {
                println!("integrity  : no manifest embedded (development build)");
            } else if report.is_ok() {
                println!("integrity  : ok ({} files verified)", report.checked);
            } else {
                println!(
                    "integrity  : FAILED — {} missing, {} changed, {} unreadable",
                    report.missing.len(),
                    report.changed.len(),
                    report.unreadable.len()
                );
                print!("{}", report.detail());
            }
            let c = tc.capabilities();
            println!("capabilities:");
            for (name, on) in [
                ("-std=legacy", c.std_legacy),
                ("-fdec", c.dec),
                ("-fno-automatic", c.no_automatic),
                ("-finit-local-zero", c.init_local_zero),
                ("-fno-range-check", c.no_range_check),
                ("-fallow-invalid-boz", c.allow_invalid_boz),
                ("-fd-lines-as-code", c.d_lines_as_code),
                ("-fmax-errors", c.max_errors),
                ("-fmax-stack-var-size", c.max_stack_var_size),
                ("-fdefault-real-8", c.default_real8),
                ("-static-libgfortran", c.static_runtime),
                ("-static", c.static_full),
                ("-s (strip)", c.strip),
            ] {
                println!("  {:<22} {}", name, if on { "yes" } else { "no" });
            }
        }
        Err(e) => println!("compiler   : NOT FOUND ({e})"),
    }
    Ok(())
}
