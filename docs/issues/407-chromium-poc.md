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

A Bun application in `ts4/` serves a single HTML page:

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

- `ts4/server.ts` (or similar) — serves `ts4/public/index.html` on
  `localhost:<port>`.
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

**`ts4/public/index.html`** — Self-contained HTML page:

- Blue spinning square via Canvas 2D, rotating 360 deg/sec (1 Hz)
- Rotation angle computed from `performance.now()` (wall-clock time, not frame
  count), so the rotation rate is consistent regardless of framerate
- On first load: generate a random 8-character string, store in `localStorage`
- On subsequent loads: read and display the stored string above the canvas
- FPS counter: track last 60 `requestAnimationFrame` timestamps in a ring
  buffer, display average FPS updated once per second
- No external dependencies — all CSS, JS, and HTML in one file

**`ts4/server.ts`** — Bun HTTP server:

- `Bun.serve()` on port 9407 (matching issue number)
- Serves `ts4/public/index.html` on `GET /`
- HTTP origin needed because `file://` URLs restrict localStorage in some
  Chromium configurations

**Verification:** Open `http://localhost:9407` in Chrome. Square spins, FPS
counter shows ~60fps, random string appears. Reload — same string persists. Open
in incognito — different string appears.

### Phase 2: Merge Chromium into the repo

Fork Chromium into the termsurf monorepo, following the same pattern used for
Ghostty (`ts1/`), WezTerm (`ts2/`, `ts3/`), and cef-rs (`cef-rs/`). The
Chromium source lives at `ts4/termsurf-chromium/` and carries the full upstream
git history.

**Step 1: Fetch the Chromium source with depot_tools**

Chromium cannot be built from a plain `git clone` — it requires `depot_tools`
and `gclient sync` to fetch hundreds of dependencies (V8, Skia, ICU, etc.).

```
# Install depot_tools
git clone https://chromium.googlesource.com/chromium/tools/depot_tools.git \
  ~/depot_tools
export PATH="$HOME/depot_tools:$PATH"

# Fetch Chromium into a staging directory
mkdir /tmp/chromium-staging && cd /tmp/chromium-staging
caffeinate fetch --no-history chromium
```

This creates `/tmp/chromium-staging/src/` with the full source and all
dependencies. ~30–50 GB. Takes hours. `caffeinate` prevents sleep.
`--no-history` skips git history to save disk (we will re-fetch with history
for the merge).

Alternatively, fetch with history for a proper merge:

```
caffeinate fetch chromium
```

This preserves upstream git history so that `git merge -X subtree` works for
future upstream merges.

**Step 2: Move everything into `ts4/termsurf-chromium/`**

Inside the Chromium repo, create a commit that moves the entire source tree
into a subdirectory:

```
cd /tmp/chromium-staging/src
git mv $(ls -A | grep -v '.git') ts4/termsurf-chromium/
# Or use a script to handle the large number of files
mkdir ts4/termsurf-chromium
git ls-files | while read f; do
  mkdir -p "ts4/termsurf-chromium/$(dirname "$f")"
  git mv "$f" "ts4/termsurf-chromium/$f"
done
git commit -m "Move Chromium source into ts4/termsurf-chromium/"
```

This commit exists only in the staging repo. It rewrites all paths so that
the subtree merge maps correctly.

**Step 3: Merge into termsurf with unrelated histories**

```
cd /Users/ryan/dev/termsurf
git remote add chromium /tmp/chromium-staging/src
git fetch chromium
git merge chromium/main --allow-unrelated-histories \
  -m "Merge Chromium into ts4/termsurf-chromium"
```

The termsurf repo now contains the Chromium source at
`ts4/termsurf-chromium/` with full upstream history. Future upstream merges
use:

```
git fetch chromium
git merge -X subtree=ts4/termsurf-chromium chromium/main \
  -m "Merge upstream Chromium"
```

**Step 4: Update merge-upstream documentation**

Add Chromium to `docs/issues/002-merge-upstream.md`:

| Project  | Directory                | Upstream                    | Remote     | Branch |
| -------- | ------------------------ | --------------------------- | ---------- | ------ |
| Chromium | `ts4/termsurf-chromium/` | chromium.googlesource.com   | `chromium` | main   |

**Step 5: Verify the source is buildable**

```
cd /Users/ryan/dev/termsurf/ts4/termsurf-chromium
# gclient sync may be needed to fetch deps that aren't in git
gclient sync
```

