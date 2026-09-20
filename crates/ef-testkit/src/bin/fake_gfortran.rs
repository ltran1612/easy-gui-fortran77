//! A stand-in for gfortran, driven by `EF77_FAKE`.
//!
//! This is what lets the entire build state machine — event ordering, per-file
//! error collection, output caps, cancellation, diagnostic rewriting — be tested on
//! a machine with no Fortran compiler installed.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Record every invocation, so a test can assert exactly which flags reached
    // the compiler rather than inferring it from whether a build happened to work.
    if !args.iter().any(|a| a == "-dumpversion") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("argv.log")
        {
            let _ = writeln!(f, "{}", args.join(" "));
        }
    }

    // The toolchain layer asks for a version before anything else.
    if args.iter().any(|a| a == "-dumpversion") {
        println!("16.2.0");
        return;
    }

    // Capability probing. Reject anything named in EF77_FAKE_REJECT so a test can
    // simulate an older compiler.
    if args.iter().any(|a| a == "-fsyntax-only") {
        if let Ok(reject) = std::env::var("EF77_FAKE_REJECT") {
            for bad in reject.split(',').filter(|s| !s.is_empty()) {
                if args.iter().any(|a| a == bad) {
                    eprintln!("gfortran: error: unrecognized command-line option '{bad}'");
                    std::process::exit(1);
                }
            }
        }
        return;
    }

    let is_compile = args.iter().any(|a| a == "-c");
    let out = arg_after(&args, "-o");
    let input = args
        .iter()
        .find(|a| a.ends_with(".f") || a.ends_with(".F"))
        .cloned()
        .unwrap_or_default();

    // The mode is read from a file in the *current directory*, which the build
    // sets to a per-test work tree. Using a file rather than an environment
    // variable keeps the tests parallel-safe.
    let mode = read_marker("FAKE_MODE").unwrap_or_else(|| "ok".into());

    match mode.as_str() {
        "error" if is_compile => {
            // Exactly the shape modern gfortran emits: location, blank, snippet,
            // then the severity line.
            println!("{input}:12:24:");
            println!();
            println!("   12 |       IF (X .EQ. 1) GOTO 100");
            println!("      |                        1");
            println!("Error: Symbol 'x' at (1) has no IMPLICIT type");
            std::process::exit(1);
        }
        "warn" if is_compile => {
            println!("{input}:5:73:");
            println!();
            println!("    5 |       X = 1");
            println!("Warning: Line truncated at (1) [-Wline-truncation]");
            touch(out.as_deref());
        }
        "flood" if is_compile => {
            let line = "Error: flood ".repeat(20);
            let mut stdout = std::io::stdout().lock();
            for _ in 0..200_000 {
                let _ = writeln!(stdout, "{input}:1:1: {line}");
            }
            std::process::exit(1);
        }
        "slow" => {
            std::thread::sleep(std::time::Duration::from_secs(30));
            touch(out.as_deref());
        }
        "linkfail" if !is_compile => {
            println!("/usr/bin/ld: ./obj/main.o: in function `MAIN__':");
            println!("main.f:(.text+0x1a): undefined reference to `mysub_'");
            println!("collect2: error: ld returned 1 exit status");
            std::process::exit(1);
        }
        // On the link step, produce something that can actually be executed, so
        // saving and running a built program can be tested with no Fortran at all.
        "interactive" if !is_compile => {
            let program =
                read_marker("FAKE_PROGRAM").expect("FAKE_PROGRAM marker must name the program");
            let out = out.expect("link step needs -o");
            std::fs::copy(&program, &out).expect("copy fake program into place");
            make_executable(std::path::Path::new(&out));
        }
        _ => touch(out.as_deref()),
    }
}

fn read_marker(name: &str) -> Option<String> {
    std::fs::read_to_string(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn arg_after(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn touch(path: Option<&str>) {
    if let Some(p) = path {
        let _ = std::fs::write(p, b"fake object\n");
    }
}

fn make_executable(p: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(md) = std::fs::metadata(p) {
            let mut perms = md.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(p, perms);
        }
    }
    #[cfg(not(unix))]
    let _ = p;
}
