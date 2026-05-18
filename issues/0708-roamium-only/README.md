+++
status = "closed"
opened = "2026-03-05"
closed = "2026-03-05"
+++

# Issue 708: Roamium-only Chromium fork

## Goal

Create a clean Chromium fork branch that contains only `libtermsurf_chromium`
(renamed from `libtermsurf_content`) and no legacy binaries. Delete the Chromium
Profile Server and Plusium entirely. Roamium — built outside the Chromium tree
with Cargo — is the only browser binary.

## Background

### How we got here

The Chromium fork has accumulated 68 commits across Issues 411–707. These
commits contain three separate binaries:

1. **Chromium Profile Server** — The original monolithic fork. ~1,050 lines of
   TermSurf-specific code spread across ~100 files inside `content/shell/`. It
   was the browser from ts5 through Issue 703. Every feature required modifying
   Chromium's Content Shell internals directly.

2. **Plusium** — A 511-line C++ binary at `content/plusium/` that wraps
   `libtermsurf_content` through its C API. Created in Issue 704, debugged
   through Issue 705, DevTools crash fixed in Issue 706. It was the proof that
   the C library design works — a stepping stone to Roamium.

3. **Roamium** — A ~400-line Rust binary at `roamium/` (repo root, outside the
   Chromium tree). Created in Issue 707. It links `libtermsurf_content.dylib`,
   uses prost for protobuf, and is a verified drop-in replacement for Plusium.
   All 27 feature tests pass in both debug and release.

Now that Roamium works, the other two binaries are dead weight:

- **Chromium Profile Server** adds ~100 forked files to the Chromium tree. Every
  upstream merge requires rebasing these. Its functionality is fully replaced by
  `libtermsurf_content` + Roamium.
- **Plusium** was always a stepping stone. It copies proto files into the
  Chromium tree (Roamium links them directly from `proto/`). Its only remaining
  value is as a debugging reference, but the C API is simple enough that Roamium
  serves that purpose.

### What stays

**`libtermsurf_content`** — Renamed to **`libtermsurf_chromium`**. This is the C
library that wraps Chromium's Content API. It exports 20 functions with C types
only. It lives inside the Chromium tree at `content/libtermsurf_chromium/` and
builds as a shared library via GN. This is the only TermSurf code inside the
Chromium fork.

The rename reflects what it actually is: TermSurf's Chromium binding layer. The
old name (`libtermsurf_content`) was an artifact of it living in `content/` — it
described the Chromium module it wraps, not its purpose.

### What gets deleted

- `content/chromium_profile_server/` — The entire Profile Server fork
- All forked Content Shell files (the ~100 files in `content/shell/` that the
  Profile Server modified)
- `content/plusium/` — Plusium binary and its local proto copy
- All GN targets for `chromium_profile_server` and `plusium`

### What gets renamed

- `content/libtermsurf_content/` → `content/libtermsurf_chromium/`
- `libtermsurf_content.h` → `libtermsurf_chromium.h`
- `libtermsurf_content.dylib` → `libtermsurf_chromium.dylib`
- All internal references (GN targets, include paths, symbol names)

### What gets updated outside Chromium

- `roamium/build.rs` — Link `libtermsurf_chromium` instead of
  `libtermsurf_content`
- `roamium/src/ffi.rs` — Update comments if header name appears
- `scripts/build-debug.sh` — Remove `plusium` from autoninja targets, update
  target name if needed
- `scripts/build-release.sh` — Same
- `gui/src/apprt/xpc.zig` — Remove `plusium` and `chromium` from browser
  registry, keep only `roamium`
- `chromium/README.md` — New branch, updated build target

## Approach

Start from the vanilla `146.0.7650.0` tag — not from the current issue-707
branch. Cherry-pick or rewrite only the commits that create
`libtermsurf_chromium`. This gives us a minimal, clean branch with no historical
baggage from the Profile Server, Plusium, or the dozens of intermediate
experiments.

The key things to preserve (from issue-707) are:

**The library itself** (`content/libtermsurf_chromium/`):

