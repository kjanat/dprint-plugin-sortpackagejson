use dprint_core::configuration::{
    ConfigKeyMap, ConfigKeyValue, ConfigurationDiagnostic, GlobalConfiguration, NewLineKind,
    RECOMMENDED_GLOBAL_CONFIGURATION, ResolveConfigurationResult, get_unknown_property_diagnostics,
    get_value,
};

use super::types::{Configuration, UnknownKeyPolicy};

pub fn resolve_config(
    config: ConfigKeyMap,
    global_config: &GlobalConfiguration,
) -> ResolveConfigurationResult<Configuration> {
    let mut diagnostics = Vec::new();
    let mut config = config;

    let sort_order = take_string_array(&mut config, "sortOrder", &mut diagnostics);

    let unknown_keys_raw: String = get_value(
        &mut config,
        "unknownKeys",
        "alphabetical".to_string(),
        &mut diagnostics,
    );
    let unknown_keys = match unknown_keys_raw.as_str() {
        "alphabetical" => UnknownKeyPolicy::Alphabetical,
        "preserve" => UnknownKeyPolicy::Preserve,
        other => {
            diagnostics.push(ConfigurationDiagnostic {
                property_name: "unknownKeys".to_string(),
                message: format!("Expected 'alphabetical' or 'preserve', got '{other}'."),
            });
            UnknownKeyPolicy::Alphabetical
        }
    };

    let resolved = Configuration {
        sort_order,
        sort_dependencies: get_value(&mut config, "sortDependencies", true, &mut diagnostics),
        sort_scripts: get_value(&mut config, "sortScripts", true, &mut diagnostics),
        sort_nested: get_value(&mut config, "sortNested", true, &mut diagnostics),
        unknown_keys,
        line_width: get_value(
            &mut config,
            "lineWidth",
            global_config
                .line_width
                .unwrap_or(RECOMMENDED_GLOBAL_CONFIGURATION.line_width),
            &mut diagnostics,
        ),
        use_tabs: get_value(
            &mut config,
            "useTabs",
            global_config
                .use_tabs
                .unwrap_or(RECOMMENDED_GLOBAL_CONFIGURATION.use_tabs),
            &mut diagnostics,
        ),
        indent_width: get_value(
            &mut config,
            "indentWidth",
            global_config.indent_width.unwrap_or(2),
            &mut diagnostics,
        ),
        new_line_kind: get_value(
            &mut config,
            "newLineKind",
            global_config.new_line_kind.unwrap_or(NewLineKind::LineFeed),
            &mut diagnostics,
        ),
    };

    diagnostics.extend(get_unknown_property_diagnostics(config));

    ResolveConfigurationResult {
        config: resolved,
        diagnostics,
    }
}

/// Pulls a `Vec<String>` out of `ConfigKeyMap`. dprint config values are
/// untyped at the wire level, so we walk the array manually and emit a
/// diagnostic for every malformed element rather than failing the whole
/// resolve.
fn take_string_array(
    config: &mut ConfigKeyMap,
    key: &str,
    diagnostics: &mut Vec<ConfigurationDiagnostic>,
) -> Vec<String> {
    let Some(value) = config.shift_remove(key) else {
        return Vec::new();
    };
    let ConfigKeyValue::Array(items) = value else {
        diagnostics.push(ConfigurationDiagnostic {
            property_name: key.to_string(),
            message: "Expected an array of strings.".to_string(),
        });
        return Vec::new();
    };
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.into_iter().enumerate() {
        match item {
            ConfigKeyValue::String(s) => out.push(s),
            _ => diagnostics.push(ConfigurationDiagnostic {
                property_name: key.to_string(),
                message: format!("Expected element at index {i} to be a string."),
            }),
        }
    }
    out
}
