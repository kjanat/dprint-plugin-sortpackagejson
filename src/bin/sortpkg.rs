//! `sortpkg` — local sanity-check binary. Sorts a `package.json` from
//! stdin (or `--check` against a path) using the same `format_text` entry
//! point that the wasm plugin runs through, with no dprint runtime in the
//! middle. Useful for quickly diffing our output against the upstream
//! `sort-package-json` CLI:
//!
//! ```sh
//! cargo run --features cli --bin sortpkg < pkg.json > ours.json
//! npx sort-package-json --stdin < pkg.json > theirs.json
//! diff ours.json theirs.json
//! ```
//!
//! Not included in the wasm artifact (gated behind `cli` feature).

use std::{
    io::{Read, Write},
    path::PathBuf,
    process::ExitCode,
};

use anyhow::{Context, Result};
use clap::Parser;
use dprint_core::configuration::NewLineKind;
use dprint_plugin_sortpackagejson::{configuration::Configuration, detect_indent, format_text};

#[derive(Parser)]
#[command(
    name = "sortpkg",
    about = "Sort a package.json. Reads stdin if no FILE is given.",
    version
)]
struct Args {
    /// Indent with tabs instead of spaces.
    #[arg(long)]
    tabs: bool,

    /// Space count for indentation when not using tabs.
    ///
    /// Defaults to whatever the input file already uses, matching the
    /// upstream `sort-package-json` CLI.
    #[arg(long, value_name = "N", conflicts_with = "tabs")]
    indent: Option<u8>,

    /// Exit 1 (and write a message to stderr) if FILE is not in canonical order.
    #[arg(long, value_name = "FILE", conflicts_with = "file")]
    check: Option<PathBuf>,

    /// Read FILE instead of stdin.
    file: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(code) => code,
        Err(err) => {
            let _ = writeln!(std::io::stderr(), "sortpkg: {err:#}");
            ExitCode::from(2)
        }
    }
}

/// Resolve the indentation to emit: explicit flags win, otherwise keep
/// whatever the document already uses, otherwise two spaces.
fn resolve_style(args: &Args, text: &str) -> Configuration {
    let (use_tabs, indent_width) = if args.tabs {
        (true, 1)
    } else if let Some(width) = args.indent {
        (false, width)
    } else {
        detect_indent(text).unwrap_or((false, 2))
    };

    Configuration {
        use_tabs,
        indent_width,
        // Keep the file's own line endings, as the upstream CLI does.
        new_line_kind: NewLineKind::Auto,
        ..Configuration::default()
    }
}

fn run(args: Args) -> Result<ExitCode> {
    if let Some(path) = args.check.clone() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let config = resolve_style(&args, &text);
        return match format_text(&path, &text, &config)? {
            None => Ok(ExitCode::SUCCESS),
            Some(_) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "sortpkg: {} is not in canonical order or is not formatted",
                    path.display()
                );
                Ok(ExitCode::from(1))
            }
        };
    }

    let (text, virtual_path) = match args.file.clone() {
        Some(path) => {
            let t = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            (t, path)
        }
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading stdin")?;
            (buf, PathBuf::from("package.json"))
        }
    };

    let config = resolve_style(&args, &text);
    let formatted = format_text(&virtual_path, &text, &config)?;
    let mut out = std::io::stdout().lock();
    out.write_all(formatted.as_deref().unwrap_or(text.as_str()).as_bytes())
        .context("writing stdout")?;
    Ok(ExitCode::SUCCESS)
}