- `libtermsurf_chromium.h` — C API header (20 functions, `TS_EXPORT` macros)
- `libtermsurf_chromium.cc` — Implementation
- `ts_browser_main_parts.h` / `ts_browser_main_parts.cc` — BrowserMainParts
  subclass (tab registry, callbacks, DevTools via `tab_id`)
- `ts_browser_client.h` / `ts_browser_client.cc` — ContentBrowserClient subclass
  (dark mode `OverrideWebPreferences`, BadgeService stub)
- `ts_main_delegate.h` / `ts_main_delegate.cc` — ContentMainDelegate subclass
  (skip bundle path DCHECK)
- `BUILD.gn` — Shared library target

**Patches to stock Chromium files:**

- `content/shell/browser/shell_devtools_frontend.h` — Made
  `ShellDevToolsFrontend` constructor **public** (was private). The library
  needs to construct DevTools frontends directly rather than going through the
  static `Show()` method.
- `content/shell/browser/shell_platform_delegate_mac.mm` — Added `--hidden`
  flag: `setAlphaValue:0` + `orderWindow:NSWindowBelow`. Keeps the compositor
  active while hiding the Content Shell window. Without this, CALayerHost
  compositing breaks.
- `content/shell/common/shell_switches.h` — Added `kHidden` switch constant.
- `BUILD.gn` (root) — Add `libtermsurf_chromium` to `gn_all` group.

**Patches that can be removed** (only existed for Profile Server or Plusium):

- `net/dns/BUILD.gn` — Profile Server visibility allowlist entry.
- `tools/gritsettings/resource_ids.spec` — Profile Server resource IDs.

Everything else — Profile Server forks, Plusium, XPC code,
FrameSinkVideoCapturer code — gets left behind.

## Ideas for experiments

1. **Create the clean branch.** Start from `146.0.7650.0`, create
   `146.0.7650.0-issue-708`. Cherry-pick or rewrite the essential commits.
   Rename `libtermsurf_content` → `libtermsurf_chromium`. Build with autoninja.

2. **Update Roamium.** Change `build.rs` and `ffi.rs` to reference the new
   library name. Update build scripts. Verify Roamium still works end-to-end.

3. **Update GUI.** Remove `plusium` and `chromium` from the browser registry.
   Make `roamium` the default browser (empty `--browser` flag resolves to
   roamium). Verify `web google.com` works without `--browser roamium`.

4. **Clean up patches.** Generate a fresh patch set for issue-708. This should
   be dramatically smaller than issue-707's 68 patches.

## Experiment 1: Create the clean Chromium branch

### Goal

Start from vanilla `146.0.7650.0` and create a minimal branch that contains only
`libtermsurf_chromium` (the renamed library) and the 4 stock Chromium patches.
No Profile Server, no Plusium, no historical baggage. Build the shared library
and verify the `.dylib` exists.

### Why start from scratch instead of cherry-picking

The issue-707 branch has 68 commits. The library files evolved across Issues
704–707 — created in 704, debugged in 705, crash-fixed in 706, feature-complete
in 707. Cherry-picking would mean untangling which commits touch the library vs.
Plusium vs. Profile Server, resolving conflicts from intermediate states, and
still ending up with a messy history. It's cleaner to copy the final state of
the 16 library files and the 4 stock patches as fresh commits.

### Steps

**1. Create the branch**

```bash
cd ~/dev/termsurf/chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
git checkout 146.0.7650.0
git checkout -b 146.0.7650.0-issue-708
```

**2. Copy the library from issue-707, renamed**

Copy all 16 files from `content/libtermsurf_content/` on issue-707 into
`content/libtermsurf_chromium/` on the new branch. Rename every occurrence:

- Directory: `libtermsurf_content` → `libtermsurf_chromium`
- Header: `libtermsurf_content.h` → `libtermsurf_chromium.h`
- GN target: `libtermsurf_content` → `libtermsurf_chromium`
- Include paths: `#include "content/libtermsurf_content/..."` →
  `#include "content/libtermsurf_chromium/..."`
- Header guard: update `LIBTERMSURF_CONTENT_H_` → `LIBTERMSURF_CHROMIUM_H_`

Files to copy and rename (16 total):

