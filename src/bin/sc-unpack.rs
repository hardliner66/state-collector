use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Parser;

#[derive(Parser)]
struct Cli {
    file: Vec<PathBuf>,
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let Cli { file, output } = Cli::parse();

    for file in file {
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .context("invalid file stem")?;
        let default_dir = file.parent().unwrap_or(Path::new("."));
        let output_dir = output
            .clone()
            .unwrap_or(default_dir.to_path_buf())
            .join(stem);

        std::fs::create_dir_all(&output_dir)?;
        let f = std::fs::File::open(file)?;
        let dec = flate2::read::GzDecoder::new(f);
        let mut archive = tar::Archive::new(dec);
        archive.unpack(&output_dir)?;

        println!("Unpacked to: {}", output_dir.display());
    }

    Ok(())
}
