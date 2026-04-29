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
use serde_json::ser::{PrettyFormatter, Serializer};

const REPO_PATH: &str = "kjanat/dprint-plugin-sortpackagejson";
const LATEST_URL: &str = concat!(
    "https://plugins.dprint.dev/",
    "kjanat/dprint-plugin-sortpackagejson",
    "/latest.json",
);

#[derive(Deserialize)]
struct Latest {
    version: String,
}

#[test]
fn schema_matches_published_latest() {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();

    let latest: Latest = match agent.get(LATEST_URL).call() {
        Ok(r) => match r.into_json() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("skip: malformed latest.json: {e}");
                return;
            }
        },
        Err(ureq::Error::Status(404, _)) => {
            eprintln!("skip: no published version yet at {LATEST_URL}");
            return;
        }
        Err(e) => {
            eprintln!("skip: network error fetching {LATEST_URL}: {e}");
            return;
        }
    };

    let schema_url = format!(
        "https://plugins.dprint.dev/{REPO_PATH}/{}/schema.json",
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
    let mut buf = Vec::new();
    let formatter = PrettyFormatter::with_indent(b"\t");
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    schema.serialize(&mut ser).expect("serialize schema");
    let mut s = String::from_utf8(buf).expect("schema is utf-8");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}
