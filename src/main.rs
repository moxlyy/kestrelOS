mod hash;
mod sandbox;
mod spec;
mod store;

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        bail!("usage: {} <spec.toml>", args.first().map(|s| s.as_str()).unwrap_or("kbuild"));
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

    match (&spec.build, &spec.fetch) {
        (Some(_), Some(_)) => bail!("spec has both [build] and [fetch] — pick exactly one"),
        (None, None) => bail!("spec has neither [build] nor [fetch]"),
        (Some(build), None) => run_build_spec(&spec, build, &spec_dir),
        (None, Some(fetch)) => run_fetch_spec(&spec, fetch),
    }
}

fn run_build_spec(spec: &spec::BuildSpec, build: &spec::BuildSection, spec_dir: &PathBuf) -> Result<()> {
    let derivation_hash = hash::derivation_hash(spec, build, spec_dir)?;
    let out_path = store::store_path(&derivation_hash, &spec.name, &spec.version);

    if store::exists(&out_path) {
        eprintln!("cache hit, nothing to do: {}", out_path.display());
        println!("{}", out_path.display());
        return Ok(());
    }

    eprintln!("building {}-{} -> {}", spec.name, spec.version, out_path.display());
    store::prepare_output_dir(&out_path)?;

    let result = sandbox::run_build(spec_dir, &build.builder, &out_path, &build.inputs, &build.env);

    match result {
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

fn run_fetch_spec(spec: &spec::BuildSpec, fetch: &spec::FetchSection) -> Result<()> {
    let fixed_hash = hash::fetch_store_hash(fetch)?;
    let out_path = store::store_path(&fixed_hash, &spec.name, &spec.version);

    if store::exists(&out_path) {
        eprintln!("cache hit, nothing to do: {}", out_path.display());
        println!("{}", out_path.display());
        return Ok(());
    }

    eprintln!(
        "fetching {}-{} from {} -> {}",
        spec.name, spec.version, fetch.url, out_path.display()
    );
    store::prepare_output_dir(&out_path)?;

    let result = (|| -> Result<()> {
        let downloaded = sandbox::run_fetch(&fetch.url, &out_path)?;
        hash::verify_sha256(&downloaded, &fetch.sha256)
            .context("downloaded content did not match the declared hash")?;
        Ok(())
    })();

    match result {
        Ok(()) => {
            store::seal_readonly(&out_path)?;
            eprintln!("fetched and verified: {}", out_path.display());
            println!("{}", out_path.display());
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&out_path);
            Err(e)
        }
    }
}
