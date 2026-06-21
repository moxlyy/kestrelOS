#!/bin/sh
set -e
mkdir -p "$out/lib" "$out/include"
$CC -c greet.c -o greet.o
ar rcs "$out/lib/libgreet.a" greet.o
cp greet.h "$out/include/greet.h"
