+++
status = "closed"
opened = "2026-02-25"
closed = "2026-02-25"
+++

# Issue 643: Zig Profile Server (Take 2)

## Goal

Same as Issue 642: rewrite the Chromium Profile Server in Zig. But this time,
the Zig code lives inside `chromium/src/` and is built as part of the Chromium
build. `autoninja` produces a complete, working app bundle in a single step — no
separate `zig build`, no `cp`, no `codesign`.

## Background

Issue 642 proved the Zig-to-Chromium bridge works (Experiments 1–2) but failed
to deploy the Zig binary into a working app bundle (Experiments 3–5). Every
failure came from the same root cause: trying to combine outputs from two
separate build systems (`autoninja` for the bundle, `zig build` for the binary)
into a single coherent app bundle. The two build systems produce binaries with
different code signing properties, different bundle expectations, and different
path resolution behavior.

The lesson: don't fight the Chromium build system. If the Zig code lives inside
`chromium/src/` and `autoninja` builds everything, the app bundle is correct by
construction. No post-build surgery. No codesigning. No path mismatches.

## What Issue 642 proved

- **Experiment 1:** Zig can dlopen the Chromium framework, resolve C API
  symbols, register callbacks, and drive ContentMain. Google.com loads in a
  Shell window.
- **Experiment 2:** Zig can create WebContents directly (no Shell window) and
  receive stable CAContext IDs from the persistent compositor.
- **Experiment 3 (partial):** The XPC gateway code works — when launched
  manually from the terminal, the Zig server connects to the gateway, creates a
  BrowserContext, and sends `server_register`. The code is correct; only the
  deployment failed.

## What Issue 642 failed at

Three deployment approaches, three different failures:

1. Copy Zig binary into autoninja bundle + `codesign --deep` → code signing
   killed the process (`linker-signed` vs full adhoc mismatch).
2. `zig build` assembles bundle with symlinked framework → `realpath()` resolves
   symlink, breaks Chromium's `GetContentsPath()` DCHECK.
3. `zig build` assembles bundle with copied framework → `GetContentsPath()`
   DCHECK still fails, bundle structure doesn't match Chromium's expectations.

All three failures disappear if `autoninja` builds the entire app bundle
including the Zig binary.

## Approach

Move the Zig source files into `chromium/src/content/zig_profile_server/` next
to the existing C++ shim. Add a GN `action()` target that invokes the Zig
compiler as part of the `autoninja` build. The GN target produces the main
executable, which GN then packages into the app bundle alongside the framework
and Helpers — exactly like it does today for the C++ executable.

### Files to move

The uncommitted Zig code from Issue 642 moves into the Chromium fork:

| From (main repo)        | To (chromium/src/content/zig_profile_server/)    |
| ----------------------- | ------------------------------------------------ |
| `browser/build.zig`     | Not needed — GN invokes Zig directly             |
| `browser/build.zig.zon` | Not needed — no Zig package manager in this path |
| `browser/src/main.zig`  | `main.zig`                                       |

The `zig_objc` dependency (ObjC blocks for XPC handlers) needs a different
solution since there's no `build.zig.zon`. Options:

- Vendor the `zig_objc` source into the Chromium tree
- Use `@cImport` to call the ObjC block runtime directly (avoiding the
  dependency)
- Download the dependency as a GN `action()` step

### GN integration

GN doesn't know how to compile `.zig` files natively, but it can run arbitrary
commands via `action()`. The approach:

```gn
action("zig_profile_server_exe") {
  script = "//content/zig_profile_server/build_zig.py"
  sources = [ "//content/zig_profile_server/main.zig" ]
  outputs = [ "$root_out_dir/zig_profile_server" ]
  args = [
    "--zig", "zig",
    "--source", rebase_path("main.zig", root_build_dir),
    "--output", rebase_path("$root_out_dir/zig_profile_server", root_build_dir),
  ]
}
```

The Python script wraps `zig build-exe` with the right flags (rpath, libc
linking, target triple). The resulting binary is then referenced by the existing
`zig_profile_server` bundle target as the main executable.

This is the same pattern Chromium uses for other non-C++ build steps (e.g.,
generating protocol buffers, building Rust targets via `gn_rs`).

### What stays in the main repo

- `gui/src/apprt/xpc.zig` — the GUI's XPC module (unchanged, points at
  `chromium/src/out/Default/Zig Profile Server.app`)
- `docs/issues/0643-zig-profile-server-2.md` — this document
- `browser/` — can be cleaned up after the code is moved

### Build workflow

```bash
# Single command builds everything: C++ shim, Zig binary, app bundle
cd chromium/src && autoninja -C out/Default zig_profile_server

# GUI (unchanged)
cd gui && zig build && open zig-out/TermSurf.app
```

