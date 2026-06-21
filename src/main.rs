mod hash;
mod sandbox;
mod spec;
mod store;

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        bail!("usage: {} <build-spec.toml>", args.first().map(|s| s.as_str()).unwrap_or("kbuild"));
    }
    let spec_path = PathBuf::from(&args[1]);

    let spec_dir = spec_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();

    let raw = std::fs::read_to_string(&spec_path)
        .with_context(|| format!("reading {}", spec_path.display()))?;
    let spec: spec::BuildSpec = toml::from_str(&raw)
        .with_context(|| format!("parsing {}", spec_path.display()))?;

    let derivation_hash = hash::derivation_hash(&spec, &spec_dir)?;
    let out_path = store::store_path(&derivation_hash, &spec.name, &spec.version);

    if store::exists(&out_path) {
        eprintln!("cache hit, nothing to do: {}", out_path.display());
        println!("{}", out_path.display());
        return Ok(());
    }

    eprintln!(
        "building {}-{} -> {}",
        spec.name,
        spec.version,
        out_path.display()
    );
    store::prepare_output_dir(&out_path)?;

    let build_result = sandbox::run_build(
        &spec_dir,
        &spec.build.builder,
        &out_path,
        &spec.build.inputs,
        &spec.build.env,
    );

    match build_result {
        Ok(()) => {
            store::seal_readonly(&out_path)?;
            eprintln!("built: {}", out_path.display());
            println!("{}", out_path.display());
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&out_path);
            Err(e)
        }
    }
}
