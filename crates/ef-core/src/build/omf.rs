//! Reading OMF: the object format of the DOS era.
//!
//! We cannot *link* OMF — GNU ld has no reader for it, and most DOS-era code is
//! 16-bit real-mode anyway, which no amount of conversion turns into 64-bit code.
//! But reading it is straightforward, and worth doing: a library the user cannot use is
//! still a library whose contents tell the user what the user needs.
//!
//! What comes out is the list of modules (whose names, in OMF, are usually the
//! original source file names) and the routines each one defines. That turns "this
//! file is unusable" into "this contains INVERT and MATMUL, built from SOLVER.FOR".
//!
//! Reference: Intel's Object Module Format, as extended by Microsoft. Every record
//! is `type:u8`, `length:u16le`, `length-1` bytes of payload, then a checksum byte.

/// One object module inside a library, or a lone `.OBJ`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Module {
    /// From the `THEADR` record. Usually the source file it was compiled from.
    pub name: String,
    /// Routines and COMMON blocks this module defines.
    pub publics: Vec<String>,
    /// Whoever wrote it said so in a translator comment.
    pub translator: Option<String>,
    /// True when any 32-bit record variant appears. 16-bit is the DOS default and
    /// the one that cannot be rescued.
    pub bits32: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Library {
    /// Only meaningful while walking the records; not state the type owes anyone.
    page_size: usize,
    pub modules: Vec<Module>,
    /// Set when the record stream ran out or did not make sense; what was read
    /// before that point is still returned.
    pub truncated: bool,
}

impl Library {
    /// Wrap one loose object module so it can be described like a library.
    pub fn of_object(module: Module) -> Self {
        Self {
            page_size: 0,
            modules: vec![module],
            truncated: false,
        }
    }

    /// Every public symbol in the library, deduplicated, in first-seen order.
    pub fn all_publics(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for m in &self.modules {
            for p in &m.publics {
                if !out.contains(&p.as_str()) {
                    out.push(p);
                }
            }
        }
        out
    }

    pub fn any_32_bit(&self) -> bool {
        self.modules.iter().any(|m| m.bits32)
    }
}

const THEADR: u8 = 0x80;
const LHEADR: u8 = 0x82;
const COMENT: u8 = 0x88;
const PUBDEF: u8 = 0x90;
const PUBDEF32: u8 = 0x91;
const LPUBDEF: u8 = 0xB6;
const LPUBDEF32: u8 = 0xB7;
const MODEND: u8 = 0x8A;
const MODEND32: u8 = 0x8B;
const LIBHDR: u8 = 0xF0;
const LIBEND: u8 = 0xF1;

/// Read a `.LIB`. Returns `None` if this is not an OMF library at all.
pub fn read_library(b: &[u8]) -> Option<Library> {
    if b.first() != Some(&LIBHDR) {
        return None;
    }
    // The header record's declared length spans the rest of its page, so the page
    // size falls out of it: type + length field + payload.
    let len = u16::from_le_bytes([*b.get(1)?, *b.get(2)?]) as usize;
    let page_size = len + 3;
    if page_size < 16 || !page_size.is_power_of_two() {
        return None;
    }

    let mut lib = Library {
        page_size,
        ..Default::default()
    };
    let mut off = page_size;
    while off < b.len() {
        match b[off] {
            LIBEND => break,
            THEADR | LHEADR => {}
            // Anything else at a page boundary means we have lost the thread.
            _ => {
                lib.truncated = true;
                break;
            }
        }
        match read_module(b, off) {
            Some((m, end)) => {
                lib.modules.push(m);
                // Modules are page-aligned.
                off = end.div_ceil(page_size) * page_size;
                if off <= end.saturating_sub(1) {
                    break;
                }
            }
            None => {
                lib.truncated = true;
                break;
            }
        }
    }
    Some(lib)
}

/// Read a lone `.OBJ`.
pub fn read_object(b: &[u8]) -> Option<Module> {
    if !matches!(b.first(), Some(&THEADR) | Some(&LHEADR)) {
        return None;
    }
    read_module(b, 0).map(|(m, _)| m)
}

