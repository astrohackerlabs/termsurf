+++
status = "closed"
opened = "2026-02-14"
closed = "2026-02-14"
+++

# Issue 501: Two Profiles in ts5

## Goal

Build a clean two-profile browser demo in ts5: a minimal Chromium embedder
(`chromium-profile-server`) that renders webpages to IOSurfaces and sends them
via XPC, and a Swift compositor (`ts5/two-profiles/`) that receives and displays
them side by side at 60fps.

This replaces the ts4 proof-of-concept code (Issues 412–416) with
production-quality implementations that will serve as the foundation for
Ghostty's browser pane integration.

## Motivation

The ts4 experiments proved the architecture works. But the Chromium side was
built as a rapid PoC with problems that need fixing before we build on top of
it:

1. **218-file Content Shell copy.** The One Profile app is a wholesale copy of
   `content/shell/` with renamed identifiers. Most of those files are irrelevant
   (DevTools, web test infrastructure, Linux/Windows platform code). The actual
   delta we need is ~10 files.

2. **Bad naming.** "One Profile" describes an architectural constraint, not what
   the app does. It's a headless browser renderer — a profile server.

3. **Dock icon.** The `--hidden` flag hides the window with `orderOut:nil`, but
   the app is still a regular `NSApplication` that shows in the Dock. Each
   profile server adds a Dock icon, which is unacceptable.

4. **Fragile capturer attachment.** The video consumer is wired up via a
   hardcoded 2-second `PostDelayedTask` to wait for the `RenderWidgetHostView`
   to exist. This is a race condition waiting to happen.

Since the architecture is proven, we can afford to get this right.

## Background

### What's proven

| Issue | Finding                                                                                                                                           |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| 406   | Chromium Content API supports multiple `BrowserContext` instances with isolated storage. CEF ruled out (31fps ceiling).                           |
| 407   | In-process Chromium PoC: two profiles coexist, single pane at 60fps. Dual-pane hit 2fps due to visibility tracking.                               |
| 413   | Root cause: two `BrowserContext` in one process = 2fps. Two `WebContents` sharing one `BrowserContext` = 60fps. One profile per process required. |
| 414   | XPC + IOSurface Mach port transfer solves the dual-pane problem. `FrameSinkVideoCapturer` delivers IOSurfaces at 60fps even with hidden window.   |
| 415   | Same architecture in pure Swift. XPC C API, IOSurface, Metal, CADisplayLink all work natively.                                                    |

### Architecture

```
chromium-profile-server A (headless)         chromium-profile-server B (headless)
--session-id=profile-a                --session-id=profile-b
IOSurface 1600x1200 @ 60fps          IOSurface 1600x1200 @ 60fps
        |                                     |
        | XPC Mach port                       | XPC Mach port
        v                                     v
    +--------------------------------------+
    |     Swift Compositor (Metal)         |
    |  +----------------+----------------+ |
    |  |  Left viewport | Right viewport | |
    |  |  profile-a     | profile-b      | |
    |  |  60fps         | 60fps          | |
    |  +----------------+----------------+ |
    +--------------------------------------+
```

Two process types:

1. **chromium-profile-server** — Headless Chromium embedder. One instance per
   browser profile. Renders a webpage via the Content API, captures frames with
   `FrameSinkVideoCapturer`, sends IOSurface Mach ports to the compositor via
   XPC. No Dock icon, no visible window.

2. **Compositor** (`ts5/two-profiles/`) — Swift app that registers as an XPC
   Mach service, receives IOSurface Mach ports, and composites two panes side by
   side in a Metal window at 60fps.

### XPC protocol

Messages from chromium-profile-server to compositor:

- **`display_surface`** —
  `{ action: "display_surface", session_id: "<id>",
  iosurface_port: <mach_send>, width: <int64>, height: <int64> }`
- **`register`** — `{ action: "register", session_id: "<id>" }`

The compositor maps `session_id` to a pane: `"profile-b"` goes right, everything
else goes left.

## Part 1: chromium-profile-server (Chromium)

### Naming

The Chromium embedder is called `chromium-profile-server`. This continues the
"profile server" concept from ts3 (`termsurf-profile`), where each browser
profile runs in its own process. The `chromium-` prefix distinguishes it from
future profile server implementations backed by other engines (e.g.,
`spidermonkey-profile-server`, `webkit-profile-server`).

Inside the Chromium source tree, the directory follows Chromium's underscore
convention: `content/chromium_profile_server/`.

The build target is `chromium_profile_server`. The output app bundle is
`Chromium Profile Server.app`.

### Approach: link against content_shell_lib, don't copy it

The ts4 approach copied all 218 files from `content/shell/`. The better approach
is to link against `//content/shell:content_shell_lib` as a dependency and
subclass only what we need. This is the same pattern used in Issue 407's
`content/two_profiles/` directory, which worked with ~10 files.

Content Shell provides:

- `Shell` — Window and WebContents lifecycle
- `ShellBrowserContext` — Profile storage with `GetPath()`
- `ShellBrowserMainParts` — Browser lifecycle
- `ShellContentBrowserClient` — Browser client delegate
- `ShellMainDelegate` — Application entry point

We subclass the parts we need to override and leave everything else to
content_shell_lib.

### Files

```
chromium/src/content/chromium_profile_server/
├── BUILD.gn
├── profile_server_main.cc          — Entry point
├── profile_server_main_mac.cc      — macOS entry (framework loading)
├── profile_server_main_delegate.h  — Subclass ShellMainDelegate
├── profile_server_main_delegate.cc
├── profile_server_main_parts.h     — Subclass ShellBrowserMainParts
├── profile_server_main_parts.cc
├── profile_server_context.h        — Subclass ShellBrowserContext
├── profile_server_context.cc
├── profile_server_video_consumer.h — FrameSinkVideoCapturer + XPC sender
├── profile_server_video_consumer.cc
└── Info.plist                       — LSUIElement = true
```

