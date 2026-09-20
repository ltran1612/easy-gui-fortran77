//! Tier 2: the pause shim is real Fortran, and the compiler agrees.
//!
//! `assets/pause-shim.f90` is Fortran source living inside a Rust crate. Nothing
//! about building `ef-core` checks a line of it — a typo in the `bind(C)`
//! interface, the free-form layout or the `GetConsoleProcessList` declaration
//! compiles perfectly well into the binary and fails at the user, at link time,
//! on Windows only.
//!
//! Every other embedded asset here is pinned by something: the icons against
//! `logo.png`, the catalogs against each other, the manifest against the bundle,
//! the examples against their guide. This is that check for the shim.

use ef_core::build::{args, PAUSE_SHIM};
use ef_core::fs_guard::FsGuard;
use ef_core::paths::{AppPaths, WorkLayout};
use ef_core::toolchain::Toolchain;

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
fn the_pause_shim_actually_compiles() {
    let Some(tc) = toolchain() else { return };

    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::under(tmp.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let layout = WorkLayout::new(paths.build_dir(1));
    for d in layout.all_dirs() {
        guard.create_dir_all(&d).unwrap();
    }

    let src = layout.src().join("pause_shim.f90");
    let obj = layout.obj().join("pause_shim.o");
    guard.write_file(&src, PAUSE_SHIM.as_bytes()).unwrap();

    let mut cmd = tc.command(&layout);
    cmd.args(args::shim_compile_args(
        tc.compile_flags(),
        &src,
        &obj,
        &layout,
    ));
    let out = cmd.output().expect("could not run the compiler");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        out.status.success() && obj.is_file(),
        "the pause shim did not compile:\n{text}"
    );
    // Warnings here would reach the user's build log for a file they did not
    // write and cannot fix.
    assert!(
        !text.to_lowercase().contains("warning"),
        "the pause shim compiled with warnings:\n{text}"
    );
}

/// The symbols the link line depends on, spelled the way the linker will ask.
///
/// `-Wl,--wrap=exit` resolves `__wrap_exit`, and the shim calls back through
/// `__real_exit`. Rename either in the Fortran and the flag silently stops
/// reaching it, which is the failure this whole file exists to catch early.
#[test]
fn the_shim_declares_the_names_the_wrap_flag_expects() {
    // Matched with the quotes, because a bare substring test passes happily
    // when the symbol has been renamed to something that contains it —
    // `__wrap_exit` is inside `__wrap_exit_RENAMED`, and the first version of
    // this test said nothing about that.
    for name in ["__wrap_exit", "__real_exit", "GetConsoleProcessList"] {
        let bound = format!("name=\"{name}\"");
        assert!(
            PAUSE_SHIM.contains(&bound),
            "the shim no longer binds {bound}, which the link line depends on"
        );
    }
}