| From (issue-707)              | To (issue-708)                |
| ----------------------------- | ----------------------------- |
| `libtermsurf_content.h`       | `libtermsurf_chromium.h`      |
| `libtermsurf_content.cc`      | `libtermsurf_chromium.cc`     |
| `ts_browser_main_parts.h`     | `ts_browser_main_parts.h`     |
| `ts_browser_main_parts.cc`    | `ts_browser_main_parts.cc`    |
| `ts_browser_client.h`         | `ts_browser_client.h`         |
| `ts_browser_client.cc`        | `ts_browser_client.cc`        |
| `ts_main_delegate.h`          | `ts_main_delegate.h`          |
| `ts_main_delegate.cc`         | `ts_main_delegate.cc`         |
| `ts_ca_layer_bridge_mac.h`    | `ts_ca_layer_bridge_mac.h`    |
| `ts_ca_layer_bridge_mac.mm`   | `ts_ca_layer_bridge_mac.mm`   |
| `ts_compositor_bridge_mac.h`  | `ts_compositor_bridge_mac.h`  |
| `ts_compositor_bridge_mac.mm` | `ts_compositor_bridge_mac.mm` |
| `ts_tab_observer.h`           | `ts_tab_observer.h`           |
| `ts_tab_observer.cc`          | `ts_tab_observer.cc`          |
| `test_main.cc`                | `test_main.cc`                |
| `BUILD.gn`                    | `BUILD.gn`                    |

Commit: "Add libtermsurf_chromium library"

**3. Apply stock Chromium patches**

Four files need small patches. Apply them manually (not cherry-pick — the
issue-707 commits bundle these with unrelated Profile Server changes).

**a. `content/shell/common/shell_switches.h`** — Add `kHidden` constant:

```cpp
// Hide the Content Shell window (make it transparent and order it behind
// all other windows). Used by TermSurf browser binaries that render via
// CALayerHost compositing instead of a native window.
inline constexpr char kHidden[] = "hidden";
```

Insert after the `kRemoteDebuggingAddress` block (line 51).

**b. `content/shell/browser/shell_platform_delegate_mac.mm`** — Add `--hidden`
flag support:

- Add `#include "base/command_line.h"` to includes
- Add `#include "content/shell/common/shell_switches.h"` to includes
- Replace `[window makeKeyAndOrderFront:nil]` with the `--hidden` conditional:

```objc
if (base::CommandLine::ForCurrentProcess()->HasSwitch(switches::kHidden)) {
    [window setAlphaValue:0.0];
    [window orderWindow:NSWindowBelow relativeTo:0];
} else {
    [window makeKeyAndOrderFront:nil];
}
```

**c. `content/shell/browser/shell_devtools_frontend.h`** — Move constructor and
destructor from `private:` to `public:`. Add comment:

```cpp
// Construct with an existing Shell. The frontend observes the Shell's
// WebContents and attaches DevTools bindings when the DOM loads.
ShellDevToolsFrontend(Shell* frontend_shell, WebContents* inspected_contents);
~ShellDevToolsFrontend() override;
```

**d. `BUILD.gn` (root)** — Add only `libtermsurf_chromium` to `gn_all`:

```gn
"//content/libtermsurf_chromium:libtermsurf_chromium",
```

Insert after the `content/browser/interest_group/tools:adjustable_auction` line
(~line 196). Do NOT add `chromium_profile_server` or `plusium`.

Commit: "Patch stock Chromium for libtermsurf_chromium"

**4. Build**

```bash
gn gen out/Default
autoninja -C out/Default libtermsurf_chromium
```

Note: `gn gen` is required because we're on a fresh branch with a new GN target.
The build cache from issue-707 is still present in `out/Default/` — the
incremental build should be fast since most object files haven't changed.

**5. Verify**

```bash
ls -la out/Default/liblibtermsurf_chromium.dylib
# or
ls -la out/Default/libtermsurf_chromium.dylib
```

Check the actual output name — GN shared_library targets may or may not prepend
`lib` depending on the target name. Also verify:

```bash
nm -gU out/Default/*termsurf_chromium*.dylib | grep "ts_"
```