~12 files instead of 218.

### Key classes

**`ProfileServerMainDelegate`** — Extends `ShellMainDelegate`. Overrides
`CreateContentBrowserClient()` to return our custom browser client that uses
`ProfileServerMainParts`.

**`ProfileServerMainParts`** — Extends `ShellBrowserMainParts`. Overrides
browser context creation to use `ProfileServerContext` with the
`--user-data-dir` path. Wires up `ProfileServerVideoConsumer` with proper
lifecycle hooks (not a hardcoded 2-second delay).

**`ProfileServerContext`** — Extends `ShellBrowserContext`. Overrides
`GetPath()` to return the `--user-data-dir` path, giving each instance isolated
cookies, localStorage, and cache.

**`ProfileServerVideoConsumer`** — Implements
`viz::mojom::FrameSinkVideoConsumer`. This is the core of the frame delivery
pipeline:

1. Creates a `viz::ClientFrameSinkVideoCapturer` from `HostFrameSinkManager`
2. Configures: `PIXEL_FORMAT_ARGB`, 16ms min capture period, auto-throttling
   disabled
3. Attaches to the WebContents' `FrameSinkId`
4. On `OnFrameCaptured`: extracts `IOSurfaceRef` from the GPU memory buffer,
   creates a Mach port with `IOSurfaceCreateMachPort()`, sends via XPC, signals
   `Done()` to release the buffer

Ported from `ShellVideoConsumer` in the One Profile app, but with proper
lifecycle management.

### Dock icon fix: LSUIElement

The Info.plist sets `LSUIElement = true`, making chromium-profile-server a
background app with no Dock icon and no menu bar. This is the standard macOS
mechanism for helper processes.

The risk is that `LSUIElement` might affect Chromium's compositor visibility
chain — the same chain that causes 2fps when a `BrowserContext`'s views aren't
"visible." However, `LSUIElement` only removes the Dock icon; it does not affect
window existence or NSView visibility. The existing `orderOut:nil` approach
already proved that hiding the window doesn't affect `FrameSinkVideoCapturer`
(Issue 414). `LSUIElement` is a strictly less invasive change than hiding the
window, so it should not introduce new problems. This needs verification in
Experiment 1.

### CLI flags

Same flags as the One Profile app, carried forward:

| Flag                     | Purpose                                          |
| ------------------------ | ------------------------------------------------ |
| `--hidden`               | Hide the window after creation (`orderOut:nil`). |
| `--xpc-service=<name>`   | Connect to a named XPC Mach service.             |
| `--session-id=<id>`      | Identifier sent with every frame.                |
| `--user-data-dir=<path>` | Profile storage directory.                       |
| (positional URL)         | Webpage to load.                                 |

### Chromium branch

Create a new branch `146.0.7650.0-issue-501` in `chromium/src/`, forked from the
vanilla `146.0.7650.0` tag (not from Issue 414). This is a clean rewrite, not a
continuation of the One Profile app. The One Profile code at
`content/one_profile/` remains on its own branches as historical reference.

```bash
cd chromium/src
git checkout -b 146.0.7650.0-issue-501 146.0.7650.0
```

### BUILD.gn

```gn
import("//build/config/features.gni")

source_set("chromium_profile_server_lib") {
  sources = [
    "profile_server_main_delegate.cc",
    "profile_server_main_delegate.h",
    "profile_server_main_parts.cc",
    "profile_server_main_parts.h",
    "profile_server_context.cc",
    "profile_server_context.h",
    "profile_server_video_consumer.cc",
    "profile_server_video_consumer.h",
  ]

  deps = [
    "//content/shell:content_shell_lib",
    "//components/viz/host:host",
    "//services/viz/privileged/mojom/compositing:compositing_interfaces",
    "//media",
  ]
}

# macOS app bundle
mac_app_bundle("chromium_profile_server") {
  output_name = "Chromium Profile Server"
  sources = [
    "profile_server_main.cc",
    "profile_server_main_mac.cc",
  ]
  deps = [
    ":chromium_profile_server_lib",
    "//content/shell:content_shell_framework",
  ]
  info_plist = "Info.plist"
}
```

Register in root BUILD.gn:

```gn
group("gn_all") {
  deps = [
    ...
    "//content/chromium_profile_server:chromium_profile_server",
  ]
}
```

### Build command

```bash
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default chromium_profile_server
```

## Part 2: Swift compositor (ts5/two-profiles/)

### Project structure

```
ts5/two-profiles/
├── Package.swift                           — SwiftPM manifest
├── Sources/
│   └── TwoProfiles/
│       ├── main.swift                      — Entry point, XPC, Metal, rendering
│       └── Shaders.metal                   — Vertex + fragment shaders
├── com.termsurf.two-profiles.plist         — Launchd agent definition
└── Makefile                                — Build shaders + swift build
```

Ported from `ts4/two-profiles-swift/Sources/Receiver/main.swift` (328 lines).
The code is small enough that a single file with MARK sections is clearer than
multiple files.

### Changes from ts4

1. **XPC service name.** `com.termsurf.two-profiles` (no `-swift` or `-ts5`
   suffix — this is the canonical name going forward).
2. **Target name.** `TwoProfiles` instead of `Receiver`.
3. **Makefile.** Compiles Metal shaders with `xcrun metal` / `xcrun metallib`
   (SPM does not compile `.metal` files).
4. **Log path.** `~/dev/termsurf/logs/two-profiles.log`.

No functional changes to XPC handling, Metal pipeline, or rendering logic.

### Known Swift gotchas (resolved in Issue 415)

