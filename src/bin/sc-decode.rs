use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, ValueEnum};
use state_collector::collector::Snapshot;

#[derive(ValueEnum, Clone, Copy, Default)]
enum Format {
    Json,
    #[default]
    PrettyJson,
    Rsn,
    PrettyRsn,
}

#[derive(Parser)]
struct Cli {
    /// Postcard binary file(s) to decode
    file: Vec<PathBuf>,
    /// Output format
    #[arg(short, long, default_value = "pretty-json")]
    format: Format,
}

fn main() -> anyhow::Result<()> {
    let Cli { file, format } = Cli::parse();
    let multiple = file.len() > 1;

    for path in &file {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let snapshot: Snapshot = postcard::from_bytes(&bytes)
            .with_context(|| format!("failed to decode {}", path.display()))?;

        if multiple {
            println!("=== {} ===", path.display());
        }

        let output = match format {
            Format::Json => serde_json::to_string(&snapshot)?,
            Format::PrettyJson => serde_json::to_string_pretty(&snapshot)?,
            Format::Rsn => rsn::to_string(&snapshot)?,
            Format::PrettyRsn => rsn::to_string_pretty(&snapshot)?,
        };

        println!("{output}");
    }

    Ok(())
}
