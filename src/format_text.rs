//! Text-level formatting for `package.json`.
//!
//! The document is parsed with `jsonc-parser`'s CST — the same parser
//! `dprint-plugin-json` uses — so comments, trailing commas and the other
//! JSONC leniencies it accepts on `.json` files are accepted here too. The
//! CST keeps every byte of trivia, which lets the sorted key order be applied
//! by moving whole properties (comments included) rather than re-serialising
//! the document.
//!
//! Whitespace between properties is re-emitted from the resolved indent and
//! newline settings, so the output is always formatted — this is the floor
//! for when no host formatter is available (under dprint with no JSON plugin
//! configured, and in the standalone CLI), and it is why `package.json` is
//! never left as the one unformatted file in a project.
//!
//! Each container keeps the shape it had: one that was written on a single
//! line stays on a single line, one that spanned lines stays expanded. That
//! matches `dprint-plugin-json` and keeps a deliberate `"dependencies": {}`
//! one-liner from being blown open.
//!
//! Trailing commas are dropped, matching what `dprint-plugin-json` does for a
//! `.json` file.
//!
//! Note the host is always given the result and its answer always wins. A
//! host returning `None` means "already formatted", *not* "no host" — the two
//! are indistinguishable through the plugin API — so falling back on `None`
//! to a second, different style makes formatting unstable. Routing everything
//! through one style avoids that.

use std::{collections::VecDeque, path::Path};

use anyhow::{Result, anyhow, bail};
use dprint_core::configuration::NewLineKind;
use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{CstNode, CstRootNode};
use serde_json::Value;

use crate::{configuration::Configuration, sort::sort_package_json};

/// Resolved output style.
#[derive(Clone, Copy)]
struct Style<'a> {
    indent: &'a str,
    newline: &'a str,
}

/// Sort `package.json` and re-emit it, preserving the original layout.
///
/// Used when a host formatter will normalise the result afterwards.
pub fn format_text(
    file_path: &Path,
    file_text: &str,
    config: &Configuration,
) -> Result<Option<String>> {
    let indent = resolve_indent(config);
    let newline = resolve_newline(config.new_line_kind, file_text);
    format_with_style(
        file_path,
        file_text,
        config,
        Style {
            indent: &indent,
            newline,
        },
    )
}

/// Sort, then hand the result to the dprint host formatter.
///
/// The host is asked to format even when the sort changed nothing, so
/// `package.json` gets the same treatment as every other JSON file. If no
/// host formatter claims it, fall back to normalising here rather than
/// returning text nobody formatted.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn format_text_with_host<F>(
    file_path: &Path,
    file_text: &str,
    config: &Configuration,
    mut host_formatter: F,
) -> Result<Option<String>>
where
    F: FnMut(&str) -> Result<Option<String>>,
{
    let sorted_text = format_text(file_path, file_text, config)?;
    let intermediate = sorted_text.as_deref().unwrap_or(file_text);

    // `None` from the host means it had nothing to change, which covers both
    // "already formatted" and "no plugin claimed it". Either way the
    // intermediate is the answer: it is already formatted by us.
    let output = host_formatter(intermediate)?.unwrap_or_else(|| intermediate.to_string());

    if output == file_text {
        Ok(None)
    } else {
        Ok(Some(output))
    }
}

fn format_with_style(
    _file_path: &Path,
    file_text: &str,
    config: &Configuration,
    style: Style,
) -> Result<Option<String>> {
    let root = CstRootNode::parse(file_text, &ParseOptions::default())
        .map_err(|err| anyhow!("parsing package.json: {err}"))?;

    let Some(root_value) = root.value() else {
        // Nothing but trivia — leave it alone.
        return Ok(None);
    };
    let Some(Value::Object(object)) = root.to_serde_value() else {
        // Top level is not an object; there is nothing to sort.
        return Ok(None);
    };

    let sorted = Value::Object(sort_package_json(object, config));

    let mut output = String::with_capacity(file_text.len());
    let mut rendered_root = false;
    for child in root.children() {
        // The root has exactly one non-trivia child: the document value.
        if !rendered_root && !child.is_trivia() && !child.is_token() {
            rendered_root = true;
            render_node(&root_value, &sorted, style, 0, &mut output)?;
        } else {
            output.push_str(&child.to_string());
        }
    }

    if output == file_text {
        Ok(None)
    } else {
        Ok(Some(output))
    }
}

