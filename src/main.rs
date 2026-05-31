use std::path::PathBuf;
use std::process::exit;
use std::sync::{Arc, OnceLock};

use tempfile::TempDir;

use chrono::Local;
use clap::{Parser, Subcommand};
mod collector;
mod constants;
use rune::{
    Diagnostics, Source, Sources, Vm,
    termcolor::{ColorChoice, StandardStream},
};

use crate::constants::{
    ARCHIVE_EXT, ARCHIVE_PREFIX, DEFAULT_SCRIPT_BASIC, DEFAULT_SCRIPT_BINARY, DEFAULT_SCRIPT_JSON,
};

static OUTDIR: OnceLock<PathBuf> = OnceLock::new();
static TEMPDIR: OnceLock<TempDir> = OnceLock::new();

#[derive(Subcommand, Default)]
enum ScriptType {
    #[default]
    Basic,
    Json,
    Binary,
    Custom {
        path: PathBuf,
    },
}

#[derive(Parser)]
struct Cli {
    /// Output path for the .sc archive (default: ./collected-state-<timestamp>.sc)
    #[arg(global = true, short, long)]
    output: Option<PathBuf>,
    /// What script to run (default: basic)
    #[command(subcommand)]
    script_type: Option<ScriptType>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Ok(Cli {
        script_type,
        output,
    }) = Cli::try_parse()
    else {
        // disable usage and help text
        exit(1);
    };

    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();

    // Temp directory that the script writes into
    let tempdir = tempfile::tempdir()?;
    OUTDIR.set(tempdir.path().to_path_buf()).ok();
    TEMPDIR.set(tempdir).ok();
    let tempdir_path = OUTDIR.get().unwrap();

    // Build rune context and compile script
    let mut context = rune_modules::default_context()?;
    context.install(collector::module()?)?;

    let mut sources = Sources::new();

    let source = match script_type.unwrap_or_default() {
        ScriptType::Basic => Source::memory(DEFAULT_SCRIPT_BASIC)?,
        ScriptType::Json => Source::memory(DEFAULT_SCRIPT_JSON)?,
        ScriptType::Binary => Source::memory(DEFAULT_SCRIPT_BINARY)?,
        ScriptType::Custom { path } => Source::from_path(path)?,
    };

    sources.insert(source)?;

    let mut diagnostics = Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();

    if !diagnostics.is_empty() {
        let mut writer = StandardStream::stderr(ColorChoice::Always);
        diagnostics.emit(&mut writer, &sources)?;
    }

    let unit = Arc::new(result?);
    let runtime = Arc::new(context.runtime()?);
    let mut vm = Vm::new(runtime, unit);

    vm.async_call(rune::Hash::type_hash(["collect"]), ())
        .await?;

    // Archive everything into a .sc file
    let default_output = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(format!(
            "{}-{ts}.{ARCHIVE_EXT}",
            std::env::current_exe()
                .map(|e| e
                    .file_stem()
                    .expect("to have a file-stem")
                    .to_string_lossy()
                    .to_string())
                .unwrap_or_else(|_| String::from(ARCHIVE_PREFIX))
        ));
    let output_path = output.as_deref().unwrap_or(&default_output);

    let file = std::fs::File::create(output_path)?;
    let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut archive = tar::Builder::new(enc);
    archive.append_dir_all(".", tempdir_path)?;
    archive.finish()?;

    // TempDir will auto-clean on drop

    println!("Written to: {}", output_path.display());
    Ok(())
}
