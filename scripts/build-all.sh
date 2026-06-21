#!/bin/sh
# Builds the "hello" example via the evaluator. No sed, no manual
# store-path wiring — keval resolves packages/hello's depends_on
# entry (libgreet) on its own, in correct order, and runs it.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

HELLO_PATH=$("$ROOT/target/release/keval" "$ROOT/packages" hello)
echo "hello -> $HELLO_PATH" >&2
echo "== running it ==" >&2
"$HELLO_PATH/bin/hello"
