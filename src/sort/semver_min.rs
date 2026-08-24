//! Minimum-satisfying-version computation for npm version ranges.
//!
//! Upstream `sort-package-json` breaks ties between `pnpm.overrides` keys
//! that name the same package by comparing
//! `semverMinVersion(range)` of each (index.js:86), so `foo@2` sorts before
//! `foo@10`. A plain string compare — what this crate did previously —
//! inverts that.
//!
//! This is a focused reimplementation rather than a dependency: the crate
//! deliberately avoids linking a semver library to keep the published wasm
//! small, and this is reached only when two override keys share a package
//! name.
//!
//! Semantics are taken from `semver/ranges/min-version.js` and verified
//! against it (see the unit tests). Notably the lower bound of `>` depends on
//! how precise the operand is: `>2` is `3.0.0`, `>2.3` is `2.4.0`, `>2.3.4`
//! is `2.3.5`, and `>1.2.3-beta.1` is `1.2.3-beta.1.0`.
//!
//! **Deliberate divergence:** node-semver *throws* on ranges it cannot parse
//! (`workspace:*`, `npm:foo@1.2.3`, `latest`). A formatter must not fail on
//! valid-but-unusual input, so [`min_version`] returns `None` instead and the
//! caller falls back to comparing the range strings.

use std::cmp::Ordering;

/// A resolved semantic version. `pre` is empty for a release version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<PreField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PreField {
    Num(u64),
    Str(String),
}