fn render_node(
    node: &CstNode,
    target: &Value,
    style: Style,
    depth: usize,
    out: &mut String,
) -> Result<()> {
    match (node.as_object(), node.as_array(), target) {
        (Some(object), _, Value::Object(target_object)) => {
            render_object(&object.into(), target_object, style, depth, out)
        }
        (_, Some(array), Value::Array(target_array)) => {
            render_array(&array.into(), target_array, style, depth, out)
        }
        // Scalars, and any shape the sort did not change, come through as
        // their original text.
        _ => {
            out.push_str(&node.to_string());
            Ok(())
        }
    }
}

/// One property (or array element) plus the trivia that belongs to it.
struct Unit {
    key: String,
    /// Trivia preceding the item, including comments on their own lines.
    leading: Vec<CstNode>,
    item: CstNode,
    /// Comments sitting after the item on the same line.
    trailing: Vec<CstNode>,
}

/// Split a container's children into `{`/`[`, the units between, the trivia
/// before the closing token, and the closing token itself.
struct Split {
    open: String,
    units: Vec<Unit>,
    close_leading: Vec<CstNode>,
    close: String,
    /// Whether the container spanned more than one line in the source.
    multiline: bool,
}

fn split_container<F>(children: Vec<CstNode>, key_of: F) -> Result<Split>
where
    F: Fn(&CstNode) -> Option<String>,
{
    let mut iter = children.into_iter().peekable();
    let open = match iter.next() {
        Some(token) if token.is_token() => token.to_string(),
        _ => bail!("expected an opening brace or bracket"),
    };

    let mut inner: Vec<CstNode> = iter.collect();
    let close = match inner.pop() {
        Some(token) if token.is_token() => token.to_string(),
        _ => bail!("expected a closing brace or bracket"),
    };

    let mut units: Vec<Unit> = Vec::new();
    let mut pending: Vec<CstNode> = Vec::new();
    let mut index = 0usize;

    while index < inner.len() {
        let node = &inner[index];
        let Some(key) = key_of(node) else {
            pending.push(node.clone());
            index += 1;
            continue;
        };

        let item = node.clone();
        // Trivia gathered so far belongs to this item and travels with it.
        let leading = std::mem::take(&mut pending);
        index += 1;

        // Consume the separating comma and any comment that stays on this
        // line; a newline ends the item's trailing trivia.
        let mut trailing: Vec<CstNode> = Vec::new();
        let mut held: Vec<CstNode> = Vec::new();
        while index < inner.len() {
            let next = &inner[index];
            if next.is_newline() {
                break;
            } else if next.is_comma() {
                held.clear();
                index += 1;
            } else if next.is_comment() {
                trailing.append(&mut held);
                trailing.push(next.clone());
                index += 1;
            } else if next.is_whitespace() {
                held.push(next.clone());
                index += 1;
            } else {
                break;
            }
        }
        // Whitespace not followed by a comment belongs to the next item.
        pending = held;

        units.push(Unit {
            key,
            leading,
            item,
            trailing,
        });
    }

    let multiline = units
        .iter()
        .any(|unit| unit.leading.iter().any(CstNode::is_newline))
        || pending.iter().any(CstNode::is_newline);

    Ok(Split {
        open,
        units,
        close_leading: pending,
        close,
        multiline,
    })
}

fn render_object(
    object: &CstNode,
    target: &serde_json::Map<String, Value>,
    style: Style,
    depth: usize,
    out: &mut String,
) -> Result<()> {
    let split = split_container(object.children(), |node| {
        node.as_object_prop()
            .and_then(|prop| prop.name())
            .and_then(|name| name.decoded_value().ok())
    })?;

    let mut lookup: std::collections::HashMap<String, VecDeque<&Unit>> =
        std::collections::HashMap::new();
    for unit in &split.units {
        lookup.entry(unit.key.clone()).or_default().push_back(unit);
    }

    let mut ordered: Vec<(&Unit, &Value)> = Vec::with_capacity(target.len());
    for (key, value) in target {
        let Some(unit) = lookup.get_mut(key).and_then(VecDeque::pop_front) else {
            bail!("sorted object contained unknown key: {key}");
        };
        ordered.push((unit, value));
    }

    emit_units(&split, &ordered, style, depth, out, |unit, value, out| {
        render_property(&unit.item, value, style, depth, out)
    })
}

