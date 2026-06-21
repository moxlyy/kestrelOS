# kbuild — a tiny reproducible builder (kestrelOS prototype, stage 3+4)

This is a minimal, *working* prototype of the core mechanism behind
Nix-style reproducibility: hash the build's inputs, build it in an
isolated sandbox, and cache the result at a path derived from that hash.

It deliberately does NOT include a package language or dependency
evaluator (stages 1-2 of the pipeline). You hand-write `build.toml`
files and hand-wire dependencies. That's the next milestone — see
"What's next" below.

## What's actually implemented

- **Derivation hashing** (`src/hash.rs`): a build's hash is computed from
  its name, version, the *content* of every source file, the builder
  script, every declared input's store path, and its environment.
  Same inputs -> same hash -> same store path, deterministically.
- **Content-addressed store** (`src/store.rs`): outputs land at
  `$KBUILD_STORE/<hash>-<name>-<version>` (defaults to `/kestrel/store`).
  If that path already exists, the build is skipped entirely (cache hit).
- **Sandboxed builds** (`src/sandbox.rs`): every build runs inside a
  `bubblewrap` sandbox with `--unshare-net` (no network device exists,
  not just a blocked one), its own scratch directory, its declared
  inputs bound read-only at their real store paths, and its own
  not-yet-sealed output directory bound at its real final path (so
  anything the builder hardcodes, like a linker path, stays correct).
- **Dependency wiring**: a build declares `inputs = ["/kestrel/store/..."]`
  and that path gets bind-mounted read-only into its sandbox. Nothing
  outside declared inputs is reachable, so a build can't quietly pick up
  an undeclared dependency.

## Try it

Requires `bubblewrap` and a Rust toolchain (`apt install bubblewrap rustc cargo`
works fine; this was built and tested against rustc 1.75 / cargo from
Ubuntu 24.04, deliberately using older crate versions to avoid needing
edition2024).

```sh
cargo build --release

# optional: use a local store instead of /kestrel/store (needs root otherwise)
export KBUILD_STORE=$HOME/kestrel-test-store

./build-all.sh
```

`build-all.sh` builds `examples/libgreet` (a tiny static library), then
patches libgreet's *real* resulting store path into
`examples/hello/build.toml.template` as a dependency, builds
`examples/hello` against it, and runs the result.

Run it twice — the second run will report cache hits and skip rebuilding
entirely. Edit `examples/libgreet/greet.c` and run again — you'll get a
brand new store path for libgreet *and* a brand new store path for hello,
even though `hello.c` never changed, because hello's hash includes
libgreet's store path. That cascade is the whole point.

## Build spec format

```toml
name = "hello"
version = "1.0.0"

[build]
sources = ["hello.c", "build.sh"]   # copied into the sandbox's scratch dir
builder = "build.sh"                # run as `/bin/sh build.sh`
inputs = ["/kestrel/store/abc...-libgreet-1.0.0"]  # bound read-only

[build.env]
CC = "gcc"
```

Inside the builder script, `$out` is the (real, absolute) store path
your build must write its output to.

## Known limitations — read this before treating it as more than a prototype

These are the honest gaps versus a real system, roughly in the order
I'd tackle them:

1. **No package language or evaluator.** You hand-resolve the dependency
   graph yourself (see how clumsy `build-all.sh`'s `sed` substitution is —
   that's exactly the job a real evaluator automates). This is the
   biggest missing piece and the natural next project.
2. **Root bypasses the read-only seal.** `store::seal_readonly` uses
   `chmod -R a-w`, which only stops *non-root* writes. I tested this live:
   running as root, I could still write into a "sealed" store path. A real
   system needs either unprivileged build users (what Nix's multi-user
   daemon does — builds run as dedicated `nixbld` users, never root) or
   to route all store mutation through a single trusted daemon process
   and nothing else.
3. **The toolchain is borrowed from the host.** `/usr`, `/bin`, `/lib` are
   bind-mounted read-only into every sandbox, so builds use whatever gcc
   happens to be on the host. A real stdenv builds and owns its *own*
   toolchain from a minimal bootstrap, so a build never depends on
   anything about the host machine. This is genuinely the LFS-shaped part
   of the work — turn that bootstrap into derivations themselves.
4. **No fixed-output / network-fetch derivations.** Nix allows exactly one
   kind of build to touch the network: a fetch whose result is verified
   against a hash you already declared. Without that, there's no way to
   pull source tarballs from the internet while keeping the "no network in
   the sandbox" guarantee. Worth adding early.
5. **No locking.** Two concurrent builds of the same spec will race on
   `prepare_output_dir`. Fine for a single-user prototype, not fine for
   anything else.
6. **No garbage collection.** Old store paths (like the libgreet/hello
   pairs still sitting in your test store from the rebuild above) are
   never cleaned up. Real GC needs to scan live "roots" (profiles,
   running systems) and walk the reference graph — which itself requires
   scanning binaries for store-path strings, the same trick Nix uses.
7. **Truncated hash, hex not base32.** Fine for a prototype; worth
   revisiting if you want shorter, more shell-friendly paths like Nix's.

## What's next, mapped back to the pipeline

This covers stage 3 (sandboxed builder) and stage 4 (content-addressed
store) end to end. In order, I'd build:

- A real package-definition language + evaluator (stage 1-2) — this is
  where Starlark or a restricted Lua is worth strong consideration
  instead of inventing your own lazy functional language from scratch.
- Fixed-output derivations, so source fetching has a legitimate escape
  hatch from the sandbox without breaking the reproducibility guarantee.
- The service-module abstraction that compiles one declared service down
  to both a runit `run` script and a systemd unit file.
- A "system closure" concept that composes a whole bootable system
  (kernel + packages + services) as one more derivation, plus bootloader
  generations for atomic switch/rollback.
