#!/bin/sh
set -e
mkdir -p "$out/lib" "$out/include"
$CC -fPIC -shared greet.c -o "$out/lib/libgreet.so"
cp greet.h "$out/include/greet.h"
