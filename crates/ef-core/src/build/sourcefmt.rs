//! Is this file actually Fortran source text?
//!
//! The same argument as `objfmt`, one step earlier. Handed a file that is not
//! source at all, gfortran does not say so — it tries to parse it and emits a
//! cascade of `Non-numeric character in statement label` and `Invalid character
//! 0x14`. Twenty-five of those in front of someone who did not write them is
//! worse than useless: nothing in them says "this is not a Fortran program".
//!
//! This is not hypothetical. One real example carried a `.FOR` name but was a data
//! file belonging to an application called FAS4, and compiling it produced exactly
//! that cascade.
//!
//! The checks are deliberately narrow. Old Fortran is full of things that look
//! wrong and are not: form feeds between listing pages, a Ctrl-Z end-of-file
//! marker from DOS, tabs, and high-bit bytes from a Vietnamese codepage. None of
//! those make a file binary, and flagging them would break real work.

use crate::error::{FileProblem, ProblemArg};

/// What a file turned out to be, when it is not Fortran source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotSource {
    Empty,
    /// Saved as Unicode rather than plain text. Recoverable, and worth saying so:
    /// Notepad and Word both offer it, and the result is unreadable to a compiler.
    Utf16,
    /// A format we recognise well enough to name. Holds the i18n key for that name.
    Known(&'static str),
    /// A compiled library or object. Worth its own case: the advice is not
    /// "this is unreadable" but "add it to the Libraries list instead".
    LibraryFile,
    /// Binary by content: a NUL byte, which no text file contains.
    Binary,
}

impl NotSource {
    pub fn key(&self) -> &'static str {
        match self {
            NotSource::Empty => "src.reject.empty",
            NotSource::Utf16 => "src.reject.utf16",
            NotSource::Known(_) => "src.reject.known",
            NotSource::LibraryFile => "src.reject.library",
            NotSource::Binary => "src.reject.binary",
        }
    }

    /// The finished, translatable description of this refusal.
    pub fn problem(&self, name: String) -> FileProblem {
        let args = match self {
            // The name of the format is itself prose, so it is translated too.
            NotSource::Known(what_key) => vec![("what", ProblemArg::Key(what_key))],
            _ => Vec::new(),
        };
        FileProblem {
            name,
            title_key: "src.unusable_title",
            reason_key: self.key(),
            args,
        }
    }

    pub fn english(&self) -> String {
        match self {
            NotSource::Empty => "the file is empty.".into(),
            NotSource::Utf16 => "saved as Unicode (UTF-16) rather than plain text. Open it in \
                 Notepad and save it again with encoding set to ANSI or UTF-8."
                .into(),
            // The format's name is a catalog entry, so English takes it from the
            // English catalog rather than keeping a second copy here. The sentence
            // around it stays in this file's own technical register.
            NotSource::Known(k) => format!(
                "a {}, not Fortran source text.",
                crate::i18n::lookup(crate::i18n::Lang::En, k)
            ),
            NotSource::LibraryFile => "a compiled library, not Fortran source text. Add it \
                 under Libraries rather than Source files."
                .into(),
            NotSource::Binary => "not a text file, so it cannot be Fortran source.".into(),
        }
    }
}

/// Where a signature has to appear for it to count.
#[derive(Debug, Clone, Copy)]
enum At {
    /// The very start of the file.
    Start,
    /// Anywhere in the first `n` bytes: some formats print a banner line first.
    Within(usize),
}

/// Signatures worth naming, because naming one tells the user what to do instead.
///
/// The name is an i18n key rather than English prose: it lands in the `{what}`
/// slot of a translated sentence, so English here would strand an English
/// fragment in the middle of a Vietnamese one.
const SIGNATURES: &[(&[u8], At, &str)] = &[
    (b"%PDF-", At::Start, "src.what.pdf"),
    (b"{\\rtf", At::Start, "src.what.rtf"),
    (b"PK\x03\x04", At::Start, "src.what.zip"),
    (
        b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1",
        At::Start,
        "src.what.office",
    ),
    (b"MZ", At::Start, "src.what.windows_program"),
    (b"\x7fELF", At::Start, "src.what.linux_program"),
    // A real case: a FAS4 data file wearing a .FOR extension. Inside the table rather
    // than a special case beside it, so the table stays the place to look.
    (b"FAS4-FILE", At::Within(64), "src.what.fas4"),
];

/// How much of the file to judge by. A source file declares itself early.
const WINDOW: usize = 8192;

