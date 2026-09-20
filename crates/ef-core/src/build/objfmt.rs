//! What kind of binary is this library file, and can our linker use it?
//!
//! A legacy project often arrives with libraries from a DOS-era Fortran compiler.
//! Those are **OMF** — the object format Intel defined for the 8086 and that
//! Microsoft's `LIB.EXE` archived. GNU `ld`, which is what our bundle ships, reads
//! ELF and PE/COFF and has never read OMF. Handing it one produces `file not
//! recognized: file format not recognized`, which tells the user nothing.
//!
//! So we sniff the file ourselves and say the true thing instead: this library
//! was built by a compiler from the DOS era, nothing here can read it, and what
//! you need is the Fortran source it was built from.
//!
//! Pure functions over bytes — no disk, no toolchain — so every branch below is
//! a unit test with a hand-built fixture.

use crate::error::{FileProblem, ProblemArg};

/// Extensions a compiled library or object file carries.
///
/// One list, because the file dialog and the staging code both need it and they
/// live in different crates; two lists would disagree the first time one grew.
pub const LIBRARY_EXTENSIONS: &[&str] = &["a", "lib", "o", "obj"];

/// The machine a COFF or ELF object was compiled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Machine {
    I386,
    Amd64,
    Arm64,
    Other(u16),
}

impl Machine {
    fn from_coff(w: u16) -> Self {
        match w {
            0x014c => Machine::I386,
            0x8664 => Machine::Amd64,
            0xaa64 => Machine::Arm64,
            other => Machine::Other(other),
        }
    }
    fn from_elf(w: u16) -> Self {
        match w {
            0x03 => Machine::I386,
            0x3e => Machine::Amd64,
            0xb7 => Machine::Arm64,
            other => Machine::Other(other),
        }
    }
    /// A short label for the UI. Not translated: these are proper names.
    pub fn label(self) -> String {
        match self {
            Machine::I386 => "32-bit x86".into(),
            Machine::Amd64 => "64-bit x86".into(),
            Machine::Arm64 => "64-bit ARM".into(),
            Machine::Other(w) => format!("0x{w:04x}"),
        }
    }
}

/// What the bytes turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Format {
    /// `!<arch>` holding COFF members: a MinGW `.a` or an MSVC `.lib`.
    CoffArchive { machine: Option<Machine> },
    /// `!<arch>` holding ELF members: a Unix `.a`.
    ElfArchive { machine: Option<Machine> },
    /// A well-formed archive with no object members at all. MinGW ships several
    /// as stubs so that `-lssp` and friends resolve to nothing; `libssp.a` in the
    /// bundle is exactly eight bytes. The linker accepts them, so we do too.
    EmptyArchive,
    /// A bare `.obj`/`.o` in PE/COFF form.
    CoffObject(Machine),
    /// A bare `.o` in ELF form.
    ElfObject(Machine),
    /// The DOS-era `.LIB`: an OMF library.
    ///
    /// `runtime` distinguishes the old compiler's own runtime from a library of
    /// their own routines. It is carried here, rather than recovered later, because
    /// the two need opposite advice and a caller must not be able to forget to ask.
    OmfLibrary { runtime: bool },
    /// A DOS-era `.OBJ`: OMF `THEADR` (`0x80`) or `LHEADR` (`0x82`).
    OmfObject,
    /// LLVM bitcode, i.e. someone's `-flto` output.
    Bitcode,
    /// Zero bytes.
    Empty,
    /// Anything else, including plain text.
    Unknown,
}

/// Why a library cannot be used, in terms that survive translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// OMF. The DOS answer, and the one the user will actually hit.
    DosEraFormat,
    /// OMF, and it is the old compiler's own runtime library rather than the user's
    /// code. Same format problem, opposite advice: the user does not need the file.
    DosEraRuntime,
    /// Readable, but built for a different processor.
    WrongMachine {
        found: Machine,
        want: Machine,
    },
    /// Readable, but the wrong container for this platform's linker.
    WrongPlatform,
    /// Not an object file at all.
    NotALibrary,
    Empty,
}