impl Ord for PreField {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (PreField::Num(a), PreField::Num(b)) => a.cmp(b),
            (PreField::Str(a), PreField::Str(b)) => a.cmp(b),
            // Numeric identifiers always have lower precedence than
            // alphanumeric ones.
            (PreField::Num(_), PreField::Str(_)) => Ordering::Less,
            (PreField::Str(_), PreField::Num(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for PreField {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                // A version with a prerelease has lower precedence than the
                // associated release.
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A version with possibly-absent minor/patch, as written in a range.
struct Partial {
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
    pre: Vec<PreField>,
}

impl Partial {
    /// Fill absent components with zero — the lower bound of the range this
    /// partial denotes.
    fn to_lower_bound(&self) -> Version {
        Version {
            major: self.major,
            minor: self.minor.unwrap_or(0),
            patch: self.patch.unwrap_or(0),
            pre: self.pre.clone(),
        }
    }
}

/// The lowest version that could satisfy `range`, or `None` if the range
/// cannot be parsed.
pub fn min_version(range: &str) -> Option<Version> {
    let mut minimum: Option<Version> = None;
    for group in range.split("||") {
        let candidate = group_lower_bound(group)?;
        if minimum.as_ref().is_none_or(|current| candidate < *current) {
            minimum = Some(candidate);
        }
    }
    minimum
}

/// Lowest version satisfying a single comparator set (one `||` alternative).
fn group_lower_bound(group: &str) -> Option<Version> {
    let group = group.trim();

    // Hyphen range: `A - B` has the same lower bound as `>=A`.
    if let Some(lower) = hyphen_range_lower(group) {
        return Some(lower);
    }

    let mut bound: Option<Version> = None;
    for token in group.split_whitespace() {
        // A group is a conjunction, so its lower bound is the greatest of
        // its comparators' lower bounds.
        if let Some(candidate) = comparator_lower_bound(token)?
            && bound.as_ref().is_none_or(|current| candidate > *current)
        {
            bound = Some(candidate);
        }
    }

    // No lower bound at all (`*`, `<3`, an empty range) admits 0.0.0.
    Some(bound.unwrap_or(Version {
        major: 0,
        minor: 0,
        patch: 0,
        pre: Vec::new(),
    }))
}

fn hyphen_range_lower(group: &str) -> Option<Version> {
    let tokens: Vec<&str> = group.split_whitespace().collect();
    if tokens.len() == 3 && tokens[1] == "-" {
        return parse_partial(tokens[0]).map(|p| p.to_lower_bound());
    }
    None
}

/// The lower bound a single comparator imposes.
///
/// `Ok(None)` means "imposes no lower bound" (`<`, `<=`, or a pure
/// wildcard); the outer `None` means the comparator could not be parsed.
fn comparator_lower_bound(token: &str) -> Option<Option<Version>> {
    let (operator, rest) = split_operator(token);

    // A bare wildcard constrains nothing.
    let rest = rest.trim();
    if rest.is_empty() || matches!(rest, "*" | "x" | "X") {
        return Some(None);
    }

    let partial = parse_partial(rest)?;

    let bound = match operator {
        "<" | "<=" => return Some(None),
        ">" => Some(exclusive_lower_bound(&partial)),
        // `>=`, `=`, `^`, `~`, `~>` and a bare version all start at the
        // partial's zero-filled form.
        _ => Some(partial.to_lower_bound()),
    };
    Some(bound)
}

/// Lower bound of `>partial`, mirroring node-semver's x-range expansion.
fn exclusive_lower_bound(partial: &Partial) -> Version {
    match (partial.minor, partial.patch) {
        // `>2` expands to `>=3.0.0`.
        (None, _) => Version {
            major: partial.major + 1,
            minor: 0,
            patch: 0,
            pre: Vec::new(),
        },
        // `>2.3` expands to `>=2.4.0`.
        (Some(minor), None) => Version {
            major: partial.major,
            minor: minor + 1,
            patch: 0,
            pre: Vec::new(),
        },
        // `>2.3.4` is `2.3.5`; `>1.2.3-beta.1` is `1.2.3-beta.1.0`.
        (Some(minor), Some(patch)) => {
            if partial.pre.is_empty() {
                Version {
                    major: partial.major,
                    minor,
                    patch: patch + 1,
                    pre: Vec::new(),
                }
            } else {
                let mut pre = partial.pre.clone();
                pre.push(PreField::Num(0));
                Version {
                    major: partial.major,
                    minor,
                    patch,
                    pre,
                }
            }
        }
    }
}

fn split_operator(token: &str) -> (&str, &str) {
    for operator in [">=", "<=", "~>", ">", "<", "=", "^", "~"] {
        if let Some(rest) = token.strip_prefix(operator) {
            return (operator, rest);
        }
    }
    ("", token)
}

fn parse_partial(text: &str) -> Option<Partial> {
    let text = text.trim();
    let text = text.strip_prefix('v').unwrap_or(text);
    // Build metadata never affects precedence.
    let text = text.split('+').next()?;

    let (core, pre) = match text.split_once('-') {
        Some((core, pre)) => (core, parse_pre(pre)?),
        None => (text, Vec::new()),
    };

    let mut parts = core.split('.');
    let major = parse_component(parts.next()?)??;
    let minor = match parts.next() {
        Some(component) => parse_component(component)?,
        None => None,
    };
    let patch = match parts.next() {
        Some(component) => parse_component(component)?,
        None => None,
    };
    if parts.next().is_some() {
        return None;
    }

    // `1.x.2` is not meaningful — a wildcard makes everything after it
    // wildcard too.
    let patch = if minor.is_none() { None } else { patch };

    Some(Partial {
        major,
        minor,
        patch,
        pre,
    })
}

/// Parse one dot-separated component. `Some(None)` is a wildcard.
fn parse_component(text: &str) -> Option<Option<u64>> {
    if matches!(text, "*" | "x" | "X") {
        return Some(None);
    }
    text.parse::<u64>().ok().map(Some)
}

fn parse_pre(text: &str) -> Option<Vec<PreField>> {
    if text.is_empty() {
        return None;
    }
    Some(
        text.split('.')
            .map(|field| match field.parse::<u64>() {
                // A leading zero makes it an alphanumeric identifier.
                Ok(n) if field.len() == 1 || !field.starts_with('0') => PreField::Num(n),
                _ => PreField::Str(field.to_string()),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(version: &Version) -> String {
        let mut out = format!("{}.{}.{}", version.major, version.minor, version.patch);
        if !version.pre.is_empty() {
            out.push('-');
            let fields: Vec<String> = version
                .pre
                .iter()
                .map(|field| match field {
                    PreField::Num(n) => n.to_string(),
                    PreField::Str(s) => s.clone(),
                })
                .collect();
            out.push_str(&fields.join("."));
        }
        out
    }

    #[test]
    fn matches_node_semver_over_fixture() {
        let corpus = include_str!("../../tests/fixtures/semver_min.txt");
        for (line_no, line) in corpus.lines().enumerate() {
            let (range, expected) = line.split_once('\t').unwrap();
            let actual = match min_version(range) {
                Some(v) => render(&v),
                None => "NONE".to_string(),
            };
            assert_eq!(actual, expected, "line {}: range {range:?}", line_no + 1);
        }
    }

    #[test]
    fn unparseable_ranges_yield_none_instead_of_panicking() {
        // node-semver throws on each of these; we must not.
        for range in [
            "workspace:*",
            "npm:foo@1.2.3",
            "latest",
            "catalog:",
            "file:../x",
        ] {
            assert_eq!(min_version(range), None, "range {range:?}");
        }
    }

    #[test]
    fn orders_numerically_not_lexicographically() {
        let two = min_version("2").unwrap();
        let ten = min_version("10").unwrap();
        assert!(two < ten, "2 should sort before 10");
    }

    #[test]
    fn prerelease_precedes_its_release() {
        assert!(min_version("1.0.0-alpha").unwrap() < min_version("1.0.0").unwrap());
        assert!(min_version("1.0.0-alpha.1").unwrap() < min_version("1.0.0-alpha.beta").unwrap());
        assert!(min_version("1.0.0-alpha").unwrap() < min_version("1.0.0-alpha.1").unwrap());
    }
}