fn render_array(
    array: &CstNode,
    target: &[Value],
    style: Style,
    depth: usize,
    out: &mut String,
) -> Result<()> {
    // Array elements have no names, so they are matched by their value. The
    // key is canonicalised (object keys sorted) because a sort pass may have
    // reordered keys *inside* an element, which must not break its identity.
    let split = split_container(array.children(), |node| {
        if node.is_trivia() || node.is_token() {
            None
        } else {
            node.to_serde_value().as_ref().map(canonical_key)
        }
    })?;

    let mut lookup: std::collections::HashMap<String, VecDeque<&Unit>> =
        std::collections::HashMap::new();
    for unit in &split.units {
        lookup.entry(unit.key.clone()).or_default().push_back(unit);
    }

    let mut ordered: Vec<(&Unit, &Value)> = Vec::with_capacity(target.len());
    for value in target {
        let key = canonical_key(value);
        let Some(unit) = lookup.get_mut(&key).and_then(VecDeque::pop_front) else {
            bail!("sorted array contained an element not present in the source");
        };
        ordered.push((unit, value));
    }

    emit_units(&split, &ordered, style, depth, out, |unit, value, out| {
        render_node(&unit.item, value, style, depth + 1, out)
    })
}

fn emit_units<F>(
    split: &Split,
    ordered: &[(&Unit, &Value)],
    style: Style,
    depth: usize,
    out: &mut String,
    mut render_item: F,
) -> Result<()>
where
    F: FnMut(&Unit, &Value, &mut String) -> Result<()>,
{
    out.push_str(&split.open);

    if ordered.is_empty() {
        // Keep an empty container's inner trivia as-is; there is nothing to
        // lay out.
        for node in &split.close_leading {
            out.push_str(&node.to_string());
        }
        out.push_str(&split.close);
        return Ok(());
    }

    // A container written on one line stays on one line, unless it carries a
    // comment that would otherwise swallow the rest of the line.
    let has_comments = ordered.iter().any(|(unit, _)| {
        unit.leading.iter().any(CstNode::is_comment)
            || unit.trailing.iter().any(CstNode::is_comment)
    }) || split.close_leading.iter().any(CstNode::is_comment);
    let inline = !split.multiline && !has_comments;

    // Arrays read better tight (`[1, 2]`); objects get inner padding
    // (`{ "a": 1 }`), matching dprint-plugin-json.
    let pad = split.open == "{";

    let last = ordered.len() - 1;
    for (position, (unit, value)) in ordered.iter().enumerate() {
        if inline {
            if position == 0 {
                if pad {
                    out.push(' ');
                }
            } else {
                out.push(' ');
            }
        } else {
            emit_leading(&unit.leading, style, depth + 1, out);
        }
        render_item(unit, value, out)?;
        if position != last {
            out.push(',');
        }
        for node in &unit.trailing {
            out.push_str(&node.to_string());
        }
    }

    if inline {
        if pad {
            out.push(' ');
        }
    } else {
        emit_close_leading(&split.close_leading, style, depth, out);
    }
    out.push_str(&split.close);
    Ok(())
}

fn emit_leading(leading: &[CstNode], style: Style, depth: usize, out: &mut String) {
    // Drop the original whitespace but keep every comment, each on its own
    // line at the item's indentation.
    for node in leading {
        if node.is_comment() {
            push_indent(out, style, depth);
            out.push_str(node.to_string().trim());
        }
    }
    push_indent(out, style, depth);
}

fn emit_close_leading(leading: &[CstNode], style: Style, depth: usize, out: &mut String) {
    for node in leading {
        if node.is_comment() {
            push_indent(out, style, depth + 1);
            out.push_str(node.to_string().trim());
        }
    }
    push_indent(out, style, depth);
}

fn push_indent(out: &mut String, style: Style, depth: usize) {
    out.push_str(style.newline);
    for _ in 0..depth {
        out.push_str(style.indent);
    }
}

/// Re-emit an object property, recursing into its value.
fn render_property(
    property: &CstNode,
    target: &Value,
    style: Style,
    depth: usize,
    out: &mut String,
) -> Result<()> {
    let Some(prop) = property.as_object_prop() else {
        out.push_str(&property.to_string());
        return Ok(());
    };
    let (Some(name), Some(value_node)) = (prop.name(), prop.value()) else {
        out.push_str(&property.to_string());
        return Ok(());
    };

    // Normalise the separator too, so `"a":1` and `"a"  :  1` both come out
    // as `"a": 1`. A loose (unquoted) name is re-emitted as written; JSON
    // proper always has it quoted.
    match (name.as_string_lit(), name.as_word_lit()) {
        (Some(string_lit), _) => out.push_str(&string_lit.to_string()),
        (_, Some(word_lit)) => out.push_str(&word_lit.to_string()),
        _ => bail!("object property has no name"),
    }
    out.push_str(": ");
    // The property itself sits at `depth + 1`, so its value's own children
    // belong one level deeper again.
    render_node(&value_node, target, style, depth + 1, out)
}

