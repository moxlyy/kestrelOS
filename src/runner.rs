//! One place that knows how a spec turns into a store path. Both the
//! `kbuild` CLI (one spec at a time) and `keval` (a whole dependency graph
//! of specs) call into this instead of each reimplementing the
//! build-vs-fetch branch and the cache-hit check.

use crate::{hash, sandbox, spec, store};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub fn run_spec(spec: &spec::BuildSpec, spec_dir: &Path) -> Result<PathBuf> {
    match (&spec.build, &spec.fetch) {
        (Some(_), Some(_)) => bail!("spec has both [build] and [fetch] — pick exactly one"),
        (None, None) => bail!("spec has neither [build] nor [fetch]"),
        (Some(build), None) => run_build(spec, build, spec_dir),
        (None, Some(fetch)) => run_fetch(spec, fetch),
    }
}

fn run_build(spec: &spec::BuildSpec, build: &spec::BuildSection, spec_dir: &Path) -> Result<PathBuf> {
    let derivation_hash = hash::derivation_hash(spec, build, spec_dir)?;
    let out_path = store::store_path(&derivation_hash, &spec.name, &spec.version);

    if store::exists(&out_path) {
        eprintln!("cache hit, nothing to do: {}", out_path.display());
        return Ok(out_path);
    }

    eprintln!("building {}-{} -> {}", spec.name, spec.version, out_path.display());
    store::prepare_output_dir(&out_path)?;

    let result = sandbox::run_build(spec_dir, &build.builder, &out_path, &build.inputs, &build.env);
    match result {
        Ok(()) => {
            store::seal_readonly(&out_path)?;
            eprintln!("built: {}", out_path.display());
            Ok(out_path)
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&out_path);
            Err(e)
        }
    }
}

fn run_fetch(spec: &spec::BuildSpec, fetch: &spec::FetchSection) -> Result<PathBuf> {
    let fixed_hash = hash::fetch_store_hash(fetch)?;
    let out_path = store::store_path(&fixed_hash, &spec.name, &spec.version);

    if store::exists(&out_path) {
        eprintln!("cache hit, nothing to do: {}", out_path.display());
        return Ok(out_path);
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
            Ok(out_path)
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&out_path);
            Err(e)
        }
    }
}