impl Reason {
    /// Plain English, for logs, the CLI and the support transcript.
    ///
    /// Deliberately a different register from the catalog entry `key()` names: this
    /// one may say "GNU ld reads ELF and PE/COFF only", which is exactly what a
    /// support transcript wants and exactly what the user's screen must never show. The
    /// two are not duplicates and should not be merged.
    pub fn english(&self) -> String {
        match self {
            Reason::DosEraRuntime => "the DOS-era compiler's own runtime library, in the OMF \
                 format. You do not need it: this application's compiler brings its own \
                 runtime, so remove it from the list and build with just your source files."
                .into(),
            Reason::DosEraFormat => "built by a DOS-era compiler, in the OMF format. GNU ld \
                 reads ELF and PE/COFF only and cannot be made to read OMF. You need the \
                 original Fortran source this was built from."
                .into(),
            Reason::WrongMachine { found, want } => format!(
                "built for {}, but programs here are built for {}.",
                found.label(),
                want.label()
            ),
            Reason::WrongPlatform => "built for a different operating system.".into(),
            Reason::NotALibrary => "not a compiled library. If it is Fortran source, add it \
                 as a source file instead."
                .into(),
            Reason::Empty => "the file is empty.".into(),
        }
    }

    /// The finished, translatable description of this refusal.
    ///
    /// Built here because this is where the facts are. Flattening it into loose
    /// error fields and rebuilding it elsewhere meant every new variant had to be
    /// remembered in three places, and a forgotten one silently showed the user nothing.
    pub fn problem(&self, name: String) -> FileProblem {
        let args = match self {
            Reason::WrongMachine { found, want } => vec![
                ("found", ProblemArg::text(found.label())),
                ("want", ProblemArg::text(want.label())),
            ],
            _ => Vec::new(),
        };
        FileProblem {
            name,
            title_key: "libs.unusable_title",
            reason_key: self.key(),
            args,
        }
    }

    /// The i18n key describing this to the user, so the message is bilingual.
    pub fn key(&self) -> &'static str {
        match self {
            Reason::DosEraFormat => "lib.reject.dos_era",
            Reason::DosEraRuntime => "lib.reject.dos_era_runtime",
            Reason::WrongMachine { .. } => "lib.reject.wrong_machine",
            Reason::WrongPlatform => "lib.reject.wrong_platform",
            Reason::NotALibrary => "lib.reject.not_a_library",
            Reason::Empty => "lib.reject.empty",
        }
    }
}

/// What the linker in the bundle can consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expect {
    pub coff: bool,
    pub machine: Machine,
}

impl Expect {
    /// Derived from the bundle's executable suffix.
    ///
    /// Both bundles we ship are x86-64, and that is asserted here rather than
    /// assumed silently: if a 32-bit or ARM bundle is ever added, this is the one
    /// place that has to learn about it.
    pub fn for_exe_suffix(suffix: &str) -> Self {
        Expect {
            coff: suffix.eq_ignore_ascii_case(".exe"),
            machine: Machine::Amd64,
        }
    }
}

pub fn sniff(bytes: &[u8]) -> Format {
    if bytes.is_empty() {
        return Format::Empty;
    }
    if bytes.starts_with(b"!<arch>\n") {
        return archive(bytes);
    }
    if bytes.starts_with(b"\x7fELF") {
        return match elf_machine(bytes) {
            Some(m) => Format::ElfObject(m),
            None => Format::Unknown,
        };
    }
    if bytes.starts_with(b"BC\xc0\xde") {
        return Format::Bitcode;
    }
    // OMF is type-tagged in the first byte — 0xF0 for the library header
    // Microsoft's LIB.EXE writes, 0x80/0x82 for a loose object module — but the
    // tag alone is not enough to go on: 0xF0 is also `d` with a stroke in the
    // Vietnamese codepage the user's sources are written in. So we parse, and a file
    // that does not parse as OMF is simply not OMF.
    if let Some(lib) = crate::build::omf::read_library(bytes) {
        return Format::OmfLibrary {
            runtime: lib.looks_like_compiler_runtime(),
        };
    }
    if crate::build::omf::read_object(bytes).is_some() {
        return Format::OmfObject;
    }
    match coff_machine(bytes) {
        Some(m) => Format::CoffObject(m),
        None => Format::Unknown,
    }
}

