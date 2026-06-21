use crate::spec::BuildSpec;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Hash the raw bytes of a single file.
pub fn file_hash(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

/// Compute the derivation hash for a build spec.
///
/// This hashes everything that can affect the output: the name/version,
/// every source file's *content* (not just its name), the builder script,
/// every declared input's store path (which already encodes that input's
/// own content hash transitively), and the environment. Same inputs in,
/// same hash out — every time, on every machine.
///
/// This is "input-addressed" hashing, the same approach Nix uses for
/// normal derivations: the path is determined before the build even runs,
/// because it only depends on the build *instructions*, not the result.
pub fn derivation_hash(spec: &BuildSpec, spec_dir: &Path) -> Result<String> {
    let mut lines = Vec::new();
    lines.push(format!("name={}", spec.name));
    lines.push(format!("version={}", spec.version));
    lines.push(format!("builder={}", spec.build.builder));

    let mut sources: Vec<(String, String)> = Vec::new();
    for src in &spec.build.sources {
        let h = file_hash(&spec_dir.join(src))?;
        sources.push((src.clone(), h));
    }
    sources.sort();
    for (path, h) in &sources {
        lines.push(format!("source={path}:{h}"));
    }

    let mut inputs = spec.build.inputs.clone();
    inputs.sort();
    for inp in &inputs {
        lines.push(format!("input={inp}"));
    }

    let env: Vec<(&String, &String)> = spec.build.env.iter().collect(); // BTreeMap already sorted
    for (k, v) in &env {
        lines.push(format!("env={k}={v}"));
    }

    let canonical = lines.join("\n");
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();

    // Truncated to 32 hex chars (128 bits) for shorter store paths.
    // A real system would likely keep the full digest or use a
    // base32 alphabet like Nix does to keep paths shell-safe and short.
    Ok(hex::encode(digest)[..32].to_string())
}
