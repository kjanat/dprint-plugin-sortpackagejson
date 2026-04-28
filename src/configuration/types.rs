use dprint_core::configuration::NewLineKind;
use serde::{Deserialize, Serialize};

/// Plugin configuration. Mirrors options exposed by the npm
/// `sort-package-json` package, with dprint-style camelCase keys.
///
/// The `use_tabs`, `indent_width`, `new_line_kind`, and `line_width` fields
/// are resolved from dprint's global config by `resolve_config` and are
/// not part of the public schema (they appear in the global `dprint.json`
/// schema instead).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Configuration {
    /// Custom top-level key order; empty falls back to the canonical
    /// sort-package-json order.
    pub sort_order: Vec<String>,
    /// Alphabetize entries inside dependency objects (`dependencies`,
    /// `devDependencies`, etc.).
    pub sort_dependencies: bool,
    /// Apply the canonical script ordering rules (pre/main/post grouping
    /// plus npm-run-all chain detection).
    pub sort_scripts: bool,
    /// Apply nested-section sort rules (engines, exports, eslintConfig,
    /// prettier, workspaces, pnpm, ...).
    pub sort_nested: bool,
    /// How to order top-level keys not present in the canonical list.
    pub unknown_keys: UnknownKeyPolicy,

    #[cfg_attr(feature = "schema", schemars(skip))]
    pub line_width: u32,
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub use_tabs: bool,
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub indent_width: u8,
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub new_line_kind: NewLineKind,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            sort_order: Vec::new(),
            sort_dependencies: true,
            sort_scripts: true,
            sort_nested: true,
            unknown_keys: UnknownKeyPolicy::default(),
            line_width: 80,
            use_tabs: false,
            indent_width: 2,
            new_line_kind: NewLineKind::LineFeed,
        }
    }
}

/// Strategy used for top-level keys that are not in the canonical
/// sort-package-json order.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum UnknownKeyPolicy {
    /// Sort unknown keys alphabetically after canonical ones (matches
    /// upstream sort-package-json behavior).
    #[default]
    Alphabetical,
    /// Keep unknown keys in their original order, appended after canonical.
    Preserve,
}
