#!/bin/sh
# Builds the "hello" example via the evaluator and roots the result, so a
# `kgc` run won't collect it. keval resolves packages/hello's depends_on
# entry (libgreet) on its own, in correct order — no manual store-path
# wiring needed.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

HELLO_PATH=$("$ROOT/target/release/keval" "$ROOT/packages" hello --root hello)
echo "hello -> $HELLO_PATH" >&2
echo "== running it ==" >&2
"$HELLO_PATH/bin/hello"
