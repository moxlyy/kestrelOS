//! Scan a store path's file contents for the literal basenames of other
//! store paths. This is the same core idea Nix uses (`scanForReferences`):
//! GC doesn't trust declared build inputs, it checks what a path's *actual
//! bytes* reference, because that's what the path needs at runtime.
//!
//! This implementation is intentionally simple: one substring search per
//! candidate per file. That's fine at the scale of a handful of example
//! packages and would NOT scale to a real store with thousands of paths —
//! a real implementation should do one pass per file checking all
//! candidates at once with something like Aho-Corasick, which is exactly
//! what this does, just without the fancy multi-pattern algorithm backing
//! it (see `find_references` — it already checks all candidates per file
//! in one read, just not in one scan-pass).

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;

/// Walk `dir` and return the subset of `candidates` (store path basenames)
/// that appear anywhere in any file's bytes under it.
pub fn find_references(dir: &Path, candidates: &[String]) -> Result<HashSet<String>> {
    let mut found = HashSet::new();
    scan_dir(dir, candidates, &mut found)
        .with_context(|| format!("scanning {} for references", dir.display()))?;
    Ok(found)
}

fn scan_dir(dir: &Path, candidates: &[String], found: &mut HashSet<String>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // unreadable dir (permissions, race) — skip rather than fail GC entirely
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            scan_dir(&path, candidates, found)?;
        } else if file_type.is_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                for candidate in candidates {
                    if found.contains(candidate) {
                        continue;
                    }
                    if contains_bytes(&bytes, candidate.as_bytes()) {
                        found.insert(candidate.clone());
                    }
                }
            }
        }
        // symlinks are not followed or content-scanned here — their target
        // path string isn't "file content" in the sense this scan cares
        // about. A more thorough implementation might also check link
        // targets directly.
    }
    Ok(())
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}