1. SPM does not compile `.metal` files — use `xcrun metal` / `xcrun metallib`.
2. `mach_task_self()` is a C macro — use `mach_task_self_` in Swift.
3. Pixel format is `.bgra8Unorm_srgb` (lowercase) in Swift.
4. `MTLTextureUsage.shaderRead` needs the explicit type prefix.
5. `IOSurfaceRef` is ARC-managed — no `CFRetain`/`CFRelease`.

## Build and run

```bash
# 1. Build chromium-profile-server
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default chromium_profile_server

# 2. Build compositor (Swift)
cd ts5/two-profiles
make

# 3. Load compositor as launchd service
launchctl bootstrap gui/$(id -u) \
  ~/dev/termsurf/ts5/two-profiles/com.termsurf.two-profiles.plist

# 4. Start test page server
cd ts4/box-demo && bun run server.ts &

# 5. Start two profile servers
cd chromium/src

out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.two-profiles \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 2>&1 &

out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.two-profiles \
  --session-id=profile-b \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-b \
  http://localhost:9407 2>&1 &
```

## Success criteria

- Two panes in one window, each showing the spinning blue square
- Different localStorage identity in each pane (profile isolation)
- Both at 60fps sustained for 60+ seconds
- Pixel-perfect Retina quality (1600x1200 IOSurface, sRGB)
- No Dock icon for chromium-profile-server processes
- Pure Swift compositor, builds with `make`
- Chromium embedder is ~12 files linking against content_shell_lib (not a copy)

## Experiments

### Experiment 1: Build chromium-profile-server

#### Hypothesis

A minimal Chromium target at `content/chromium_profile_server/` that links
against `content_shell_lib` and subclasses only what's needed (~12 files) will
produce a working headless browser that captures frames and sends IOSurface Mach
ports via XPC at 60fps — without a Dock icon.

#### Steps

##### Step 1: Create Chromium branch

```bash
cd chromium/src
git checkout -b 146.0.7650.0-issue-501 146.0.7650.0
```

##### Step 2: Create directory and BUILD.gn

Create `content/chromium_profile_server/` with the BUILD.gn from the design
above. Register the target in the root BUILD.gn.

##### Step 3: Write the entry points

- `profile_server_main.cc` — Calls `content::ContentMain()` with our delegate
- `profile_server_main_mac.cc` — macOS framework loading (same pattern as
  content_shell)

##### Step 4: Write the delegate classes

- `ProfileServerMainDelegate` — Extends `ShellMainDelegate`, overrides
  `CreateContentBrowserClient()` to return our browser client
- `ProfileServerMainParts` — Extends `ShellBrowserMainParts`, overrides browser
  context creation, wires up `ProfileServerVideoConsumer`
- `ProfileServerContext` — Extends `ShellBrowserContext`, overrides `GetPath()`
  for `--user-data-dir`

##### Step 5: Write the video consumer

Port `ShellVideoConsumer` from the One Profile app as
`ProfileServerVideoConsumer`. Same `FrameSinkVideoCapturer` + XPC pipeline, but
with proper lifecycle management instead of a hardcoded delay.

##### Step 6: Create Info.plist with LSUIElement

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>LSUIElement</key>
    <true/>
    ...
</dict>
</plist>
```

##### Step 7: Build and test

```bash
autoninja -C out/Default chromium_profile_server
```

Test with the existing ts4/two-profiles-swift receiver to verify frames arrive
at 60fps. Verify no Dock icon appears.

#### Result: Failed

The build succeeded after resolving several compile and link errors, but the app
crashes on launch with a DCHECK failure in
`content/shell/app/paths_apple.mm:41`.

**Root cause:** `paths_apple.mm` uses `base::apple::IsBackgroundOnlyProcess()`
to determine the app bundle's `Contents` directory. When `LSUIElement=true` is
set in Info.plist, `IsBackgroundOnlyProcess()` returns true, which triggers a
code path designed for Content Shell's helper processes (renderer, GPU,
utility). That code path walks up **9 directory levels** from the executable to
find `Contents`, because helper processes are deeply nested inside
`Content Shell Framework.framework/Versions/C/Helpers/Content Shell Helper.app/Contents/MacOS/`.
Our main executable is only **2 levels** deep
(`Chromium Profile Server.app/Contents/MacOS/`), so the 9-level walk lands in
the user's home directory tree instead of `Contents`.

```
DCHECK failed: "Contents" == path.BaseName().value() (Contents vs. dev).
```

**Why this matters:** `LSUIElement=true` is not optional — the profile server
must run as a background process with no Dock icon. But
`IsBackgroundOnlyProcess()` is called from
`ShellMainDelegate::BasicStartupComplete()`, which is inherited code in
`content_shell_app`. The path resolution is deeply coupled to Content Shell's
specific bundle layout assumptions.

**Conclusion:** The "link against content_shell_lib, subclass only what's
needed" approach fails because Content Shell's inherited code makes assumptions
about the app bundle layout that conflict with our requirements. The
`LSUIElement` / `IsBackgroundOnlyProcess()` collision is one example, but the
tight coupling between Content Shell's startup sequence and its expected bundle
structure means other assumptions will likely surface too. Subclassing Content
Shell is not a viable path for a background-mode embedder.

**Build errors resolved along the way:**

1. GN doesn't auto-discover new directories — had to register the target in root
   `BUILD.gn`'s `gn_all` deps.
2. `profile_server_main.cc` in `mac_app_bundle` sources lacked include paths for
   `//base` — moved it to the `source_set`.
3. Missing `#include "base/files/file_util.h"` for `base::MakeAbsoluteFilePath`.
4. `ContentMainParams::argv` expects `const char**`, not `char**` — fixed with
   `const_cast`.
5. `ShellMainDelegate` lives in `content_shell_app` (a separate static library
   from `content_shell_lib`) — had to add both as deps.

### Experiment 2: Verify One Profile baseline from Issue 414

#### Hypothesis

