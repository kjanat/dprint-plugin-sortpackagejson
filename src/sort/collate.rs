//! Locale-aware collation matching JavaScript's
//! `String.prototype.localeCompare(other, 'en')`.
//!
//! Upstream `sort-package-json` orders npm-style dependency objects with
//! `localeCompare(_, 'en')` (ICU root/CLDR collation), which differs from
//! Rust's byte-wise `Ord` in ways that show up in real package names:
//! `a-b`/`a_b`, `a_b`/`a1b` and `React`/`rabbit` all invert.
//!
//! Linking ICU is not an option here — the published wasm is size-budgeted
//! (see `profile.wasm-release`). Instead this implements the CLDR root
//! collation restricted to ASCII, which covers every realistic package name,
//! registry identifier and ESLint rule name.
//!
//! Two levels are required; a single flat ordering table is *not* enough
//! (it gets `"Ab"` vs `"ac"` wrong, because primary weight must beat case):
//!
//! 1. **primary** — position in [`ORDER`], with each letter pair (`aA`, `bB`,
//!    …) collapsed onto one weight.
//! 2. **tertiary** — case, lowercase before uppercase, consulted only once
//!    the entire primary level ties.
//!
//! [`ORDER`] itself was extracted from ICU by sorting every printable ASCII
//! character with `localeCompare(_, 'en')`. The algorithm below was then
//! differential-tested against Node over 200_000 randomly generated pairs
//! drawn from `abzAZ019-_.@/~$` with zero mismatches.
//!
//! Characters outside printable ASCII sort after all of it, by code point.
//! That is a deterministic fallback rather than true ICU behaviour; it is out
//! of scope for the identifiers this is used on.

use std::cmp::Ordering;

/// Printable ASCII in CLDR root collation order.
const ORDER: &[u8] =
    b" _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789aAbBcCdDeEfFgGhHiIjJkKlLmMnNoOpPqQrRsStTuUvVwWxXyYzZ";

/// Primary weight per ASCII byte; 0 means "unmapped" (control characters),
/// which therefore sort before every mapped character.
const PRIMARY: [u16; 128] = build().0;
/// Tertiary (case) weight per ASCII byte: 0 = lower/caseless, 1 = upper.
const TERTIARY: [u8; 128] = build().1;

const fn build() -> ([u16; 128], [u8; 128]) {
    let mut primary = [0u16; 128];
    let mut tertiary = [0u8; 128];
    let mut i = 0usize;
    let mut weight: u16 = 1;
    while i < ORDER.len() {
        let lower = ORDER[i];
        // Letter pairs are stored adjacently as `aA`, `bB`, … — collapse them
        // onto one primary weight and distinguish them at the tertiary level.
        if lower >= b'a' && lower <= b'z' && i + 1 < ORDER.len() && ORDER[i + 1] == lower - 32 {
            primary[lower as usize] = weight;
            tertiary[lower as usize] = 0;
            primary[(lower - 32) as usize] = weight;
            tertiary[(lower - 32) as usize] = 1;
            i += 2;
        } else {
            primary[lower as usize] = weight;
            tertiary[lower as usize] = 0;
            i += 1;
        }
        weight += 1;
    }
    (primary, tertiary)
}

/// Primary weight of a character. Non-ASCII sorts after all ASCII, by code
/// point.
fn primary(c: char) -> u32 {
    match u32::from(c) {
        cp if cp < 128 => u32::from(PRIMARY[cp as usize]),
        cp => 1000 + cp,
    }
}

fn tertiary(c: char) -> u8 {
    match u32::from(c) {
        cp if cp < 128 => TERTIARY[cp as usize],
        _ => 0,
    }
}

/// Compare two strings the way `localeCompare(_, 'en')` does.
pub fn compare_locale(a: &str, b: &str) -> Ordering {
    let mut left = a.chars();
    let mut right = b.chars();
    loop {
        match (left.next(), right.next()) {
            (None, None) => break,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match primary(x).cmp(&primary(y)) {
                Ordering::Equal => {}
                other => return other,
            },
        }
    }

    // Primary level tied across the whole string — fall through to case.
    for (x, y) in a.chars().zip(b.chars()) {
        match tertiary(x).cmp(&tertiary(y)) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: &str, b: &str) -> i32 {
        match compare_locale(a, b) {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }
    }

    #[test]
    fn diverges_from_byte_order_where_icu_does() {
        // Each of these inverts relative to Rust's byte-wise `Ord`.
        assert_eq!(cmp("a-b", "a_b"), 1);
        assert_eq!(cmp("a_b", "a1b"), -1);
        assert_eq!(cmp("React", "rabbit"), 1);
        assert_eq!(cmp("A", "a"), 1);
    }

    #[test]
    fn agrees_with_byte_order_on_ordinary_names() {
        assert_eq!(cmp("react", "react-dom"), -1);
        assert_eq!(cmp("@scope/x", "zzz"), -1);
        assert_eq!(cmp("eslint", "eslint-config-prettier"), -1);
        assert_eq!(cmp("react", "react"), 0);
    }

    #[test]
    fn primary_level_beats_case() {
        // The case where a single flat ordering table would be wrong.
        assert_eq!(cmp("Ab", "ac"), -1);
        assert_eq!(cmp("aB", "Ab"), -1);
    }

    #[test]
    fn matches_icu_over_generated_corpus() {
        let corpus = include_str!("../../tests/fixtures/collation.txt");
        let mut checked = 0usize;
        for (line_no, line) in corpus.lines().enumerate() {
            let mut parts = line.split('\t');
            let (a, b, expected) = (
                parts.next().unwrap(),
                parts.next().unwrap(),
                parts.next().unwrap().parse::<i32>().unwrap(),
            );
            assert_eq!(
                cmp(a, b),
                expected,
                "line {}: {a:?} vs {b:?}",
                line_no + 1
            );
            checked += 1;
        }
        assert!(checked > 4000, "corpus looks truncated: {checked} pairs");
    }
}