pub fn check(bytes: &[u8]) -> Result<(), NotSource> {
    if bytes.is_empty() {
        return Err(NotSource::Empty);
    }
    // A UTF-16 BOM. Checked before the NUL scan, which would otherwise call this
    // binary and give the user advice the user cannot act on.
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(NotSource::Utf16);
    }
    for (sig, at, what) in SIGNATURES {
        let hit = match at {
            At::Start => bytes.starts_with(sig),
            At::Within(n) => bytes[..bytes.len().min(*n)]
                .windows(sig.len())
                .any(|w| w == *sig),
        };
        if hit {
            return Err(NotSource::Known(what));
        }
    }
    // A library added to the wrong list. Parsed rather than sniffed by its first
    // byte: 0xF0 is also `đ` in the Vietnamese codepage the user's sources may use, and
    // calling a Fortran file a library would be its own kind of unhelpful.
    if bytes.starts_with(b"!<arch>")
        || crate::build::omf::read_library(bytes).is_some()
        || crate::build::omf::read_object(bytes).is_some()
    {
        return Err(NotSource::LibraryFile);
    }
    // A NUL byte settles it. Nothing else does: form feeds separate listing
    // pages, Ctrl-Z ends a DOS text file, and a Vietnamese codepage fills the
    // high half of the byte range with perfectly ordinary letters.
    let window = &bytes[..bytes.len().min(WINDOW)];
    if window.contains(&0) {
        return Err(NotSource::Binary);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------- real Fortran passes
    //
    // These matter more than the rejections. A false positive here refuses work
    // the user is entitled to do, on a file that is perfectly good.

    #[test]
    fn ordinary_fixed_form_source_is_accepted() {
        assert_eq!(check(b"      PROGRAM MAIN\n      END\n"), Ok(()));
    }

    #[test]
    fn the_things_old_fortran_is_full_of_are_not_binary() {
        // A form feed between listing pages.
        assert_eq!(check(b"      PROGRAM MAIN\n\x0c      END\n"), Ok(()));
        // A DOS Ctrl-Z end-of-file marker.
        assert_eq!(check(b"      END\n\x1a"), Ok(()));
        // Tabs, which gfortran warns about and compiles anyway.
        assert_eq!(check(b"\tPROGRAM MAIN\n\tEND\n"), Ok(()));
        // CRLF, and a lone CR from a Mac-era editor.
        assert_eq!(check(b"      PROGRAM MAIN\r\n      END\r\n"), Ok(()));
        assert_eq!(check(b"      PROGRAM MAIN\r      END\r"), Ok(()));
        // A UTF-8 BOM, which is not UTF-16 and is harmless.
        assert_eq!(check(b"\xef\xbb\xbf      END\n"), Ok(()));
    }

    #[test]
    fn a_vietnamese_codepage_is_text_however_high_its_bytes() {
        // CP-1258 fills the high half of the byte range with ordinary letters.
        // Judging "binary" by the high bit would reject their own comments.
        let mut src = b"C T\xednh d\xe2\xf9 b\xea t\xf4ng\n      END\n".to_vec();
        assert_eq!(check(&src), Ok(()));
        // ...including one that opens with 0xF0, which is `d` with a stroke here
        // and an OMF library header elsewhere.
        src.insert(0, 0xF0);
        assert_eq!(check(&src), Ok(()));
    }

    // ------------------------------------------------------------- rejections

    #[test]
    fn a_fas4_file_is_named_rather_than_parsed_as_fortran() {
        // The real one, whose first bytes these are. Compiling it produced
        // twenty-five errors about invalid characters and never said why.
        let b = b"\r\n FAS4-FILE ; Do not change it!\r\n1295\r\n108 $\x14\x01\x01\x01\x00";
        assert_eq!(check(b), Err(NotSource::Known("src.what.fas4")));
    }

    #[test]
    fn a_file_saved_as_unicode_gets_advice_he_can_act_on() {
        // Notepad and Word both offer this, and the result compiles to nonsense.
        let mut b = vec![0xFF, 0xFE];
        for c in "      END\n".bytes() {
            b.push(c);
            b.push(0);
        }
        assert_eq!(check(&b), Err(NotSource::Utf16));
        // and it must be recognised before the NUL scan calls it merely binary
        assert_ne!(check(&b), Err(NotSource::Binary));
    }

    #[test]
    fn documents_chosen_by_mistake_are_named() {
        assert_eq!(check(b"%PDF-1.4\n"), Err(NotSource::Known("src.what.pdf")));
        assert_eq!(
            check(b"PK\x03\x04\x14\x00"),
            Err(NotSource::Known("src.what.zip"))
        );
        assert_eq!(
            check(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"),
            Err(NotSource::Known("src.what.office"))
        );
        assert_eq!(
            check(b"MZ\x90\x00"),
            Err(NotSource::Known("src.what.windows_program"))
        );
    }

    #[test]
    fn a_library_added_to_the_wrong_list_is_sent_to_the_right_one() {
        assert_eq!(check(b"!<arch>\n"), Err(NotSource::LibraryFile));
        assert_eq!(NotSource::LibraryFile.key(), "src.reject.library");

        // A real OMF library header, parsed rather than guessed from byte one.
        let mut omf = vec![0xF0u8, 0x0D, 0x00];
        omf.extend_from_slice(&1024u32.to_le_bytes());
        omf.extend_from_slice(&2u16.to_le_bytes());
        omf.push(0x00);
        omf.resize(64, 0);
        assert_eq!(check(&omf), Err(NotSource::LibraryFile));
    }

    #[test]
    fn a_nul_byte_settles_it() {
        assert_eq!(check(b"      PROG\x00RAM\n"), Err(NotSource::Binary));
    }

    #[test]
    fn an_empty_file_says_so_rather_than_failing_obscurely() {
        assert_eq!(check(b""), Err(NotSource::Empty));
    }

    #[test]
    fn every_rejection_carries_a_key_and_plain_english() {
        for p in [
            NotSource::Empty,
            NotSource::Utf16,
            NotSource::Known("src.what.pdf"),
            NotSource::LibraryFile,
            NotSource::Binary,
        ] {
            assert!(p.key().starts_with("src.reject."));
            assert!(!p.english().is_empty());
        }
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        let mut seed = 0x5bf03635u32;
        for _ in 0..3000 {
            let mut b = Vec::new();
            for _ in 0..48 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                b.push((seed >> 16) as u8);
            }
            let _ = check(&b);
        }
    }
}

