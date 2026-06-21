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
/// used by garbage collection, never by ordinary builds.
pub fn unseal(path: &Path) -> Result<()> {
    let _ = Command::new("chattr").arg("-R").arg("-i").arg(path).status();
    let _ = Command::new("chmod").arg("-R").arg("u+w").arg(path).status();
    Ok(())
}

/// Where GC roots live. Roots are symlinks pointing at a top-level store
/// path; anything reachable from a root (directly, or transitively via a
/// reference found inside a reachable path's files) survives collection.
/// Defaults next to the store unless overridden.
pub fn gcroots_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("KBUILD_GCROOTS") {
        return PathBuf::from(dir);
    }
    store_root()
        .parent()
        .map(|p| p.join("gcroots"))
        .unwrap_or_else(|| PathBuf::from("/kestrel/gcroots"))
}

/// Create or replace a named root pointing at `target` (mirrors Nix's
/// `nix-build -o`). Roots are opt-in here — nothing is rooted unless you
/// ask for it with `--root <name>`, so an un-rooted build is fair game for
/// the next GC run by design, not by accident.
pub fn make_root(name: &str, target: &Path) -> Result<PathBuf> {
    let dir = gcroots_dir();
    std::fs::create_dir_all(&dir)?;
    let link_path = dir.join(name);
    let _ = std::fs::remove_file(&link_path); // replace if it already exists
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, &link_path)
        .with_context(|| format!("linking {} -> {}", link_path.display(), target.display()))?;
    Ok(link_path)
}

/// All top-level store paths currently present (direct children of the
/// store root that are directories).
pub fn list_store_paths() -> Result<Vec<PathBuf>> {
    let root = store_root();
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&root).with_context(|| format!("reading {}", root.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            paths.push(entry.path());
        }
    }
    Ok(paths)
}

/// Every root's resolved target, skipping broken symlinks (e.g. a root left
/// over from a path that was already collected, or that never built).
pub fn list_root_targets() -> Result<Vec<PathBuf>> {
    let dir = gcroots_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut targets = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        match std::fs::read_link(&path) {
            Ok(target) if target.exists() => targets.push(target),
            _ => continue,
        }
    }
    Ok(targets)
}
