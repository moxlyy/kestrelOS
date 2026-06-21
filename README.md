# kestrelOS — early tooling (v1.2.0-alpha)

## What is Kestrel?

kestrelOS is a from-scratch Linux distribution project, inspired by
NixOS but deliberately distinct from it: reproducible builds and a
content-addressed package store, but built on its own engine rather than
Nix's, and with first-class support for **both** runit and systemd as
init systems instead of committing to one.

This repository is the earliest tooling for that project — not the
distro itself yet, just the build engine and service-definition compiler
it'll eventually run on. Three small Rust binaries live here:

- **`kbuild`** — builds one package: hash its inputs, build it sandboxed,
  cache it by content hash.
- **`keval`** — resolves a whole directory of packages that reference
  each other by name into a dependency graph, building each in order.
- **`kservice`** — compiles one declared service into both a runit `run`
  script and a systemd `.service` unit.

## Project goals

- **Reproducible builds.** Same inputs always produce the same output at
  the same path. A build's hash is computed from everything that can
  affect it — source content, build script, dependencies, environment —
  before the build even runs.
- **A content-addressed store.** Every build's output lives at a path
  named after that hash (`<hash>-<name>-<version>`), so different
  versions or variants of something coexist without conflict, and a
  result is reused automatically whenever its inputs are unchanged.
- **Real sandboxing, not a convention.** Builds run with no network
  access and no visibility outside their declared inputs, enforced by
  Linux namespaces, not by builders agreeing to behave.
- **Dual init support as a first-class goal**, not an afterthought. Most
  Nix-like systems pick one init system and bake assumptions about it
  throughout. Here, a service is declared once and compiled to either
  backend — the goal is for neither to be the "real" one.

## Current status

This is a prototype, not yet an installable distribution, and is missing
the things that would make it one:

- No package language — package definitions are plain TOML. `keval`
  resolves named dependencies between them, but there's no real
  evaluator-as-a-language yet (see Roadmap).
- No toolchain bootstrap — builds borrow gcc/sh/ar from the host system
  rather than building and owning their own, like a real `stdenv` would.
- No kernel, no installer, no bootable image of any kind yet.
- No garbage collection or build locking.

Eventually this project will likely split into a `kestrel/` monorepo
along these lines:

```
kestrel/
├── kbuild/
├── kservice/
├── kernel/
├── packages/
├── bootstrap/
├── stage0/
├── stage1/
├── stage2/
├── stage3/
└── docs/
```

That split hasn't happened yet — it's premature while the project is
this small. Today, this single repository corresponds to roughly what
will become `kbuild/` and `kservice/` in that future layout, and its
internal layout (`packages/`, `services/`, `scripts/`) is organized to
make that eventual split easy rather than disruptive.

## Getting started

Requires `bubblewrap` and a Rust toolchain
(`apt install bubblewrap rustc cargo` is enough; this is tested against
rustc 1.75 deliberately, using older crate versions to avoid needing
newer Cargo features).

```sh
cargo build --release

# use a local store instead of /kestrel/store unless running as root
export KBUILD_STORE=$HOME/kestrel-test-store

# build the "hello" example via the evaluator — it resolves hello's
# dependency on libgreet automatically, no manual wiring
./scripts/build-all.sh

# a single package directly, no evaluator
./target/release/kbuild packages/libgreet/build.toml

# a fixed-output fetch derivation — pulls real content over HTTPS,
# verified against a hash declared in the spec
./target/release/kbuild packages/linux-copying/build.toml

# the dual-init service compiler
./target/release/kservice services/hello-daemon-standalone.toml --out /tmp/svc-out
cat /tmp/svc-out/runit/hello-daemon/run
cat /tmp/svc-out/systemd/hello-daemon.service
```

Run `scripts/build-all.sh` twice — the second run is all cache hits.
Edit `packages/libgreet/greet.c` and run it again — both `libgreet` and
`hello` get new store paths automatically, even though `hello.c` itself
never changed, because `hello`'s hash includes `libgreet`'s resolved
store path.

## Technical details / what's changed

### The evaluator (`keval`) — new in v1.2.0-alpha

Each package under `packages/<name>/build.toml` can declare
`depends_on = ["other-name"]` instead of a literal store path. `keval`:

1. Scans `packages/*/build.toml` and indexes each by its directory name
   (erroring if a spec's internal `name` doesn't match its directory —
   cheap to catch here, expensive to debug as a silent wrong cache hit
   later).
2. Resolves the dependency graph from a target package via DFS, in
   topological order, with real cycle detection — tested by deliberately
   constructing a two-package cycle; it reports the actual cycle
   (`a -> b -> a`), not just "a cycle exists."
