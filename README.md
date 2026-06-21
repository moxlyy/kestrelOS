# kbuild / kservice — a tiny reproducible builder + dual-init service compiler

Two small Rust tools, both tested against real systems, not just sketched:

- **`kbuild`** — hash a build's inputs, build it in an isolated sandbox,
  cache the result at a path derived from that hash. Now also supports
  fixed-output (network-fetch) derivations.
- **`kservice`** — compile one declared service into both a runit `run`
  script and a systemd `.service` unit.

This still has no package language or dependency evaluator. You
hand-write `build.toml` files and hand-wire dependencies. That remains the
single biggest missing piece — see "What's next."

## What changed since the first prototype

**Store sealing is now actually root-resistant.** The original
`chmod -R a-w` only stopped non-root writes — verified by testing: running
as root, I could still write into a "sealed" path. `store::seal_readonly`
now uses `chattr -R +i` (the filesystem immutable attribute), which I
re-verified blocks writes, new files, and deletions for *every* uid,
including root. It falls back to chmod with a loud warning if the
filesystem doesn't support it (some overlay/tmpfs setups don't).
`store::unseal` reverses this for the garbage collector that doesn't exist
yet.

**Builds and fetches now run as an unprivileged uid inside the sandbox**
(`--unshare-user --uid 65534 --gid 65534`). Worth being precise about what
this does and doesn't buy you: a process bind-mounting into a directory
that lives outside its own user namespace (like our store output dir)
still has its files attributed to the *real* underlying uid of whoever
launched bwrap — which is root, since the daemon runs as root. So this
doesn't change file ownership in the store. What it does do is stop the
build/fetch process itself from doing privileged things (binding low
ports, claiming capabilities, etc.) if a build script is malicious or
just buggy. Real privilege separation for the daemon itself (so root
isn't the one running arbitrary build scripts at all) is still an open
problem — Nix solves it with a pool of dedicated `nixbld` users and a
daemon that's careful never to run untrusted code as root in the first
place. That's a bigger rework than this prototype attempts.

**Fixed-output (fetch) derivations** (`[fetch]` instead of `[build]` in a
spec). This is the *only* code path that leaves networking enabled in the
sandbox — every `[build]` derivation still gets zero network, always. The
store path is named after a hash *you* declare up front; after fetching,
the real content hash must match exactly or the build fails and cleans up
after itself. Tested both directions:
- Correct hash → fetches, verifies, caches.
- Deliberately wrong hash → rejected, no leftover store path.

**The dual-init service compiler** (`kservice`). One `service.toml`
produces a runit `run` script and a systemd unit. Both were validated
against real tooling, not just read by eye:
- The generated runit script was run under a real `runsv` process and
  actually executed, looping and printing output as expected.
- The generated systemd unit passes `systemd-analyze verify` cleanly
  (when it doesn't reference a dependency unit that simply isn't
  installed on the validating machine — see the two example specs below).

## Try it

```sh
cargo build --release

export KBUILD_STORE=$HOME/kestrel-test-store   # or omit and use /kestrel/store as root

# package builds, same as before
./build-all.sh

# fixed-output fetch derivation — pulls a real file over HTTPS,
# verified against a hash declared in the spec
./target/release/kbuild examples/linux-copying/build.toml

# dual-init service compiler
./target/release/kservice examples/services/hello-daemon-standalone.toml --out /tmp/svc-out
cat /tmp/svc-out/runit/hello-daemon/run
cat /tmp/svc-out/systemd/hello-daemon.service
```

`examples/services/hello-daemon.toml` includes `depends_on = ["network"]`
to show dependency wiring; it will fail `systemd-analyze verify` on a
machine without a `network.service` unit installed — that's correct
systemd behavior, not a bug. `hello-daemon-standalone.toml` has no
dependencies and verifies cleanly anywhere.

## Fetch spec format

```toml
name = "linux-copying"
version = "6.6"

[fetch]
url = "https://raw.githubusercontent.com/torvalds/linux/v6.6/COPYING"
sha256 = "fb5a425bd3b3cd6071a3a9aff9909a859e7c1158d54d32e07658398cd67eb6a0"
```

## Service spec format

```toml
name = "hello-daemon"
description = "loops the kestrelOS hello example"
exec = "/bin/sh -c 'while true; do echo hi; sleep 2; done'"
depends_on = ["postgres"]   # maps to postgres.service on the systemd side
restart = "always"          # or "no"
```

`restart = "no"` is honored as-is on the systemd side (`Restart=no`). On
the runit side it's flagged with a comment rather than faked — runit
always respawns a `run` script that exits; real one-shot semantics need a
separate `down` file plus `sv once`, which this generator doesn't attempt
to set up automatically.

## Known limitations, in the order I'd tackle them

1. **No package language or evaluator.** Still the big one. Dependency
   graphs are hand-resolved (see `build-all.sh`'s `sed` substitution).
2. **The daemon itself still runs as root.** The uid-drop inside the
   sandbox is real but partial, as explained above. True privilege
   separation needs dedicated build users and a daemon designed around
   never executing untrusted code with its own (root) credentials.
3. **The toolchain is still borrowed from the host** (`/usr`, `/bin`,
   `/lib` bind-mounted read-only). A real stdenv builds its own.
4. **`kservice`'s dependency mapping is service-only.** Every entry in
   `depends_on` becomes `<name>.service`; there's no way to depend on a
   systemd `.target` instead, and no equivalent dependency-ordering
   concept on the runit side at all (runit relies on convention and
   `sv start` ordering in stage 2 scripts, not a generic dependency
   graph — that gap is real and unresolved here).
5. **No locking.** Concurrent builds of the same spec will race.
6. **No garbage collection.** `store::unseal` exists for this purpose but
   nothing calls it yet. Old store paths accumulate forever.
7. **Truncated hash, hex not base32.** Cosmetic; revisit if path length
   or shell-safety becomes a real concern.

## What's next, mapped back to the pipeline

- A real package-definition language + evaluator (Starlark or a
  restricted Lua are still where I'd start, over inventing a new lazy
  functional language from scratch).
- Garbage collection: walk live roots, scan store contents for
  store-path string references (the same trick Nix uses), `unseal` +
  remove anything unreachable.
- A real stage-2-script generator for runit (the boot-sequence side, not
  just individual services) and the systemd-target equivalent, so a full
  "system closure" can actually decide what starts at boot under either
  backend.
- A "system closure" derivation that composes kernel + packages +
  services into one buildable, switchable unit, plus bootloader
  generations for atomic switch/rollback.
