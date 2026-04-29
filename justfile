# https://just.systems
#
# Common dev tasks for dprint-plugin-sortpackagejson.
# Run `just` (no args) to list all recipes.

set shell := ["bash", "-cu"]

alias lint := clippy

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------

default:
    @just --list --unsorted

# ---------------------------------------------------------------------------
# Build / test / lint
# ---------------------------------------------------------------------------

# Build the artifacts this repo publishes: plugin.wasm + schema.json.
build: wasm schema

# Build the local sortpkg CLI binary (release).
cli:
    cargo build --release --features cli --bin sortpkg

# Run all unit + integration tests.
test:
    cargo test --all-features

# Quick sanity check — only library tests, no features.
test-lib:
    cargo test --lib

# Format Rust sources.
# Uses nightly rustfmt for unstable imports_granularity + group_imports

# in rustfmt.toml. Install: rustup toolchain install nightly --component rustfmt.
fmt:
    cargo +nightly fmt --all

# Verify formatting without writing.
fmt-check:
    cargo +nightly fmt --all -- --check

# Lint with clippy at the same strictness CI uses.
clippy:
    cargo clippy --all-targets --all-features --workspace -- -D clippy::all

# Auto-fix as much as possible (formatting + clippy --fix).
fix:
    cargo +nightly fmt --all
    cargo clippy --all-targets --all-features --workspace --fix --allow-dirty -- -D clippy::all

# All quality gates: format check + clippy + tests.
ci:
    just fmt-check
    just clippy
    just test

# ---------------------------------------------------------------------------
# CLI smoke tests against the upstream npm `sort-package-json`
# ---------------------------------------------------------------------------
# Diff our output against the upstream npm package's output for a fixture.

# Usage: just diff fixtures/some-pkg.json
diff fixture:
    @echo '== ours =='
    @cargo run --quiet --features cli --bin sortpkg < {{ fixture }} > /tmp/ours.json
    @cat /tmp/ours.json
    @echo '== upstream =='
    @bunx sort-package-json --stdin < {{ fixture }} > /tmp/upstream.json
    @cat /tmp/upstream.json
    @echo '== diff =='
    @diff /tmp/ours.json /tmp/upstream.json && echo 'MATCH' || echo 'DIFFERS'

# ---------------------------------------------------------------------------
# WASM build + size measurement
# ---------------------------------------------------------------------------

WASM_TARGET := "target/wasm32-unknown-unknown/wasm-release/dprint_plugin_sortpackagejson.wasm"

# Build the wasm artifact at the size-optimized profile.
wasm:
    rustup target add wasm32-unknown-unknown
    cargo build --target wasm32-unknown-unknown --profile wasm-release --no-default-features

# Print the size of the wasm artifact.
wasm-size: wasm
    @ls -lh {{ WASM_TARGET }} | awk '{print $5, $9}'

# Stage release artifacts: plugin.wasm + schema.json + .sha256 sidecars at repo root.
package: wasm schema
    cp {{ WASM_TARGET }} plugin.wasm
    sha256sum plugin.wasm | tee plugin.wasm.sha256
    sha256sum schema.json | tee schema.json.sha256

# ---------------------------------------------------------------------------
# Schema generation (Stage 8 wiring)
# ---------------------------------------------------------------------------

# Regenerate schema.json from the Configuration struct.
schema:
    cargo run --features schema --bin gen_schema > schema.json

# Verify schema.json is in sync with the Configuration struct (drift gate).
schema-check:
    cargo test --features schema --test schema_in_sync

# ---------------------------------------------------------------------------
# CodeRabbit review helpers
# ---------------------------------------------------------------------------

# Run a coderabbit review of unstaged changes.
review:
    coderabbit review --base-commit HEAD

# Review against an arbitrary base.
review-from base:
    coderabbit review --base-commit {{ base }}
