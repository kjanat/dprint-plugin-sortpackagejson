//! Regenerate `schema.json` from the `Configuration` struct via schemars.
//! Run via `cargo run --features schema --bin gen_schema > schema.json`.
//! Drift between code and schema is enforced by `tests/schema_in_sync.rs`.
//!
//! Output uses tab indentation to match the rest of the repo (dprint config
//! formats JSON with tabs); serde_json's default `to_string_pretty` uses 2
//! spaces, which would force a reformat after every regen.

use dprint_plugin_sortpackagejson::configuration::Configuration;
use serde::Serialize;
use serde_json::ser::{PrettyFormatter, Serializer};

fn main() {
    let schema = schemars::schema_for!(Configuration);
    let mut buf = Vec::new();
    let formatter = PrettyFormatter::with_indent(b"\t");
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    schema.serialize(&mut ser).expect("serialize schema");
    let out = String::from_utf8(buf).expect("schema is utf-8");
    println!("{out}");
}
