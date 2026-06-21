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

/// Make a finished store path immutable, mimicking Nix's store immutability —
/// if the inputs ever change, that produces a *new* path instead of mutating
/// this one.
///
/// `chmod -R a-w` alone is NOT enough: a process running as root bypasses
/// standard Unix permission checks entirely and can still write into a
/// "read-only" tree (verified by testing this directly — see the README).
/// `chattr +i` sets the filesystem-level immutable attribute, which blocks
/// modification, deletion, and even adding new files to a directory for
/// EVERY uid, including root. Reversing it (for garbage collection) requires
/// an explicit `chattr -i` first — see `unseal()`.
///
/// Not all filesystems support the immutable attribute (notably some
/// overlay/tmpfs configurations). We fall back to chmod with a warning
/// rather than hard-failing, so the tool still works there, just with the
/// weaker guarantee.
pub fn seal_readonly(path: &Path) -> Result<()> {
    let chattr_ok = Command::new("chattr")
        .arg("-R")
        .arg("+i")
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !chattr_ok {
        eprintln!(
            "warning: chattr +i not supported on this filesystem; falling back to chmod \
             (this will NOT stop a root process from writing into the store path)"
        );
        let status = Command::new("chmod")
            .arg("-R")
            .arg("a-w")
            .arg(path)
            .status()
            .context("running chmod to seal store path")?;
        if !status.success() {
            bail!("chmod failed while sealing {}", path.display());
        }
    }
    Ok(())
}

/// Reverse `seal_readonly` so a path can be modified or removed again —
/// used by garbage collection, never by ordinary builds. Not called yet
/// anywhere in this codebase; kept here because GC is the next consumer.
#[allow(dead_code)]
pub fn unseal(path: &Path) -> Result<()> {
    let _ = Command::new("chattr").arg("-R").arg("-i").arg(path).status();
    let _ = Command::new("chmod").arg("-R").arg("u+w").arg(path).status();
    Ok(())
}
