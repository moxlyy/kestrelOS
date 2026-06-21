use serde::Deserialize;
use std::collections::BTreeMap;

/// A build spec is the "derivation" — everything needed to reproduce
/// a build deterministically. This is deliberately tiny: no language,
/// no evaluator, just a direct TOML description. The real evaluator
/// (stage 1-2 from the pipeline) would generate files like this one
/// from a higher-level language instead of you hand-writing them.
#[derive(Debug, Deserialize)]
pub struct BuildSpec {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub build: Option<BuildSection>,
    #[serde(default)]
    pub fetch: Option<FetchSection>,
}

/// A fixed-output derivation: instead of hashing inputs to determine the
/// path (like a normal build), the path is determined by the hash you
/// declare here — known before the fetch even runs. After fetching, the
/// real content hash must match `sha256` exactly or the build fails. This
/// is the ONLY kind of derivation allowed to use the network — see
/// sandbox::run_fetch.
#[derive(Debug, Deserialize)]
pub struct FetchSection {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
pub struct BuildSection {
    /// files (relative to the spec's directory) copied into the build dir
    pub sources: Vec<String>,
    /// which source file to execute as the builder, via `/bin/sh <builder>`
    pub builder: String,
    /// other store paths this build depends on — bound read-only into the
    /// sandbox at their real absolute path so hardcoded references stay valid
    #[serde(default)]
    pub inputs: Vec<String>,
    /// extra environment variables exposed to the builder
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}
