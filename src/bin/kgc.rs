// kgc: garbage collection via real reference scanning, not declared
// metadata. Starts from GC roots, then expands outward: anything a live
// path's file contents actually reference gets pulled in too, repeated
// until nothing new turns up. Whatever's left in the store but never
// reached is dead.
//
// Nothing is rooted automatically anywhere in this project — `kbuild` and
// `keval` only create a root if you pass `--root <name>`. An un-rooted
// build is fair game for the next GC run by design, not by accident.
//
// This deliberately scans actual bytes instead of trusting each build's
// declared `inputs`/`depends_on`. The difference matters: a statically
// linked binary doesn't need its build-time dependency anymore and won't
// reference it in its output bytes at all — GC correctly considers that
// dependency collectible once nothing else needs it, even though it WAS
// declared as an input. That's the same behavior a real Nix-like system
// has, not a bug here.

use anyhow::{Context, Result};
use kbuild::{refscan, store};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

fn main() -> Result<()> {
    let dry_run = std::env::args().any(|a| a == "--dry-run");

    let all_paths = store::list_store_paths()?;
    let basename = |p: &PathBuf| -> String { p.file_name().unwrap().to_string_lossy().to_string() };
    let by_name: HashMap<String, PathBuf> =
        all_paths.iter().map(|p| (basename(p), p.clone())).collect();

    let roots = store::list_root_targets()?;
    eprintln!("roots resolve to:");
    if roots.is_empty() {
        eprintln!("  (none — everything in the store is unreachable and will be removed)");
    }
    for r in &roots {
        eprintln!("  {}", r.display());
    }

    let mut live: HashSet<String> = HashSet::new();
    let mut to_scan: Vec<PathBuf> = Vec::new();
    for r in roots {
        let name = basename(&r);
        if live.insert(name.clone()) {
            to_scan.push(r);
        }
    }

    // BFS outward: scan each newly-live path exactly once, against
    // whatever candidates are still unresolved at that point. Anything
    // found gets added to the live set and queued for its OWN scan next
    // round, since a dependency can itself depend on something else.
    while !to_scan.is_empty() {
        let candidate_names: Vec<String> =
            by_name.keys().filter(|n| !live.contains(*n)).cloned().collect();
        if candidate_names.is_empty() {
            break;
        }

        let mut newly_found: HashSet<String> = HashSet::new();
        for path in &to_scan {
            let found = refscan::find_references(path, &candidate_names)
                .with_context(|| format!("scanning {}", path.display()))?;
            newly_found.extend(found);
        }

        to_scan = newly_found
            .into_iter()
            .filter(|n| live.insert(n.clone()))
            .map(|n| by_name[&n].clone())
            .collect();
    }

    let dead: Vec<&PathBuf> = all_paths.iter().filter(|p| !live.contains(&basename(p))).collect();

    eprintln!("\n{} live, {} dead:", live.len(), dead.len());
    for p in &dead {
        eprintln!("  {} {}", if dry_run { "would remove" } else { "removing" }, p.display());
    }

    if dry_run {
        eprintln!("\ndry run — nothing deleted. Drop --dry-run to actually collect.");
        return Ok(());
    }

    for p in &dead {
        store::unseal(p)?;
        std::fs::remove_dir_all(p).with_context(|| format!("removing {}", p.display()))?;
    }
    eprintln!("\nremoved {} path(s)", dead.len());
    Ok(())
}
