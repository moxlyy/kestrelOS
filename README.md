# kestrelOS — early tooling (v1.3.0-alpha)

## What is Kestrel?

kestrelOS is a from-scratch Linux distribution project, inspired by
NixOS but deliberately distinct from it: reproducible builds and a
content-addressed package store, but built on its own engine rather than
Nix's, and with first-class support for **both** runit and systemd as
init systems instead of committing to one.

This repository is the earliest tooling for that project — not the
distro itself yet, just the build engine, evaluator, GC, and
service-definition compiler it'll eventually run on. Four small Rust
binaries live here:

- **`kbuild`** — builds one package: hash its inputs, build it sandboxed,
  cache it by content hash.
- **`keval`** — resolves a directory of packages that reference each
  other by name into a dependency graph, building each in order.
- **`kgc`** — garbage collection: removes store paths nothing actually
  references anymore.
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
  Seriously evaluated this release; see "Toolchain bootstrap" below for
  why it's not done yet and what's actually been checked.
- No kernel, no installer, no bootable image of any kind yet.
- No build locking (concurrent builds of the same spec will race).

What IS implemented and tested, as of this release: reproducible
sandboxed builds, fixed-output network fetches, a dependency evaluator,
garbage collection, and the dual-init service compiler — see "Technical
details" for what was actually verified for each, not just written.

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
will become `kbuild/`, `kservice/`, and part of `bootstrap/` in that
future layout, and its internal layout (`packages/`, `services/`,
`scripts/`) is organized to make that eventual split easy rather than
disruptive.

## Getting started

Requires `bubblewrap` and a Rust toolchain
(`apt install bubblewrap rustc cargo` is enough; this is tested against
rustc 1.75 deliberately, using older crate versions to avoid needing
newer Cargo features).

```sh
cargo build --release

# use a local store instead of /kestrel/store unless running as root
export KBUILD_STORE=$HOME/kestrel-test-store
export KBUILD_GCROOTS=$HOME/kestrel-test-roots   # defaults next to the store

# build "hello" via the evaluator, rooting the result so GC won't touch it
./scripts/build-all.sh

# a single package directly, no evaluator, no root
./target/release/kbuild packages/libgreet/build.toml

# a fixed-output fetch derivation — pulls real content over HTTPS,
# verified against a hash declared in the spec
./target/release/kbuild packages/linux-copying/build.toml

# garbage collection — see what's reachable and what isn't
./target/release/kgc --dry-run
./target/release/kgc          # actually removes anything unreachable

# the dual-init service compiler
./target/release/kservice services/hello-daemon-standalone.toml --out /tmp/svc-out
cat /tmp/svc-out/runit/hello-daemon/run
cat /tmp/svc-out/systemd/hello-daemon.service
```

Run `scripts/build-all.sh` twice — the second run is all cache hits.
Edit `packages/libgreet/greet.c` and run it again — both `libgreet` and
`hello` get new store paths automatically. Then run `kbuild` on
`packages/linux-copying/build.toml` without `--root`, and `kgc --dry-run`
— it'll correctly offer to remove only that path, keeping `hello` and
`libgreet` because `hello`'s rooted and actually references `libgreet`'s
store path at runtime.

## Technical details / what's changed

### Garbage collection (`kgc`) — new in v1.3.0-alpha

GC roots are opt-in: `kbuild`/`keval` only create one if you pass
`--root <name>`, which writes a symlink under `$KBUILD_GCROOTS` pointing
at the result (mirrors `nix-build -o`). Anything not reachable from some
root is collectible.

Reachability is computed by actually scanning bytes, not by trusting
each build's declared inputs. Starting from roots, `kgc` reads every
reachable path's files looking for the literal basename of any other
store path; matches get pulled into the live set and queued to be
scanned themselves (a dependency can depend on something else), repeated
to a fixed point. This is the same core idea as Nix's
`scanForReferences`, just without the multi-pattern matching that would
make it scale past a handful of example packages — a real implementation
would want something like Aho-Corasick instead of one substring search
per candidate per file.

This is also why the `libgreet`/`hello` example switched from static to
dynamic linking this release. Tested directly: the static version left
*zero* trace of `libgreet`'s store path anywhere in `hello`'s bytes,
which means a byte-scanning GC would correctly consider `libgreet`
collectible the moment it's no longer needed at build time — accurate
behavior, but a confusing demo. Switching to a shared `.so` plus
`-Wl,-rpath` (the same pattern real Nix systems use for exactly this
reason) gives the scanner something genuine to find. Verified end to
end: built `hello` (rooted), built an unrelated unrooted fetch derivation
(a real orphan), and confirmed `kgc --dry-run` flagged only the orphan —
correctly treating `libgreet` as live purely from scanning `hello`'s
bytes, with no metadata lookup involved. Running it for real removed
exactly the orphan; `hello` still ran correctly afterward. Also checked
the inverse: removing the root makes everything collectible, confirmed
by actually running GC with no roots and watching the whole store empty
out.

## Roadmap

Roughly in the order I'd tackle these:

1. **Toolchain bootstrap, stage0.** Use the existing `[fetch]` mechanism
   to pull a hash-pinned seed (most likely an apt pool URL for something
   small and close to freestanding) and get one real build working
   inside a sandbox that does NOT bind-mount the host's `/usr`, `/bin`,
   `/lib` at all. That's a meaningfully smaller, achievable first step
   toward the eventual real bootstrap chain.
2. **A real package-definition language.** `keval`'s graph-resolution
   logic doesn't need to change for this — only how the graph gets
   produced. Starlark or a restricted Lua are still where I'd start over
   inventing a new lazy functional language from scratch.
3. **The daemon's own privilege.** Move build/fetch execution off root
   entirely (dedicated build users, a daemon designed around never
   running untrusted code with its own credentials), rather than the
   partial uid-drop that exists today.
4. **Build locking**, so concurrent builds of the same spec don't race.
5. **A faster reference scanner.** Aho-Corasick (or similar) instead of
   one substring search per candidate per file, so GC scales past a
   handful of example packages.
6. **A runit stage-2 generator and the systemd-target equivalent** for
   whole-system boot sequencing, not just individual services — and a
   way to depend on a systemd `.target` from `kservice`, which today only
   maps `depends_on` entries to `<name>.service`.
7. **A "system closure"** that composes kernel + packages + services into
   one buildable, switchable unit, plus bootloader generations for
   atomic switch/rollback — the actual distro-shaped milestone everything
   above is in service of.