## Stages

Same stages as Issue 642, but the deployment problem is solved from the start.

### Stage 1: GN builds Zig binary into app bundle

Move `main.zig` into the Chromium tree. Write the GN `action()` and Python
wrapper. `autoninja` produces a complete app bundle with the Zig executable.
Verify it launches standalone (Experiment 2 equivalent).

### Stage 2: XPC gateway

Port the XPC code from the Issue 642 `main.zig` (already written, already tested
from the terminal). Resolve the `zig_objc` dependency. Connect to the GUI's XPC
gateway, receive `create_tab`, send back `ca_context_id`. Web content renders in
the terminal pane.

### Stage 3: Input forwarding

Port mouse, keyboard, and scroll event forwarding. The Zig code receives XPC
input messages and calls the C shim's `ts_forward_*` functions.

### Stage 4: WebContents observation

Port URL, title, loading state, and cursor change observation. The C++ shim
fires callbacks into Zig, which sends XPC messages back to the GUI.

### Stage 5: Navigation and remaining features

Port navigation actions, new-tab link interception, dock icon hiding, focus
management, and auto-exit.

### Stage 6: Replace chromium_profile_server

Once the Zig Profile Server has feature parity, switch the GUI to connect to it
instead of the C++ profile server. Remove the old `chromium_profile_server`
target.

## Chromium Branch

`146.0.7650.0-issue-643` — forked from `146.0.7650.0-issue-642` to preserve the
C++ shim from Issue 642's Experiments 1–2.

## Experiments

### Experiment 1: GN Builds the Zig Binary

Prove that `autoninja` can compile a Zig source file and produce a working app
bundle. This is the 642 Experiment 2 result (standalone WebContents + CAContext
ID) reproduced through the Chromium build system. No XPC code — that's Stage 2.
No `zig_objc` dependency — that's only needed for XPC blocks.

One variable changes from the working 642 Experiment 2: the binary is built by
GN instead of `zig build`. Everything else stays the same.

#### Chromium branch

Create `146.0.7650.0-issue-643` from `146.0.7650.0-issue-642`.

#### Files to add

All in `chromium/src/content/zig_profile_server/`:

**`main.zig`** — Copy from the 642 Experiment 2 version of
`browser/src/main.zig` (the standalone version, before XPC was added). This is
the version that dlopen's the framework, creates a BrowserContext, calls
`ts_create_web_contents("https://google.com", 1280, 720)`, and prints
`ca_context_id` to stderr. ~160 lines, no external dependencies.

**`build_zig.py`** — Python wrapper script that GN's `action()` invokes. Calls
`zig build-exe` with the right flags:

```python
#!/usr/bin/env python3
import argparse, subprocess, sys

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--zig', default='zig')
    parser.add_argument('--source', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()

    cmd = [
        args.zig, 'build-exe',
        args.source,
        '-lc',
        '-rpath', '@executable_path/../Frameworks',
        '-femit-bin=' + args.output,
    ]
    sys.exit(subprocess.call(cmd))

if __name__ == '__main__':
    main()
```

**`BUILD.gn`** — Modify the existing BUILD.gn to add:

```gn
action("zig_profile_server_exe") {
  script = "build_zig.py"
  sources = [ "main.zig" ]
  outputs = [ "$root_out_dir/zig_profile_server" ]
  args = [
    "--zig", "zig",
    "--source", rebase_path("main.zig"),
    "--output", rebase_path("$root_out_dir/zig_profile_server"),
  ]
}
```

Then modify the existing bundle target to use this binary as the main executable
instead of the C++ one. The exact mechanism depends on how the current
`zig_profile_server` target is structured — check the existing BUILD.gn to see
how the app bundle is assembled and where the executable comes from.

#### What NOT to change

- The C++ shim (`content_api_shim.h`, `content_api_shim.cc`) — unchanged
- The framework target — unchanged
- The app bundle structure (Info.plist, Helpers, Frameworks/) — unchanged

#### Build

```bash
cd chromium/src
autoninja -C out/Default zig_profile_server
```

Single command. If this works, the output at
`out/Default/Zig Profile Server.app` is a complete, correctly-signed app bundle
with the Zig binary as the main executable.

#### Verification

```bash
open "chromium/src/out/Default/Zig Profile Server.app"
```

Expected output on stderr:

```
[ZigProfileServer] Standalone mode
[ZigProfileServer] Created persistent compositor
[ZigProfileServer] Created WebContents, navigating to https://google.com
ca_context_id=<nonzero>
```

Same result as 642 Experiment 2: no Shell window, nonzero CAContext ID, no
crash. The only difference is the binary was built by `autoninja` instead of
`zig build`.

