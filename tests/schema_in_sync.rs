//! Drift gate: regenerate `schema.json` and assert byte-equality with the
//! committed copy. If this fails, run:
//!
//!   cargo run --features schema --example gen_schema > schema.json
//!
//! and commit the result. Doing this in a test (rather than only in CI)
//! means the failure shows up locally too.

#![cfg(feature = "schema")]

use dprint_plugin_sortpackagejson::configuration::Configuration;

#[test]
fn schema_json_matches_generated() {
    let schema = schemars::schema_for!(Configuration);
    let mut generated = serde_json::to_string_pretty(&schema).expect("serialize schema");
    // `println!` (used by gen_schema) appends a trailing newline; normalize
    // both sides so the test reflects what gets written to disk.
    if !generated.ends_with('\n') {
        generated.push('\n');
    }

    let committed = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/schema.json"))
        .expect("read schema.json");

    if generated != committed {
        panic!(
            "schema.json is out of date. Run:\n  \
             cargo run --features schema --example gen_schema > schema.json"
        );
    }
}
