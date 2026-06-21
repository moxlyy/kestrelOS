use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Root of the store. Override with KBUILD_STORE for testing without root
/// (the real thing would live at a fixed system path like /kestrel/store).
pub fn store_root() -> PathBuf {
    std::env::var("KBUILD_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/kestrel/store"))
}

pub fn store_path(hash: &str, name: &str, version: &str) -> PathBuf {
    store_root().join(format!("{hash}-{name}-{version}"))
}

pub fn exists(path: &Path) -> bool {
    path.exists()
}

/// Create the (empty, writable) output directory ahead of the build.
/// We bind THIS exact path into the sandbox, so anything the builder
/// writes already has the correct final absolute path baked in.
pub fn prepare_output_dir(path: &Path) -> Result<()> {
    if path.exists() {
        bail!("store path already exists: {}", path.display());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir(path)?;
    Ok(())
}

/// Make a finished store path read-only, mimicking Nix's store immutability.
/// Once sealed, nothing (including the daemon itself, ideally) should write
/// to it again — if the inputs ever change, that produces a *new* path
/// instead of mutating this one.
pub fn seal_readonly(path: &Path) -> Result<()> {
    let status = Command::new("chmod")
        .arg("-R")
        .arg("a-w")
        .arg(path)
        .status()
        .context("running chmod to seal store path")?;
    if !status.success() {
        bail!("chmod failed while sealing {}", path.display());
    }
    Ok(())
}
