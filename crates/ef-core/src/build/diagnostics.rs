//! Parsing gfortran's output.
//!
//! Deliberately a pure `&str -> Vec<Diagnostic>` function with no I/O: that is what
//! makes it testable from captured fixture files on a machine with no gfortran
//! installed.
//!
//! Modern gfortran puts the location on its own line and the severity *after* the
//! source snippet:
//!
//! ```text
//! ./src/solver.f:12:24:
//!
//!    12 |       IF (X .EQ. 1) GOTO 100
//!       |                        1
//! Error: Symbol 'x' at (1) has no IMPLICIT type
//! ```

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub fn is_error(self) -> bool {
        matches!(self, Severity::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// The file as the compiler named it — a staged name until `rewrite_names` runs.
    pub file: Option<String>,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub message: String,
    pub snippet: Vec<String>,
    /// Set for `undefined reference to 'foo_'`, which usually means a file is
    /// missing from the program rather than that the code is wrong.
    pub undefined_symbol: Option<String>,
}

impl Diagnostic {
    fn new(severity: Severity, message: String) -> Self {
        Self {
            severity,
            file: None,
            line: None,
            col: None,
            message,
            snippet: Vec::new(),
            undefined_symbol: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Pending {
    file: String,
    line: Option<u32>,
    col: Option<u32>,
    snippet: Vec<String>,
}

pub fn parse(output: &str) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut pending: Option<Pending> = None;

    for raw in output.lines() {
        let line = raw.trim_end_matches(['\r']);

        // `file:line:col:` on its own — the start of a diagnostic.
        if let Some(loc) = parse_location_header(line) {
            pending = Some(loc);
            continue;
        }

        // `Error: ...` / `Warning: ...` — the end of one.
        if let Some((sev, msg)) = parse_severity(line) {
            let mut d = Diagnostic::new(sev, msg);
            if let Some(p) = pending.take() {
                d.file = Some(p.file);
                d.line = p.line;
                d.col = p.col;
                d.snippet = p.snippet;
            }
            out.push(d);
            continue;
        }

        // Driver-level failures carry no location header.
        if let Some(rest) = strip_tool_prefix(line) {
            if let Some((sev, msg)) = parse_severity(rest) {
                out.push(Diagnostic::new(sev, msg));
                continue;
            }
        }

        // Linker: the message we most want to explain in plain language.
        if let Some(sym) = parse_undefined_reference(line) {
            let mut d = Diagnostic::new(Severity::Error, format!("undefined reference to `{sym}'"));
            d.undefined_symbol = Some(sym);
            out.push(d);
            continue;
        }

        // Anything else between a header and its severity is the snippet.
        if let Some(p) = pending.as_mut() {
            if !line.trim().is_empty() {
                p.snippet.push(line.to_string());
            }
        }
    }

    out
}

/// `path:line:col:` — nothing else on the line.
///
/// Split from the right so a Windows drive letter (`C:\work\src\a.f:12:24:`) is
/// handled correctly.
fn parse_location_header(line: &str) -> Option<Pending> {
    let body = line.strip_suffix(':')?;
    if body.is_empty() {
        return None;
    }
    let (rest, col) = body.rsplit_once(':')?;
    let (file, lineno) = rest.rsplit_once(':')?;
    let col: u32 = col.trim().parse().ok()?;
    let lineno: u32 = lineno.trim().parse().ok()?;
    if file.trim().is_empty() {
        return None;
    }
    Some(Pending {
        file: file.trim().to_string(),
        line: Some(lineno),
        col: Some(col),
        snippet: Vec::new(),
    })
}

fn parse_severity(line: &str) -> Option<(Severity, String)> {
    for (prefix, sev) in [
        ("Fatal Error:", Severity::Error),
        ("Internal Error:", Severity::Error),
        ("Error:", Severity::Error),
        ("error:", Severity::Error),
        ("Warning:", Severity::Warning),
        ("warning:", Severity::Warning),
        ("Note:", Severity::Note),
        ("note:", Severity::Note),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some((sev, rest.trim().to_string()));
        }
    }
    None
}

/// `f951: `, `gfortran: `, `collect2: `, `/usr/bin/ld: ` and friends.
fn strip_tool_prefix(line: &str) -> Option<&str> {
    for tool in ["f951:", "gfortran:", "collect2:", "cc1:", "as:"] {
        if let Some(rest) = line.strip_prefix(tool) {
            return Some(rest.trim_start());
        }
    }
    None
}

fn parse_undefined_reference(line: &str) -> Option<String> {
    let idx = line.find("undefined reference to ")?;
    let rest = &line[idx + "undefined reference to ".len()..];
    let rest = rest.trim_start_matches(['`', '\'', '"']);
    let end = rest.find(['\'', '`', '"']).unwrap_or(rest.len());
    let sym = rest[..end].trim();
    if sym.is_empty() {
        None
    } else {
        Some(sym.to_string())
    }
}

/// Replace staged file names with the names the user actually recognises.
///
/// The compiler only ever sees `solver.f`; the user only ever saw `SOLVER.FOR`.
pub fn rewrite_names(diags: &mut [Diagnostic], staged: &[crate::build::stage::StagedSource]) {
    for d in diags.iter_mut() {
        let Some(file) = d.file.as_ref() else {
            continue;
        };
        let base = file
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(file.as_str())
            .to_string();
        if let Some(s) = staged.iter().find(|s| s.staged_name == base) {
            d.file = Some(s.display_name.clone());
        }
    }
}

pub fn count_errors(diags: &[Diagnostic]) -> usize {
    diags.iter().filter(|d| d.severity.is_error()).count()
}

pub fn count_warnings(diags: &[Diagnostic]) -> usize {
    diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_modern_multi_line_error() {
        let out = "\
./src/solver.f:12:24:

   12 |       IF (X .EQ. 1) GOTO 100
      |                        1
Error: Symbol 'x' at (1) has no IMPLICIT type
";
        let d = parse(out);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[0].file.as_deref(), Some("./src/solver.f"));
        assert_eq!(d[0].line, Some(12));
        assert_eq!(d[0].col, Some(24));
        assert!(d[0].message.contains("has no IMPLICIT type"));
        assert_eq!(d[0].snippet.len(), 2);
    }

    #[test]
    fn a_windows_drive_letter_does_not_confuse_the_split() {
        let out = "C:\\work\\build-1\\src\\solver.f:7:3:\n\nError: Something went wrong\n";
        let d = parse(out);
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].file.as_deref(),
            Some("C:\\work\\build-1\\src\\solver.f")
        );
        assert_eq!(d[0].line, Some(7));
        assert_eq!(d[0].col, Some(3));
    }

    #[test]
    fn parses_warnings_and_counts_separately() {
        let out = "\
./src/a.f:5:73:

    5 |       X = 1
Warning: Line truncated at (1) [-Wline-truncation]
./src/a.f:9:1:

Error: Unterminated character constant at (1)
";
        let d = parse(out);
        assert_eq!(count_errors(&d), 1);
        assert_eq!(count_warnings(&d), 1);
    }

    #[test]
    fn parses_a_driver_level_fatal_error_with_no_location() {
        let out =
            "f951: Fatal Error: solver.f: No such file or directory\ncompilation terminated.\n";
        let d = parse(out);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
        assert!(d[0].file.is_none());
        assert!(d[0].message.contains("No such file"));
    }

    #[test]
    fn parses_an_undefined_reference_and_extracts_the_symbol() {
        let out = "\
/usr/bin/ld: ./obj/main.o: in function `MAIN__':
main.f:(.text+0x1a): undefined reference to `mysub_'
collect2: error: ld returned 1 exit status
";
        let d = parse(out);
        let undef: Vec<_> = d.iter().filter(|x| x.undefined_symbol.is_some()).collect();
        assert_eq!(undef.len(), 1);
        assert_eq!(undef[0].undefined_symbol.as_deref(), Some("mysub_"));
        assert_eq!(undef[0].severity, Severity::Error);
    }

    #[test]
    fn staged_names_are_rewritten_back_to_the_original_names() {
        use crate::build::stage::StagedSource;
        use std::path::PathBuf;
        let mut d = parse("./src/solver.f:1:1:\n\nError: boom\n");
        let staged = vec![StagedSource {
            original: PathBuf::from("/home/user/SOLVER.FOR"),
            staged: PathBuf::from("/work/src/solver.f"),
            staged_name: "solver.f".into(),
            display_name: "SOLVER.FOR".into(),
            obj: PathBuf::from("/work/obj/solver.o"),
        }];
        rewrite_names(&mut d, &staged);
        assert_eq!(d[0].file.as_deref(), Some("SOLVER.FOR"));
    }

    #[test]
    fn empty_and_noise_output_yields_nothing() {
        assert!(parse("").is_empty());
        assert!(parse("\n\n   \n").is_empty());
        assert!(parse("just some text that is not a diagnostic\n").is_empty());
    }

    #[test]
    fn a_bare_colon_line_is_not_mistaken_for_a_location() {
        assert!(parse(":\n").is_empty());
        assert!(parse("::\n").is_empty());
        assert!(parse("a:b:c:\n").is_empty());
    }

    #[test]
    fn crlf_output_parses_identically() {
        let unix = parse("./src/a.f:1:2:\n\nError: boom\n");
        let dos = parse("./src/a.f:1:2:\r\n\r\nError: boom\r\n");
        assert_eq!(unix, dos);
    }
}
