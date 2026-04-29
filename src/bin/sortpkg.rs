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
//! Flags:
//!   --tabs              indent with tabs instead of spaces
//!   --indent=N          space count for indentation (default 2)
//!   --check FILE        exit 1 if FILE would change; print diff to stderr
//!   FILE (positional)   read FILE instead of stdin
//!
//! Not included in the wasm artifact (gated behind `cli` feature).

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use dprint_plugin_sortpackagejson::configuration::Configuration;
use dprint_plugin_sortpackagejson::format_text;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            let _ = writeln!(std::io::stderr(), "sortpkg: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut use_tabs = false;
    let mut indent_width: u8 = 2;
    let mut check_path: Option<PathBuf> = None;
    let mut input_path: Option<PathBuf> = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--tabs" => use_tabs = true,
            s if s.starts_with("--indent=") => {
                indent_width = s["--indent=".len()..]
                    .parse()
                    .context("--indent must be a number")?;
            }
            "--check" => {
                let p = iter.next().ok_or_else(|| anyhow!("--check needs a path"))?;
                check_path = Some(PathBuf::from(p));
            }
            "-h" | "--help" => {
                println!("{}", help());
                return Ok(ExitCode::SUCCESS);
            }
            other if other.starts_with("--") => {
                return Err(anyhow!("unknown flag: {other}"));
            }
            _ => input_path = Some(PathBuf::from(arg)),
        }
    }

    let config = Configuration {
        use_tabs,
        indent_width,
        ..Configuration::default()
    };

    if let Some(path) = check_path {
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

    let (text, virtual_path) = match input_path {
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

fn help() -> &'static str {
    "sortpkg [--tabs] [--indent=N] [--check FILE] [FILE]\n\
     \n\
     Sort a package.json. Reads stdin if no FILE is given.\n\
     Writes sorted output to stdout, or exit 1 with --check when changes are needed."
}