The One Profile app from Issue 414 (`146.0.7650.0-issue-414`) still builds and
runs correctly with the ts4 Swift receiver (`ts4/two-profiles-swift/`). If it
does, this branch becomes the starting point for subsequent experiments that
rename and improve the One Profile fork into chromium-profile-server.

Experiment 1 failed because linking against `content_shell_lib` as a library
introduced tight coupling to Content Shell's bundle layout assumptions. The
alternative is the fork approach: take the full Content Shell copy from Issue
414 (the 218-file `content/one_profile/` directory) and modify it directly.
Before building on this foundation, we must verify the baseline still works —
the One Profile app hasn't been tested since Issue 414/415/416, and the build
environment may have changed.

#### Approach

**Zero code changes.** This experiment changes nothing. It creates a new
Chromium branch from `146.0.7650.0-issue-414`, builds the existing One Profile
app, and runs it against the existing Swift receiver to verify 60fps IOSurface
delivery.

#### Steps

##### Step 1: Create Chromium branch

Delete the failed Experiment 1 branch and create a new one from Issue 414:

```bash
cd chromium/src
git checkout 146.0.7650.0-issue-414
git branch -D 146.0.7650.0-issue-501
git checkout -b 146.0.7650.0-issue-501
```

This starts with the complete, proven One Profile app at `content/one_profile/`
— the same code that ran at 60fps in Issues 414, 415, and 416.

##### Step 2: Build One Profile

```bash
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default one_profile
```

The `one_profile` target should already exist in the root `BUILD.gn` from
Issue 414. If the build environment has changed (new SDK, updated deps), fix any
build errors.

##### Step 3: Start the Swift receiver

The Swift receiver from Issue 415 (`ts4/two-profiles-swift/`) uses the XPC
service name `com.termsurf.two-profiles-swift`.

```bash
# Build the Swift receiver (if not already built)
cd ts4/two-profiles-swift
swift build

# Load the launchd plist
launchctl bootstrap gui/$(id -u) \
  ~/dev/termsurf/ts4/two-profiles-swift/com.termsurf.two-profiles-swift.plist
```

##### Step 4: Start test page server

```bash
cd ts4/box-demo && bun run server.ts &
```

##### Step 5: Launch One Profile with XPC

```bash
cd chromium/src

out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-swift \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 2>&1
```

##### Step 6: Verify

Check the Swift receiver log for 60fps:

```bash
tail -f ~/dev/termsurf/logs/two-profiles-swift.log
```

Expected output:

```
[Receiver] Profile server connected
[Receiver] 60 frames (60.0 fps) | IOSurface 1600x1200
[Receiver] 60 frames (60.0 fps) | IOSurface 1600x1200
...
```

#### Success criteria

- One Profile app builds without errors on the current Chromium checkout
- One Profile app launches with `--hidden` (no visible window)
- Swift receiver receives IOSurface Mach ports at 60fps
- IOSurfaces are correct dimensions (Retina, e.g. 1600x1200)
- No crashes, no frame drops over 30+ seconds

#### What a failure would mean

- **Build failure:** The build environment has changed since Issue 414. Fix
  build errors and document them.
- **0fps in receiver:** The `--xpc-service` flag or XPC protocol may have
  changed. Verify CLI flags against the source in
  `content/one_profile/common/shell_switches.h`.
- **Low fps:** The `FrameSinkVideoCapturer` configuration may need updating.
  Check that `SetAutoThrottlingEnabled(false)` and
  `SetMinCapturePeriod(base::Milliseconds(16))` are still set.
- **Crash:** Document the crash and investigate before proceeding.

#### Result: Passed

The One Profile app from Issue 414 builds and runs correctly on the current
build environment. Two profile server instances running simultaneously deliver
IOSurface Mach ports to the Swift receiver at 60fps per pane.

##### Build

197 targets, zero errors. The branch switch from vanilla `146.0.7650.0` (used by
Experiment 1) to `146.0.7650.0-issue-414` triggered a rebuild of the
one_profile-specific targets. No source changes were needed.

##### Test output

```
[Receiver] L: 60 (60.0 fps) R: 60 (60.0 fps) | IOSurface 1600x1200
[Receiver] L: 61 (60.0 fps) R: 61 (60.0 fps) | IOSurface 1600x1200
[Receiver] L: 60 (60.0 fps) R: 60 (60.0 fps) | IOSurface 1600x1200
[Receiver] L: 61 (60.0 fps) R: 61 (59.0 fps) | IOSurface 1600x1200
```

Both panes sustained 60fps with 1600x1200 Retina IOSurfaces for 30+ seconds.

##### Success criteria checklist

- [x] One Profile app builds without errors
- [x] One Profile app launches with `--hidden` (no visible window)
- [x] Swift receiver receives IOSurface Mach ports at 60fps — both panes
- [x] IOSurfaces are 1600x1200 (Retina)
- [x] No crashes, no frame drops over 30+ seconds

#### Conclusion

The fork approach works. The `146.0.7650.0-issue-501` branch (forked from
`146.0.7650.0-issue-414`) is a proven two-profile baseline. The One Profile app
at `content/one_profile/` — a full Content Shell fork with
FrameSinkVideoCapturer, XPC IOSurface transfer, `--hidden` window,
`--session-id`, and Retina capture — builds and runs identically to when it was
first created in Issues 414–416.

This validates the fork approach as the path forward. Subsequent experiments
will rename `content/one_profile/` to `content/chromium_profile_server/`, add
`LSUIElement` support, and replace the hardcoded 2-second capturer attachment
delay with a `WebContentsObserver`.

### Experiment 3: Hide Dock icon with runtime activation policy

#### Hypothesis

Calling `[NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory]`
immediately after `RegisterShellCrApp()` in `PreBrowserMain()` will hide the One
Profile app from the Dock — without setting `LSUIElement=true` in the main app's
Info.plist and without triggering the `IsBackgroundOnlyProcess()` path
resolution bug that crashed Experiment 1.