/// Every key these enums can name must exist in both catalogs.
///
/// The xtask key check greps literal `tr!` call sites, and cannot see a key
/// chosen at run time — which is every key in this module and in `objfmt`. They
/// reach the interface through `problem.reason_key`, so without this they were
/// the only user-facing strings nothing verified.
#[cfg(test)]
mod key_coverage {
    use crate::build::objfmt::{Machine, Reason};
    use crate::build::sourcefmt::NotSource;
    use crate::error::ProblemArg;
    use crate::i18n::{keys, Lang};

    /// Checked against each catalog's own key set, NOT through `lookup`.
    ///
    /// `lookup` falls back to English when a Vietnamese key is missing, so asking
    /// it whether a key resolves can never detect the failure that matters here —
    /// the interface silently reverting to English in front of someone who does
    /// not read it. This test was written that way first and passed happily with
    /// a key deleted from vi.toml.
    fn assert_resolves(key: &str) {
        for lang in [Lang::Vi, Lang::En] {
            assert!(
                keys(lang).contains(&key),
                "{key} is missing from the {lang:?} catalog, so it would silently \
                 fall back to the other language"
            );
        }
    }

    #[test]
    fn every_source_rejection_resolves_in_both_languages() {
        for p in [
            NotSource::Empty,
            NotSource::Utf16,
            NotSource::LibraryFile,
            NotSource::Binary,
            NotSource::Known("src.what.pdf"),
        ] {
            assert_resolves(p.key());
            // and the nouns that fill `{what}` are catalog entries too, which is
            // what stops an English phrase landing inside a Vietnamese sentence
            for (_, arg) in p.problem("x".into()).args {
                if let ProblemArg::Key(k) = arg {
                    assert_resolves(k);
                }
            }
        }
    }

    #[test]
    fn every_named_file_format_resolves_in_both_languages() {
        for (_, _, key) in super::SIGNATURES {
            assert_resolves(key);
        }
    }

    #[test]
    fn every_library_rejection_resolves_in_both_languages() {
        for r in [
            Reason::DosEraFormat,
            Reason::DosEraRuntime,
            Reason::WrongPlatform,
            Reason::NotALibrary,
            Reason::Empty,
            Reason::WrongMachine {
                found: Machine::I386,
                want: Machine::Amd64,
            },
        ] {
            assert_resolves(r.key());
            assert_resolves(r.problem("x".into()).title_key);
        }
    }

    #[test]
    fn every_problem_title_resolves_in_both_languages() {
        assert_resolves(NotSource::Binary.problem("x".into()).title_key);
    }
}
