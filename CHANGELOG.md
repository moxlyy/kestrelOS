# Changelog

All notable changes to kbuild/kservice/keval, the early tooling for the
kestrelOS project. Versioned as `<major>.<minor>.<patch>-alpha` while
everything here is still a prototype, not a real release.

## v1.2.0-alpha

- **Added `keval`**, the dependency-graph evaluator. Packages under a
  `packages/` directory can now reference each other by name
  (`depends_on = ["libgreet"]`) instead of needing a literal store path
  hand-wired in. `keval` discovers all packages in a directory, resolves
  the dependency graph in topological order (with real cycle detection,
  reporting the actual cycle), builds each one, and automatically injects
  each resolved dependency's store path into both the next package's
  sandbox inputs and an upper-cased environment variable
  (`libgreet` -> `$LIBGREET`).
- Refactored `kbuild`'s internals into a shared library (`src/lib.rs`,
  `src/runner.rs`) so `keval` and `kbuild` use the exact same
  build/fetch/cache logic instead of duplicating it.
- Restructured the repo layout: package definitions moved to a top-level
  `packages/` directory, service definitions to `services/`, and the
  orchestration script to `scripts/`, anticipating the eventual move to a
  proper `kestrel/` monorepo with `kbuild/`, `kservice/`, `kernel/`,
  `bootstrap/`, and `packages/` as siblings.
- README restructured to lead with what the project is and where it's
  going, before the implementation diary.

## v1.1.0-alpha

- Fixed store sealing: `chmod -R a-w` was found (by testing it directly,
  as root) to not actually stop root from writing into a "sealed" store
  path. Replaced with `chattr -R +i`, verified to block root too, with a
  chmod fallback if the filesystem doesn't support the immutable
  attribute.
- Builds and fetches now run as an unprivileged uid (65534) inside the
  sandbox via bubblewrap's user namespace remapping, as partial
  defense-in-depth (the daemon itself is still root — see the README's
  limitations section for what this does and doesn't protect against).
- Added fixed-output (`[fetch]`) derivations: the only code path allowed
  to leave networking enabled, used to pull real content from the
  network with the result verified against a hash declared up front.
  Tested both a correct fetch (succeeds, caches) and a deliberately wrong
  hash (rejected, no leftover store path).
- Added `kservice`, the dual-init compiler: one declared service compiles
  to both a runit `run` script and a systemd `.service` unit. Validated
  against real tooling — the runit script was run under an actual
  `runsv` process, and the systemd unit passes `systemd-analyze verify`.

## v1.0.0-alpha

- First working prototype: `kbuild`, a single Rust binary implementing
  the core reproducibility mechanism. Hashes a build's declared inputs
  (source file contents, builder script, dependency store paths,
  environment), runs the build inside a `bubblewrap` sandbox with no
  network device and no visibility outside its declared inputs, and
  caches the result at a content-addressed store path.
- Demonstrated with two example packages (`libgreet`, a tiny static
  library, and `hello`, which links against it) wired together by hand
  via a `sed`-based orchestration script — the dependency cascade worked
  (changing `libgreet`'s source produced new store paths for both
  packages), but resolving the graph itself was entirely manual.
