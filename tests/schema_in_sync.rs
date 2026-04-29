//! Drift gate. Compares schema generated from the current `Configuration`
//! against the schema served at `plugins.dprint.dev` for the latest
//! published version. If they differ, the next release ships a different
//! schema URL than IDEs already cache — bump `CARGO_PKG_VERSION` and let
//! the new version's URL serve the new schema.
//!
//! Skipped (not failed) when the network is unreachable or the plugin has
//! never been published.

#![cfg(feature = "schema")]

use std::time::Duration;

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
    if generated != published {
        panic!(
            "schema for current code differs from published v{}.\n  \
             URL:  {schema_url}\n  \
             Bump CARGO_PKG_VERSION before merging this schema change so the \
             new schema lives at a new URL.",
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
