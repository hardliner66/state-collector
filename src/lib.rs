use std::path::PathBuf;
use std::sync::OnceLock;

pub static OUTDIR: OnceLock<PathBuf> = OnceLock::new();

pub mod collector;
pub mod constants;