**Note on `.gclient`:** The `gclient` configuration file and `depot_tools`
integration may need adjustment since the source now lives inside a
subdirectory of the termsurf repo rather than at the root of its own repo.
This may require a `.gclient` file in `ts4/` or `ts4/termsurf-chromium/`
that points to the correct paths.

**Note on the existing shallow clone:** The existing
`/Users/ryan/dev/termsurf/chromium/` directory is a shallow read-only clone
from Issue 401 research. It is separate from `ts4/termsurf-chromium/` and
can be removed after this phase.

### Phase 3: Build Chromium from source

Get `content_shell` building and running on macOS from the merged source.

- [ ] Exclude `ts4/termsurf-chromium/` from Spotlight indexing
- [ ] Configure GN:
      ```
      cd ts4/termsurf-chromium
      gn gen out/Default --args='
        is_debug = false
        symbol_level = 0
        enable_nacl = false
        is_component_build = true
      '
      ```
- [ ] Build content_shell: `autoninja -C out/Default content_shell` (1–7 hours
      depending on hardware)
- [ ] Add `ts4/termsurf-chromium/out/` to `.gitignore`
- [ ] Verify it runs:
      ```
      ./ts4/termsurf-chromium/out/Default/Content\ Shell.app/Contents/MacOS/Content\ Shell \
        http://localhost:9407
      ```
      Start the Bun server first. Confirm the test page loads with spinning
      square, FPS counter, and localStorage string.

### Phase 4: Modify content_shell for dual profiles

Modify ~5 files inside the Chromium tree to create two `ShellBrowserContext`
instances with different storage paths and display two `WebContents` side by
side in one window.

**Step 1: Add custom-path constructor to ShellBrowserContext**

`content/shell/browser/shell_browser_context.h` and `.cc` — Add a constructor
that accepts a `base::FilePath` so two instances can have different storage
directories:

- Profile A: `~/.config/termsurf/poc/profile-a/`
- Profile B: `~/.config/termsurf/poc/profile-b/`

**Step 2: Create two BrowserContexts in ShellBrowserMainParts**

`content/shell/browser/shell_browser_main_parts.h` — Add members:

```cpp
std::unique_ptr<ShellBrowserContext> browser_context_b_;
std::unique_ptr<WebContents> web_contents_b_;
```

`content/shell/browser/shell_browser_main_parts.cc` — Modify:

- `InitializeBrowserContexts()`: Create `browser_context_` with profile-a path,
  `browser_context_b_` with profile-b path
- `InitializeMessageLoopContext()`: Create Shell A with `browser_context_`
  loading `http://localhost:9407`. Create a second `WebContents` with
  `browser_context_b_` loading the same URL. Add the second WebContents view to
  Shell A's window (right half).
- `PostMainMessageLoopRun()`: Clean up `web_contents_b_` and
  `browser_context_b_`

**Step 3: Side-by-side layout on macOS**

`content/shell/browser/shell_platform_delegate_mac.mm` or
`shell_browser_main_parts_mac.mm` — After both WebContents are created:

- Get Shell A's NSWindow contentView
- Set Shell A's WebContents view frame to the left half of the window
- Set Shell B's WebContents view frame to the right half
- Add both as subviews with autoresizing masks

**Step 4: Build and run**

```
autoninja -C ts4/termsurf-chromium/out/Default content_shell
```

Incremental build after ~5 file changes: 1–5 minutes.

```
# Terminal 1:
cd /Users/ryan/dev/termsurf/ts4 && bun run server.ts

# Terminal 2:
./ts4/termsurf-chromium/out/Default/Content\ Shell.app/Contents/MacOS/Content\ Shell \
  --hide-toolbar
```

**Expected result:** One window, two panes. Both show the blue spinning square.
Left pane shows one localStorage string, right pane shows a different one. Both
strings persist across app restarts.

### Phase 5: Measure and document

- [ ] Verify profile isolation: two different localStorage strings, persisting
      across restarts. Check `~/.config/termsurf/poc/profile-a/` and
      `profile-b/` exist with separate data on disk.
- [ ] Measure framerate from FPS counters in both panes. On 60Hz display: expect
      ~60fps. On ProMotion: expect up to 120fps.
- [ ] Test higher framerates with `--disable-frame-rate-limit` and
      `--disable-gpu-vsync` flags. These affect the in-process windowed
      rendering path (unlike CEF's OSR path where they had no effect).
- [ ] Measure CPU usage via Activity Monitor or `top`.
- [ ] Document findings in this issue.
