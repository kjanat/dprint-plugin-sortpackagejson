//! Composition root for the sort pipeline. Each module under `sort` exposes
//! a `pass(map, config) -> map` function; this file wires them in execution
//! order. Adding a new sort rule means: write `pass()` in a new module, list
//! it in [`PASSES`].
//!
//! Each pass self-gates on its relevant `Configuration` flags — top-level
//! sees the full config, decides whether to act. This keeps the pipeline
//! mechanical and the pass code self-contained for testing.

mod canonical;
mod dependencies;
mod eslint;
mod exports;
mod helpers;
mod nested_alpha;
mod pnpm;
mod prettier;
mod scripts;
mod top_level;
mod workspaces;

use serde_json::{Map, Value};

use crate::configuration::Configuration;

type Pass = fn(Map<String, Value>, &Configuration) -> Map<String, Value>;

/// Ordered pipeline. Top-level sort must run first so subsequent passes
/// operate on a canonically-keyed object; the rest are independent and only
/// touch the keys they own.
const PASSES: &[Pass] = &[
    top_level::pass,
    dependencies::pass,
    scripts::pass,
    exports::pass,
    eslint::pass,
    prettier::pass,
    workspaces::pass,
    pnpm::pass,
    nested_alpha::pass,
];

/// Top-level entry point: sort a parsed `package.json` object by running
/// every pass in [`PASSES`].
pub fn sort_package_json(input: Map<String, Value>, config: &Configuration) -> Map<String, Value> {
    PASSES.iter().fold(input, |acc, pass| pass(acc, config))
}