/// Detect the indentation a document already uses.
///
/// Mirrors what upstream `sort-package-json` does with `detect-indent` when
/// it is handed a string: the file's own style wins unless the caller asks
/// for something specific. Returns `None` for a document with no nested
/// content to measure.
pub fn detect_indent(file_text: &str) -> Option<(bool, u8)> {
    let root = CstRootNode::parse(file_text, &ParseOptions::default()).ok()?;
    let indent = root.single_indent_text()?;
    if indent.starts_with('\t') {
        Some((true, 1))
    } else {
        u8::try_from(indent.len()).ok().map(|width| (false, width))
    }
}

/// A stable identity for an array element: the value serialised with every
/// object's keys in sorted order, so reordering keys inside an element does
/// not change it.
fn canonical_key(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap_or_default());
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}

fn resolve_indent(config: &Configuration) -> String {
    if config.use_tabs {
        "\t".to_string()
    } else {
        " ".repeat(config.indent_width as usize)
    }
}

fn resolve_newline(kind: NewLineKind, file_text: &str) -> &'static str {
    match kind {
        NewLineKind::LineFeed => "\n",
        NewLineKind::CarriageReturnLineFeed => "\r\n",
        NewLineKind::Auto => detect_newline(file_text),
    }
}

/// Match the document's own line endings, defaulting to LF.
///
/// dprint defines `NewLineKind::Auto` in terms of the *last* newline in the
/// file, so use that rather than the first.
fn detect_newline(file_text: &str) -> &'static str {
    match file_text.rfind('\n') {
        Some(index) if index > 0 && file_text.as_bytes()[index - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

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

    #[test]
    fn host_formatter_runs_even_when_sort_is_noop() {
        let input = "{\n\t\"devDependencies\": { \"sort-package-json\": \"3.6.1\" }}\n";
        let expected = "{\n\t\"devDependencies\": { \"sort-package-json\": \"3.6.1\" }\n}\n";
        let config = Configuration {
            use_tabs: true,
            ..Configuration::default()
        };

        let mut host_calls = 0;
        let out = format_text_with_host(Path::new("package.json"), input, &config, |text| {
            host_calls += 1;
            // The host is handed our formatted text, not the raw input.
            assert_eq!(text, expected);
            Ok(Some(expected.to_string()))
        })
        .unwrap();

        assert_eq!(host_calls, 1, "the host must be consulted");
        assert_eq!(out.as_deref(), Some(expected));
    }

    #[test]
    fn host_formatter_sees_sorted_text_when_sort_changes() {
        let input = "{\n\t\"version\": \"1.0.0\",\n\t\"name\": \"demo\"\n}\n";
        let sorted = "{\n\t\"name\": \"demo\",\n\t\"version\": \"1.0.0\"\n}\n";
        let config = Configuration {
            use_tabs: true,
            ..Configuration::default()
        };

        let out = format_text_with_host(Path::new("package.json"), input, &config, |text| {
            assert_eq!(text, sorted);
            Ok(None)
        })
        .unwrap();

        assert_eq!(out.as_deref(), Some(sorted));
    }

    #[test]
    fn preserves_multiline_nested_object_layout_on_noop() {
        let input = concat!(
            "{\n",
            "\t\"name\": \"demo\",\n",
            "\t\"exports\": {\n",
            "\t\t\".\": {\n",
            "\t\t\t\"import\": \"./dist/index.mjs\",\n",
            "\t\t\t\"require\": \"./dist/index.cjs\",\n",
            "\t\t\t\"default\": \"./dist/index.mjs\"\n",
            "\t\t}\n",
            "\t}\n",
            "}\n"
        );
        let config = Configuration {
            use_tabs: true,
            ..Configuration::default()
        };

        let result = format_text(Path::new("package.json"), input, &config).unwrap();
        assert!(
            result.is_none(),
            "already-sorted multiline object should stay untouched"
        );
    }

    #[test]
    fn preserves_nested_layout_when_parent_keys_move() {
        let input = concat!(
            "{\n",
            "\t\"version\": \"1.0.0\",\n",
            "\t\"name\": \"demo\",\n",
            "\t\"exports\": {\n",
            "\t\t\".\": {\n",
            "\t\t\t\"import\": \"./dist/index.mjs\",\n",
            "\t\t\t\"require\": \"./dist/index.cjs\",\n",
            "\t\t\t\"default\": \"./dist/index.mjs\"\n",
            "\t\t}\n",
            "\t}\n",
            "}\n"
        );
        let expected = concat!(
            "{\n",
            "\t\"name\": \"demo\",\n",
            "\t\"version\": \"1.0.0\",\n",
            "\t\"exports\": {\n",
            "\t\t\".\": {\n",
            "\t\t\t\"import\": \"./dist/index.mjs\",\n",
            "\t\t\t\"require\": \"./dist/index.cjs\",\n",
            "\t\t\t\"default\": \"./dist/index.mjs\"\n",
            "\t\t}\n",
            "\t}\n",
            "}\n"
        );
        assert_eq!(fmt(input), expected);
    }

    #[test]
    fn preserves_multiline_array_layout_when_sorting_values() {
        let input = concat!(
            "{\n",
            "\t\"name\": \"demo\",\n",
            "\t\"bundledDependencies\": [\n",
            "\t\t\"zod\",\n",
            "\t\t\"axios\"\n",
            "\t]\n",
            "}\n"
        );
        let expected = concat!(
            "{\n",
            "\t\"name\": \"demo\",\n",
            "\t\"bundledDependencies\": [\n",
            "\t\t\"axios\",\n",
            "\t\t\"zod\"\n",
            "\t]\n",
            "}\n"
        );
        assert_eq!(fmt(input), expected);
    }

    /// Format with an explicit config (`fmt` uses tabs).
    fn fmt_with(input: &str, config: &Configuration) -> String {
        format_text(Path::new("package.json"), input, config)
            .unwrap()
            .unwrap_or_else(|| input.to_string())
    }

    /// Two-space indentation, the default style.
    fn spaces() -> Configuration {
        Configuration::default()
    }

    #[test]
    fn accepts_comments_instead_of_erroring() {
        let input = "{\n  // leading\n  \"version\": \"1.0.0\",\n  \"name\": \"x\"\n}\n";
        let out = fmt(input);
        assert!(out.contains("// leading"), "comment must survive: {out:?}");
    }

    #[test]
    fn comment_travels_with_its_property() {
        let input = concat!(
            "{\n",
            "  // about version\n",
            "  \"version\": \"1.0.0\",\n",
            "  \"name\": \"x\"\n",
            "}\n",
        );
        let expected = concat!(
            "{\n",
            "  \"name\": \"x\",\n",
            "  // about version\n",
            "  \"version\": \"1.0.0\"\n",
            "}\n",
        );
        assert_eq!(fmt_with(input, &spaces()), expected);
    }

    #[test]
    fn trailing_comment_stays_on_its_property() {
        let input = concat!(
            "{\n",
            "  \"version\": \"1.0.0\", // pinned\n",
            "  \"name\": \"x\"\n",
            "}\n",
        );
        let out = fmt(input);
        let name_line = out.lines().position(|l| l.contains("\"name\""));
        let pinned_line = out.lines().position(|l| l.contains("// pinned"));
        assert!(
            name_line < pinned_line,
            "comment should follow version: {out:?}"
        );
        assert!(
            out.contains("\"version\": \"1.0.0\" // pinned")
                || out.contains("\"version\": \"1.0.0\", // pinned"),
            "trailing comment must stay on the same line: {out:?}"
        );
    }

    #[test]
    fn trailing_comma_is_dropped() {
        let input = "{\n  \"version\": \"1.0.0\",\n  \"name\": \"x\",\n}\n";
        let out = fmt(input);
        assert!(!out.contains(",\n}"), "trailing comma must go: {out:?}");
        assert!(out.contains("\"name\""), "content preserved: {out:?}");
    }

    #[test]
    fn normalize_applies_indent_width() {
        let input = "{\n      \"name\": \"x\",\n            \"version\": \"1.0.0\"\n}\n";
        let config = Configuration {
            indent_width: 2,
            ..Configuration::default()
        };
        let expected = "{\n  \"name\": \"x\",\n  \"version\": \"1.0.0\"\n}\n";
        assert_eq!(fmt_with(input, &config), expected);
    }

    #[test]
    fn normalize_applies_tabs() {
        let input = "{\n    \"name\": \"x\",\n    \"version\": \"1.0.0\"\n}\n";
        let config = Configuration {
            use_tabs: true,
            ..Configuration::default()
        };
        let expected = "{\n\t\"name\": \"x\",\n\t\"version\": \"1.0.0\"\n}\n";
        assert_eq!(fmt_with(input, &config), expected);
    }

    #[test]
    fn a_one_line_object_stays_on_one_line() {
        // dprint-plugin-json keeps a container's shape; so do we, so a
        // deliberate one-liner is not blown open.
        let input = "{\n \"name\": \"x\",\n \"dependencies\": {\"b\":\"1\",\"a\":\"1\"}\n}\n";
        let expected = concat!(
            "{\n",
            "  \"name\": \"x\",\n",
            "  \"dependencies\": { \"a\": \"1\", \"b\": \"1\" }\n",
            "}\n",
        );
        assert_eq!(fmt_with(input, &spaces()), expected);
    }

    #[test]
    fn a_multiline_object_stays_expanded_and_is_reindented() {
        let input = concat!(
            "{\n",
            "        \"name\": \"x\",\n",
            "        \"dependencies\": {\n",
            "                    \"b\": \"1\",\n",
            "          \"a\": \"1\"\n",
            "        }\n",
            "}\n",
        );
        let expected = concat!(
            "{\n",
            "  \"name\": \"x\",\n",
            "  \"dependencies\": {\n",
            "    \"a\": \"1\",\n",
            "    \"b\": \"1\"\n",
            "  }\n",
            "}\n",
        );
        assert_eq!(fmt_with(input, &spaces()), expected);
    }

    #[test]
    fn a_one_line_array_stays_tight() {
        let input = "{\n  \"name\": \"x\",\n  \"bundledDependencies\": [\"b\",\"a\"]\n}\n";
        let expected = "{\n  \"name\": \"x\",\n  \"bundledDependencies\": [\"a\", \"b\"]\n}\n";
        assert_eq!(fmt_with(input, &spaces()), expected);
    }

    #[test]
    fn normalize_keeps_comments() {
        let input = "{\n        // note\n        \"name\": \"x\"\n}\n";
        let out = fmt_with(input, &spaces());
        assert_eq!(out, "{\n  // note\n  \"name\": \"x\"\n}\n");
    }

    #[test]
    fn normalize_is_idempotent() {
        let input = "{\n   \"version\": \"1.0.0\",\n  \"name\": \"x\"\n}\n";
        let config = Configuration::default();
        let once = fmt_with(input, &config);
        let twice = fmt_with(&once, &config);
        assert_eq!(once, twice, "second pass must not change output");
    }

    #[test]
    fn host_declining_falls_back_to_normalizing() {
        // This is the DX regression: with no JSON plugin configured,
        // package.json used to come out sorted but unformatted.
        let input = "{\n      \"version\": \"1.0.0\",\n            \"name\": \"x\"\n}\n";
        let out = format_text_with_host(
            Path::new("package.json"),
            input,
            &Configuration::default(),
            |_| Ok(None),
        )
        .unwrap();
        assert_eq!(
            out.as_deref(),
            Some("{\n  \"name\": \"x\",\n  \"version\": \"1.0.0\"\n}\n")
        );
    }

    #[test]
    fn crlf_is_preserved_when_normalizing() {
        let input = "{\r\n   \"version\": \"1.0.0\",\r\n  \"name\": \"x\"\r\n}\r\n";
        let config = Configuration {
            new_line_kind: NewLineKind::Auto,
            ..Configuration::default()
        };
        let out = fmt_with(input, &config);
        assert!(out.contains("\r\n"), "CRLF must be kept: {out:?}");
        assert!(!out.contains("\n\n"), "no stray bare LF: {out:?}");
    }

    #[test]
    fn array_elements_keep_identity_when_their_own_keys_move() {
        // A `wireit` dependency object is reordered *inside* the array
        // element, which must not stop the element being matched to its
        // source node.
        let input = concat!(
            "{\n",
            "  \"wireit\": {\n",
            "    \"test\": {\n",
            "      \"dependencies\": [\n",
            "        { \"cascade\": false, \"script\": \"build\" },\n",
            "        \"lint\"\n",
            "      ]\n",
            "    }\n",
            "  }\n",
            "}\n",
        );
        let out = fmt_with(input, &spaces());
        assert!(
            out.contains("{ \"script\": \"build\", \"cascade\": false }"),
            "element keys should reorder in place: {out:?}"
        );
        assert!(out.contains("\"lint\""), "sibling element kept: {out:?}");
    }
}
