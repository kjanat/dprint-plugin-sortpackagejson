use std::{
    collections::{HashMap, VecDeque},
    path::Path,
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::{configuration::Configuration, sort::sort_package_json};

/// Format a `package.json`: parse, sort the semantic structure, then rebuild
/// the text by reordering only the parts that changed. Existing layout stays
/// intact whenever the sorted value is semantically identical.
pub fn format_text(
    _file_path: &Path,
    file_text: &str,
    config: &Configuration,
) -> Result<Option<String>> {
    let Some(sorted_value) = sort_value(file_text, config)? else {
        return Ok(None);
    };

    let document = ParsedDocument::parse(file_text)?;
    let mut output = String::with_capacity(file_text.len());
    output.push_str(&file_text[..document.root.span().start]);
    output.push_str(&render_node(&document.root, &sorted_value, file_text)?);
    output.push_str(&file_text[document.root.span().end..]);

    if output == file_text {
        Ok(None)
    } else {
        Ok(Some(output))
    }
}

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
    let host_text = host_formatter(intermediate)?;
    let output = host_text.unwrap_or_else(|| intermediate.to_string());

    if output == file_text {
        Ok(None)
    } else {
        Ok(Some(output))
    }
}

fn sort_value(file_text: &str, config: &Configuration) -> Result<Option<Value>> {
    let value: Value = serde_json::from_str(file_text).context("parsing package.json")?;
    let original_compact = serialize_compact(&value)?;

    let Value::Object(object) = value else {
        // Top-level is not an object — nothing to sort.
        return Ok(None);
    };

    let sorted = Value::Object(sort_package_json(object, config));
    if serialize_compact(&sorted)? == original_compact {
        return Ok(None);
    }

    Ok(Some(sorted))
}

fn render_node(original: &Node, target: &Value, file_text: &str) -> Result<String> {
    if compact_slice(original.span().slice(file_text))? == serialize_compact(target)? {
        return Ok(original.span().slice(file_text).to_string());
    }

    match (original, target) {
        (Node::Object(original_object), Value::Object(target_object)) => {
            render_object(original_object, target_object, file_text)
        }
        (Node::Array(original_array), Value::Array(target_array)) => {
            render_array(original_array, target_array, file_text)
        }
        _ => serialize_compact(target),
    }
}

fn render_object(
    original: &ObjectNode,
    target: &serde_json::Map<String, Value>,
    file_text: &str,
) -> Result<String> {
    let property_lookup: HashMap<&str, &PropertyNode> = original
        .properties
        .iter()
        .map(|property| (property.key.as_str(), property))
        .collect();

    let mut rendered_properties = Vec::with_capacity(target.len());
    for (key, value) in target {
        let Some(property) = property_lookup.get(key.as_str()) else {
            bail!("sorted object contained unknown key: {key}");
        };
        let rendered_value = render_node(&property.value, value, file_text)?;
        if rendered_value == property.value.span().slice(file_text) {
            rendered_properties.push(property.span.slice(file_text).to_string());
        } else {
            let mut rendered_property = String::with_capacity(property.span.len());
            rendered_property
                .push_str(&file_text[property.span.start..property.value.span().start]);
            rendered_property.push_str(&rendered_value);
            rendered_properties.push(rendered_property);
        }
    }

    Ok(rebuild_object(original, &rendered_properties, file_text))
}

fn render_array(original: &ArrayNode, target: &[Value], file_text: &str) -> Result<String> {
    let mut original_elements: HashMap<String, VecDeque<&Node>> = HashMap::new();
    for element in &original.elements {
        original_elements
            .entry(canonical_slice(element.span().slice(file_text))?)
            .or_default()
            .push_back(element);
    }

    let mut rendered_elements = Vec::with_capacity(target.len());
    for value in target {
        let key = canonical_value(value)?;
        let rendered = match original_elements
            .get_mut(&key)
            .and_then(VecDeque::pop_front)
        {
            Some(element) => render_node(element, value, file_text)?,
            None => serialize_compact(value)?,
        };
        rendered_elements.push(rendered);
    }

    Ok(rebuild_array(original, &rendered_elements, file_text))
}

