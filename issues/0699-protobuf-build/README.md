+++
status = "closed"
opened = "2026-03-03"
closed = "2026-03-03"
+++

# Issue 699: Build Protobuf-C into the GUI

## Goal

Get protobuf-c to compile and link into the GUI's final macOS binary. This
unblocks Issue 700 (replace TUI↔GUI XPC with Unix sockets + protobuf).

## Background

Issue 698 Experiment 4 attempted to integrate protobuf-c into the GUI by
referencing files outside the build root (`../proto/test-socket/`). The Zig
compiler compiled the objects, but the final Xcode link failed with undefined
symbols (`_protobuf_c_empty_string`). The objects didn't survive Ghostty's
multi-step archive pipeline.

### The build pipeline

```
SharedDeps.add(lib)          → adds C sources, include paths, system libs
                              → returns lib_list (list of dependency .a files)
lib_list.append(lib.a)       → adds the main library itself

libtool -static -o fat.a     → combines ALL .a files into one mega archive

xcodebuild -create-xcframework → wraps the fat archive + headers

Xcode links against xcframework → final binary
```

### How stb.c works

`stb.c` is a third-party C file that builds successfully through this pipeline:

```zig
// SharedDeps.zig
step.addIncludePath(b.path("src/stb"));
step.addCSourceFiles(.{ .files = &.{"src/stb/stb.c"} });
```

It lives inside `gui/` at `gui/src/stb/stb.c`, uses a path relative to the build
root, and compiles directly into `libtermsurf.a`. libtool merges that into the
fat archive. Xcode links it. Symbols resolve.

### Why Issue 698 failed

The protobuf-c files were referenced via `../proto/test-socket/` — outside the
build root. The likely cause is that `addCSourceFile` with paths outside the
build root behaves differently in the multi-architecture pipeline. The stb.c
pattern (files inside `gui/`, relative paths) works. The outside-build-root
pattern doesn't.

### The fix: copy generated files into each project

`proto/` is the source of truth for the `.proto` schema and code generation. But
each subproject needs its own copy of the generated files, inside its own build
root:

- **GUI (Zig):** Needs `termsurf.pb-c.c`, `termsurf.pb-c.h` (protoc `--c_out`),
  plus the protobuf-c runtime (`protobuf-c.c`, `protobuf-c.h`). These must live
  inside `gui/` to follow the stb.c pattern.
- **TUI (Rust):** No copy needed. Prost generates code at build time from
  `proto/termsurf.proto` via `build.rs` — it reads the `.proto` file directly.

A build script in `proto/` regenerates and copies the files when the schema
changes. The copied files are committed — they're source files from the build
system's perspective.

## Experiments

### Experiment 1: Copy protobuf-c into gui/ and build

Copy the protobuf-c runtime and generated files into `gui/src/protobuf/`, add
them to `SharedDeps.zig` using the stb.c pattern, and verify the symbols survive
the full pipeline.

#### Changes

**1. Create `gui/src/protobuf/` with all required files**

```
gui/src/protobuf/
├── protobuf-c.c            ← runtime source (protobuf-c v1.5.2)
├── protobuf-c.h            ← runtime header
├── protobuf-c/
│   └── protobuf-c.h        ← copy (for #include <protobuf-c/protobuf-c.h>)
├── termsurf.pb-c.c         ← generated from proto/termsurf.proto
└── termsurf.pb-c.h         ← generated header
```

The `protobuf-c/protobuf-c.h` subdirectory exists because the generated
`termsurf.pb-c.h` includes `<protobuf-c/protobuf-c.h>` with the subdirectory
prefix.

Source files:

- `protobuf-c.c` and `protobuf-c.h`: download from
  [protobuf-c v1.5.2](https://github.com/protobuf-c/protobuf-c/tree/v1.5.2/protobuf-c)
- `termsurf.pb-c.c` and `termsurf.pb-c.h`: generate with
  `protoc --c_out=gui/src/protobuf --proto_path=proto proto/termsurf.proto`

**2. Add to `SharedDeps.zig` using the stb.c pattern**

After the stb.c block:

```zig
// Protobuf-c (Issue 699).
step.addIncludePath(b.path("src/protobuf"));
step.addCSourceFiles(.{ .files = &.{
    "src/protobuf/protobuf-c.c",
    "src/protobuf/termsurf.pb-c.c",
} });
```

Mirrors stb.c exactly: paths inside `gui/`, relative to the build root,
`addCSourceFiles` (plural).

**3. Add a minimal Zig reference to force linking**

In `xpc.zig`, add a protobuf-c import and a function that calls one protobuf-c
symbol — just enough to force the linker to include the objects:

```zig
const pb = @cImport({
    @cInclude("termsurf.pb-c.h");
});

pub fn testProtobuf() void {
    var msg: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&msg);
}
```

**4. Create `proto/generate.sh`**

A script that regenerates and copies:

```bash
#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Generate C code from the proto schema.
protoc --c_out=gui/src/protobuf --proto_path=proto proto/termsurf.proto

echo "Generated gui/src/protobuf/termsurf.pb-c.{c,h}"
```

The protobuf-c runtime files (`protobuf-c.c`, `protobuf-c.h`) are copied once
manually and only change when upgrading the protobuf-c version.

#### Verification

```bash
cd gui && zig build
```

Then check the final archive:

```bash
nm gui/macos/TermSurfKit.xcframework/macos-arm64_x86_64/libtermsurf.a \
    | grep protobuf_c_empty_string
```

**Pass criterion:** `zig build` succeeds with no undefined symbol errors, and
`nm` shows `protobuf_c_empty_string` as defined (T or S, not U) in the final
archive.

#### Result: PASS

`zig build` succeeded with zero errors. `nm` confirms all protobuf-c symbols are
defined in the final archive:

```
0000000000027478 S _protobuf_c_empty_string
000000000000b768 S _termsurf__term_surf_message__descriptor
0000000000000430 T _termsurf__term_surf_message__free_unpacked
0000000000000030 T _termsurf__term_surf_message__get_packed_size
```

The `S` (data) and `T` (text/code) entries confirm the symbols survived the full
pipeline: `addCSourceFiles` → `libtermsurf.a` → `libtool -static` →
`libtermsurf-fat.a` → `xcodebuild -create-xcframework` → final archive.

The stb.c pattern works for protobuf-c: files inside `gui/`, relative paths,
`addCSourceFiles` (plural).

## Conclusion

Protobuf-c compiles and links into the GUI's final macOS binary. The fix was
simple: copy the protobuf-c runtime source and generated files into
`gui/src/protobuf/`, mirroring the stb.c pattern. Files inside the build root
survive the multi-step archive pipeline. Files outside it don't.

### What was added

- `gui/src/protobuf/protobuf-c.c` — runtime source (protobuf-c v1.5.2)
- `gui/src/protobuf/protobuf-c.h` — runtime header
- `gui/src/protobuf/protobuf-c/protobuf-c.h` — subdirectory copy (for
  `#include <protobuf-c/protobuf-c.h>`)
- `gui/src/protobuf/termsurf.pb-c.c` — generated from `proto/termsurf.proto`
- `gui/src/protobuf/termsurf.pb-c.h` — generated header
- `proto/generate.sh` — regenerates and copies generated files
- `SharedDeps.zig` — two lines added after the stb.c block
- `xpc.zig` — `@cImport` of `termsurf.pb-c.h` and `testProtobuf()` to force
  linking

### Next steps

Issue 700: Replace TUI↔GUI XPC with Unix domain sockets + protobuf, using these
now-proven protobuf-c bindings.
