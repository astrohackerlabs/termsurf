# Issue 705: Browser bindings for Chromium (continued)

## Goal

Same goal as Issue 704: create standalone browser binaries (Roamium, Zoomium,
Plusium) that wrap Chromium's Content API through a shared C library
(`libtermsurf_content`). Each binary speaks the TermSurf IPC protocol (Unix
sockets + length-prefixed protobuf). The TUI's `--browser` flag selects which
binary to use: `web google.com --browser plusium`. After all three work, Roamium
becomes the default and the Chromium Profile Server is retired.

## Background

### What Issue 704 accomplished

Issue 704 ran five experiments across two sessions:

1. **C library extraction** — Confirmed `libtermsurf_content` already provides a
   clean C API boundary (`chromium/src/content/libtermsurf_content/`). No new
   library needed.

2. **Profile server dependency audit** — Verified `libtermsurf_content` has no
   dependencies on the Chromium Profile Server's forked code. It links against
   stock `content/shell`.

3. **Plusium C++ binary** — Built a working C++ binary
   (`content/plusium/plusium_main.cc`) with its own `BUILD.gn` and protobuf IPC.
   Compiles and links successfully.

4. **`--browser` flag** — Added end-to-end support for selecting browser
   binaries: proto schema changes (`browser` field on SetOverlay/
   SetDevtoolsOverlay, `browsers` on HelloReply), TUI `--browser` CLI flag, GUI
   browser registry with composite `(profile, browser)` server keys, and dynamic
   binary path resolution in `spawnServerProcess()`. All compiles clean.

5. **Skip bundle path check** — Overrode `BasicStartupComplete()` in
   `TsMainDelegate` to skip `EnsureCorrectResolutionSettings()`, which crashes
   with a DCHECK when the binary isn't inside a `.app` bundle. Fixed the crash.

### Where it stopped

Plusium starts without crashing but a Content Shell window appears on screen
because stock `shell_platform_delegate_mac.mm` has no `--hidden` support.

The existing Chromium Profile Server solved this with a **forked**
`shell_platform_delegate_mac.mm` in `content/chromium_profile_server/browser/`
that checks for a `--hidden` flag:

```objc
if (base::CommandLine::ForCurrentProcess()->HasSwitch(switches::kHidden)) {
    [window setAlphaValue:0.0];
    [window orderWindow:NSWindowBelow relativeTo:0];
} else {
    [window makeKeyAndOrderFront:nil];
}
```

This makes the window fully transparent and orders it behind all other windows.
`orderOut:` would detach the compositor (it triggers
`NSWindowDidChangeOcclusionStateNotification` which sets
`render_widget_host_is_hidden_ = true`). The `setAlphaValue:0` trick keeps the
window in the window list so the compositor stays active and CAContext survives
navigation.

The GUI already passes `--hidden` to server processes (line 1004 of `xpc.zig`),
but stock Content Shell doesn't recognize it.

### Chromium branch

All Chromium work is on `146.0.7650.0-issue-704` (4 commits: C library
extraction, profile server dependency removal, Plusium binary, bundle path
DCHECK fix). This issue continues on a new branch created from it.

### Code already in place

**Chromium fork (`146.0.7650.0-issue-704`):**

- `content/libtermsurf_content/` — C library with `TsMainDelegate` (overrides
  `BasicStartupComplete()`), `TsBrowserClient`, `TsBrowserMainParts`, tab
  management, input forwarding, persistent compositor, CALayerHost bridge
- `content/plusium/plusium_main.cc` — C++ binary with socket IPC, protobuf
  message dispatch, tab registry, callback wiring
- `content/plusium/BUILD.gn` — Build target linking `libtermsurf_content`
- `content/plusium/termsurf.proto` — Local copy of proto schema

**Main repo (`main`):**

- `proto/termsurf.proto` — `browser` field on SetOverlay/SetDevtoolsOverlay,
  `browsers` on HelloReply
- `tui/src/main.rs` — `--browser` CLI flag, forwarded to overlay/devtools
  messages
- `tui/src/ipc.rs` — `browser` parameter on `send_set_overlay()` and
  `send_set_devtools_overlay()`
- `gui/src/apprt/xpc.zig` — Browser registry (`browser_paths` map), composite
  `(profile, browser)` server keys, `resolveBrowserPath()`,
  `initBrowserRegistry()`, `spawnServerProcess()` with dynamic binary path
- `gui/src/protobuf/termsurf.pb-c.{h,c}` — Regenerated for new proto fields

## Ideas for future experiments

These are rough ideas for after Plusium is working end-to-end. Each will be
designed when the previous one is complete.

1. **End-to-end Plusium verification** — Once `--hidden` is fixed, run the full
   test matrix: browse, navigate, resize, mouse input, keyboard input, scroll,
   DevTools, dark mode, multiple profiles. Verify Plusium is functionally
   equivalent to the Chromium Profile Server.

