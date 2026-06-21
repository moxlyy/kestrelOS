// keval: stage 1-2 of the pipeline, in the smallest form that's honest.
//
// This is deliberately NOT a new programming language. The package
// definition format is still plain TOML — keval's job is narrower than
// that: given a directory of TOML specs that reference each other by
// NAME (`depends_on = ["libgreet"]`), resolve that graph in dependency
// order, build each package, and automatically substitute each
// dependency's real, just-computed store path into the next package's
// `inputs` and environment before building it.
//
// That's the part that used to be a `sed` hack in build-all.sh. It's also
// genuinely the job a real evaluator does — turn named references into a
// concrete derivation graph — even though there's no language here yet.
// A future language's evaluator would still end up producing exactly this
// kind of graph; it would just generate the TOML (or skip the TOML
// entirely and call into the same `kbuild` library functions directly)
// instead of you hand-writing it. The graph-resolution logic in this file
// does not change either way.

use anyhow::{bail, Context, Result};
use kbuild::{runner, spec};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

struct Package {
    dir: PathBuf,
    spec: spec::BuildSpec,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        bail!("usage: {} <packages-dir> <target-package-name>", args.first().map(|s| s.as_str()).unwrap_or("keval"));
    }
    let packages_dir = PathBuf::from(&args[1]);
    let target = args[2].clone();

    let packages = discover_packages(&packages_dir)?;
    if !packages.contains_key(&target) {
        bail!(
            "no package named '{target}' found under {} (found: {})",
            packages_dir.display(),
            packages.keys().cloned().collect::<Vec<_>>().join(", ")
        );
    }

    let order = topological_order(&packages, &target)?;
    eprintln!("resolved build order: {}", order.join(" -> "));

    let mut built: HashMap<String, PathBuf> = HashMap::new();

    for name in &order {
        let pkg = &packages[name];
        let resolved_path = build_one(pkg, &built)?;
        built.insert(name.clone(), resolved_path);
    }

    println!("{}", built[&target].display());
    Ok(())
}

/// Scan `<packages_dir>/*/build.toml`, parse each, and index by name.
/// Errors loudly if a directory's spec.name doesn't match its directory
/// name — that mismatch is exactly the kind of thing that's cheap to catch
/// here and expensive to debug later as a wrong cache hit.
fn discover_packages(packages_dir: &Path) -> Result<BTreeMap<String, Package>> {
    let mut packages = BTreeMap::new();

    let entries = std::fs::read_dir(packages_dir)
        .with_context(|| format!("reading packages dir {}", packages_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir = entry.path();
        let build_toml = dir.join("build.toml");
        if !build_toml.exists() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();

        let raw = std::fs::read_to_string(&build_toml)
            .with_context(|| format!("reading {}", build_toml.display()))?;
        let parsed: spec::BuildSpec =
            toml::from_str(&raw).with_context(|| format!("parsing {}", build_toml.display()))?;

        if parsed.name != dir_name {
            bail!(
                "package directory '{dir_name}' has name = \"{}\" in its build.toml — \
                 these must match so the directory name can be used as a dependency reference",
                parsed.name
            );
        }

        packages.insert(dir_name, Package { dir, spec: parsed });
    }

    Ok(packages)
}

/// DFS-based topological sort starting from `target`, visiting only what
/// `target` actually transitively depends on (not the whole directory).
/// Detects cycles and reports the actual cycle, not just "a cycle exists".
fn topological_order(packages: &BTreeMap<String, Package>, target: &str) -> Result<Vec<String>> {
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    let mut stack = Vec::new(); // current DFS path, for cycle reporting

    visit(target, packages, &mut visited, &mut stack, &mut order)?;
    Ok(order)
}

fn visit(
    name: &str,
    packages: &BTreeMap<String, Package>,
    visited: &mut HashSet<String>,
    stack: &mut Vec<String>,
    order: &mut Vec<String>,
) -> Result<()> {
    if visited.contains(name) {
        return Ok(());
    }
    if stack.contains(&name.to_string()) {
        stack.push(name.to_string());
        bail!("dependency cycle detected: {}", stack.join(" -> "));
    }

    let pkg = packages
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("'{name}' is depended on but no such package exists"))?;

    stack.push(name.to_string());
    let deps = match &pkg.spec.build {
        Some(build) => build.depends_on.clone(),
        None => Vec::new(), // fetch derivations have no dependencies
    };
    for dep in &deps {
        visit(dep, packages, visited, stack, order)?;
    }
    stack.pop();

    visited.insert(name.to_string());
    order.push(name.to_string());
    Ok(())
}

/// Build (or fetch) one package, having already built everything it
/// depends on. Resolves `depends_on` names to real store paths using
/// `built`, injects them into `inputs` and into upper-cased env vars, then
/// hands off to the same runner logic kbuild itself uses.
fn build_one(pkg: &Package, built: &HashMap<String, PathBuf>) -> Result<PathBuf> {
    let mut resolved = spec::BuildSpec {
        name: pkg.spec.name.clone(),
        version: pkg.spec.version.clone(),
        build: pkg.spec.build.clone(),
        fetch: pkg.spec.fetch.clone(),
    };

    if let Some(build) = &mut resolved.build {
        for dep in &build.depends_on.clone() {
            let dep_path = built
                .get(dep)
                .ok_or_else(|| anyhow::anyhow!("'{dep}' should have been built already but wasn't"))?;
            let dep_path_str = dep_path.to_string_lossy().to_string();

            build.inputs.push(dep_path_str.clone());

            let env_key = dep.to_uppercase().replace('-', "_");
            build.env.entry(env_key).or_insert(dep_path_str);
        }
    }

    runner::run_spec(&resolved, &pkg.dir)
}
