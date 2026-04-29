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
use dprint_plugin_sortpackagejson::{configuration::Configuration, format_text};

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
    #[arg(long, default_value_t = 2, value_name = "N")]
    indent: u8,

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

fn run(args: Args) -> Result<ExitCode> {
    let config = Configuration {
        use_tabs: args.tabs,
        indent_width: args.indent,
        ..Configuration::default()
    };

    if let Some(path) = args.check {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        return match format_text(&path, &text, &config)? {
            None => Ok(ExitCode::SUCCESS),
            Some(_) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "sortpkg: {} is not in canonical order",
                    path.display()
                );
                Ok(ExitCode::from(1))
            }
        };
    }

    let (text, virtual_path) = match args.file {
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

    let formatted = format_text(&virtual_path, &text, &config)?;
    let mut out = std::io::stdout().lock();
    out.write_all(formatted.as_deref().unwrap_or(text.as_str()).as_bytes())
        .context("writing stdout")?;
    Ok(ExitCode::SUCCESS)
}