#### Result: Pass

`autoninja -C out/Default zig_profile_server` builds the full app bundle in a
single command. The GN `action()` invokes `zig build-exe`, a second `action()`
swaps the Zig binary over the C++ launcher in the app bundle.

Output from launching via the terminal:

```
[ZigProfileServer] Created persistent compositor
[ZigProfileServer] Set parent_ui_layer_ on view
[ZigProfileServer] Created WebContents, navigating to https://google.com
ca_context_id=4090419826
```

The binary has `flags=0x20002(adhoc,linker-signed)` — the correct code signing
produced natively by the Zig compiler. No `codesign` step needed. The app also
launches correctly via `open`, with the full Chromium process tree (GPU,
Network, Storage, and Renderer helpers).

This solves the root cause of all Issue 642 deployment failures: the binary is
built and bundled by `autoninja`, so the app bundle is correct by construction.

### Experiment 2: XPC Gateway

Connect the Zig profile server to the GUI via XPC. The GUI spawns the server,
sends `create_tab`, the server creates WebContents and sends back
`ca_context_id`. Web content renders in the terminal pane.

The XPC code was already written and tested in Issue 642 Experiment 3 — it
worked when launched manually from the terminal. The only failure was
deployment, which Experiment 1 solved. This experiment ports that XPC code into
the `autoninja`-built binary.

#### The `zig_objc` problem

XPC event handlers require ObjC blocks. The 642 code used
`objc.Block(struct{}, .{xpc_object_t}, void)` from the `zig_objc` package. But
inside the Chromium tree there's no `build.zig.zon` — `zig build-exe` via the GN
`action()` can't use the Zig package manager.

**Solution: inline the block implementation.** The ObjC block ABI is small and
stable. A block is an extern struct with 5 fields (`isa`, `flags`, `reserved`,
`invoke`, `descriptor`) plus any captures. The runtime functions are
`_Block_copy` and `_Block_release`. For our use case (one block type, no
captures, stack-allocated, passed to `xpc_connection_set_event_handler` which
copies it internally), we need:

1. A `BlockLiteral` extern struct matching the ObjC block layout
2. A `BlockDescriptor` extern struct with `reserved`, `size`, and optionally
   `copy_helper`/`dispose_helper`
3. The `_NSConcreteStackBlock` extern symbol (block ISA for stack blocks)
4. An invoke function with `callconv(.c)` whose first argument is
   `*const BlockLiteral`

No `_Block_copy`/`_Block_release` calls needed — the XPC runtime copies the
block when we pass it to `xpc_connection_set_event_handler`.

This is ~30 lines of Zig, replacing the entire `zig_objc` dependency.

#### Changes

**`chromium/src/content/zig_profile_server/main.zig`** — Replace the Experiment
1 standalone code with the full XPC gateway version. Specifically:

1. Remove `const objc = @import("objc")` and the `EventBlock` type
2. Add inline block implementation (~30 lines):
   - `BlockDescriptor` extern struct
   - `BlockLiteral` extern struct
   - `extern const _NSConcreteStackBlock: anyopaque`
   - `makeEventBlock(invoke_fn)` helper that returns a `BlockLiteral`
3. Add XPC extern declarations (same as 642 Experiment 3):
   `xpc_connection_create_mach_service`, `xpc_connection_set_event_handler`,
   `xpc_connection_resume`, `xpc_connection_send_message`,
   `xpc_dictionary_create`, `xpc_dictionary_set_string`,
   `xpc_dictionary_set_uint64`, `xpc_dictionary_get_string`,
   `xpc_dictionary_get_uint64`, `xpc_get_type`, `dispatch_queue_create`,
   `_xpc_type_dictionary`, `_xpc_type_error`
4. Add arg parsing (`--xpc-service`, `--user-data-dir`)
5. Add XPC state globals (`g_gateway`, `g_browser_ctx`, `g_xpc_service`,
   `g_user_data_dir`)
6. Add tab mapping (`TabEntry`, `wc_to_pane`, `findTabByWc`, `findFreeTab`)
7. Rewrite `onInitialized`: if `--xpc-service` is set, create BrowserContext,
   connect to gateway, send `server_register`. Otherwise fall back to standalone
   mode.
8. Rewrite `onCAContextChanged`: look up pane_id from tab map, send `ca_context`
   XPC message to GUI.
9. Add `xpcEventHandler` dispatch: handle `create_tab`, log-and-ignore
   everything else.
10. Rewrite `onShutdown`: destroy all tracked WebContents.

The logic is identical to `browser/src/main.zig` from 642 Experiment 3 — only
the block type changes from `objc.Block(...)` to the inlined `BlockLiteral`.

