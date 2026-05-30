use std::path::PathBuf;
use std::process::exit;
use std::sync::{Arc, OnceLock};

use anyhow::Context;
use chrono::Local;
use clap::Parser;
use postcard;
mod collector;
use rune::{
    ContextError, Diagnostics, Module, Source, Sources, Vm,
    runtime::Bytes,
    termcolor::{ColorChoice, StandardStream},
};

const MODULE_NAME: &str = match option_env!("STATE_COLLECTOR_MODULE_NAME") {
    Some(name) => name,
    None => "sc",
};

const LOG_PREFIX: &str = match option_env!("STATE_COLLECTOR_LOG_PREFIX") {
    Some(name) => name,
    None => "[collector]",
};

const ARCHIVE_EXT: &str = match option_env!("STATE_COLLECTOR_ARCHIVE_EXT") {
    Some(ext) => ext,
    None => "sc",
};

const TEMP_DIR_SUFFIX: &str = match option_env!("STATE_COLLECTOR_TEMP_DIR_SUFFIX") {
    Some(suffix) => suffix,
    None => "sc",
};

const ARCHIVE_PREFIX: &str = match option_env!("STATE_COLLECTOR_ARCHIVE_PREFIX") {
    Some(prefix) => prefix,
    None => "collected-state",
};

static OUTDIR: OnceLock<PathBuf> = OnceLock::new();

const DEFAULT_SCRIPT: &str = include_str!("../examples/basic.rn");
const OS_RELEASE_PATH: &str = "/etc/os-release";
const UPTIME_PATH: &str = "/proc/uptime";

#[derive(Parser)]
struct Cli {
    /// Rune script defining the `collect` function
    script: Option<PathBuf>,
    /// Output path for the .sc archive (default: ./collected-state-<timestamp>.sc)
    #[arg(short, long)]
    output: Option<PathBuf>,
}

// ── Rune-exposed functions ────────────────────────────────────────────────────

#[rune::function]
pub(crate) fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[rune::function]
fn hostname() -> Result<String, anyhow::Error> {
    let mut buffer = vec![0u8; 256];
    let result =
        unsafe { libc::gethostname(buffer.as_mut_ptr() as *mut libc::c_char, buffer.len()) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    let nul = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    buffer.truncate(nul);
    Ok(String::from_utf8(buffer)?)
}

#[rune::function]
fn os_pretty_name() -> Result<String, anyhow::Error> {
    let contents = std::fs::read_to_string(OS_RELEASE_PATH)?;

    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            return Ok(value.trim_matches('"').to_string());
        }
    }

    Ok(String::new())
}

#[rune::function]
fn uptime() -> Result<String, anyhow::Error> {
    let contents = std::fs::read_to_string(UPTIME_PATH)?;
    let seconds = contents
        .split_whitespace()
        .next()
        .context("missing uptime seconds")?
        .parse::<f64>()?;

    Ok(format_uptime(seconds))
}

/// Write `content` to `path` relative to the output directory.
/// Parent directories are created automatically.
#[rune::function]
fn write(path: String, content: String) -> Result<(), anyhow::Error> {
    let outdir = OUTDIR.get().context("output directory not initialized")?;
    let full = outdir.join(&path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full, content)?;
    Ok(())
}

/// Serialize a Rune value to postcard binary encoding.
#[rune::function]
fn to_postcard_bytes(value: rune::runtime::Value) -> Result<Bytes, anyhow::Error> {
    let raw = postcard::to_allocvec(&value)?;
    Ok(Bytes::from_slice(raw).map_err(|e| anyhow::anyhow!("{e}"))?)
}

/// Write binary `content` to `path` relative to the output directory.
/// Parent directories are created automatically.
#[rune::function]
fn write_bytes(path: String, content: Bytes) -> Result<(), anyhow::Error> {
    let outdir = OUTDIR.get().context("output directory not initialized")?;
    let full = outdir.join(&path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full, content.as_slice())?;
    Ok(())
}

/// Print a progress message to stderr.
#[rune::function]
fn log(msg: String) {
    eprintln!("{LOG_PREFIX} {msg}");
}

/// Return the absolute path of the output directory.
#[rune::function]
pub(crate) fn outdir() -> String {
    OUTDIR
        .get()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Return the current local time as `YYYY-MM-DD HH:MM:SS`.
#[rune::function]
fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn format_uptime(seconds: f64) -> String {
    let total_seconds = seconds.max(0.0).floor() as u64;
    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;

    if days > 0 {
        format!(
            "up {days} day{}, {hours:02}:{minutes:02}:{seconds:02}",
            if days == 1 { "" } else { "s" }
        )
    } else {
        format!("up {hours:02}:{minutes:02}:{seconds:02}")
    }
}

// ── Module registration ───────────────────────────────────────────────────────

pub fn module() -> Result<Module, ContextError> {
    let mut m = Module::with_item([MODULE_NAME])?;
    m.function_meta(version)?;
    m.function_meta(hostname)?;
    m.function_meta(os_pretty_name)?;
    m.function_meta(uptime)?;
    m.function_meta(collector::snapshot)?;
    m.function_meta(collector::log_units)?;
    m.function_meta(collector::resources_text)?;
    m.function_meta(collector::sysinfo_text)?;
    m.function_meta(collector::hardware_text)?;
    m.function_meta(collector::services_text)?;
    m.function_meta(collector::systemd_status_text)?;
    m.function_meta(collector::network_text)?;
    m.function_meta(collector::wifi_text)?;
    m.function_meta(collector::ports_text)?;
    m.function_meta(collector::filesystems_text)?;
    m.function_meta(collector::processes_text)?;
    m.function_meta(collector::summary_text)?;
    m.function_meta(write)?;
    m.function_meta(to_postcard_bytes)?;
    m.function_meta(write_bytes)?;
    m.function_meta(log)?;
    m.function_meta(outdir)?;
    m.function_meta(timestamp)?;
    Ok(m)
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Ok(Cli { script, output }) = Cli::try_parse() else {
        // disable usage and help text
        exit(1);
    };

    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();

    // Temp directory that the script writes into
    let tempdir = std::env::temp_dir().join(format!("{TEMP_DIR_SUFFIX}-{ts}"));
    std::fs::create_dir_all(&tempdir)?;
    OUTDIR.set(tempdir.clone()).ok();

    // Build rune context and compile script
    let mut context = rune_modules::default_context()?;
    context.install(module()?)?;

    let mut sources = Sources::new();
    if let Some(script) = script {
        sources.insert(Source::from_path(&script)?)?;
    } else {
        sources.insert(Source::memory(DEFAULT_SCRIPT)?)?;
    }

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
    archive.append_dir_all(".", &tempdir)?;
    archive.finish()?;

    if let Err(e) = std::fs::remove_dir_all(&tempdir) {
        eprintln!("warning: failed to clean up temp dir: {e}");
    }

    println!("Written to: {}", output_path.display());
    Ok(())
}
