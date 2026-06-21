#!/bin/sh
# Demonstrates dependency wiring: build libgreet, then patch its real
# store path into hello's spec as a dependency, then build hello against it.
set -e
BIN="$(dirname "$0")/target/release/kbuild"

echo "== building libgreet ==" >&2
LIBGREET_PATH=$("$BIN" "$(dirname "$0")/examples/libgreet/build.toml")
echo "libgreet -> $LIBGREET_PATH" >&2

sed "s|@LIBGREET_PATH@|$LIBGREET_PATH|g" \
    "$(dirname "$0")/examples/hello/build.toml.template" \
    > "$(dirname "$0")/examples/hello/build.toml"

echo "== building hello (depends on libgreet) ==" >&2
HELLO_PATH=$("$BIN" "$(dirname "$0")/examples/hello/build.toml")
echo "hello -> $HELLO_PATH" >&2

echo "== running it ==" >&2
"$HELLO_PATH/bin/hello"
