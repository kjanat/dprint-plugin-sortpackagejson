//! Regenerate `schema.json` from the `Configuration` struct via schemars.
//! Run via `cargo run --features schema --example gen_schema > schema.json`.
//! Drift between code and schema is enforced by `tests/schema_in_sync.rs`.

use dprint_plugin_sortpackagejson::configuration::Configuration;

fn main() {
    let schema = schemars::schema_for!(Configuration);
    let pretty = serde_json::to_string_pretty(&schema).expect("serialize schema");
    println!("{pretty}");
}
