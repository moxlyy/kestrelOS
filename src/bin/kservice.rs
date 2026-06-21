// kservice: the dual-init differentiator.
//
// You declare a service ONCE, at the level both init systems actually
// agree on (a command to run, what it depends on, whether to restart it).
// This tool compiles that single declaration into both a runit `run`
// script and a systemd `.service` unit. Neither backend is primary; a
// real system would pick one set of generated files to actually install
// based on user/system configuration, but both are always generated so
// switching later is just a different symlink, not a rewrite.
//
// What does NOT translate cleanly between the two, on purpose left out of
// this prototype: socket activation, cgroup resource limits, systemd's
// richer target/ordering graph beyond simple After=. Each of those needs
// a real decision (expose only on one backend? find a runit equivalent?
// drop it?) rather than a generic mapping, so they're deliberately absent
// here rather than faked.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct ServiceSpec {
    name: String,
    #[serde(default)]
    description: Option<String>,
    /// the full command line to run — interpreted by /bin/sh
    exec: String,
    /// names of other services this one starts after
    #[serde(default)]
    depends_on: Vec<String>,
    /// "always" (default) or "no" — anything else is rejected, not guessed at
    #[serde(default = "default_restart")]
    restart: String,
}

fn default_restart() -> String {
    "always".to_string()
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 || args[2] != "--out" {
        bail!("usage: kservice <service.toml> --out <dir>");
    }
    let spec_path = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[3]);

    if args[3] == args[1] {
        bail!("output dir and spec path must differ");
    }

    let raw = fs::read_to_string(&spec_path)
        .with_context(|| format!("reading {}", spec_path.display()))?;
    let spec: ServiceSpec =
        toml::from_str(&raw).with_context(|| format!("parsing {}", spec_path.display()))?;

    if spec.restart != "always" && spec.restart != "no" {
        bail!("restart must be \"always\" or \"no\", got: {}", spec.restart);
    }

    let runit_path = write_runit(&spec, &out_dir)?;
    let systemd_path = write_systemd(&spec, &out_dir)?;

    println!("wrote {}", runit_path.display());
    println!("wrote {}", systemd_path.display());
    Ok(())
}

fn write_runit(spec: &ServiceSpec, out_dir: &Path) -> Result<PathBuf> {
    let dir = out_dir.join("runit").join(&spec.name);
    fs::create_dir_all(&dir)?;
    let run_path = dir.join("run");

    let mut script = String::from("#!/bin/sh\n");
    script.push_str("exec 2>&1\n");
    if spec.restart == "no" {
        // runit's default behavior is to respawn whatever exits — a true
        // one-shot needs a `down` file (preventing autostart) PLUS manual
        // invocation via `sv once`, which this generator deliberately does
        // not attempt to fake. Flag it loudly instead of guessing.
        script.push_str("# NOTE: restart=\"no\" was requested, but runit always respawns a\n");
        script.push_str("# `run` script that exits. Real one-shot semantics need a `down`\n");
        script.push_str("# file plus `sv once`, set up separately — not generated here.\n");
    }
    script.push_str(&format!("exec {}\n", spec.exec));

    fs::write(&run_path, script).with_context(|| format!("writing {}", run_path.display()))?;
    let mut perms = fs::metadata(&run_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&run_path, perms)?;

    Ok(run_path)
}

fn write_systemd(spec: &ServiceSpec, out_dir: &Path) -> Result<PathBuf> {
    let dir = out_dir.join("systemd");
    fs::create_dir_all(&dir)?;
    let unit_path = dir.join(format!("{}.service", spec.name));

    let mut unit = String::new();
    unit.push_str("[Unit]\n");
    unit.push_str(&format!(
        "Description={}\n",
        spec.description.clone().unwrap_or_else(|| spec.name.clone())
    ));
    if !spec.depends_on.is_empty() {
        let after: Vec<String> = spec.depends_on.iter().map(|d| format!("{d}.service")).collect();
        unit.push_str(&format!("After={}\n", after.join(" ")));
        unit.push_str(&format!("Requires={}\n", after.join(" ")));
    }
    unit.push('\n');
    unit.push_str("[Service]\n");
    unit.push_str(&format!("ExecStart={}\n", spec.exec));
    unit.push_str(&format!("Restart={}\n", spec.restart));
    unit.push('\n');
    unit.push_str("[Install]\n");
    unit.push_str("WantedBy=multi-user.target\n");

    fs::write(&unit_path, unit).with_context(|| format!("writing {}", unit_path.display()))?;
    Ok(unit_path)
}
