use dprint_core::configuration::{ConfigKeyMap, GlobalConfiguration};
use dprint_core::generate_plugin_code;
use dprint_core::plugins::{
    CheckConfigUpdatesMessage, ConfigChange, FileMatchingInfo, FormatResult, PluginInfo,
    PluginResolveConfigurationResult, SyncFormatRequest, SyncHostFormatRequest, SyncPluginHandler,
};

use crate::configuration::{Configuration, resolve_config};

/// Versioned schema URL for IDE autocomplete in users' `dprint.json`.
/// Bare-version GitHub release tag (NO `v-` prefix — enforced by the
/// release workflow's tag-format gate).
const SCHEMA_URL: &str = const_format::concatcp!(
    "https://github.com/kjanat/dprint-plugin-sortpackagejson/releases/download/",
    env!("CARGO_PKG_VERSION"),
    "/schema.json"
);

const HELP_URL: &str = "https://github.com/kjanat/dprint-plugin-sortpackagejson";

struct SortPackageJsonPluginHandler;

impl SyncPluginHandler<Configuration> for SortPackageJsonPluginHandler {
    fn resolve_config(
        &mut self,
        config: ConfigKeyMap,
        global_config: &GlobalConfiguration,
    ) -> PluginResolveConfigurationResult<Configuration> {
        let result = resolve_config(config, global_config);
        PluginResolveConfigurationResult {
            config: result.config,
            diagnostics: result.diagnostics,
            file_matching: FileMatchingInfo {
                // Empty extensions: we deliberately do not claim every `.json`
                // file. Matching exclusively on the basename keeps
                // `dprint-plugin-json` in charge of the rest.
                file_extensions: Vec::new(),
                file_names: vec!["package.json".to_string()],
            },
        }
    }

    fn check_config_updates(
        &self,
        _message: CheckConfigUpdatesMessage,
    ) -> Result<Vec<ConfigChange>, anyhow::Error> {
        Ok(Vec::new())
    }

    fn plugin_info(&mut self) -> PluginInfo {
        PluginInfo {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_key: "sortPackageJson".to_string(),
            help_url: HELP_URL.to_string(),
            config_schema_url: SCHEMA_URL.to_string(),
            update_url: None,
        }
    }

    fn license_text(&mut self) -> String {
        std::str::from_utf8(include_bytes!("../LICENSE"))
            .expect("LICENSE is valid UTF-8")
            .to_string()
    }

    fn format(
        &mut self,
        request: SyncFormatRequest<Configuration>,
        _format_with_host: impl FnMut(SyncHostFormatRequest) -> FormatResult,
    ) -> FormatResult {
        let file_text = String::from_utf8(request.file_bytes)?;
        crate::format_text(request.file_path, &file_text, request.config)
            .map(|maybe_text| maybe_text.map(|t| t.into_bytes()))
    }
}

generate_plugin_code!(SortPackageJsonPluginHandler, SortPackageJsonPluginHandler);
