use serde::Deserialize;
use std::collections::BTreeMap;

/// A build spec is the "derivation" — everything needed to reproduce
/// a build deterministically. The format itself is still plain TOML, not
/// a real language — but `keval` (see src/bin/keval.rs) now resolves
/// `depends_on` references between specs automatically, so you only
/// hand-write the graph's edges, not its resolved paths.
#[derive(Debug, Deserialize, Clone)]
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
#[derive(Debug, Deserialize, Clone)]
pub struct FetchSection {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BuildSection {
    /// files (relative to the spec's directory) copied into the build dir
    pub sources: Vec<String>,
    /// which source file to execute as the builder, via `/bin/sh <builder>`
    pub builder: String,
    /// other store paths this build depends on — bound read-only into the
    /// sandbox at their real absolute path so hardcoded references stay valid.
    /// Set directly when you already know a literal store path; for
    /// evaluator-driven builds, prefer `depends_on` instead and let `keval`
    /// fill this in automatically.
    #[serde(default)]
    pub inputs: Vec<String>,
    /// names of sibling packages (by directory name, under a packages/ dir)
    /// this build depends on. Resolved by `keval`, not by `kbuild` directly —
    /// kbuild ignores this field entirely. For each name here, keval adds
    /// the resolved store path to `inputs` AND injects an environment
    /// variable named after it in upper-case (`libgreet` -> `$LIBGREET`)
    /// pointing at that store path, so build scripts need no extra wiring.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// extra environment variables exposed to the builder
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}