fn rebuild_object(
    original: &ObjectNode,
    rendered_properties: &[String],
    file_text: &str,
) -> String {
    rebuild_sequence(
        '{',
        '}',
        original.span,
        &original
            .properties
            .iter()
            .map(|property| property.span)
            .collect::<Vec<_>>(),
        rendered_properties,
        file_text,
    )
}

fn rebuild_array(original: &ArrayNode, rendered_elements: &[String], file_text: &str) -> String {
    rebuild_sequence(
        '[',
        ']',
        original.span,
        &original.elements.iter().map(Node::span).collect::<Vec<_>>(),
        rendered_elements,
        file_text,
    )
}

fn rebuild_sequence(
    open: char,
    close: char,
    span: Span,
    original_items: &[Span],
    rendered_items: &[String],
    file_text: &str,
) -> String {
    let mut output = String::new();
    output.push(open);

    if let Some(first_item) = original_items.first() {
        output.push_str(&file_text[span.start + 1..first_item.start]);
        for (index, rendered_item) in rendered_items.iter().enumerate() {
            output.push_str(rendered_item);
            if index + 1 < rendered_items.len() {
                output.push_str(
                    &file_text[original_items[index].end..original_items[index + 1].start],
                );
            }
        }
        if let Some(last_item) = original_items.last() {
            output.push_str(&file_text[last_item.end..span.end - 1]);
        }
    } else {
        output.push_str(&file_text[span.start + 1..span.end - 1]);
    }

    output.push(close);
    output
}

fn serialize_compact(value: &Value) -> Result<String> {
    serde_json::to_string(value).context("serializing compact JSON")
}

fn compact_slice(text: &str) -> Result<String> {
    let value: Value = serde_json::from_str(text).context("parsing JSON slice")?;
    serialize_compact(&value)
}

fn canonical_slice(text: &str) -> Result<String> {
    let value: Value = serde_json::from_str(text).context("parsing JSON slice")?;
    canonical_value(&value)
}

fn canonical_value(value: &Value) -> Result<String> {
    let mut output = String::new();
    write_canonical_value(value, &mut output)?;
    Ok(output)
}

fn write_canonical_value(value: &Value, output: &mut String) -> Result<()> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(boolean) => output.push_str(if *boolean { "true" } else { "false" }),
        Value::Number(number) => output.push_str(&number.to_string()),
        Value::String(string) => output.push_str(&serde_json::to_string(string)?),
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical_value(value, output)?;
            }
            output.push(']');
        }
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|left, right| left.0.cmp(right.0));

            output.push('{');
            for (index, (key, value)) in entries.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&serde_json::to_string(key)?);
                output.push(':');
                write_canonical_value(value, output)?;
            }
            output.push('}');
        }
    }

    Ok(())
}

struct ParsedDocument {
    root: Node,
}

impl ParsedDocument {
    fn parse(file_text: &str) -> Result<Self> {
        let mut parser = Parser::new(file_text);
        parser.skip_whitespace();
        let root = parser.parse_value()?;
        parser.skip_whitespace();
        if parser.position != file_text.len() {
            bail!("unexpected trailing content after root JSON value");
        }
        Ok(Self { root })
    }
}

