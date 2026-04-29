use dprint_core::{
    configuration::{ConfigKeyMap, GlobalConfiguration},
    generate_plugin_code,
    plugins::{
        CheckConfigUpdatesMessage, ConfigChange, FileMatchingInfo, FormatResult, PluginInfo,
        PluginResolveConfigurationResult, SyncFormatRequest, SyncHostFormatRequest,
        SyncPluginHandler,
    },
};

use crate::configuration::{Configuration, resolve_config};

const HELP_URL: &str = env!("CARGO_PKG_REPOSITORY");

/// Strip the `https://github.com/` prefix from `CARGO_PKG_REPOSITORY` to
/// yield `<owner>/<repo>` for the dprint plugin registry URLs. GitHub-only
/// by convention — fork hosts (GitLab, self-hosted) need a different
/// derivation.
fn repo_path() -> &'static str {
    env!("CARGO_PKG_REPOSITORY").trim_start_matches("https://github.com/")
}

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
            config_schema_url: format!(
                "https://plugins.dprint.dev/{}/{}/schema.json",
                repo_path(),
                env!("CARGO_PKG_VERSION"),
            ),
            update_url: Some(format!(
                "https://plugins.dprint.dev/{}/latest.json",
                repo_path(),
            )),
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
        mut format_with_host: impl FnMut(SyncHostFormatRequest) -> FormatResult,
    ) -> FormatResult {
        // Sorting needs the whole document; partial-range format is meaningless.
        if request.range.is_some() {
            return Ok(None);
        }
        let file_text = String::from_utf8(request.file_bytes)?;
        let host_format_path = request
            .file_path
            .with_file_name("sortpackagejson-host.json");
        let override_config = ConfigKeyMap::new();
        crate::format_text::format_text_with_host(
            request.file_path,
            &file_text,
            request.config,
            |text| match format_with_host(SyncHostFormatRequest {
                file_path: &host_format_path,
                file_bytes: text.as_bytes(),
                range: None,
                override_config: &override_config,
            }) {
                Ok(Some(bytes)) => Ok(Some(String::from_utf8(bytes)?)),
                Ok(None) => Ok(None),
                Err(err) => Err(err),
            },
        )
        .map(|maybe_text| maybe_text.map(|t| t.into_bytes()))
    }
}

generate_plugin_code!(SortPackageJsonPluginHandler, SortPackageJsonPluginHandler);
