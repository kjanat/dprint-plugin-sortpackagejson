# https://just.systems
#
# Common dev tasks for dprint-plugin-sortpackagejson.
# Run `just` (no args) to list all recipes.

TARGET := "wasm32-unknown-unknown"
PROFILE := "wasm-release"
WASM_TARGET := "target" / TARGET / PROFILE / "dprint_plugin_sortpackagejson.wasm"

alias b := build
alias t := test
alias c := ci
alias l := clippy
alias lint := clippy

[private]
default:
    @just --list --unsorted

# Build the artifacts this repo publishes: plugin.wasm + schema.json.
[group('build / test / lint')]
build: wasm schema

# Build the local sortpkg CLI binary (release).
[group('build / test / lint')]
cli:
    cargo build --release --features cli --bin sortpkg

# Run all unit + integration tests.
[group('build / test / lint')]
test:
    cargo test --all-features

# Quick sanity check — only library tests, no features.
[group('build / test / lint')]
test-lib:
    cargo test --lib

# Format Rust sources.
# Uses nightly rustfmt for unstable imports_granularity + group_imports

# in rustfmt.toml. Install: rustup toolchain install nightly --component rustfmt.
[group('build / test / lint')]
fmt:
    @cargo +nightly fmt --all
    @dprint fmt

# Verify formatting without writing.
[group('build / test / lint')]
fmt-check:
    cargo +nightly fmt --all -- --check

# Lint with clippy at the same strictness CI uses.
[group('build / test / lint')]
clippy:
    cargo clippy --all-targets --all-features --workspace -- -D clippy::all

# Auto-fix as much as possible (formatting + clippy --fix).
[group('build / test / lint')]
fix: fmt
    cargo clippy --all-targets --all-features --workspace --fix --allow-dirty -- -D clippy::all

# All quality gates: format check + clippy + tests.
[group('build / test / lint')]
ci: fmt-check clippy test

# CLI smoke tests against the upstream npm `sort-package-json`
# Diff our output against the upstream npm package's output for a fixture.

# Usage: just diff fixtures/some-pkg.json
[group('cli smoke tests')]
diff fixture:
    #!/usr/bin/env bash
    @echo '== ours =='
    @cargo run --quiet --features cli --bin sortpkg < {{ fixture }} > /tmp/ours.json
    @cat /tmp/ours.json
    @echo '== upstream =='
    @bunx sort-package-json --stdin < {{ fixture }} > /tmp/upstream.json
    @cat /tmp/upstream.json
    @echo '== diff =='
    @diff /tmp/ours.json /tmp/upstream.json && echo 'MATCH' || echo 'DIFFERS'

# Build the wasm artifact at the size-optimized profile.
[group('wasm')]
wasm:
    @rustup target list --installed | rg -qx "wasm32-unknown-unknown" || rustup target add wasm32-unknown-unknown
    cargo build --target wasm32-unknown-unknown --profile wasm-release --no-default-features

# Print the size of the wasm artifact.
[group('wasm')]
wasm-size: wasm
    @ls -lh {{ WASM_TARGET }} | awk '{print $5, $9}'

# Stage release artifacts: plugin.wasm + schema.json + .sha256 sidecars at repo root.
[group('release')]
package: wasm schema
    #!/usr/bin/env bash
    cp '{{ WASM_TARGET }}' plugin.wasm
    sha256sum plugin.wasm | tee plugin.wasm.sha256
    sha256sum schema.json | tee schema.json.sha256

# ---------------------------------------------------------------------------
# Schema generation (Stage 8 wiring)
# ---------------------------------------------------------------------------
# Regenerate schema.json from the Configuration struct.

[group('schema')]
[no-cd]
schema $file="schema.json":
    #!/usr/bin/env bash
    OUTDIR="$(dirname "${file}")"
    [[ -d "${OUTDIR}" ]] || mkdir -p "${OUTDIR}"
    cargo run --features schema --bin gen_schema > "${file}"
    echo "Generated ${file}"

# Verify schema.json is in sync with the Configuration struct (drift gate).
[group('schema')]
schema-check:
    cargo test --features schema --test schema_in_sync

# ---------------------------------------------------------------------------
# CodeRabbit review helpers
# ---------------------------------------------------------------------------

# Run a coderabbit review of unstaged changes. Pass `--base-commit` to review against an arbitrary base.
[group('code review')]
review base-commit="HEAD":
    coderabbit review --base-commit {{ base-commit }}
