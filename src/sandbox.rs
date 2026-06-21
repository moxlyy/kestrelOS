use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Run the builder inside a bubblewrap sandbox.
///
/// What this buys you, mechanically:
/// - --unshare-net: no network device exists inside the sandbox at all.
///   Not "blocked by a firewall rule" — there is no interface to use.
/// - The build only ever sees: its own scratch dir, its declared inputs
///   (bound read-only at their real store paths), and its own not-yet-sealed
///   output dir. It cannot reach into the rest of the store, so it cannot
///   pick up an undeclared dependency.
/// - The host's /usr, /bin, /lib are bound read-only so a normal toolchain
///   (gcc, sh, ar) is available. This is the biggest shortcut versus a real
///   Nix-like system: a real stdenv builds and owns its OWN toolchain rather
///   than trusting whatever happens to be on the host. That's the natural
///   next milestone once this mechanism feels solid.
pub fn run_build(
    spec_dir: &Path,
    builder: &str,
    out_path: &Path,
    inputs: &[String],
    env: &BTreeMap<String, String>,
) -> Result<()> {
    let build_dir = scratch_dir()?;
    copy_dir_contents(spec_dir, &build_dir)
        .with_context(|| format!("copying sources from {}", spec_dir.display()))?;

    let mut cmd = Command::new("bwrap");
    cmd.arg("--unshare-net")
        .arg("--unshare-pid")
        .arg("--unshare-ipc")
        .arg("--unshare-uts")
        .arg("--die-with-parent")
        .arg("--proc")
        .arg("/proc")
        .arg("--dev")
        .arg("/dev")
        .arg("--tmpfs")
        .arg("/tmp")
        .arg("--ro-bind")
        .arg("/usr")
        .arg("/usr")
        .arg("--ro-bind")
        .arg("/bin")
        .arg("/bin")
        .arg("--ro-bind")
        .arg("/lib")
        .arg("/lib");

    if Path::new("/lib64").exists() {
        cmd.arg("--ro-bind").arg("/lib64").arg("/lib64");
    }
    if Path::new("/etc/alternatives").exists() {
        cmd.arg("--ro-bind").arg("/etc/alternatives").arg("/etc/alternatives");
    }

    // writable scratch space, thrown away after the build
    cmd.arg("--bind").arg(&build_dir).arg("/build");

    // the output dir, bound at its REAL final path so hardcoded
    // references the builder writes (e.g. linker paths) stay correct
    cmd.arg("--bind").arg(out_path).arg(out_path);

    // declared dependencies — read-only, at their real store paths
    for input in inputs {
        cmd.arg("--ro-bind").arg(input).arg(input);
    }

    cmd.arg("--chdir").arg("/build");
    cmd.arg("--setenv").arg("out").arg(out_path.to_str().unwrap());
    cmd.arg("--setenv").arg("PATH").arg("/usr/bin:/bin");
    cmd.arg("--setenv").arg("HOME").arg("/build");

    for (k, v) in env {
        cmd.arg("--setenv").arg(k).arg(v);
    }

    cmd.arg("--").arg("/bin/sh").arg(builder);

    let status = cmd.status().context("spawning bwrap")?;
    let _ = std::fs::remove_dir_all(&build_dir);

    if !status.success() {
        bail!("build failed: bwrap exited with {:?}", status.code());
    }
    Ok(())
}

fn scratch_dir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("kbuild-{}-{}", std::process::id(), nanos()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&dest_path)?;
            copy_dir_contents(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}