/// A PE/COFF object begins with its machine word — unless it is an import stub,
/// which begins `0x0000 0xFFFF` and carries the machine two words later.
fn coff_machine(b: &[u8]) -> Option<Machine> {
    let w = u16::from_le_bytes([*b.first()?, *b.get(1)?]);
    if w == 0x0000 && u16::from_le_bytes([*b.get(2)?, *b.get(3)?]) == 0xFFFF {
        let m = u16::from_le_bytes([*b.get(4)?, *b.get(5)?]);
        return Some(Machine::from_coff(m));
    }
    match Machine::from_coff(w) {
        // An unrecognised machine word is far more likely to be an ordinary file
        // than a COFF object for a processor we have never heard of.
        Machine::Other(_) => None,
        m => Some(m),
    }
}

fn elf_machine(b: &[u8]) -> Option<Machine> {
    // e_machine sits at offset 18 in both ELF32 and ELF64. Byte order follows
    // EI_DATA at offset 5.
    let lo = *b.get(18)?;
    let hi = *b.get(19)?;
    let w = if *b.get(5)? == 2 {
        u16::from_be_bytes([lo, hi])
    } else {
        u16::from_le_bytes([lo, hi])
    };
    Some(Machine::from_elf(w))
}

/// Walk `ar` member headers to the first real member and identify it.
///
/// The leading members are indexes, not objects: GNU writes `/` (symbols) and
/// `//` (long names), MSVC writes two members both called `/`, and BSD writes
/// `__.SYMDEF`. Reading the machine word off one of those would be reading a
/// symbol table.
///
/// The subtlety is that `/` is also how a *real* member with a long name is
/// stored — as `/123`, an offset into the `//` string table. Treating those as
/// indexes skips every object in the archive, which is exactly what MinGW's
/// import libraries look like.
fn archive(b: &[u8]) -> Format {
    const HDR: usize = 60;
    let mut off = 8;
    for _ in 0..16 {
        if off + HDR > b.len() {
            break;
        }
        let hdr = &b[off..off + HDR];
        if &hdr[58..60] != b"`\n" {
            break;
        }
        let raw = String::from_utf8_lossy(&hdr[0..16]).to_string();
        let name = raw.trim_end();
        let size: usize = match String::from_utf8_lossy(&hdr[48..58]).trim().parse() {
            Ok(s) => s,
            Err(_) => break,
        };
        let data = off + HDR;

        if is_index_member(name) {
            // Members are padded to an even offset.
            off = data + size + (size & 1);
            continue;
        }

        // BSD stores a long name in the first `n` bytes of the member data,
        // declared as `#1/n`. The object begins after it.
        let skip = name
            .strip_prefix("#1/")
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0);
        let member = b.get(data + skip..).unwrap_or(&[]);
        if member.starts_with(b"\x7fELF") {
            return Format::ElfArchive {
                machine: elf_machine(member),
            };
        }
        return Format::CoffArchive {
            machine: coff_machine(member),
        };
    }
    // No object member. Either the archive holds none — a stub — or it is
    // truncated. Both are safe to hand the linker; neither contributes code.
    Format::EmptyArchive
}

/// Is this member an index rather than an object?
///
/// Exactly `/` or `//`, or a BSD `__.SYMDEF`. Note that `/123` is *not* one: it
/// is an ordinary member whose name lives in the string table.
fn is_index_member(name: &str) -> bool {
    name == "/" || name == "//" || name == "/SYM64/" || name.starts_with("__.SYMDEF")
}

