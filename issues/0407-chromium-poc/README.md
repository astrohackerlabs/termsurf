+++
status = "closed"
opened = "2026-02-08"
closed = "2026-02-08"
+++

# Issue 407: In-Process Chromium Proof of Concept

## Goal

Prove that Chromium can be embedded in-process inside a Swift macOS application
with multiple browser profiles rendering simultaneously at high framerate. No
custom XPC protocol. No out-of-process browser host. Chromium runs inside the
same process as the Swift window.

## What This Proves

This PoC validates three things that the entire ts4 architecture depends on:

1. **In-process Chromium embedding works on macOS.** We can embed Chromium's
   browser process directly inside a Swift application — not via CEF, not via a
   separate process, but as an in-process library that the Swift app links
   against.

2. **Multiple profiles coexist in one process.** Two `BrowserContext` instances
   with different storage paths run simultaneously, each with isolated cookies,
   localStorage, and cache. Issue 406 established that this is architecturally
   possible. This PoC proves it works in practice.

3. **High framerate rendering.** Chromium delivers frames at 120fps or higher to
   the Swift window. Issue 403 proved the compositing pipeline is fast (0.04ms
   per frame). Issues 325–350 proved CEF cannot sustain 60fps headless. This PoC
   proves the Content API path achieves what CEF could not.

## The Test Page

A Bun application in `ts4/box-demo/` serves a single HTML page:

- **Blue spinning square.** A square rendered via WebGL (or Canvas 2D) that
  rotates 360 degrees per second (1 Hz). The rotation rate is chosen so that
  smoothness differences between 60fps and 120fps are visible to the eye — at
  60fps the square moves 6 degrees per frame, at 120fps it moves 3 degrees.

- **localStorage identity.** On first load, the page generates a random string
  and stores it in `localStorage`. On subsequent loads, it reads the stored
  string. The string is displayed above the spinning square. This serves as
  proof that profiles are isolated — the left and right panes will show
  different random strings because they use different `BrowserContext` storage
  paths.

- **FPS counter.** The page displays its own measured framerate (frames per
  `requestAnimationFrame` callback per second). This is the content-side
  framerate — the rate at which the page's animation loop runs inside Chromium.

- **High framerate target.** The animation loop should run faster than 60fps.
  The target is 120fps or higher, ideally 240fps. This may require disabling
  Chromium's internal vsync throttling or using `requestAnimationFrame` with
  appropriate display configuration.

- **No external dependencies.** The page is fully self-contained — no CDN, no
  npm packages, no network requests. It works offline.

## The Swift Application

A macOS Swift application in `ts4/` that embeds Chromium and displays two
browser panes side by side:

- **Single window, two panes.** The window is split vertically — left half and
  right half — similar to the blue/green split from Issue 403 Phase 7.

- **Left pane: Profile A.** Loads the test page using one `BrowserContext` with
  its own storage directory (e.g., `~/.config/termsurf/poc/profile-a/`).

- **Right pane: Profile B.** Loads the same test page using a different
  `BrowserContext` with a separate storage directory (e.g.,
  `~/.config/termsurf/poc/profile-b/`).

- **In-process Chromium.** Chromium's browser process runs inside the Swift
  application's process. Chromium may still spawn its own renderer and GPU
  sub-processes internally (this is Chromium's own multi-process architecture
  and is expected). The point is that we are not inventing a custom XPC or IPC
  protocol between our Swift app and a separate browser host process — the
  browser host is in-process.

- **Compositing.** Each Chromium pane renders to a surface (IOSurface or
  similar) that the Swift app composites into its Metal render pass. The
  compositing approach is flexible — whatever works for in-process embedding.

- **FPS measurement.** The Swift app measures and displays the frame delivery
  rate for each pane — how many new frames Chromium delivers per second to each
  pane's surface.

## Success Criteria

### Must have

