//! Drift gate. Compares schema generated from the current `Configuration`
//! against the schema served at `plugins.dprint.dev` for the latest
//! published version. If they differ, the next release ships a different
//! schema URL than IDEs already cache — bump `CARGO_PKG_VERSION` and let
//! the new version's URL serve the new schema.
//!
//! Skipped (not failed) when the network is unreachable or the plugin has
//! never been published.

#![cfg(feature = "schema")]

use std::{env, fs::OpenOptions, io::Write, time::Duration};

use dprint_plugin_sortpackagejson::configuration::Configuration;
use serde::{Deserialize, Serialize};
use serde_json::{
    Value,
    ser::{PrettyFormatter, Serializer},
};

const REPO_PATH: &str = "kjanat/dprint-plugin-sortpackagejson";
const PLUGINS_BASE_URL: &str = "https://plugins.dprint.dev";

#[derive(Deserialize)]
struct Latest {
    version: String,
}

#[test]
fn schema_matches_published_latest() {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();
    let latest_url = format!("{PLUGINS_BASE_URL}/{REPO_PATH}/latest.json");

    let latest: Latest = match agent.get(&latest_url).call() {
        Ok(r) => match r.into_json() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("skip: malformed latest.json: {e}");
                return;
            }
        },
        Err(ureq::Error::Status(404, _)) => {
            eprintln!("skip: no published version yet at {latest_url}");
            return;
        }
        Err(e) => {
            eprintln!("skip: network error fetching {latest_url}: {e}");
            return;
        }
    };

    let schema_url = format!(
        "{PLUGINS_BASE_URL}/{REPO_PATH}/{}/schema.json",
        latest.version
    );

    let published = match agent.get(&schema_url).call() {
        Ok(r) => match r.into_string() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("skip: read body of {schema_url}: {e}");
                return;
            }
        },
        Err(e) => {
            eprintln!("skip: fetch {schema_url} failed: {e}");
            return;
        }
    };

    let generated = generate_schema();
    let normalized_generated = normalize_schema(&generated);
    let normalized_published = normalize_schema(&published);
    if normalized_generated != normalized_published {
        let generated_pretty = format_normalized_schema(&normalized_generated);
        let published_pretty = format_normalized_schema(&normalized_published);
        let diff = diff_lines(&published_pretty, &generated_pretty);
        report_schema_drift_to_ci(&latest.version, &schema_url, &diff);
        panic!(
            "schema for current code differs from published v{}.\n\
             URL:  {schema_url}\n\
             Diff (-published, +generated):\n\n```diff\n{diff}```\n\n\
             Bump CARGO_PKG_VERSION before merging this schema change so the new schema lives at a new URL.",
            latest.version,
        );
    }
}

fn generate_schema() -> String {
    let schema = schemars::schema_for!(Configuration);
    let mut value = serde_json::to_value(&schema).expect("schema to value");

    if let Value::Object(map) = value {
        let id = format!(
            "https://plugins.dprint.dev/{REPO_PATH}/{}/schema.json",
            env!("CARGO_PKG_VERSION"),
        );

        let mut ordered = serde_json::Map::new();
        if let Some(v) = map.get("$schema").cloned() {
            ordered.insert("$schema".to_string(), v);
        }
        ordered.insert("$id".to_string(), Value::String(id));
        for (k, v) in map {
            if k != "$schema" {
                ordered.insert(k, v);
            }
        }
        value = Value::Object(ordered);
    }

    let mut buf = Vec::new();
    let formatter = PrettyFormatter::with_indent(b"\t");
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut ser).expect("serialize schema");
    let mut s = String::from_utf8(buf).expect("schema is utf-8");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

fn normalize_schema(schema: &str) -> Value {
    let mut value: Value = serde_json::from_str(schema).expect("schema json parses");
    strip_non_semantic_fields(&mut value);
    value
}

fn strip_non_semantic_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("$id");
            for value in map.values_mut() {
                strip_non_semantic_fields(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                strip_non_semantic_fields(value);
            }
        }
        Value::String(text) => {
            *text = text.replace('\n', " ");
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn format_normalized_schema(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("normalized schema serializes")
}

fn diff_lines(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let max_len = old_lines.len().max(new_lines.len());
    let mut out = String::new();

    for i in 0..max_len {
        match (old_lines.get(i), new_lines.get(i)) {
            (Some(old_line), Some(new_line)) if old_line == new_line => {
                out.push(' ');
                out.push_str(old_line);
                out.push('\n');
            }
            (Some(old_line), Some(new_line)) => {
                out.push('-');
                out.push_str(old_line);
                out.push('\n');
                out.push('+');
                out.push_str(new_line);
                out.push('\n');
            }
            (Some(old_line), None) => {
                out.push('-');
                out.push_str(old_line);
                out.push('\n');
            }
            (None, Some(new_line)) => {
                out.push('+');
                out.push_str(new_line);
                out.push('\n');
            }
            (None, None) => {}
        }
    }

    out
}

fn report_schema_drift_to_ci(latest_version: &str, schema_url: &str, diff: &str) {
    if env::var_os("GITHUB_ACTIONS").is_none() {
        return;
    }

    let summary = format!(
        "## Schema drift\n\nPublished version: `{latest_version}`\n\nURL: <{schema_url}>\n\n```diff\n{diff}```\n",
    );

    if let Some(path) = env::var_os("GITHUB_STEP_SUMMARY")
        && let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path)
    {
        let _ = writeln!(file, "{summary}");
    }

    let annotation = format!(
        "schema drift vs published v{latest_version}. See step summary. URL: {schema_url}",
    );
    eprintln!(
        "::error title=schema drift::{}",
        escape_github_command(&annotation)
    );
}

fn escape_github_command(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}
