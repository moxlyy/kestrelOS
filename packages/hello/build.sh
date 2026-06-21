#!/bin/sh
set -e
mkdir -p "$out/bin"
$CC hello.c -I "$LIBGREET/include" -L "$LIBGREET/lib" -Wl,-rpath,"$LIBGREET/lib" -lgreet -o "$out/bin/hello"