#### Background

The One Profile app currently shows a Dock icon for each profile server process.
This is unacceptable — the profile server is a headless background process.

Experiment 1 tried `LSUIElement=true` in Info.plist, which caused
`IsBackgroundOnlyProcess()` to return true and crash `paths_apple.mm`. The
runtime approach avoids this entirely: the app launches as a regular app (so
path resolution works correctly), then immediately switches its activation
policy to `.accessory`, which hides the Dock icon and menu bar.

Note: the helper process plist (`helper-Info.plist`) already has `LSUIElement=1`
— that's correct because helpers use the 9-level path walk. This experiment only
changes the **main** app's behavior.

#### Approach

**One line of code.** In `shell_main_delegate_mac.mm`, add
`NSApplicationActivationPolicyAccessory` right after `RegisterShellCrApp()`
creates `[ShellCrApplication sharedApplication]`. This is the earliest point
where `NSApp` exists and can have its policy changed.

The change goes in `RegisterShellCrApp()` rather than `PreBrowserMain()` so the
policy is set in the same function that creates `NSApp`, keeping the logic
co-located.

#### Steps

##### Step 1: Modify RegisterShellCrApp

In `content/one_profile/app/shell_main_delegate_mac.mm`, add one line after the
`[ShellCrApplication sharedApplication]` call:

```objc
void RegisterShellCrApp() {
  [ShellCrApplication sharedApplication];
  [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];

  CHECK([NSApp isKindOfClass:[ShellCrApplication class]]);
}
```