/// Can the bundle's linker consume this?
pub fn assess(fmt: &Format, want: Expect) -> Result<(), Reason> {
    let machine_ok = |m: Option<Machine>| match m {
        Some(m) if m != want.machine => Err(Reason::WrongMachine {
            found: m,
            want: want.machine,
        }),
        _ => Ok(()),
    };
    match fmt {
        Format::OmfLibrary { runtime: true } => Err(Reason::DosEraRuntime),
        Format::OmfLibrary { runtime: false } | Format::OmfObject => Err(Reason::DosEraFormat),
        Format::Empty => Err(Reason::Empty),
        Format::Unknown | Format::Bitcode => Err(Reason::NotALibrary),
        // An archive with nothing in it belongs to no platform and no machine.
        Format::EmptyArchive => Ok(()),
        Format::CoffArchive { .. } | Format::CoffObject(_) if !want.coff => {
            Err(Reason::WrongPlatform)
        }
        Format::ElfArchive { .. } | Format::ElfObject(_) if want.coff => Err(Reason::WrongPlatform),
        Format::CoffArchive { machine } | Format::ElfArchive { machine } => machine_ok(*machine),
        Format::CoffObject(m) | Format::ElfObject(m) => machine_ok(Some(*m)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win() -> Expect {
        Expect::for_exe_suffix(".exe")
    }
    fn nix() -> Expect {
        Expect::for_exe_suffix("")
    }

    /// A Microsoft/Borland OMF library begins with a LIBHDR record: type byte
    /// `0xF0`, a two-byte record length, the dictionary offset and size.
    fn omf_library() -> Vec<u8> {
        let mut v = vec![0xF0, 0x0D, 0x00];
        v.extend_from_slice(&1024u32.to_le_bytes()); // dictionary offset
        v.extend_from_slice(&2u16.to_le_bytes()); // dictionary size, in pages
        v.push(0x00); // flags
        v.resize(64, 0);
        v
    }

    /// A loose DOS `.OBJ`: a THEADR record naming the source module.
    fn omf_object() -> Vec<u8> {
        let name = b"SOLVER.FOR";
        let mut v = vec![0x80, (name.len() + 2) as u8, 0x00, name.len() as u8];
        v.extend_from_slice(name);
        v.push(0x00);
        v
    }

    fn ar_member(name: &str, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(format!("{name:<16}").as_bytes());
        v.extend_from_slice(b"0           "); // date
        v.extend_from_slice(b"0     "); // uid
        v.extend_from_slice(b"0     "); // gid
        v.extend_from_slice(b"100644  "); // mode
        v.extend_from_slice(format!("{:<10}", data.len()).as_bytes());
        v.extend_from_slice(b"`\n");
        v.extend_from_slice(data);
        if data.len() % 2 == 1 {
            v.push(b'\n');
        }
        v
    }

    fn coff_obj(machine: u16) -> Vec<u8> {
        let mut v = machine.to_le_bytes().to_vec();
        v.resize(40, 0);
        v
    }

    fn elf_obj(machine: u16) -> Vec<u8> {
        let mut v = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0];
        v.resize(18, 0);
        v.extend_from_slice(&machine.to_le_bytes());
        v.resize(64, 0);
        v
    }

    fn archive(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut v = b"!<arch>\n".to_vec();
        for (n, d) in members {
            v.extend_from_slice(&ar_member(n, d));
        }
        v
    }

    // ------------------------------------------------------------ the DOS case

    #[test]
    fn a_dos_era_lib_is_recognised_and_refused() {
        let f = sniff(&omf_library());
        assert_eq!(f, Format::OmfLibrary { runtime: false });
        // and on both platforms, because OMF is not an architecture problem —
        // no linker we ship has ever read it.
        assert_eq!(assess(&f, win()), Err(Reason::DosEraFormat));
        assert_eq!(assess(&f, nix()), Err(Reason::DosEraFormat));
    }

    #[test]
    fn a_dos_era_obj_is_recognised_and_refused() {
        assert_eq!(sniff(&omf_object()), Format::OmfObject);
        assert_eq!(
            assess(&sniff(&omf_object()), win()),
            Err(Reason::DosEraFormat)
        );
        // LHEADR, the other module header, too.
        let mut lheadr = omf_object();
        lheadr[0] = 0x82;
        assert_eq!(sniff(&lheadr), Format::OmfObject);
    }

    #[test]
    fn the_dos_reason_carries_a_translatable_key() {
        assert_eq!(Reason::DosEraFormat.key(), "lib.reject.dos_era");
    }

    // ------------------------------------------------------------- what works

    #[test]
    fn a_mingw_archive_is_accepted_on_windows() {
        let a = archive(&[("/", vec![0; 8]), ("solver.o", coff_obj(0x8664))]);
        assert_eq!(
            sniff(&a),
            Format::CoffArchive {
                machine: Some(Machine::Amd64)
            }
        );
        assert_eq!(assess(&sniff(&a), win()), Ok(()));
    }

    #[test]
    fn a_unix_archive_is_accepted_on_linux() {
        let a = archive(&[("/", vec![0; 8]), ("solver.o", elf_obj(0x3e))]);
        assert_eq!(
            sniff(&a),
            Format::ElfArchive {
                machine: Some(Machine::Amd64)
            }
        );
        assert_eq!(assess(&sniff(&a), nix()), Ok(()));
    }

    #[test]
    fn the_symbol_table_is_skipped_rather_than_read_as_an_object() {
        // A GNU archive puts `/` (symbols) and `//` (long names) first. Reading
        // the machine word off one of those would give a symbol count.
        let a = archive(&[
            ("/", vec![0xAA; 12]),
            ("//", vec![0xBB; 7]),
            ("solver.o", coff_obj(0x014c)),
        ]);
        assert_eq!(
            sniff(&a),
            Format::CoffArchive {
                machine: Some(Machine::I386)
            }
        );
    }

    #[test]
    fn an_odd_sized_member_does_not_desynchronise_the_walk() {
        // ar pads members to an even offset; forgetting that lands the next read
        // one byte into the header.
        let a = archive(&[("/", vec![0xAA; 7]), ("solver.o", coff_obj(0x8664))]);
        assert_eq!(
            sniff(&a),
            Format::CoffArchive {
                machine: Some(Machine::Amd64)
            }
        );
    }

    // ----------------------------------------------------------- mismatches

    #[test]
    fn a_32_bit_library_is_refused_with_both_machines_named() {
        let a = archive(&[("solver.o", coff_obj(0x014c))]);
        assert_eq!(
            assess(&sniff(&a), win()),
            Err(Reason::WrongMachine {
                found: Machine::I386,
                want: Machine::Amd64
            })
        );
    }

    #[test]
    fn an_elf_archive_is_refused_for_a_windows_build_and_the_reverse() {
        let elf = archive(&[("s.o", elf_obj(0x3e))]);
        let coff = archive(&[("s.o", coff_obj(0x8664))]);
        assert_eq!(assess(&sniff(&elf), win()), Err(Reason::WrongPlatform));
        assert_eq!(assess(&sniff(&coff), nix()), Err(Reason::WrongPlatform));
    }

    #[test]
    fn an_import_stub_reports_the_machine_behind_its_sentinel() {
        // A short import library member starts 0x0000 0xFFFF, then the machine.
        let mut m = vec![0x00, 0x00, 0xFF, 0xFF];
        m.extend_from_slice(&0x8664u16.to_le_bytes());
        m.resize(20, 0);
        let a = archive(&[("k.o", m)]);
        assert_eq!(
            sniff(&a),
            Format::CoffArchive {
                machine: Some(Machine::Amd64)
            }
        );
    }

    // ------------------------------------------------------------- not a lib

    #[test]
    fn ordinary_files_are_refused_without_pretending_to_know_what_they_are() {
        assert_eq!(sniff(b""), Format::Empty);
        assert_eq!(assess(&Format::Empty, win()), Err(Reason::Empty));

        // Fortran source, chosen by mistake in the library picker.
        let src = b"      PROGRAM MAIN\n      END\n";
        assert_eq!(sniff(src), Format::Unknown);
        assert_eq!(assess(&sniff(src), win()), Err(Reason::NotALibrary));
    }

    #[test]
    fn a_long_member_name_is_not_mistaken_for_the_symbol_table() {
        // GNU ar stores a name longer than 16 bytes as `/<offset>` into the `//`
        // string table. MinGW's import libraries are full of them
        // (`libgfortran_5_dll_d001740.o`), so reading `/` as "index member"
        // skipped every object and lost the architecture check entirely.
        let a = archive(&[
            ("/", vec![0xAA; 12]),
            ("//", b"libgfortran_5_dll_d001740.o/\n".to_vec()),
            ("/0", coff_obj(0x8664)),
        ]);
        assert_eq!(
            sniff(&a),
            Format::CoffArchive {
                machine: Some(Machine::Amd64)
            }
        );
    }

    #[test]
    fn a_bsd_long_name_is_skipped_before_reading_the_object() {
        let mut data = b"solver_with_a_long_name.o\0\0\0".to_vec();
        let namelen = data.len();
        data.extend_from_slice(&coff_obj(0x8664));
        let a = archive(&[(&format!("#1/{namelen}"), data)]);
        assert_eq!(
            sniff(&a),
            Format::CoffArchive {
                machine: Some(Machine::Amd64)
            }
        );
    }

    /// Every archive the shipped bundles contain must be readable by the
    /// detector, and must be readable *as* the platform it belongs to. Synthetic
    /// fixtures cannot catch a real-world container quirk; this does.
    #[test]
    fn every_archive_in_a_fetched_bundle_is_identified_correctly() {
        for (dir, want) in [
            (
                "target/toolchain/windows-x86_64",
                Expect::for_exe_suffix(".exe"),
            ),
            ("target/toolchain/linux-x86_64", Expect::for_exe_suffix("")),
        ] {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(dir);
            if !root.exists() {
                continue; // bundle not fetched on this machine
            }
            let mut seen = 0;
            let mut stack = vec![root];
            while let Some(d) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else {
                    continue;
                };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().is_some_and(|x| x == "a") {
                        let bytes = std::fs::read(&p).unwrap();
                        let f = sniff(&bytes);
                        assert!(
                            matches!(
                                f,
                                Format::CoffArchive { machine: Some(_) }
                                    | Format::ElfArchive { machine: Some(_) }
                                    | Format::ElfObject(_)
                                    | Format::CoffObject(_)
                                    | Format::EmptyArchive
                            ),
                            "{}: not identified, got {f:?}",
                            p.display()
                        );
                        assert_eq!(assess(&f, want), Ok(()), "{} rejected", p.display());
                        seen += 1;
                    }
                }
            }
            assert!(seen > 0, "no archives found under {dir}");
        }
    }

    #[test]
    fn a_truncated_archive_never_panics() {
        let full = archive(&[("/", vec![0; 8]), ("solver.o", coff_obj(0x8664))]);
        for n in 0..full.len() {
            let _ = assess(&sniff(&full[..n]), win());
        }
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        let mut seed = 0x12345678u32;
        for _ in 0..2000 {
            let mut buf = Vec::new();
            for _ in 0..64 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                buf.push((seed >> 16) as u8);
            }
            let _ = assess(&sniff(&buf), win());
        }
    }
}

#[cfg(test)]
mod empty_archive_tests {
    use super::*;

    #[test]
    fn an_empty_stub_archive_is_accepted_rather_than_called_unreadable() {
        // MinGW's libssp.a really is just the magic and nothing else.
        assert_eq!(sniff(b"!<arch>\n"), Format::EmptyArchive);
        assert_eq!(
            assess(&Format::EmptyArchive, Expect::for_exe_suffix(".exe")),
            Ok(())
        );
        assert_eq!(
            assess(&Format::EmptyArchive, Expect::for_exe_suffix("")),
            Ok(())
        );
    }
}