All 20 `ts_*` symbols should be visible.

### Success criteria

- Branch `146.0.7650.0-issue-708` exists with exactly 2 commits on top of the
  vanilla tag
- `content/libtermsurf_chromium/` exists with 16 files, all references renamed
- No `content/chromium_profile_server/`, no `content/plusium/`, no forked
  `content/shell/` files
- The 8 stock Chromium patches are applied (shell_switches.h,
  shell_platform_delegate_mac.mm, shell_devtools_frontend.h, root BUILD.gn,
  render_widget_host_view_mac.h/.mm, render_widget_host_impl.h/.cc)
- `autoninja -C out/Default libtermsurf_chromium` succeeds
- The `.dylib` exists and exports all 20 `ts_*` symbols

### Result

**Success.** Branch `146.0.7650.0-issue-708` has 2 commits, 24 files changed
(2,013 insertions). Build produced `libtermsurf_chromium.dylib` (10.8 MB) with
23 exported `ts_*` symbols. The original issue doc listed 4 stock patches, but
the build revealed 4 more — CALayerParams callback on `RenderWidgetHostViewMac`
and cursor change callback on `RenderWidgetHostImpl` — both needed by the
library for compositing and cursor forwarding.

## Experiment 2: Update Roamium to link libtermsurf_chromium

### Goal

Change Roamium's build configuration to link `libtermsurf_chromium` instead of
`libtermsurf_content`. Update build scripts to remove dead
`chromium_profile_server` and `plusium` autoninja targets. Verify end-to-end:
launch TermSurf, type `web google.com`, confirm page loads.

### What changes

**`roamium/build.rs`** (2 lines):

- Line 11 comment: `libtermsurf_content.dylib` → `libtermsurf_chromium.dylib`
- Line 13 link directive: `termsurf_content` → `termsurf_chromium`

**`scripts/build-debug.sh`** (1 line):

- Line 50: `autoninja -C out/Default chromium_profile_server plusium` →
  `autoninja -C out/Default libtermsurf_chromium`

**`scripts/build-release.sh`** (1 line):

- Line 50: `autoninja -C out/Default chromium_profile_server plusium` →
  `autoninja -C out/Default libtermsurf_chromium`

### What does NOT change

- `roamium/src/ffi.rs` — FFI declarations reference C function names (`ts_*`),
  not the library name. No change needed.
- `roamium/src/main.rs`, `dispatch.rs`, `ipc.rs`, `proto.rs` — No library name
  references.
- `roamium/Cargo.toml` — No library name references.

### Steps

1. Edit `roamium/build.rs` — update comment and link directive.
2. Edit `scripts/build-debug.sh` — replace autoninja targets.
3. Edit `scripts/build-release.sh` — replace autoninja targets.
4. `cargo clean && cargo build` — force relink against the new dylib name.
5. Copy binary: `cp target/debug/roamium chromium/src/out/Default/roamium`
6. Launch TermSurf, type `web google.com`, verify page loads.
7. `cargo build --release` — verify release build too.

### Success criteria

- `cargo build` succeeds (links `libtermsurf_chromium.dylib`)
- `cargo build --release` succeeds
- `web google.com` loads in TermSurf with `--browser roamium`
- Build scripts reference only `libtermsurf_chromium` (no dead targets)

### Result

**Success.** Three files changed: `build.rs` link directive, both build scripts.
Debug and release builds link `libtermsurf_chromium.dylib`. `web google.com`
loads end-to-end. The clean Chromium branch is a verified drop-in replacement.

## Experiment 3: GUI cleanup, patches, and documentation

### Goal

Remove dead browser entries from the GUI, make `roamium` the default browser,
generate the fresh patch set for issue-708, and update `chromium/README.md`.
After this, the entire stack — GUI, Roamium, Chromium fork, patches, docs — is
consistent and complete.

### Part A: GUI browser registry

**`gui/src/apprt/xpc.zig`** — `initBrowserRegistry()` (line 843):

Remove the `chromium` and `plusium` entries, keep only `roamium`:

