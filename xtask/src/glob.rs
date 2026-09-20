//! A very small glob matcher for the toolchain prune rules.
//!
//! Only `*` is supported, and it matches any sequence of characters **including
//! `/`** — the same semantics Python's `fnmatch` gave the rules when they were
//! worked out and validated, so the patterns in the recipe mean exactly what they
//! meant when they were proven against the corpus.

/// Does `text` match `pattern`?
pub fn matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    is_match(&p, &t)
}

fn is_match(p: &[char], t: &[char]) -> bool {
    // Iterative backtracking: linear in the common case, and no recursion depth
    // worries on long paths.
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);

    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn literal_paths_match_exactly() {
        assert!(matches("bundle.toml", "bundle.toml"));
        assert!(!matches("bundle.toml", "bundle.tom"));
        assert!(!matches("bundle.toml", "xbundle.toml"));
    }

    #[test]
    fn a_star_crosses_directory_separators() {
        // This is the property the recipe's rules rely on.
        assert!(matches(
            "*/sysroot/usr/lib64/*",
            "x86_64-conda/sysroot/usr/lib64/libc.a"
        ));
        assert!(matches(
            "*/sysroot/usr/lib64/*",
            "x86_64-conda/sysroot/usr/lib64/nested/deep/thing.o"
        ));
        assert!(matches(
            "lib/gcc/*/*/finclude/*",
            "lib/gcc/trip/16.2.0/finclude/omp_lib.mod"
        ));
    }

    #[test]
    fn prefix_and_suffix_stars_work() {
        assert!(matches(
            "bin/*-gfortran",
            "bin/x86_64-conda-linux-gnu-gfortran"
        ));
        assert!(matches("lib/libzstd.so*", "lib/libzstd.so.1"));
        assert!(matches("lib/libzstd.so*", "lib/libzstd.so"));
        assert!(!matches("bin/*-gfortran", "bin/gfortran-wrapper"));
    }

    #[test]
    fn the_rules_that_were_validated_still_select_what_they_did() {
        let keep = [
            "libexec/gcc/*/*/f951",
            "lib/gcc/*/*/*.o",
            "*/lib/ldscripts/*",
        ];
        let kept = |f: &str| keep.iter().any(|p| matches(p, f));
        assert!(kept("libexec/gcc/trip/16.2.0/f951"));
        assert!(kept("lib/gcc/trip/16.2.0/crtbegin.o"));
        assert!(kept("trip/lib/ldscripts/elf_x86_64.x"));
        // and the big things we deliberately drop
        assert!(!kept("libexec/gcc/trip/16.2.0/cc1"));
        assert!(!kept("libexec/gcc/trip/16.2.0/lto1"));
        assert!(!kept("lib/gcc/trip/16.2.0/libasan.a"));
    }

    #[test]
    fn a_lone_star_matches_everything_and_empty_matches_only_empty() {
        assert!(matches("*", "anything/at/all"));
        assert!(matches("", ""));
        assert!(!matches("", "x"));
        assert!(matches("**", "anything"));
    }

    #[test]
    fn backtracking_terminates_on_pathological_input() {
        // Naive implementations blow up here.
        let text = "a".repeat(64);
        assert!(!matches("*a*a*a*a*a*a*a*b", &text));
    }
}