`NSApplicationActivationPolicyAccessory` means: the app does not appear in the
Dock, does not have a menu bar, but can still create windows (needed for the
hidden window that Chromium's compositor renders to).

##### Step 2: Build

```bash
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default one_profile
```

Only `shell_main_delegate_mac.mm` changed, so the incremental build should be
fast.

##### Step 3: Test

Run the same two-profile test from Experiment 2:

```bash
# Start test page server
cd ts4/box-demo && bun run server.ts &

# Ensure Swift receiver is loaded
launchctl bootstrap gui/$(id -u) \
  ~/dev/termsurf/ts4/two-profiles-swift/com.termsurf.two-profiles-swift.plist

# Launch two profile servers
cd chromium/src

out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-swift \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 2>&1 &

out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-swift \
  --session-id=profile-b \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-b \
  http://localhost:9407 2>&1 &
```

##### Step 4: Verify

1. **No Dock icon:** Check the macOS Dock — there should be no "One Profile"
   icon.
2. **60fps maintained:** Check the Swift receiver log for sustained 60fps on
   both panes.
3. **No crashes:** The app should not hit the `paths_apple.mm` DCHECK.

#### Success criteria

- No Dock icon for either profile server process
- Both panes at 60fps sustained for 30+ seconds
- No crashes, no DCHECK failures
- `IsBackgroundOnlyProcess()` returns false (path resolution uses the 2-level
  walk, not the 9-level helper walk)

#### What a failure would mean

- **Dock icon still shows:** `NSApplicationActivationPolicyAccessory` might need
  to be set earlier, or the Chromium startup sequence might override it.
  Investigate when `NSApp` activation policy is set in the Content API
  lifecycle.
- **Crash in paths_apple.mm:** Would mean something else is setting
  `LSUIElement` or `IsBackgroundOnlyProcess()` is checking something other than
  the plist. Investigate what `IsBackgroundOnlyProcess()` actually checks.
- **0fps or low fps:** The activation policy change might affect Chromium's
  compositor visibility. If so, this would indicate that the compositor checks
  `NSApp.activationPolicy` rather than window visibility — an important finding
  that would rule out runtime policy changes entirely.

#### Result: Passed

One line of code hides both profile servers from the Dock while maintaining
60fps on both panes with zero crashes.

##### Change

In `content/one_profile/app/shell_main_delegate_mac.mm`, added
`[NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory]` inside
`RegisterShellCrApp()`, immediately after
`[ShellCrApplication sharedApplication]`.

##### Build

20 targets (incremental), zero errors.

##### Test output

```
Profile A: 60 frames in 1.00023s (59.9861 fps) | IOSurface 1600x1200
Profile B: 60 frames in 1.00005s (59.9967 fps) | IOSurface 1600x1200
```

Both panes sustained 60fps for 30+ seconds. No Dock icon appeared — verified via
`System Events` (no "One Profile" in visible processes).

##### Success criteria checklist

- [x] No Dock icon for either profile server process
- [x] Both panes at 60fps sustained for 30+ seconds
- [x] No crashes, no DCHECK failures
- [x] `IsBackgroundOnlyProcess()` returns false (2-level path walk, not 9-level)

#### Conclusion

The runtime activation policy approach works perfectly. Setting
`NSApplicationActivationPolicyAccessory` after creating `NSApp` hides the Dock
icon without affecting `IsBackgroundOnlyProcess()`, path resolution, or
compositor frame rates. This sidesteps the `LSUIElement` / `paths_apple.mm`
collision that killed Experiment 1 — no plist changes needed, no path logic to
fix.

However, the Dock icon briefly flashes on launch before the runtime policy takes
effect. Experiment 4 fixes this with `LSUIElement` in the plist instead.

### Experiment 4: LSUIElement in plist + fix paths_apple.mm

#### Hypothesis

Setting `LSUIElement=true` in `app-Info.plist` will prevent the Dock icon from
ever appearing (no flash). The `paths_apple.mm` crash from Experiment 1 can be
fixed by replacing `IsBackgroundOnlyProcess()` with a check for
`switches::kProcessType` — the standard Chromium flag that identifies child
processes.

#### Background

Experiment 3 proved that hiding from the Dock doesn't affect compositor frame
rates. But the runtime `setActivationPolicy` approach causes the Dock icon to
briefly flash on launch before disappearing. `LSUIElement=true` in the plist
prevents this entirely — macOS never creates the Dock icon in the first place.

Experiment 1 failed because `LSUIElement=true` makes
`base::apple::IsBackgroundOnlyProcess()` return true, which causes
`GetContentsPath()` in `paths_apple.mm` to use the 9-level helper path walk
instead of the 2-level main app walk. But `IsBackgroundOnlyProcess()` is a proxy
for "is this a child process?" — and a bad one, since it conflates `LSUIElement`
apps with helper processes.

The direct way to check is `switches::kProcessType`: Chromium sets `--type=` on
all child processes (renderer, GPU, utility). If the flag is present, it's a
child process. If absent, it's the main browser process. This is the same check
already used by `EnsureCorrectResolutionSettings()` in
`shell_main_delegate_mac.mm:25`.

#### Approach

Three changes:

1. **`paths_apple.mm`** — Replace `base::apple::IsBackgroundOnlyProcess()` with
   `base::CommandLine::ForCurrentProcess()->HasSwitch(switches::kProcessType)`.
   Add the necessary includes for `base/command_line.h` and
   `content/public/common/content_switches.h`.

2. **`app-Info.plist`** — Add `LSUIElement=true`. This tells macOS the app is a
   background-only UI element: no Dock icon, no menu bar, ever.

3. **`shell_main_delegate_mac.mm`** — Remove the runtime
   `setActivationPolicy:NSApplicationActivationPolicyAccessory` call from
   `RegisterShellCrApp()`. It's no longer needed since the plist handles it.

#### Steps

##### Step 1: Fix paths_apple.mm

In `content/one_profile/app/paths_apple.mm`, replace the
`IsBackgroundOnlyProcess()` check:

```cpp
#include "base/command_line.h"
#include "content/public/common/content_switches.h"

// ...

base::FilePath GetContentsPath() {
  base::FilePath path;
  base::PathService::Get(base::FILE_EXE, &path);

  // Child processes (renderer, GPU, utility) have --type= set by Chromium.
  // They are nested deep inside the framework bundle and need 9 levels up.
  // The main browser process has no --type= flag and needs only 2 levels.
  if (base::CommandLine::ForCurrentProcess()->HasSwitch(
          switches::kProcessType)) {
    path = path.DirName()
               .DirName()
               .DirName()
               .DirName()
               .DirName()
               .DirName()
               .DirName()
               .DirName()
               .DirName();
  } else {
    path = path.DirName().DirName();
  }
  DCHECK_EQ("Contents", path.BaseName().value());

  return path;
}
```

##### Step 2: Add LSUIElement to app-Info.plist

```xml
<key>LSUIElement</key>
<true/>
```

##### Step 3: Remove runtime setActivationPolicy

In `shell_main_delegate_mac.mm`, remove the `setActivationPolicy` call and its
comment from `RegisterShellCrApp()`.

##### Step 4: Build

```bash
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default one_profile
```

##### Step 5: Test

Same two-profile test as Experiments 2 and 3. Additionally, watch the Dock
carefully during launch to confirm no icon flash.

#### Success criteria

- No Dock icon at any point — not even a brief flash during launch
- Both panes at 60fps sustained for 30+ seconds
- No crashes, no DCHECK failures in `paths_apple.mm`
- Helper processes (renderer, GPU) still resolve paths correctly

#### What a failure would mean

- **DCHECK crash:** `switches::kProcessType` might not be set on the command
  line early enough for `GetContentsPath()`. Check when `paths_apple.mm` is
  first called in the startup sequence relative to command line parsing.
- **Helper process crash:** The `--type=` flag might not be present for all
  helper types that go through `GetContentsPath()`. Check which process types
  call path resolution functions.
- **Dock icon still flashes:** Would mean `LSUIElement` is not being read from
  the plist correctly. Verify the plist key spelling and value format.

#### Result: Passed (with design adjustment)

`LSUIElement=true` in the plist eliminates the Dock icon entirely — no flash, no
momentary appearance. Both panes sustain 60fps with zero crashes.

##### Design adjustment: path detection via `/Helpers/` instead of kProcessType

The original design called for replacing `IsBackgroundOnlyProcess()` with
`HasSwitch(switches::kProcessType)`. This crashed immediately:

```
DCHECK failed: current_process_commandline_.
```

`GetContentsPath()` is called from `OverrideFrameworkBundlePath()` during
`ContentMain()`, **before** `CommandLine::Init()`. The command line simply
doesn't exist yet.

The fix: check for `/Helpers/` in the executable path. Helper processes live at
paths like `.../Frameworks/.../Helpers/One Profile Helper.app/...`, while the
main process lives at `.../One Profile.app/Contents/MacOS/...`. This is
available immediately from `base::FILE_EXE` and requires no command line.

```cpp
if (path.value().find("/Helpers/") != std::string::npos) {
    // 9-level walk for helper processes
} else {
    // 2-level walk for main process
}
```

##### Changes

1. **`paths_apple.mm`** — Replaced `IsBackgroundOnlyProcess()` with
   `path.value().find("/Helpers/") != std::string::npos`. Removed
   `base/command_line.h` and `content/public/common/content_switches.h` includes
   (not needed). Updated comments explaining why `IsBackgroundOnlyProcess()` and
   `CommandLine` cannot be used here.

2. **`app-Info.plist`** — Added `LSUIElement=true`.

3. **`shell_main_delegate_mac.mm`** — Removed the runtime
   `setActivationPolicy:NSApplicationActivationPolicyAccessory` call from
   `RegisterShellCrApp()` (no longer needed).

##### Build

27 targets (incremental), zero errors.

##### Test output

```
Profile A: 60 frames in 1.00004s (59.9976 fps) | IOSurface 1600x1200
Profile B: 60 frames in 1.00014s (59.9915 fps) | IOSurface 1600x1200
```

Both panes sustained 60fps for 30+ seconds. No Dock icon appeared at any point
during launch or execution.

##### Success criteria checklist

- [x] No Dock icon at any point — no flash during launch
- [x] Both panes at 60fps sustained for 30+ seconds
- [x] No crashes, no DCHECK failures in `paths_apple.mm`
- [x] Helper processes (renderer, GPU) still resolve paths correctly

#### Conclusion

`LSUIElement=true` in the plist is the correct approach for hiding the Dock icon
— it prevents macOS from ever creating one, unlike the runtime
`setActivationPolicy` approach from Experiment 3 which caused a brief flash.

The `paths_apple.mm` fix required checking the executable path for `/Helpers/`
rather than using `IsBackgroundOnlyProcess()` (which conflates `LSUIElement`
with helper processes) or `CommandLine` (which isn't initialized yet). This is
robust: the `/Helpers/` directory is a structural property of Chromium's bundle
layout, not a runtime flag that could be absent or delayed.

### Experiment 5: Rename One Profile to Chromium Profile Server

#### Hypothesis

Renaming `content/one_profile/` to `content/chromium_profile_server/` — with all
internal references updated — will produce a working build with the correct app
bundle name (`Chromium Profile Server.app`) and no functional regressions.

#### Background

"One Profile" was a descriptive name for the ts4 PoC constraint (one
`BrowserContext` per process). Now that the architecture is proven and this is
becoming a real component of TermSurf, it needs its proper name:
`chromium-profile-server` (as defined in the Issue 501 design).

The rename is purely mechanical — no logic changes, no new features. But the
scope is large: 577 occurrences of "one_profile" / "One Profile" / "OneProfile"
across 110 files, plus 1 reference in the root `BUILD.gn`.

#### Scope

**Directory rename:**

- `content/one_profile/` → `content/chromium_profile_server/`

**String replacements (all files under `content/chromium_profile_server/`):**

| Old                   | New                               | Context                                                               |
| --------------------- | --------------------------------- | --------------------------------------------------------------------- |
| `one_profile`         | `chromium_profile_server`         | Target names, variable names, include paths, header guards, pak files |
| `One Profile`         | `Chromium Profile Server`         | Product name, app bundle, framework name, helper name, comments       |
| `OneProfile`          | `ChromiumProfileServer`           | Bundle IDs, class name prefixes                                       |
| `one-profile`         | `chromium-profile-server`         | Any hyphenated references                                             |
| `CONTENT_ONE_PROFILE` | `CONTENT_CHROMIUM_PROFILE_SERVER` | Macros, header guards                                                 |

**Root BUILD.gn:**

- `"//content/one_profile:one_profile"` →
  `"//content/chromium_profile_server:chromium_profile_server"`

**Bundle IDs:**

- `org.chromium.OneProfile` → `com.termsurf.chromium-profile-server`
- `org.chromium.OneProfile.helper` →
  `com.termsurf.chromium-profile-server.helper`

#### Approach

**Phase 1: Directory rename.** Use
`git mv content/one_profile content/chromium_profile_server` to preserve
history.

**Phase 2: Bulk string replacement.** Run `sed` across all files in
`content/chromium_profile_server/` for each replacement pattern. Order matters —
replace longer patterns first to avoid partial matches (e.g.,
`CONTENT_ONE_PROFILE` before `one_profile`).

**Phase 3: Root BUILD.gn.** Update the single reference.

**Phase 4: Build and test.** Same two-profile 60fps test as previous
experiments.

#### Steps

##### Step 1: Rename directory

```bash
cd chromium/src
git mv content/one_profile content/chromium_profile_server
```

##### Step 2: Bulk string replacements

Apply replacements in this order (longest first):

1. `CONTENT_ONE_PROFILE` → `CONTENT_CHROMIUM_PROFILE_SERVER`
2. `one_profile_product_name` → `chromium_profile_server_product_name`
3. `org.chromium.OneProfile` → `com.termsurf.chromium-profile-server`
4. `One Profile` → `Chromium Profile Server`
5. `OneProfile` → `ChromiumProfileServer`
6. `one_profile` → `chromium_profile_server`

Run across all files in `content/chromium_profile_server/`.

##### Step 3: Update root BUILD.gn

Change:

```gn
"//content/one_profile:one_profile",
```

To:

```gn
"//content/chromium_profile_server:chromium_profile_server",
```

##### Step 4: Build

```bash
autoninja -C out/Default chromium_profile_server
```

##### Step 5: Test

```bash
out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-swift \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 2>&1 &

out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-swift \
  --session-id=profile-b \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-b \
  http://localhost:9407 2>&1 &
```

#### Success criteria

- `autoninja -C out/Default chromium_profile_server` builds with zero errors
- App bundle is `Chromium Profile Server.app`
- No references to "one_profile" or "One Profile" remain in
  `content/chromium_profile_server/`
- Both panes at 60fps sustained for 30+ seconds
- No Dock icon (LSUIElement still works after rename)

#### What a failure would mean

- **Build errors:** Missed a rename somewhere — a file still references the old
  name. Fix the remaining references and rebuild.
- **Linker errors:** A target name was renamed inconsistently between BUILD.gn
  files. Check all `deps` lists.
- **0fps:** The pak file name or resource path changed incorrectly, preventing
  the web page from loading. Check `chromium_profile_server.pak` and resource
  paths.
- **Dock icon appears:** The plist rename broke LSUIElement. Check
  `app-Info.plist`.

#### Result: Passed

The rename is complete. `Chromium Profile Server.app` builds and runs
identically to the old `One Profile.app` — 60fps on both panes, no Dock icon, no
crashes.

##### Changes

**Directory rename:**

- `git mv content/one_profile content/chromium_profile_server`

**Bulk string replacements** (6 patterns across 110+ files):

1. `CONTENT_ONE_PROFILE` → `CONTENT_CHROMIUM_PROFILE_SERVER`
2. `org.chromium.OneProfile` → `com.termsurf.chromium-profile-server`
3. `One Profile` → `Chromium Profile Server`
4. `OneProfile` → `ChromiumProfileServer`
5. `one_profile` → `chromium_profile_server`
6. `one-profile` → `chromium-profile-server`

**External files updated** (not discovered until build failed):

- `BUILD.gn` — target reference in `gn_all`
- `components/cdm/renderer/BUILD.gn` — visibility list
- `components/cdm/browser/BUILD.gn` — visibility list
- `components/cdm/common/BUILD.gn` — visibility list
- `net/dns/BUILD.gn` — dep reference
- `tools/gritsettings/resource_ids.spec` — GRIT resource ID mapping

##### Build

194 targets, zero errors. Full rebuild required due to directory rename.

##### Test output

```
Profile A: 60 frames in 1.00009s (59.9944 fps) | IOSurface 1600x1200
Profile B: 60 frames in 1.00003s (59.9982 fps) | IOSurface 1600x1200
```

Both panes sustained 60fps for 30+ seconds. No Dock icon appeared.

##### Success criteria checklist

- [x] `autoninja -C out/Default chromium_profile_server` builds with zero errors
- [x] App bundle is `Chromium Profile Server.app`
- [x] No references to "one_profile" or "One Profile" remain in
      `content/chromium_profile_server/`
- [x] Both panes at 60fps sustained for 30+ seconds
- [x] No Dock icon (LSUIElement still works after rename)

#### Conclusion

The rename from "One Profile" to "Chromium Profile Server" is purely mechanical
and introduces no regressions. The only surprises were external references: 4
visibility lists in `components/cdm/` and `net/dns/` BUILD.gn files, plus a GRIT
resource ID mapping in `tools/gritsettings/resource_ids.spec`. These were added
by Issue 414 when the One Profile fork was created and needed updating alongside
the directory rename.

## Conclusion

Issue 501 set out to solve four problems with the ts4 proof-of-concept: the
218-file Content Shell copy, the bad naming, the Dock icon, and the fragile
capturer attachment. Five experiments addressed the first three. The fourth
(replacing the hardcoded 2-second delay with `WebContentsObserver`) remains for
a future experiment.

### What was accomplished

**Chromium Profile Server** (`content/chromium_profile_server/`) is a properly
named, Dock-invisible Content Shell fork that delivers IOSurface frames at 60fps
via XPC. It lives on the `146.0.7650.0-issue-501` branch in the Chromium repo,
built as 4 commits on top of the Issue 414 baseline:

1. Hide from Dock via runtime `setActivationPolicy` (Experiment 3)
2. Replace runtime policy with `LSUIElement` in plist + fix `paths_apple.mm`
   (Experiment 4)
3. Rename from "One Profile" to "Chromium Profile Server" (Experiment 5)

### What was learned

**The library approach doesn't work.** Experiment 1 tried to build a minimal
12-file embedder that links against `content_shell_lib`. This failed because
Content Shell's startup code makes assumptions about bundle layout that conflict
with `LSUIElement=true`. The tight coupling is fundamental — Content Shell was
never designed to be used as a library by background processes.

**The fork approach works.** Experiments 2–5 proved that forking Content Shell
and modifying it directly gives full control over startup behavior, path
resolution, and naming. The 218-file fork carries unused code (DevTools, web
test infrastructure, platform code for Linux/Windows/Android), but that baggage
is inert and the features it provides (DevTools, downloads, context menus) will
be needed as TermSurf becomes a real browser.

**Three approaches to hiding the Dock icon, only one is correct:**

| Approach                                          | Result                                                                     |
| ------------------------------------------------- | -------------------------------------------------------------------------- |
| `LSUIElement=true` in plist (Exp 1)               | Crashed — `IsBackgroundOnlyProcess()` misidentifies main process as helper |
| Runtime `setActivationPolicy` (Exp 3)             | Works but Dock icon flashes briefly on launch                              |
| `LSUIElement=true` + fix `paths_apple.mm` (Exp 4) | Correct — no flash, no crash                                               |

The `paths_apple.mm` fix checks for `/Helpers/` in the executable path instead
of using `IsBackgroundOnlyProcess()` (which conflates `LSUIElement` with helper
processes) or `CommandLine` (which isn't initialized yet at that point in
startup).

### What remains

The original Issue 501 design included two parts: the Chromium embedder (Part 1)
and the Swift compositor (Part 2). Only Part 1 has been addressed — the
experiments used the existing ts4 Swift receiver (`ts4/two-profiles-swift/`)
rather than building a new one at `ts5/two-profiles/`.

Outstanding work:

1. **Swift compositor** (`ts5/two-profiles/`) — Port from
   `ts4/two-profiles-swift/` with updated XPC service name
   (`com.termsurf.two-profiles`).
2. **WebContentsObserver for capturer attachment** — Replace the hardcoded
   2-second `PostDelayedTask` with a proper lifecycle hook that attaches the
   `FrameSinkVideoCapturer` when the `RenderWidgetHostView` is ready.
3. **Strip unused code** — Remove Android, iOS, Fuchsia, and Windows platform
   code from `content/chromium_profile_server/`. The fork carries 218 files but
   only ~30 are relevant to macOS.

### Chromium branch state

- **Branch:** `146.0.7650.0-issue-501`
- **Parent:** `146.0.7650.0-issue-414`
- **Commits:** 4 (runtime policy, LSUIElement fix, rename — Experiment 3's
  runtime policy commit is superseded by Experiment 4 but preserved in history)
- **Build target:** `chromium_profile_server`
- **App bundle:** `Chromium Profile Server.app`