**`gui/src/apprt/xpc.zig`** — Change the server path (line 719) to point at the
`autoninja` output:

```
"{s}/dev/termsurf/chromium/src/out/Default/Zig Profile Server.app/Contents/MacOS/Zig Profile Server"
```

**No other files change.** The GN targets, `build_zig.py`, `swap_executable.py`,
C++ shim, framework, and app bundle structure are all unchanged from Experiment

1.

#### Build

```bash
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default zig_profile_server

cd ../../gui && zig build
```

#### Verification

1. `open gui/zig-out/TermSurf.app`
2. Type `web google.com` in a terminal pane
3. GUI spawns Zig Profile Server (check `ps aux | grep "Zig Profile"`)
4. Server logs: `XPC mode: connecting to com.termsurf.xpc-gateway`
5. Server logs: `server_register sent profile=default`
6. Server logs: `create_tab url=https://google.com`
7. Server logs: `ca_context_id=<nonzero>`
8. **Google.com renders in the terminal pane**

Pass criteria: web content renders in the terminal pane via the Zig profile
server built by `autoninja`. Full pipeline: GUI → XPC → Zig → C++ shim →
Chromium → GPU → CAContext → XPC → GUI → CALayerHost → display.

Not tested in this experiment: input forwarding, resize, navigation, title/URL
sync, destroy_tab. The page renders but is not interactive.

#### Result: Fail

The server process spawns correctly — `ps aux` confirms it running with the
right arguments (`--xpc-service=com.termsurf.xpc-gateway`,
`--user-data-dir=...`). The full Chromium process tree starts (GPU, Network
helpers). But no web page renders in the terminal pane.

The standalone mode (no `--xpc-service`) still works — Experiment 1's
`ca_context_id` output is verified. The failure is in the XPC path. Possible
causes:

1. The inline ObjC block ABI doesn't work — the XPC event handler never fires
2. The XPC connection to the gateway fails silently
3. `create_tab` is received but WebContents creation fails
4. `ca_context` is sent back but the GUI doesn't process it

No stderr output was captured — the GUI spawns the server via
`std.process.Child` which doesn't capture stderr, and Zig's `std.debug.print`
doesn't go to Chromium's `--log-file`. Root cause unknown without further
debugging.

## Conclusion

Issue 643 solved the deployment problem from Issue 642. Experiment 1 proved that
`autoninja` can compile Zig source via a GN `action()` and produce a correct,
launchable app bundle in a single command. The Zig binary gets `linker-signed`
code signing natively — no `codesign` step, no manual copying, no bundle
surgery. The standalone mode works: `ca_context_id` is returned, the full
Chromium process tree starts.

But Experiment 2 (XPC gateway) failed. The server spawns with the right
arguments, Chromium initializes, but no web page renders in the terminal pane.
The failure is somewhere in the XPC pipeline — the inline ObjC block ABI, the
gateway connection, the message dispatch, or the GUI's handling of the response.
Without stderr visibility from the spawned process, the root cause is unknown.

### What worked

- **GN `action()` builds Zig code.** The pattern — Python wrapper invoking
  `zig build-exe`, a second action swapping the binary into the app bundle — is
  proven and reusable.
- **Standalone mode.** The Zig binary creates WebContents, gets CAContext IDs,
  and runs the full Chromium stack. The Zig-to-Chromium bridge works.
- **Code signing.** The Zig compiler produces `linker-signed` binaries that
  macOS accepts without re-signing. This was the root cause of all Issue 642
  failures.

### What didn't work

- **XPC gateway.** The same XPC code that worked in Issue 642 (when launched
  manually from the terminal) doesn't work when built by `autoninja` and spawned
  by the GUI. The inline ObjC block ABI is untested — it may not match what
  `xpc_connection_set_event_handler` expects. Or the failure may be elsewhere in
  the pipeline.

### Across Issues 642 and 643

Two issues, 7 experiments, and the Zig Profile Server still doesn't work
end-to-end. The pattern is consistent: standalone Chromium works (Experiments
642-1, 642-2, 643-1), but XPC integration fails (642-3, 643-2) or deployment
fails (642-4, 642-5). The XPC code was only ever verified working once — from a
manually-launched terminal process in Issue 642 Experiment 3 — and that test was
partial (the server connected and sent `server_register`, but `create_tab` was
never received because the GUI wasn't involved).

The fundamental problem may be that rewriting the profile server in Zig is
solving the wrong problem. The existing C++ profile server works. The
Zig-to-Chromium bridge adds complexity (dlopen, function pointers, ObjC blocks,
build system integration) without clear user-facing benefit. A different
approach may be more productive.