3. Builds each package in that order. For each `depends_on` entry, it
   takes the dependency's just-computed store path and both appends it
   to that package's sandbox `inputs` and injects it as an upper-cased
   environment variable (`libgreet` -> `$LIBGREET`), so build scripts
   need no extra wiring at all.

This replaces what used to be a `sed`-based hack in the build script
(visible in the v1.0.0-alpha changelog entry) with the actual job an
evaluator does: turning named references into a concrete, ordered
derivation graph. It's deliberately *not* a new programming language —
the format is still plain TOML. A real language's evaluator would still
need to produce exactly this kind of graph; it would just generate it
(or skip the TOML and call the same library functions directly) instead
of you hand-writing the edges. `kbuild` itself remains unaware that
`keval` exists — it only ever sees fully-resolved specs with literal
`inputs`, the same contract it's always had.

The refactor that made this possible: `kbuild`'s build/fetch/cache logic
moved into a shared library (`src/lib.rs`, `src/runner.rs`), so `keval`
and the `kbuild` CLI call the exact same code instead of two copies of
it drifting apart.

### Store sealing is now root-resistant — fixed in v1.1.0-alpha

The original `chmod -R a-w` only stopped non-root writes — found by
testing it directly: running as root, the write still succeeded.
`store::seal_readonly` now uses `chattr -R +i` (the filesystem immutable
attribute), re-verified to block writes, new files, and deletions for
every uid including root. Falls back to chmod with a loud warning if the
filesystem doesn't support it.

### Privilege inside the sandbox, precisely

Builds and fetches run with `--unshare-user --uid 65534 --gid 65534`
inside bubblewrap. Worth being exact about what this does and doesn't
buy you: a process writing into a directory that lives outside its own
user namespace (like the store output dir, bind-mounted in from the
real host) still has its files attributed to the *real* uid of whoever
launched bwrap — root, since the daemon runs as root. So this doesn't
change file ownership in the store. It does stop the build/fetch process
itself from doing privileged things if a build script is malicious or
buggy. Real privilege separation for the daemon — so root isn't the one
running untrusted build scripts at all — is still unsolved here; Nix
solves it with a pool of dedicated `nixbld` users.

### Fixed-output fetch derivations

`[fetch]` (instead of `[build]`) declares a URL and a `sha256` up front.
The store path is named after that declared hash; after fetching, the
real content hash must match exactly or the build fails and cleans up
after itself. This is the *only* code path with networking enabled —
every `[build]` derivation still gets zero network, always. Tested both
directions: a correct fetch (succeeds, caches) and a deliberately wrong
hash (rejected, no leftover store path).

### The dual-init compiler (`kservice`)

One `service.toml` produces a runit `run` script and a systemd unit.
Both were checked against real tooling, not just read by eye: the runit
script was executed under an actual `runsv` process and ran correctly;
the systemd unit passes `systemd-analyze verify` (when it doesn't
reference a dependency unit that isn't installed on the validating
machine — see `services/hello-daemon.toml` vs
`services/hello-daemon-standalone.toml` for both cases).

## Roadmap

Roughly in the order I'd tackle these:

1. **A real package-definition language.** `keval`'s graph-resolution
   logic doesn't need to change for this — only how the graph gets
   produced. Starlark or a restricted Lua are still where I'd start over
   inventing a new lazy functional language from scratch.
2. **The daemon's own privilege.** Move build/fetch execution off root
   entirely (dedicated build users, a daemon designed around never
   running untrusted code with its own credentials), rather than just
   the partial uid-drop that exists today.
3. **A real toolchain bootstrap.** Stop borrowing `/usr`, `/bin`, `/lib`
   from the host; build and own the toolchain the way a real `stdenv`
   does. This is the most LFS-shaped piece of the remaining work.
4. **Garbage collection.** `store::unseal` exists for this but nothing
   calls it yet — need to walk live roots, scan store contents for
   store-path string references (the same trick Nix uses), and remove
   anything unreachable.
5. **Build locking**, so concurrent builds of the same spec don't race.
6. **A runit stage-2 generator and the systemd-target equivalent** for
   whole-system boot sequencing, not just individual services — and a
   way to depend on a systemd `.target` from `kservice`, which today only
   maps `depends_on` entries to `<name>.service`.
7. **A "system closure"** that composes kernel + packages + services into
   one buildable, switchable unit, plus bootloader generations for
   atomic switch/rollback — the actual distro-shaped milestone everything
   above is in service of.
