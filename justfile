# https://just.systems
#
# Common dev tasks for dprint-plugin-sortpackagejson.
# Run `just` (no args) to list all recipes.

set shell := ["bash", "-cu"]

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------

default:
    @just --list --unsorted

# ---------------------------------------------------------------------------
# Build / test / lint
# ---------------------------------------------------------------------------

# Native debug build of the lib + cli bin.
build:
    cargo build --features cli

# Build the cli bin in release mode.
build-cli:
    cargo build --release --features cli --bin sortpkg

# Run all unit + integration tests.
test:
    cargo test --all-features

# Quick sanity check — only library tests, no features.
test-lib:
    cargo test --lib

# Format Rust sources.
fmt:
    cargo fmt --all

# Verify formatting without writing.
fmt-check:
    cargo fmt --all -- --check

# Lint with clippy at the same strictness CI uses.
clippy:
    cargo clippy --all-targets --all-features --workspace -- -D clippy::all

# Auto-fix as much as possible (formatting + clippy --fix).
fix:
    cargo fmt --all
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

# Run wasm-opt -Oz -- requires `wasm-opt` on PATH (binaryen).
wasm-opt: wasm
    wasm-opt -Oz --strip-debug --strip-producers \
        -o {{ WASM_TARGET }} {{ WASM_TARGET }}
    @ls -lh {{ WASM_TARGET }}

# Print the size of the optimized wasm artifact.
wasm-size: wasm-opt
    @ls -lh {{ WASM_TARGET }} | awk '{print $5, $9}'

# ---------------------------------------------------------------------------
# Schema generation (Stage 8 wiring)
# ---------------------------------------------------------------------------

# Regenerate schema.json from the Configuration struct.
schema:
    cargo run --features schema --example gen_schema > schema.json

# ---------------------------------------------------------------------------
# CodeRabbit review helpers
# ---------------------------------------------------------------------------

# Run a coderabbit review of unstaged changes.
review:
    coderabbit review --base-commit HEAD

# Review against an arbitrary base.
review-from base:
    coderabbit review --base-commit {{ base }}