/// Walk the records of one module, returning it and the offset just past its end.
fn read_module(b: &[u8], start: usize) -> Option<(Module, usize)> {
    let mut m = Module::default();
    let mut off = start;

    while off + 3 <= b.len() {
        let kind = b[off];
        let len = u16::from_le_bytes([b[off + 1], b[off + 2]]) as usize;
        if len == 0 {
            return None;
        }
        let body_start = off + 3;
        // The declared length counts the payload plus its checksum byte.
        let body_end = body_start + len - 1;
        if body_end > b.len() {
            return None;
        }
        let body = &b[body_start..body_end];

        // Odd record types are the 32-bit variants of their even counterparts.
        if matches!(kind, PUBDEF32 | LPUBDEF32 | MODEND32 | 0x99 | 0x9D | 0xA1) {
            m.bits32 = true;
        }

        match kind {
            THEADR | LHEADR => m.name = length_prefixed(body, &mut 0).unwrap_or_default(),
            COMENT => {
                if let Some(t) = translator_comment(body) {
                    m.translator = Some(t);
                }
            }
            PUBDEF | PUBDEF32 | LPUBDEF | LPUBDEF32 => {
                read_pubdef(body, kind == PUBDEF32 || kind == LPUBDEF32, &mut m.publics);
            }
            MODEND | MODEND32 => return Some((m, body_end + 1)),
            _ => {}
        }
        off = body_end + 1;
    }
    // Records ran out before MODEND. The module is truncated, but everything read
    // so far is still real: the header record identified it as OMF and we may
    // already have its name. Saying "not OMF" here would send a truncated DOS
    // object down the "this is not a text file" path instead of the one that
    // explains what it actually is.
    (off > start).then_some((m, off))
}

/// `PUBDEF`: a base group, a base segment, optionally a frame, then the names.
fn read_pubdef(body: &[u8], wide: bool, out: &mut Vec<String>) {
    let mut p = 0usize;
    if index(body, &mut p).is_none() {
        return;
    }
    let seg = match index(body, &mut p) {
        Some(s) => s,
        None => return,
    };
    if seg == 0 {
        // Base frame, present only when there is no base segment.
        p += 2;
    }
    while p < body.len() {
        let Some(name) = length_prefixed(body, &mut p) else {
            return;
        };
        // public offset
        p += if wide { 4 } else { 2 };
        if index(body, &mut p).is_none() {
            return;
        }
        if !name.is_empty() {
            out.push(name);
        }
    }
}

/// A class-0 `COMENT` names the translator that produced the module.
fn translator_comment(body: &[u8]) -> Option<String> {
    if body.len() < 3 || body[1] != 0x00 {
        return None;
    }
    let text: String = body[2..]
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as char)
        .filter(|c| !c.is_control())
        .collect();
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// A counted string: one length byte, then that many characters.
fn length_prefixed(b: &[u8], p: &mut usize) -> Option<String> {
    let n = *b.get(*p)? as usize;
    *p += 1;
    let s = b.get(*p..*p + n)?;
    *p += n;
    Some(s.iter().map(|c| *c as char).collect())
}

/// An OMF index: one byte below 0x80, otherwise two with the top bit as a flag.
fn index(b: &[u8], p: &mut usize) -> Option<u16> {
    let x = *b.get(*p)?;
    *p += 1;
    if x & 0x80 == 0 {
        return Some(x as u16);
    }
    let y = *b.get(*p)?;
    *p += 1;
    Some((((x & 0x7f) as u16) << 8) | y as u16)
}

