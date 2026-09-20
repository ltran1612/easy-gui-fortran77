//! A small script saved beside the program, so double-clicking it works.
//!
//! A Fortran program is a console program. Double-clicked from Explorer it
//! opens a black window, prints, and closes the instant it finishes — too fast
//! to read, and with nowhere to type when the program asks for input. The guide
//! has always explained the fix: put a `.bat` beside it that runs the program
//! and then waits. It also asked the user to write that file themselves, in
//! Notepad, which silently saves it as `.txt`.
//!
//! So the application writes it.
//!
//! The script never names the program. `%~dpn0` is the batch file's own drive,
//! path and stem, so `"%~dpn0.exe"` is its sibling — which means a program
//! called `Tính dầm.exe` needs no part of its name to survive cmd.exe's OEM
//! codepage.

use std::path::{Path, PathBuf};

/// Windows batch. `cd /d "%~dp0"` so the program finds its data files beside
/// itself however it was started, and `pause` so the window stays.
const BAT: &str = "\
@echo off\r\n\
chcp 65001 >nul 2>&1\r\n\
cd /d \"%~dp0\"\r\n\
\"%~dpn0.exe\" %*\r\n\
echo.\r\n\
echo === Chuong trinh da ket thuc / Program finished ===\r\n\
pause\r\n";

/// The same for a Unix shell, resolved before the `cd` so a relative `$0` still
/// points at the right program afterwards.
const SH: &str = "\
#!/bin/sh\n\
dir=$(cd \"$(dirname \"$0\")\" && pwd) || exit 1\n\
prog=\"$dir/$(basename \"$0\" .sh)\"\n\
cd \"$dir\" || exit 1\n\
\"$prog\" \"$@\"\n\
printf '\\n=== Chuong trinh da ket thuc / Program finished ===\\n'\n\
printf 'Nhan Enter de dong / Press Enter to close: '\n\
read -r _\n";

/// Where the launcher goes and what is in it, for a program saved at `exe`.
///
/// `None` when the script would land on the program itself — which also covers
/// a path with no file name, since `set_extension` leaves those unchanged.
///
/// See also `assets/pause-shim.f90`, which solves the same problem one layer
/// down for a program that has travelled away from this script.
pub fn beside(exe: &Path, windows: bool) -> Option<(PathBuf, &'static str)> {
    let path = exe.with_extension(if windows { "bat" } else { "sh" });
    // `with_extension` on a name that already ends in the launcher suffix would
    // hand back the same path and the script would overwrite the program.
    if path == exe {
        return None;
    }
    Some((path, if windows { BAT } else { SH }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launcher_sits_beside_the_program_with_the_same_name() {
        let (p, _) = beside(Path::new("/home/u/Tinh dam.exe"), true).unwrap();
        assert_eq!(p, Path::new("/home/u/Tinh dam.bat"));

        let (p, _) = beside(Path::new("/home/u/tinhdam"), false).unwrap();
        assert_eq!(p, Path::new("/home/u/tinhdam.sh"));
    }

    #[test]
    fn the_script_never_spells_the_program_name() {
        // The whole reason for `%~dpn0`: a Vietnamese file name must not have to
        // survive cmd.exe's OEM codepage.
        let (_, bat) = beside(Path::new("/home/u/Tính dầm bê tông.exe"), true).unwrap();
        assert!(!bat.contains("dầm"), "the name leaked into the script");
        assert!(
            bat.contains("%~dpn0.exe"),
            "it must find its sibling by its own name"
        );

        let (_, sh) = beside(Path::new("/home/u/Tính dầm"), false).unwrap();
        assert!(!sh.contains("dầm"));
        assert!(sh.contains("basename"));
    }

    #[test]
    fn it_would_never_overwrite_the_program_it_launches() {
        assert!(beside(Path::new("/home/u/run.bat"), true).is_none());
        assert!(beside(Path::new("/home/u/run.sh"), false).is_none());
        assert!(beside(Path::new("/"), true).is_none());
    }

    #[test]
    fn the_batch_file_waits_and_uses_crlf() {
        let (_, bat) = beside(Path::new("x.exe"), true).unwrap();
        assert!(
            bat.contains("pause"),
            "without this the window still vanishes"
        );
        assert!(bat.contains("cd /d"), "data files live beside the program");
        // cmd.exe is happiest with CRLF, and a .bat is a file people open in
        // Notepad, which shows LF-only text as one long line.
        assert!(bat.contains("\r\n"));
        assert!(!bat.replace("\r\n", "").contains('\n'), "no bare LF");
    }

    #[test]
    fn the_shell_script_waits_too_and_has_a_shebang() {
        let (_, sh) = beside(Path::new("x"), false).unwrap();
        assert!(sh.starts_with("#!/bin/sh\n"));
        assert!(
            sh.contains("read -r _"),
            "without this it closes immediately"
        );
        assert!(!sh.contains('\r'), "a CR would break the shebang line");
    }
}