2. **Build Roamium (Rust)** — Create a Rust crate that links
   `libtermsurf_content` via FFI (`bindgen` or manual declarations). The main
   challenge is build system integration: Cargo needs to find the Chromium-built
   static library and headers. Verify equivalence.

3. **Build Zoomium (Zig)** — Create a Zig package that links
   `libtermsurf_content` via `@cImport`. Same build system challenge as Roamium
   but for Zig. Verify equivalence.

4. **Make Roamium the default** — Once all three work, switch the default from
   Chromium Profile Server to Roamium. Update the GUI's `initBrowserRegistry()`
   to list Roamium first.

5. **Retire the Chromium Profile Server** — Delete `chromium_profile_server/`
   from the active Chromium branch once all three bindings are verified
   equivalent. This removes ~100 forked files and ~1050 lines of
   TermSurf-specific code.

## Experiments

### Experiment 1: Add `--hidden` support to stock Content Shell

Patch `content/shell/browser/shell_platform_delegate_mac.mm` to recognize the
`--hidden` flag. This is the same `setAlphaValue:0` +
`orderWindow:NSWindowBelow` trick the Profile Server uses, applied to the stock
file that Plusium links against.

#### What to change

**`content/shell/common/shell_switches.h`** — Add:

```cpp
inline constexpr char kHidden[] = "hidden";
```

**`content/shell/browser/shell_platform_delegate_mac.mm`** — In the function
that shows the window (the line `[window makeKeyAndOrderFront:nil]`), wrap it:

```objc
if (base::CommandLine::ForCurrentProcess()->HasSwitch(switches::kHidden)) {
    [window setAlphaValue:0.0];
    [window orderWindow:NSWindowBelow relativeTo:0];
} else {
    [window makeKeyAndOrderFront:nil];
}
```

Also add the `#include` for `shell_switches.h` and `base/command_line.h` if not
already present.

#### Verification

1. `autoninja -C out/Default plusium` — compiles.
2. Run `web google.com --browser plusium` — no Content Shell window appears on
   screen, page loads in the terminal.
3. Verify default browser (no `--browser` flag) still works.

#### Result: Failure

The `--hidden` patch works — no Content Shell window appears on screen. But
Plusium's child processes (renderer, GPU) crash with:

```
FATAL:content/app/content_main_runner_impl.cc:1002]
Check failed: sandbox::Seatbelt::IsSandboxed().
```

Chromium's multi-process architecture on macOS requires child processes to be
sandboxed via `Seatbelt`. The sandbox profile is read from the app bundle's code
signature. Plusium is a bare executable with no bundle, so child processes can't
initialize the sandbox and crash.

The existing Profile Server avoids this because it's packaged as
`Chromium Profile Server.app` — a signed `.app` bundle with entitlements.

**Solution:** Pass `--no-sandbox` when spawning Plusium. Electron apps ship with
`--no-sandbox` by default — it's proven safe for embedders that don't need
Chromium's full browser-grade sandbox. TermSurf's use case (developers browsing
localhost and docs in a terminal) has a narrower attack surface than a
general-purpose browser.

### Experiment 2: Pass `--no-sandbox` to Plusium

Add `--no-sandbox` to the spawn args for non-bundled browser binaries. The GUI
already constructs the argument list in `spawnServerProcess()`. The simplest
approach: always pass `--no-sandbox` for all browser binaries (the Profile
Server's bundle entitlements override it, so it's harmless there).

#### What to change

**`gui/src/apprt/xpc.zig`** — In `spawnServerProcess()`, add `--no-sandbox` to
the argv array passed to the child process. It goes alongside `--hidden`,
`--enable-logging`, and `--log-file`.

#### Verification

1. `cd gui && zig build` — compiles.
2. Run `web google.com --browser plusium` — page loads in the terminal, no
   Content Shell window, no sandbox crash.
3. Verify default browser (no `--browser` flag) still works.
4. Check `~/.local/state/termsurf/chromium-server.log` — no sandbox errors.

#### Result: Failure (partial progress)

The `--no-sandbox` fix works — Plusium starts successfully. The GUI log
confirms:

```
[libtermsurf_content] Initialized, firing callback
DevTools listening on ws://127.0.0.1:56508/devtools/browser/...
```

No sandbox crash, no Content Shell window. Both the Experiment 1 (`--hidden`)
and Experiment 2 (`--no-sandbox`) fixes are working.

But the page still doesn't render. Plusium initializes but the TUI times out
waiting for a response. The IPC handshake (socket connect → ServerRegister →
CreateTab → TabReady → CaContext) is breaking somewhere downstream.

The GUI's Zig logs (`std.log.scoped(.ipc)`) don't appear in stdout/stderr — they
use Ghostty's internal logging system. Without these logs, we can't see whether:

- Plusium connected to the socket
- The GUI received the `ServerRegister` message
- The GUI matched it to a server entry
- The GUI sent `CreateTab`
- Plusium sent back `TabReady` / `CaContext`

The next experiment needs to add debug tracing to identify where the handshake
breaks.