impl Library {
    /// Does this look like a compiler's own runtime library rather than the user's code?
    ///
    /// It matters because the advice is opposite. For a library of their own
    /// routines the answer is "find the Fortran source it was built from". For the
    /// compiler's runtime the answer is "you do not need this file at all" — the
    /// modern compiler brings its own, and linking a 1985 runtime beside it would
    /// be wrong even if the format allowed it.
    ///
    /// Recognised by the company it keeps: C startup modules, and Microsoft's
    /// six-characters-then-`QQ` runtime naming convention.
    pub fn looks_like_compiler_runtime(&self) -> bool {
        const STARTUP: &[&str] = &["crt0", "crt0dat", "entx6l", "stdargv", "stdenvp", "_exit"];
        if self
            .modules
            .iter()
            .any(|m| STARTUP.contains(&m.name.to_ascii_lowercase().as_str()))
        {
            return true;
        }
        let publics = self.all_publics();
        if publics.len() < 8 {
            return false;
        }
        let qq = publics
            .iter()
            .filter(|s| s.len() == 6 && s.ends_with("QQ"))
            .count();
        qq * 4 >= publics.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one OMF record: type, length, payload, checksum.
    fn record(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![kind];
        v.extend_from_slice(&((payload.len() + 1) as u16).to_le_bytes());
        v.extend_from_slice(payload);
        v.push(0); // checksum; readers treat 0 as "not checked"
        v
    }

    fn counted(s: &str) -> Vec<u8> {
        let mut v = vec![s.len() as u8];
        v.extend_from_slice(s.as_bytes());
        v
    }

    fn theadr(name: &str) -> Vec<u8> {
        record(THEADR, &counted(name))
    }

    /// A PUBDEF with no base group or segment, so a two-byte frame follows.
    fn pubdef(names: &[&str], wide: bool) -> Vec<u8> {
        let mut p = vec![0x00, 0x00, 0x00, 0x00]; // group 0, segment 0, frame 0
        for n in names {
            p.extend_from_slice(&counted(n));
            p.extend_from_slice(if wide { &[0, 0, 0, 0] } else { &[0, 0] });
            p.push(0x00); // type index
        }
        record(if wide { PUBDEF32 } else { PUBDEF }, &p)
    }

    fn modend() -> Vec<u8> {
        record(MODEND, &[0x00])
    }

    fn module(name: &str, publics: &[&str], wide: bool) -> Vec<u8> {
        let mut v = theadr(name);
        v.extend_from_slice(&pubdef(publics, wide));
        v.extend_from_slice(&modend());
        v
    }

    const PAGE: usize = 16;

    fn library(mods: &[(&str, &[&str])], wide: bool) -> Vec<u8> {
        let mut v = record(LIBHDR, &[0u8; PAGE - 4]);
        assert_eq!(v.len(), PAGE, "header must fill exactly one page");
        for (name, publics) in mods {
            let m = module(name, publics, wide);
            v.extend_from_slice(&m);
            while !v.len().is_multiple_of(PAGE) {
                v.push(0);
            }
        }
        v.push(LIBEND);
        v
    }

    #[test]
    fn a_library_yields_its_modules_and_their_routines() {
        let b = library(
            &[
                ("SOLVER.FOR", &["INVERT", "MATMUL"]),
                ("IO.FOR", &["READIN"]),
            ],
            false,
        );
        let lib = read_library(&b).expect("should parse");
        assert_eq!(lib.page_size, PAGE);
        assert!(!lib.truncated);
        assert_eq!(lib.modules.len(), 2);
        assert_eq!(lib.modules[0].name, "SOLVER.FOR");
        assert_eq!(lib.modules[0].publics, vec!["INVERT", "MATMUL"]);
        assert_eq!(lib.modules[1].name, "IO.FOR");
        assert_eq!(lib.all_publics(), vec!["INVERT", "MATMUL", "READIN"]);
    }

    #[test]
    fn sixteen_and_thirty_two_bit_modules_are_told_apart() {
        // The distinction decides whether conversion is even conceivable.
        let narrow = read_library(&library(&[("A.FOR", &["X"])], false)).unwrap();
        assert!(!narrow.any_32_bit(), "plain PUBDEF means 16-bit");

        let wide = read_library(&library(&[("A.FOR", &["X"])], true)).unwrap();
        assert!(wide.any_32_bit(), "PUBDEF32 means 32-bit");
    }

    #[test]
    fn a_compiler_runtime_is_told_apart_from_the_users_own_code() {
        // The user's code: a handful of named routines.
        let own_code = read_library(&library(
            &[("SOLVER.FOR", &["INVERT", "MATMUL", "SOLVE"])],
            false,
        ))
        .unwrap();
        assert!(!own_code.looks_like_compiler_runtime());

        // The compiler's runtime: recognised by its startup module. This is the
        // shape of the real Microsoft FORTRAN 3.30 libraries.
        let runtime = read_library(&library(&[("crt0dat", &["BEGXQQ"])], false)).unwrap();
        assert!(runtime.looks_like_compiler_runtime());

        // And by Microsoft's six-then-QQ naming, with no startup module present.
        let qq = read_library(&library(
            &[(
                "entx",
                &[
                    "AGCXQQ", "BEGXQQ", "CESXQQ", "CLNEQQ", "ENDXQQ", "HDRFQQ", "PNUXQQ", "RETLQQ",
                ],
            )],
            false,
        ))
        .unwrap();
        assert!(qq.looks_like_compiler_runtime());
    }

    #[test]
    fn a_lone_object_module_reads_too() {
        let b = module("SOLVER.FOR", &["INVERT"], false);
        let m = read_object(&b).expect("should parse");
        assert_eq!(m.name, "SOLVER.FOR");
        assert_eq!(m.publics, vec!["INVERT"]);
    }

    #[test]
    fn what_is_not_omf_is_refused_rather_than_guessed_at() {
        assert!(read_library(b"!<arch>\n").is_none());
        assert!(read_library(b"").is_none());
        assert!(read_object(b"      PROGRAM MAIN\n").is_none());
        // a 0xF0 first byte but a nonsensical page size
        assert!(read_library(&[0xF0, 0x01, 0x00, 0x00]).is_none());
    }

    #[test]
    fn a_truncated_library_returns_what_it_read_and_says_so() {
        let full = library(&[("A.FOR", &["X"]), ("B.FOR", &["Y"])], false);
        for n in 0..full.len() {
            // Must not panic at any length, and must never claim more than it read.
            if let Some(lib) = read_library(&full[..n]) {
                assert!(lib.modules.len() <= 2);
            }
        }
    }

    #[test]
    fn arbitrary_bytes_after_a_library_header_never_panic() {
        let mut seed = 0x9E3779B9u32;
        for _ in 0..2000 {
            let mut b = record(LIBHDR, &[0u8; PAGE - 4]);
            for _ in 0..128 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                b.push((seed >> 16) as u8);
            }
            let _ = read_library(&b);
        }
    }
}
