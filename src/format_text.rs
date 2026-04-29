use std::path::Path;

use anyhow::{Context, Result};
use dprint_core::configuration::NewLineKind;
use serde_json::{Serializer, Value, ser::PrettyFormatter};

use crate::{configuration::Configuration, sort::sort_package_json};

/// Format a `package.json`: parse, sort top-level keys, re-emit using the
/// resolved indent / newline settings. Returns `Ok(None)` when the input is
/// already in canonical form.
pub fn format_text(
    _file_path: &Path,
    file_text: &str,
    config: &Configuration,
) -> Result<Option<String>> {
    let value: Value = serde_json::from_str(file_text).context("parsing package.json")?;

    let Value::Object(object) = value else {
        // Top-level is not an object — nothing to sort.
        return Ok(None);
    };

    let sorted = sort_package_json(object, config);
    let mut output = serialize_pretty(&Value::Object(sorted), config)?;

    // Match newline style: serde_json always emits LF; rewrite if needed.
    let target_newline = resolve_newline(config.new_line_kind, file_text);
    if target_newline != "\n" {
        output = output.replace('\n', target_newline);
    }

    // Preserve a trailing newline if the input had one. sort-package-json
    // behaves the same way; round-tripping a file with no terminator does
    // not introduce one.
    if file_text.ends_with('\n') && !output.ends_with('\n') {
        output.push_str(target_newline);
    }

    if output == file_text {
        Ok(None)
    } else {
        Ok(Some(output))
    }
}

fn serialize_pretty(value: &Value, config: &Configuration) -> Result<String> {
    let indent: Vec<u8> = if config.use_tabs {
        vec![b'\t']
    } else {
        vec![b' '; config.indent_width as usize]
    };

    let mut buf = Vec::with_capacity(256);
    let formatter = PrettyFormatter::with_indent(&indent);
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    serde::Serialize::serialize(value, &mut ser).context("serializing JSON")?;
    String::from_utf8(buf).context("UTF-8 from serializer")
}

fn resolve_newline(kind: NewLineKind, file_text: &str) -> &'static str {
    match kind {
        NewLineKind::LineFeed => "\n",
        NewLineKind::CarriageReturnLineFeed => "\r\n",
        // dprint-core's `Auto` is "decide based on the file's last newline";
        // we approximate by detecting any CRLF in the input and falling back
        // to LF when there is no newline at all.
        NewLineKind::Auto => {
            if file_text.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn fmt(input: &str) -> String {
        let config = Configuration {
            use_tabs: true,
            ..Configuration::default()
        };
        format_text(&PathBuf::from("package.json"), input, &config)
            .unwrap()
            .unwrap_or_else(|| input.to_string())
    }

    #[test]
    fn reorders_to_canonical() {
        let input = "{\n\t\"version\": \"1.0.0\",\n\t\"name\": \"demo\"\n}\n";
        let expected = "{\n\t\"name\": \"demo\",\n\t\"version\": \"1.0.0\"\n}\n";
        assert_eq!(fmt(input), expected);
    }

    #[test]
    fn already_sorted_returns_none() {
        let input = "{\n\t\"name\": \"demo\",\n\t\"version\": \"1.0.0\"\n}\n";
        let config = Configuration {
            use_tabs: true,
            ..Configuration::default()
        };
        let result = format_text(&PathBuf::from("package.json"), input, &config).unwrap();
        assert!(
            result.is_none(),
            "no-op expected for already-canonical input"
        );
    }

    #[test]
    fn idempotent() {
        let input =
            "{\n\t\"version\": \"1.0.0\",\n\t\"description\": \"x\",\n\t\"name\": \"demo\"\n}\n";
        let once = fmt(input);
        let twice = fmt(&once);
        assert_eq!(once, twice, "second pass must not change output");
    }

    #[test]
    fn missing_trailing_newline_is_not_added() {
        let input = "{\n\t\"version\": \"1\",\n\t\"name\": \"demo\"\n}";
        let out = fmt(input);
        assert!(!out.ends_with('\n'), "no trailing newline should be added");
    }
}
