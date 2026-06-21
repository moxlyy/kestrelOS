use anyhow::{bail, Context, Result};
use kbuild::{runner, spec, store};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (spec_path, root_name) = parse_args(&args)?;

    let spec_dir = spec_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();

    let raw = std::fs::read_to_string(&spec_path)
        .with_context(|| format!("reading {}", spec_path.display()))?;
    let parsed: spec::BuildSpec =
        toml::from_str(&raw).with_context(|| format!("parsing {}", spec_path.display()))?;

    let out_path = runner::run_spec(&parsed, &spec_dir)?;

    if let Some(name) = root_name {
        let link = store::make_root(&name, &out_path)?;
        eprintln!("root: {} -> {}", link.display(), out_path.display());
    }

    println!("{}", out_path.display());
    Ok(())
}

fn parse_args(args: &[String]) -> Result<(PathBuf, Option<String>)> {
    if args.len() < 2 {
        bail!(
            "usage: {} <spec.toml> [--root <name>]",
            args.first().map(|s| s.as_str()).unwrap_or("kbuild")
        );
    }
    let spec_path = PathBuf::from(&args[1]);
    let mut root_name = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                let name = args
                    .get(i + 1)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("--root requires a name"))?;
                root_name = Some(name);
                i += 2;
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok((spec_path, root_name))
}