struct Parser<'a> {
    file_text: &'a str,
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(file_text: &'a str) -> Self {
        Self {
            file_text,
            bytes: file_text.as_bytes(),
            position: 0,
        }
    }

    fn parse_value(&mut self) -> Result<Node> {
        self.skip_whitespace();
        match self.peek_byte() {
            Some(b'{') => self.parse_object().map(Node::Object),
            Some(b'[') => self.parse_array().map(Node::Array),
            Some(b'"') => self.parse_string_span().map(Node::Primitive),
            Some(b'-' | b'0'..=b'9' | b't' | b'f' | b'n') => {
                self.parse_primitive().map(Node::Primitive)
            }
            Some(byte) => bail!("unexpected JSON byte: {}", byte as char),
            None => bail!("unexpected end of JSON input"),
        }
    }

    fn parse_object(&mut self) -> Result<ObjectNode> {
        let start = self.position;
        self.position += 1; // '{'
        self.skip_whitespace();

        let mut properties = Vec::new();
        if self.peek_byte() == Some(b'}') {
            self.position += 1;
            return Ok(ObjectNode {
                span: Span {
                    start,
                    end: self.position,
                },
                properties,
            });
        }

        loop {
            self.skip_whitespace();
            let key_span = self.parse_string_span()?;
            let key: String = serde_json::from_str(key_span.slice(self.file_text))
                .context("decoding object key")?;
            self.skip_whitespace();
            self.expect_byte(b':')?;
            self.position += 1;
            let value = self.parse_value()?;
            properties.push(PropertyNode {
                key,
                span: Span {
                    start: key_span.start,
                    end: value.span().end,
                },
                value,
            });
            self.skip_whitespace();
            match self.peek_byte() {
                Some(b',') => self.position += 1,
                Some(b'}') => {
                    self.position += 1;
                    break;
                }
                Some(byte) => bail!("unexpected object separator byte: {}", byte as char),
                None => bail!("unterminated JSON object"),
            }
        }

        Ok(ObjectNode {
            span: Span {
                start,
                end: self.position,
            },
            properties,
        })
    }

    fn parse_array(&mut self) -> Result<ArrayNode> {
        let start = self.position;
        self.position += 1; // '['
        self.skip_whitespace();

        let mut elements = Vec::new();
        if self.peek_byte() == Some(b']') {
            self.position += 1;
            return Ok(ArrayNode {
                span: Span {
                    start,
                    end: self.position,
                },
                elements,
            });
        }

        loop {
            let value = self.parse_value()?;
            elements.push(value);
            self.skip_whitespace();
            match self.peek_byte() {
                Some(b',') => self.position += 1,
                Some(b']') => {
                    self.position += 1;
                    break;
                }
                Some(byte) => bail!("unexpected array separator byte: {}", byte as char),
                None => bail!("unterminated JSON array"),
            }
        }

        Ok(ArrayNode {
            span: Span {
                start,
                end: self.position,
            },
            elements,
        })
    }

    fn parse_string_span(&mut self) -> Result<Span> {
        let start = self.position;
        self.expect_byte(b'"')?;
        self.position += 1;

        while let Some(byte) = self.peek_byte() {
            match byte {
                b'"' => {
                    self.position += 1;
                    return Ok(Span {
                        start,
                        end: self.position,
                    });
                }
                b'\\' => {
                    self.position += 1;
                    if self.peek_byte().is_none() {
                        bail!("unterminated JSON string escape");
                    }
                    self.position += 1;
                }
                _ => self.position += 1,
            }
        }

        bail!("unterminated JSON string")
    }

    fn parse_primitive(&mut self) -> Result<Span> {
        let start = self.position;
        while let Some(byte) = self.peek_byte() {
            if matches!(byte, b',' | b']' | b'}' | b' ' | b'\t' | b'\r' | b'\n') {
                break;
            }
            self.position += 1;
        }
        if self.position == start {
            bail!("expected JSON primitive");
        }
        Ok(Span {
            start,
            end: self.position,
        })
    }

    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.peek_byte() {
            if matches!(byte, b' ' | b'\t' | b'\r' | b'\n') {
                self.position += 1;
            } else {
                break;
            }
        }
    }

    fn expect_byte(&self, expected: u8) -> Result<()> {
        match self.peek_byte() {
            Some(byte) if byte == expected => Ok(()),
            Some(byte) => bail!(
                "expected JSON byte {:?}, found {:?}",
                expected as char,
                byte as char
            ),
            None => bail!(
                "expected JSON byte {:?}, found end of input",
                expected as char
            ),
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }
}

#[derive(Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

impl Span {
    fn len(self) -> usize {
        self.end - self.start
    }

    fn slice<'a>(self, file_text: &'a str) -> &'a str {
        &file_text[self.start..self.end]
    }
}

enum Node {
    Object(ObjectNode),
    Array(ArrayNode),
    Primitive(Span),
}

impl Node {
    fn span(&self) -> Span {
        match self {
            Node::Object(object) => object.span,
            Node::Array(array) => array.span,
            Node::Primitive(span) => *span,
        }
    }
}

struct ObjectNode {
    span: Span,
    properties: Vec<PropertyNode>,
}

struct PropertyNode {
    key: String,
    span: Span,
    value: Node,
}

struct ArrayNode {
    span: Span,
    elements: Vec<Node>,
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

        let out = format_text_with_host(Path::new("package.json"), input, &config, |text| {
            assert_eq!(text, input);
            Ok(Some(expected.to_string()))
        })
        .unwrap();

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
}