- Two panes rendering the same webpage simultaneously in one window.
- Each pane shows a different localStorage string, proving profile isolation.
- The localStorage strings persist across app restarts (profiles are stored on
  disk, not in-memory).
- Both panes render the spinning square smoothly at 60fps or higher.
- Chromium is embedded in-process — no custom XPC/IPC protocol between the Swift
  app and a separate browser host.

### Target

- Frame delivery at 120fps for both panes (matching ProMotion displays).
- Frame-to-frame intervals are consistent (not bursty — no bimodal clustering).
- CPU usage is reasonable (no 100% busy-wait loops).

### Stretch

- Frame delivery at 240fps.
- Resize both panes dynamically without dropping frames.
- Keyboard input forwarded to the focused pane (typing in a text field).

## What This Does NOT Need

- No tab management, no URL bar, no navigation controls.
- No terminal pane — this is a pure Chromium embedding PoC.
- No Ghostty integration — that comes after this PoC succeeds.
- No custom XPC protocol — Chromium's internal IPC between its own browser,
  renderer, and GPU processes is fine (that is Chromium's concern, not ours).
- No production-quality error handling or crash recovery.

## Test Page Server

The Bun app is minimal:

- `ts4/box-demo/server.ts` — serves `ts4/box-demo/public/index.html` on
  `localhost:9407`.
- The Swift app loads `http://localhost:<port>` in both panes.
- The server exists only because Chromium needs an HTTP origin for localStorage
  to work (file:// URLs have localStorage restrictions in some configurations).

## Relationship to Other Issues

| Issue | Relationship                                                                     |
| ----- | -------------------------------------------------------------------------------- |
| 403   | Proved IOSurface compositing at 60fps with colored rectangles                    |
| 404   | Selected Ghostty as the terminal emulator                                        |
| 405   | Chose Ghostty fork + out-of-process Chromium architecture                        |
| 406   | Proved multiple profiles work in one Chromium process; ruled out CEF             |
| 407   | This issue — proves in-process Chromium with multiple profiles at high framerate |

If this PoC succeeds, we proceed to integrate Chromium into the Ghostty fork
(the architecture from Issue 405). If it fails, we revisit the embedding
strategy before touching Ghostty.

## Implementation Plan

The PoC modifies Chromium's `content_shell` — the minimal Content API embedder
that ships with the Chromium source — inside the Chromium source tree. The
modifications are small (~5 files changed) and use Chromium's native windowed
rendering, not off-screen capture. The resulting `.app` bundle is built by
Chromium's GN/Ninja build system.

content_shell already uses Objective-C++ for its macOS shell, so the PoC is
written in ObjC++, not Swift. The Swift integration comes later when we
integrate with the Ghostty fork (which has a Swift macOS shell).

### Phase 1: Create the test page

Write the spinning blue square page and its HTTP server.

**`ts4/box-demo/public/index.html`** — Self-contained HTML page:

- Blue spinning square via Canvas 2D, rotating 360 deg/sec (1 Hz)
- Rotation angle computed from `performance.now()` (wall-clock time, not frame
  count), so the rotation rate is consistent regardless of framerate
- On first load: generate a random 8-character string, store in `localStorage`
- On subsequent loads: read and display the stored string above the canvas
- FPS counter: track last 60 `requestAnimationFrame` timestamps in a ring
  buffer, display average FPS updated once per second
- No external dependencies — all CSS, JS, and HTML in one file

**`ts4/box-demo/server.ts`** — Bun HTTP server:

- `Bun.serve()` on port 9407 (matching issue number)
- Serves `ts4/box-demo/public/index.html` on `GET /`
- HTTP origin needed because `file://` URLs restrict localStorage in some
  Chromium configurations

**Verification:** Open `http://localhost:9407` in Chrome. Square spins, FPS
counter shows ~60fps, random string appears. Reload — same string persists. Open
in incognito — different string appears.

### Phase 2: Merge Chromium into the repo

The Chromium source is added to the termsurf repo as a **git submodule**, not
a subtree merge. Chromium's build tools (`gclient`, `gn`, `depot_tools`)
require the source to be at the root of its own git repo. A subtree merge
breaks this — `gclient sync` cannot run inside a subdirectory of another repo.
A submodule preserves the repo boundary.

The upstream fork lives at `~/dev/termsurf-chromium/` (standard Chromium
layout with `.gclient` + `src/`). The submodule in the termsurf repo points
to this local path. When the software is ready, the fork will be pushed to
`github.com/termsurf/termsurf-chromium` and the submodule URL updated.

**Directory layout:**

```
~/dev/termsurf-chromium/              ← Chromium workspace (local upstream)
~/dev/termsurf-chromium/.gclient      ← gclient config (name = "src")
~/dev/termsurf-chromium/src/          ← Chromium source (git repo, full history)

~/dev/termsurf/ts4/termsurf-chromium/          ← wrapper directory in termsurf repo
~/dev/termsurf/ts4/termsurf-chromium/.gclient   ← gclient config (committed to termsurf)
~/dev/termsurf/ts4/termsurf-chromium/src/       ← git submodule → ~/dev/termsurf-chromium/src
```

**Why `src/` cannot be renamed:** Chromium's DEPS file hardcodes `src/` as
the path prefix for all dependencies. `gclient` resolves these paths relative
to the `.gclient` file location, using the solution name as the directory
name. The solution name must be `src` to match DEPS. This is a Chromium
build system constraint.

**CRITICAL: Full history is required.** The `fetch chromium` command must be
used (not `fetch --no-history`). A shallow clone produces grafted roots that
break `gclient sync` when the repo is moved or cloned. Full history also
enables future upstream merges.

**Step 1: Install depot_tools**

```
git clone https://chromium.googlesource.com/chromium/tools/depot_tools.git \
  ~/dev/termsurf/ts4/termsurf-chromium/depot_tools
export PATH="$HOME/dev/termsurf/ts4/termsurf-chromium/depot_tools:$PATH"
```

**Step 2: Fetch the Chromium source with full history**

Chromium cannot be built from a plain `git clone` — it requires `depot_tools`
and `gclient sync` to fetch hundreds of dependencies (V8, Skia, ICU, etc.).

```
mkdir ~/dev/termsurf-chromium && cd ~/dev/termsurf-chromium
caffeinate fetch chromium
```

This creates `~/dev/termsurf-chromium/src/` with the full source, all
dependencies, and full git history. ~100+ GB. Takes hours. `caffeinate`
prevents sleep.

**Step 3: Add the submodule to the termsurf repo**

```
cd /Users/ryan/dev/termsurf
git submodule add ~/dev/termsurf-chromium/src ts4/termsurf-chromium/src
```

This registers `ts4/termsurf-chromium/src/` as a submodule pointing to the
local upstream. The submodule contains the full Chromium source and all
`gclient`-managed dependencies.

**Step 4: Create `.gclient` for the submodule workspace**

Create `ts4/termsurf-chromium/.gclient` (committed to the termsurf repo, not
the submodule) so that `gclient sync` and `gn gen` work from within the
termsurf tree:

```
solutions = [
  {
    "name": "src",
    "url": "~/dev/termsurf-chromium/src",
    "managed": False,
    "custom_deps": {},
    "custom_vars": {},
  },
]
```

**Step 5: Verify the build system works**

```
cd ts4/termsurf-chromium
gclient sync
cd src
gn gen out/Default --args='is_debug=false symbol_level=0 enable_nacl=false is_component_build=true'
```

If `gn gen` succeeds, the source is buildable.

**Note on GitHub:** The Chromium fork is too large to push to GitHub in one
shot (pack exceeds GitHub's 2GB limit). Incremental pushing in batches of
~500K commits is possible but not needed yet. The submodule URL will be
updated from the local path to `github.com/termsurf/termsurf-chromium` when
the software is ready for distribution.

**Note on the existing shallow clone:** The existing
`/Users/ryan/dev/termsurf/chromium/` directory is a shallow read-only clone
from Issue 401 research. It is separate from the fork and can be removed.

### Phase 3: Build Chromium from source

Get `content_shell` building and running on macOS.

- [x] Exclude `ts4/termsurf-chromium/src/` from Spotlight indexing
- [x] Configure GN:
      ```
      cd ts4/termsurf-chromium/src
      gn gen out/Default --args='
        is_debug = false
        symbol_level = 0
        enable_nacl = false
        is_component_build = true
      '
      ```
- [x] Build content_shell: `autoninja -C out/Default content_shell` (1h31m,
      42,918 steps at 7.83/s)
- [x] Add `ts4/termsurf-chromium/src/out/` to `.gitignore`
- [x] Verify it runs: test page loads at 60fps with spinning square, FPS
      counter, and localStorage string.

### Phase 4: Create "Two Profiles" app

Create a new app called "Two Profiles" alongside content_shell — a separate
build target that reuses content_shell's infrastructure (`content_shell_lib`)
without modifying any existing content_shell files.

The new app lives at `content/two_profiles/` inside the Chromium tree. It
depends on `//content/shell:content_shell_lib` for all browser infrastructure
(Shell, ShellPlatformDelegate, devtools, downloads, permissions, etc.) and only
adds the minimal code needed for dual-profile behavior and side-by-side layout.

**New files (all in `content/two_profiles/`):**

**`BUILD.gn`** — Build target for the Two Profiles app:

- `two_profiles` target (mac\_app\_bundle on macOS, executable elsewhere)
- Depends on `//content/shell:content_shell_lib` and
  `//content/shell:content_shell_app`
- Sources: the new files listed below
- Same packaging structure as content_shell (framework, helpers, resources)

**`two_profiles_main.cc`** — Entry point:

- Same pattern as `content/shell/app/shell_main.cc`
- Creates `ShellMainDelegate`, calls `content::ContentMain()`
- No customization needed — all behavior comes from the BrowserMainParts
  override

**`two_profiles_main_mac.cc`** — macOS entry point:

- Same pattern as `content/shell/app/shell_main_mac.cc`
- Handles framework loading and helper process setup

**`two_profiles_browser_context.h/.cc`** — Subclass of `ShellBrowserContext`:

- Constructor accepts a `base::FilePath` for the profile directory
- Overrides `GetPath()` to return the custom path instead of the default
- Profile A: `~/.config/termsurf/poc/profile-a/`
- Profile B: `~/.config/termsurf/poc/profile-b/`

**`two_profiles_main_parts.h/.cc`** — Subclass of `ShellBrowserMainParts`:

- Overrides `InitializeBrowserContexts()` to create two
  `TwoProfilesBrowserContext` instances with different storage paths
- Overrides `InitializeMessageLoopContext()` to create two Shell windows (or
  one window with two WebContents), each using a different BrowserContext,
  both loading `http://localhost:9407`
- Overrides `PostMainMessageLoopRun()` to clean up the second context

**`two_profiles_content_browser_client.h/.cc`** — Subclass of
`ShellContentBrowserClient`:

- Overrides `CreateBrowserMainParts()` to return `TwoProfilesMainParts`
  instead of the default `ShellBrowserMainParts`

**`two_profiles_layout_mac.mm`** — Side-by-side layout on macOS:

- After both WebContents are created, arranges them side by side in one
  NSWindow
- Gets Shell A's NSWindow contentView
- Sets Shell A's WebContents view frame to the left half
- Sets Shell B's WebContents view frame to the right half
- Adds both as subviews with `NSViewWidthSizable | NSViewHeightSizable`

**Build and run:**

```
autoninja -C ts4/termsurf-chromium/src/out/Default two_profiles
```

Incremental build after new files: 1–5 minutes.

```
# Terminal 1:
cd /Users/ryan/dev/termsurf/ts4/box-demo && bun run server.ts

# Terminal 2:
./ts4/termsurf-chromium/src/out/Default/Two\ Profiles.app/Contents/MacOS/Two\ Profiles
```

**Expected result:** One window, two panes. Both show the blue spinning square.
Left pane shows one localStorage string, right pane shows a different one. Both
strings persist across app restarts. Content Shell remains unmodified and still
builds and runs independently.

#### Experiments

##### Experiment 1: Raw dual WebContents (2fps — partial success)

Created a `TwoProfilesMainParts` subclass that overrides
`InitializeBrowserContexts()` to create two `ShellBrowserContext` instances with
different `SHELL_DIR_USER_DATA` paths, and `InitializeMessageLoopContext()` to
create two `WebContents` in one window — Shell A (profile A) via
`Shell::CreateNewWindow()`, and WebContents B (profile B) via raw
`WebContents::Create()`, manually added as a subview to Shell A's window.

**Result:** Partial success. Two panes rendered side by side in one window,
each showing the spinning blue square with a different localStorage identity
string — proving profile isolation works. However, both panes rendered at only
2fps instead of 60fps.

**Diagnosis:** Chromium throttles `requestAnimationFrame` to ~1-2fps for
WebContents it considers hidden or in a background state. WebContents B was
created via raw `WebContents::Create()` without ever receiving a `WasShown()`
call, so Chromium treated it as a background tab. Shell A's WebContents may
also have been affected by the manual NSView frame manipulation disrupting
the Shell's internal visibility tracking.

##### Experiment 2: WasShown() calls (3fps — failed)

Added `web_contents_b_->WasShown()` and `shell_a->web_contents()->WasShown()`
after laying out both views side by side. The hypothesis was that Chromium was
throttling `requestAnimationFrame` because it considered the WebContents hidden.

**Result:** Failed. Framerate went from ~2fps to ~3fps — no meaningful
improvement. `WasShown()` alone does not fix the throttling. The root cause
is something else — possibly macOS occlusion detection misclassifying the
views, `RenderWidgetHostView` not receiving resize notifications after the
manual frame changes, or a deeper issue with how content_shell's platform
delegate manages visibility for reparented views.

## Conclusion

### What we proved

1. **Multiple BrowserContexts work in one process.** Two
   `ShellBrowserContext` instances with different `SHELL_DIR_USER_DATA` paths
   coexist in the same Chromium process. Each has its own cookies, localStorage,
   and cache. This confirms Issue 406's architectural analysis — the
   one-profile-per-process limitation is CEF-specific, not a Chromium
   constraint.

2. **Profile isolation is real.** The Two Profiles app displays two panes side
   by side, each showing a different localStorage identity string. The strings
   persist across app restarts. Profile data is stored on disk at
   `~/.config/termsurf/poc/profile-a/` and `profile-b/`.

3. **Chromium builds and runs from our fork.** The Chromium source lives as a
   git submodule at `ts4/termsurf-chromium/src/`. We can add new build targets
   (`content/two_profiles/`) alongside content_shell without modifying any
   existing Chromium files (except one line in `//BUILD.gn` to register the
   target). Incremental builds after code changes take ~15-20 seconds.

4. **content_shell renders at 60fps.** A single WebContents in a single Shell
   window renders the spinning blue square at a steady 60fps. The Content API's
   windowed rendering path has no framerate ceiling like CEF's off-screen path
   did.

### What we did not prove

**Multiple WebContents at 60fps in one window.** The Two Profiles app renders
both panes at only 2-3fps. This is the critical unsolved problem.

### Why Two Profiles renders at 2-3fps

The root cause is that content_shell's rendering pipeline assumes **one
WebContents owns one window's content area**, managed entirely through
Chromium's platform delegate. Two Profiles breaks this assumption by manually
manipulating the NSView hierarchy, and Chromium's internal visibility tracking
breaks as a result.

**Chromium's framerate control chain:**

```
NSWindow visibility
  → RenderWidgetHostViewCocoa (NSView) detects visibility
  → RenderWidgetHostImpl::WasShown() / WasHidden()
  → Blink PageSchedulerImpl::SetPageVisible()
  → CC SchedulerStateMachine::visible_
  → ShouldSubscribeToBeginFrames()
  → IF visible: subscribe to vsync → 60fps
    IF hidden:  unsubscribe from vsync → ~1fps (background throttle)
```

When the CC scheduler's `visible_` flag is false, it **unsubscribes from vsync
entirely** — no BeginFrame signals are delivered, no `requestAnimationFrame`
callbacks fire. The ~2-3fps we observe matches Chromium's background tab
throttling rate.

**Why both panes are throttled:**

- **WebContents B** was created via raw `WebContents::Create()` without going
  through `Shell::CreateShell()` → `SetContents()`. It was never linked to a
  platform window through Chromium's internal channels. The
  `RenderWidgetHostViewCocoa` does not know it is in a visible window.

- **WebContents A** (Shell A) was created through the proper pipeline and
  initially renders at 60fps. But then we manually resize its NSView from
  full-window to half-window and add a sibling view — bypassing Chromium's
  view management. The `RenderWidgetHostViewCocoa` has its own NSView-level
  visibility tracking that can override explicit `WasShown()` calls, and the
  manual frame manipulation causes it to misreport its state.

- **Explicit `WasShown()` calls don't help** (Experiment 2). `WasShown()` sets
  visibility at the `WebContentsImpl` level, but `RenderWidgetHostViewCocoa`
  has independent visibility tracking at the NSView level that overrides it.

- **Broken autoresizing masks compound the problem.** Both views were set to
  `NSViewWidthSizable | NSViewHeightSizable`, which tells AppKit to stretch
  each view to fill the entire container on resize. The views would overlap
  on any window resize, potentially causing layout thrashing that further
  confuses the visibility detector.

- **content_shell doesn't use Chromium's `views` framework.** Chrome's
  `NativeWidgetNSWindowBridge` with `windowDidChangeOcclusionState:` handles
  sophisticated occlusion tracking — but content_shell uses raw NSWindows
  without this infrastructure. The `RenderWidgetHostViewCocoa` falls back to
  its own NSView-level visibility detection, which was not designed for
  manually reparented views.

### What comes next

The framerate problem is solvable but requires deeper integration with
Chromium's rendering pipeline. Possible approaches:

1. **Use Chromium's `views` framework** instead of raw NSWindows. Create a
   `views::Widget` with two child `views::WebView` instances. The `views`
   framework handles visibility, layout, and resize notifications through
   proper Chromium channels, avoiding the NSView manipulation that breaks
   `RenderWidgetHostViewCocoa`.

2. **Off-screen compositing.** Use `RenderWidgetHostViewBase`'s
   `CopyFromSurface()` or a custom `DelegatedFrameHost` to capture each
   WebContents' rendered output to an IOSurface, then composite both surfaces
   in a single Metal render pass — similar to the CEF approach from ts3 but
   without CEF's framerate ceiling.

3. **Patch `RenderWidgetHostViewCocoa`** to support externally-managed
   visibility. Override the NSView-level visibility detection so that explicit
   `WasShown()` calls are respected even when the view hierarchy doesn't match
   what the view expects.

4. **Create two Shell windows and composite them.** Create two full Shell
   instances (each with proper platform window setup), then either reparent
   their content views into a shared parent NSWindow or capture their
   IOSurfaces for compositing. This preserves Chromium's one-WebContents-
   per-window assumption while achieving the side-by-side visual result.

Each approach has different tradeoffs in complexity, invasiveness to the
Chromium source, and risk. This will be explored in Issue 408.