```zig
const browsers = [_]struct { name: []const u8, suffix: []const u8 }{
    .{ .name = "roamium", .suffix = "/dev/termsurf/chromium/src/out/Default/roamium" },
};
```

**`gui/src/apprt/xpc.zig`** — `resolveBrowserPath()` (line 886):

Change the default from `"chromium"` to `"roamium"`:

```zig
const name = if (browser.len == 0) "roamium" else browser;
```

After this, `web google.com` works without `--browser roamium`.

### Part B: Generate patches

```bash
cd ~/dev/termsurf/chromium/src
rm -rf ../../chromium/patches/issue-708/
git format-patch 146.0.7650.0..HEAD -o ../../chromium/patches/issue-708/
```

This should produce exactly 2 patch files (vs. issue-707's 68).

### Part C: Update chromium/README.md

1. Update current branch: `146.0.7650.0-issue-707` → `146.0.7650.0-issue-708`
2. Add issue-708 to the Branches table
3. Update the build target in the Build section: `chromium_profile_server` →
   `libtermsurf_chromium`

### Steps

1. Edit `gui/src/apprt/xpc.zig` — remove dead browser entries, change default.
2. `cd gui && zig build` — verify GUI compiles.
3. Generate patches for issue-708.
4. Update `chromium/README.md`.
5. Launch TermSurf, type `web google.com` (no `--browser` flag) — verify it
   works with roamium as default.

### Success criteria

- GUI compiles with only `roamium` in browser registry
- `web google.com` works without `--browser roamium`
- `chromium/patches/issue-708/` contains exactly 2 patches
- `chromium/README.md` lists issue-708 as current branch with
  `libtermsurf_chromium` build target

### Result

**Success.** Removed `chromium` and `plusium` from the browser registry, made
`roamium` the default in three places (`initBrowserRegistry`,
`resolveBrowserPath`, `getOrCreateServer`). Generated 2 patches for issue-708
(down from 68). Updated README with new branch and build target. `web` works
without `--browser` flag.

## Conclusion

Issue 708 is complete. The Chromium fork is now a clean, minimal branch
containing only `libtermsurf_chromium` — a 16-file shared library that wraps
Chromium's Content API through 23 exported C functions. Everything else is gone.

### What changed

**Chromium fork** — Branch `146.0.7650.0-issue-708` has 2 commits and 24 files
on top of the vanilla tag. Compare this to issue-707's 68 commits and 258 files.
The entire TermSurf footprint inside Chromium is now:

- `content/libtermsurf_chromium/` — 16 source files (renamed from
  `libtermsurf_content`)
- 8 stock patches — 4 to Content Shell (hidden window, public DevTools
  constructor, `kHidden` switch, GN target) and 4 to renderer host
  (CALayerParams callback, cursor change callback)

**Roamium** — Links `libtermsurf_chromium.dylib` instead of
`libtermsurf_content.dylib`. One line changed in `build.rs`.

**GUI** — Browser registry contains only `roamium`. The default browser resolves
to `roamium` in all three code paths (`initBrowserRegistry`,
`resolveBrowserPath`, `getOrCreateServer`). `web google.com` works without
`--browser roamium`.

**Build scripts** — `build-debug.sh` and `build-release.sh` build
`libtermsurf_chromium` instead of `chromium_profile_server plusium`.

**Patches** — `chromium/patches/issue-708/` contains 2 patch files (down from
68).

### What was deleted

- **Chromium Profile Server** — ~100 forked Content Shell files, ~1,050 lines of
  TermSurf code. Gone.
- **Plusium** — 511-line C++ binary at `content/plusium/`. Gone.
- **Browser registry entries** — `chromium` and `plusium` entries in GUI. Gone.
- **68 commits of history** — Replaced by 2 clean commits from the vanilla tag.

### Why this matters

Every upstream Chromium merge now touches 24 files instead of 258. The patch set
is 2 files instead of 68. The only TermSurf code inside the Chromium tree is a
self-contained shared library with a stable C API. Roamium — built with Cargo
outside the tree — is the sole browser binary. The architecture is clean: Zig
GUI → Unix sockets → Rust Roamium → C API → Chromium Content API.
