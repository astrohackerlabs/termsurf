#!/usr/bin/env bash
# Issue 802 / Exp 6 — build libroastty + assemble RoasttyKit.xcframework (the link
# artifact the renamed app consumes), mirroring GhosttyKit's structure. Gitignored.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
CONF="${1:-debug}"
if [ "$CONF" = release ]; then cargo build -p roastty --release; else cargo build -p roastty; fi
LIB="$ROOT/target/$CONF/libroastty.a"; [ -f "$LIB" ] || { echo "no $LIB"; exit 1; }
HDRS="$(mktemp -d)"; cp "$ROOT/roastty/include/roastty.h" "$ROOT/roastty/include/module.modulemap" "$HDRS/"
OUT="$ROOT/roastty/macos/RoasttyKit.xcframework"; rm -rf "$OUT"; mkdir -p "$ROOT/roastty/macos"
xcodebuild -create-xcframework -library "$LIB" -headers "$HDRS" -output "$OUT" >/dev/null
rm -rf "$HDRS"; echo "built: $OUT"
