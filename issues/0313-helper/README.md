+++
status = "closed"
opened = "2026-01-31"
closed = "2026-01-31"
+++

# Issue 313: Bundle termsurf-profile as Helper App

## Background

### Current State

In issue 312, we fixed the focus-stealing problem by programmatically setting
`NSApplicationActivationPolicyProhibited` at the start of `termsurf-profile`'s
`main()` function. This works, but it's not the "proper" macOS pattern for
background helper processes.

Currently, `termsurf-profile` is placed directly in:

```
wezterm-gui.app/Contents/MacOS/termsurf-profile
```

This is unconventional. macOS expects helper processes to be bundled as separate
`.app` bundles inside the `Frameworks` directory, each with their own
`Info.plist` that declares `LSUIElement=1`.

### The Proper Pattern

CEF's own helper processes follow this pattern:

```
wezterm-gui.app/
└── Contents/
    └── Frameworks/
        ├── WezTerm Helper.app/
        │   └── Contents/
        │       ├── MacOS/WezTerm Helper
        │       └── Info.plist          ← Contains LSUIElement=1
        ├── WezTerm Helper (GPU).app/
        ├── WezTerm Helper (Renderer).app/
        └── ...
```

Each helper app has an `Info.plist` with:

```xml
<key>LSUIElement</key>
<string>1</string>
```

This tells macOS at the bundle level that this is a UI Element (background
process) that should never appear in the dock or receive focus.

### Why This Matters

1. **Declarative vs Imperative**: `LSUIElement` is a static declaration that
   macOS reads when the process launches. Our current fix requires code to run
   before CEF initializes — if CEF ever changes its initialization order, our
   fix could break.

2. **Code Signing**: Proper helper app bundles are easier to code sign correctly
   for distribution. Apple's notarization process expects this structure.

3. **Consistency**: Following the same pattern as CEF's own helpers makes the
   bundle structure more predictable and maintainable.

4. **Removal of Workaround**: Once bundled properly, we can remove the
   `set_background_activation_policy()` function and the `objc` dependency from
   termsurf-profile — the `Info.plist` handles it declaratively.

## Goal

Bundle `termsurf-profile` as `TermSurf Profile Helper.app` inside the Frameworks
directory, with proper `Info.plist` containing `LSUIElement=1`.

## Implementation

### New Bundle Structure

```
wezterm-gui.app/
└── Contents/
    ├── MacOS/
    │   ├── wezterm-gui
    │   ├── wezterm
    │   └── web
    └── Frameworks/
        ├── Chromium Embedded Framework.framework/
        ├── WezTerm Helper.app/
        ├── WezTerm Helper (GPU).app/
        ├── WezTerm Helper (Renderer).app/
        ├── WezTerm Helper (Plugin).app/
        ├── WezTerm Helper (Alerts).app/
        └── TermSurf Profile Helper.app/      ← NEW
            └── Contents/
                ├── MacOS/
                │   └── termsurf-profile
                └── Info.plist
```

### Files to Create

**`ts3/termsurf-profile/helper-app/Info.plist`**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>TermSurf Profile Helper</string>
    <key>CFBundleIdentifier</key>
    <string>com.termsurf.profile-helper</string>
    <key>CFBundleDisplayName</key>
    <string>TermSurf Profile Helper</string>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleVersion</key>
    <string>1.0.0</string>
    <key>CFBundleExecutable</key>
    <string>termsurf-profile</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>LSEnvironment</key>
    <dict>
        <key>MallocNanoZone</key>
        <string>0</string>
    </dict>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSUIElement</key>
    <string>1</string>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
```

### Files to Modify

**`ts3/scripts/build-debug.sh`**

Replace the current termsurf-profile copy (line 73):

```bash
cp "$REPO_DIR/target/debug/termsurf-profile" "$APP_BUNDLE/Contents/MacOS/"
```

With helper app creation:

```bash
# Create TermSurf Profile Helper app bundle
PROFILE_HELPER="$APP_BUNDLE/Contents/Frameworks/TermSurf Profile Helper.app"
mkdir -p "$PROFILE_HELPER/Contents/MacOS"
cp "$REPO_DIR/target/debug/termsurf-profile" "$PROFILE_HELPER/Contents/MacOS/"
cp "$REPO_DIR/termsurf-profile/helper-app/Info.plist" "$PROFILE_HELPER/Contents/"
```

Also update the build summary (lines 164-168) to reflect the new location.

**`ts3/scripts/build-release.sh`**

Make the same changes as build-debug.sh.

**`ts3/termsurf-launcher/src/main.rs`**

Update the path resolution (lines 49-66):

```rust
// Path to profile server binary
// Launcher is at: .app/Contents/XPCServices/com.termsurf.launcher.xpc/Contents/MacOS/termsurf-launcher
// Profile is at:  .app/Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/termsurf-profile
let exe_path = env::current_exe().expect("Failed to get exe path");
let profile_bin_path = exe_path
    .parent() // MacOS
    .and_then(|p| p.parent()) // Contents
    .and_then(|p| p.parent()) // com.termsurf.launcher.xpc
    .and_then(|p| p.parent()) // XPCServices
    .and_then(|p| p.parent()) // Contents
    .map(|p| {
        p.join("Frameworks")
            .join("TermSurf Profile Helper.app")
            .join("Contents/MacOS/termsurf-profile")
    })
    .unwrap_or_else(|| {
        // Fallback for testing outside app bundle
        exe_path
            .parent()
            .map(|p| p.join("termsurf-profile"))
            .unwrap_or_default()
    });
```

**`ts3/termsurf-profile/src/main.rs`**

Remove the programmatic activation policy code (optional but recommended):

- Delete the `set_background_activation_policy()` function
- Delete the call to it in `main()`

**`ts3/termsurf-profile/Cargo.toml`**

Remove the `objc` dependency (optional but recommended):

```diff
 [target.'cfg(target_os = "macos")'.dependencies]
 cef = { path = "../../cef-rs/cef", features = ["accelerated_osr"] }
 ctrlc = "3.4"
-objc = "0.2"
```

## Verification

```bash
# Build and open
cd ts3 && ./scripts/build-debug.sh --open

# 1. Verify bundle structure
ls -la target/debug/wezterm-gui.app/Contents/Frameworks/ | grep Profile
# Expected: "TermSurf Profile Helper.app"

ls -la "target/debug/wezterm-gui.app/Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/"
# Expected: termsurf-profile binary

cat "target/debug/wezterm-gui.app/Contents/Frameworks/TermSurf Profile Helper.app/Contents/Info.plist" | grep -A1 LSUIElement
# Expected: <key>LSUIElement</key> followed by <string>1</string>

# 2. Verify termsurf-profile NOT in MacOS folder
ls target/debug/wezterm-gui.app/Contents/MacOS/ | grep profile
# Expected: no output (termsurf-profile should not be there)

# 3. Test functionality
web google.com
# Expected: No dock icon, no focus stealing, webview renders

# 4. Test multiple webviews
web github.com
web apple.com
# Expected: All work, no dock icons

# 5. Check logs for correct path
grep "Profile binary path" /tmp/termsurf-launcher.log
# Expected: Path includes "Frameworks/TermSurf Profile Helper.app"
```

## Success Criteria

1. [x] `TermSurf Profile Helper.app` exists in Frameworks directory
2. [x] Helper app contains correct `Info.plist` with `LSUIElement=1`
3. [x] `termsurf-profile` binary is inside helper app, not in MacOS folder
4. [x] Launcher correctly resolves path to helper app binary
5. [x] No dock icon appears when opening webviews
6. [x] No focus stealing occurs
7. [x] Webviews render and resize correctly
8. [x] (Optional) `objc` dependency removed from termsurf-profile

## Next Steps for ts3

After completing this issue, the following work remains for ts3:

### High Priority

1. **Input Forwarding** — Keyboard and mouse events need to be forwarded from
   the terminal to the webview. Currently webviews are display-only.

2. **Profile Process Reuse** — When a second `web` command uses the same
   profile, it should reuse the existing profile process rather than spawning a
   new one. The launcher has infrastructure for this (`running_profiles` map)
   but it needs testing and polish.

3. **Dynamic Resize** — Issue 311 fixed resize accuracy, but resize commands
   need to be sent when panes change size (splits, window resize). This may
   already work but needs verification.

### Medium Priority

4. **Webview Pane Lifecycle** — Clean up resources when a webview pane is
   closed. Remove XPC connections, deallocate textures, potentially shut down
   profile process if no browsers remain.

5. **Error Handling** — Handle profile process crashes gracefully. Show error in
   pane, allow retry.

6. **Multiple Profiles** — Test and verify that different profiles (e.g.,
   `web --profile work google.com`) use separate CEF processes with isolated
   cookies/storage.

### Lower Priority

7. **Navigation Commands** — Back, forward, refresh, stop, go to URL.

8. **DevTools** — Open Chrome DevTools for debugging web pages.

9. **Tab Bar / URL Display** — Show current URL, loading state, page title.

10. **Bookmarks** — Save and restore URLs.

## References

- Issue 312: Focus stealing fix (programmatic approach)
- [Apple: LSUIElement](https://developer.apple.com/documentation/bundleresources/information_property_list/lsuielement)
- [Apple: Bundle Structure](https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/BundleTypes/BundleTypes.html)

---

## Experiments

### Experiment 1: Bundle as Helper App with LSUIElement

**Goal:** Replace the programmatic activation policy workaround with proper
macOS helper app bundling, using `LSUIElement=1` in the helper's `Info.plist`.

**Hypothesis:** If we bundle `termsurf-profile` inside a proper `.app` bundle
with `LSUIElement=1`, macOS will treat it as a background process declaratively,
and we can remove the `objc` dependency and runtime workaround code.

#### Changes

**Step 1: Create Info.plist for helper app**

Create `ts3/termsurf-profile/helper-app/Info.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>TermSurf Profile Helper</string>
    <key>CFBundleIdentifier</key>
    <string>com.termsurf.profile-helper</string>
    <key>CFBundleDisplayName</key>
    <string>TermSurf Profile Helper</string>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleVersion</key>
    <string>1.0.0</string>
    <key>CFBundleExecutable</key>
    <string>termsurf-profile</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>LSEnvironment</key>
    <dict>
        <key>MallocNanoZone</key>
        <string>0</string>
    </dict>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSUIElement</key>
    <string>1</string>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
```

**Step 2: Update build-debug.sh**

Replace line 73:

```bash
cp "$REPO_DIR/target/debug/termsurf-profile" "$APP_BUNDLE/Contents/MacOS/"
```

With:

```bash
# Create TermSurf Profile Helper app bundle (proper macOS helper pattern)
PROFILE_HELPER="$APP_BUNDLE/Contents/Frameworks/TermSurf Profile Helper.app"
mkdir -p "$PROFILE_HELPER/Contents/MacOS"
cp "$REPO_DIR/target/debug/termsurf-profile" "$PROFILE_HELPER/Contents/MacOS/"
cp "$REPO_DIR/termsurf-profile/helper-app/Info.plist" "$PROFILE_HELPER/Contents/"
```

Update the build summary to show new location.

**Step 3: Update build-release.sh**

Make the same changes as build-debug.sh (adjust path for release build).

**Step 4: Update launcher path resolution**

In `ts3/termsurf-launcher/src/main.rs`, update the path to find the helper app:

```rust
// Path to profile server binary
// Launcher is at: .app/Contents/XPCServices/com.termsurf.launcher.xpc/Contents/MacOS/termsurf-launcher
// Profile is at:  .app/Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/termsurf-profile
let exe_path = env::current_exe().expect("Failed to get exe path");
let profile_bin_path = exe_path
    .parent() // MacOS
    .and_then(|p| p.parent()) // Contents
    .and_then(|p| p.parent()) // com.termsurf.launcher.xpc
    .and_then(|p| p.parent()) // XPCServices
    .and_then(|p| p.parent()) // Contents
    .map(|p| {
        p.join("Frameworks")
            .join("TermSurf Profile Helper.app")
            .join("Contents/MacOS/termsurf-profile")
    })
    .unwrap_or_else(|| {
        // Fallback for testing outside app bundle
        exe_path
            .parent()
            .map(|p| p.join("termsurf-profile"))
            .unwrap_or_default()
    });
```

**Step 5: Remove workaround code (optional)**

In `ts3/termsurf-profile/src/main.rs`:

- Delete `set_background_activation_policy()` function
- Delete the call to it in `main()`

In `ts3/termsurf-profile/Cargo.toml`:

- Remove `objc = "0.2"` dependency

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# 1. Verify helper app exists
ls -la target/debug/wezterm-gui.app/Contents/Frameworks/ | grep Profile

# 2. Verify binary is in helper app
ls "target/debug/wezterm-gui.app/Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/"

# 3. Verify LSUIElement in Info.plist
grep -A1 LSUIElement "target/debug/wezterm-gui.app/Contents/Frameworks/TermSurf Profile Helper.app/Contents/Info.plist"

# 4. Verify NOT in MacOS folder
ls target/debug/wezterm-gui.app/Contents/MacOS/ | grep -c profile
# Expected: 0

# 5. Check launcher log for correct path
grep "Profile binary path" /tmp/termsurf-launcher.log

# 6. Test webview functionality
web google.com
# No dock icon, no focus stealing, renders correctly

# 7. Test multiple webviews
web github.com
web apple.com
```

#### Success Criteria

1. [x] Helper app bundle created at `Frameworks/TermSurf Profile Helper.app`
2. [x] `Info.plist` contains `LSUIElement=1`
3. [x] Binary located inside helper app, not in `Contents/MacOS/`
4. [x] Launcher finds and spawns the binary correctly
5. [ ] No dock icon when opening webviews
6. [ ] No focus stealing
7. [ ] Webviews render correctly
8. [ ] Resize still works (issue 311 fixes intact)
9. [x] (Optional) `objc` dependency removed, workaround code deleted

#### Result

**Failed.** The profile process crashes immediately on startup before any
webview can be created.

#### Conclusion

The bundle structure changes were successful (criteria 1-4, 9), but the profile
process fails to start because the CEF library loader cannot find the framework.

**Root Cause:**

The `LibraryLoader` in `cef-rs/cef/src/library_loader.rs` uses the binary's
location to resolve the CEF framework path. It has two modes controlled by a
`helper` parameter:

```rust
let resolver = if helper { "../../.." } else { "../Frameworks" };
```

- `helper: false` → `../Frameworks` (for binaries in `Contents/MacOS/`)
- `helper: true` → `../../..` (for binaries in
  `Contents/Frameworks/Helper.app/Contents/MacOS/`)

The current code in `termsurf-profile/src/main.rs:115` calls:

```rust
let _loader = LibraryLoader::new(&exe, false);
```

With the binary now at:

```
Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/termsurf-profile
```

The `false` (non-helper) path resolves to:

```
Contents/Frameworks/TermSurf Profile Helper.app/Contents/Frameworks/
```

This path doesn't exist, causing `canonicalize().unwrap()` to panic.

**Error from log:**

```
thread 'main' panicked at library_loader.rs:20:14:
called `Result::unwrap()` on an `Err` value: Os { code: 2, kind: NotFound, message: "No such file or directory" }
```

**Hypothesis for Fix:**

Change the `LibraryLoader::new()` call from `helper: false` to `helper: true`:

```rust
let _loader = LibraryLoader::new(&exe, true);
```

This will use the `../../..` resolver, which correctly navigates from:

```
Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/
    ↑ ..    → Contents/Frameworks/TermSurf Profile Helper.app/Contents/
    ↑ ../.. → Contents/Frameworks/TermSurf Profile Helper.app/
    ↑ ../../.. → Contents/Frameworks/
```

Then joins with `Chromium Embedded Framework.framework/...` to find the correct
framework location.

---

### Experiment 2: Fix CEF Library Loader Path Resolution

**Goal:** Fix the CEF framework loading failure from Experiment 1 by telling the
`LibraryLoader` that termsurf-profile is now a helper app.

**Problem:** Experiment 1 successfully bundled termsurf-profile as a helper app,
but the process crashes on startup because `LibraryLoader::new(&exe, false)`
uses the wrong path resolver. The `false` parameter assumes the binary is at
`Contents/MacOS/`, but it's now at
`Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/`.

**Hypothesis:** Changing `helper: false` to `helper: true` will make the library
loader use the correct path resolution for helper app binaries.

#### Changes

**File: `ts3/termsurf-profile/src/main.rs`**

Change line ~115 from:

```rust
let _loader = LibraryLoader::new(&exe, false);
```

To:

```rust
let _loader = LibraryLoader::new(&exe, true);
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# 1. Verify helper app structure (from Experiment 1)
ls -la target/debug/wezterm-gui.app/Contents/Frameworks/ | grep Profile
ls "target/debug/wezterm-gui.app/Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/"
grep -A1 LSUIElement "target/debug/wezterm-gui.app/Contents/Frameworks/TermSurf Profile Helper.app/Contents/Info.plist"

# 2. Verify NOT in MacOS folder
ls target/debug/wezterm-gui.app/Contents/MacOS/ | grep -c profile
# Expected: 0

# 3. Check launcher log for correct path
grep "Profile binary path" /tmp/termsurf-launcher.log

# 4. Check profile log for successful CEF initialization
grep "CEF framework loaded" /tmp/termsurf-profile-default.log
# Expected: "Profile: CEF framework loaded"

# 5. Test webview functionality
web google.com
# Expected: No dock icon, no focus stealing, webview renders

# 6. Test multiple webviews
web github.com
web apple.com

# 7. Test resize
# Drag window edge, verify no black borders
```

#### Success Criteria

1. [x] Helper app bundle exists at `Frameworks/TermSurf Profile Helper.app`
2. [x] `Info.plist` contains `LSUIElement=1`
3. [x] Binary located inside helper app, not in `Contents/MacOS/`
4. [x] Launcher finds and spawns the binary correctly
5. [x] CEF framework loads successfully (no panic)
6. [ ] No dock icon when opening webviews
7. [ ] No focus stealing
8. [ ] Webviews render correctly
9. [ ] Resize still works (issue 311 fixes intact)
10. [x] `objc` dependency removed, workaround code deleted

#### Result

**Failed.** The CEF framework now loads successfully, but the GPU and network
subprocesses crash immediately, preventing any rendering.

#### Conclusion

The `LibraryLoader` fix worked — CEF framework loading succeeded. However, a new
path resolution problem emerged: the CEF helper subprocess path is now wrong.

**Symptom:**

The profile log shows:

```
Profile: Helper: ".../TermSurf Profile Helper.app/Contents/Frameworks/WezTerm Helper.app/..." (exists=false)
```

Followed by GPU process crashes:

```
GPU process launch failed: error_code=1003
GPU process isn't usable. Goodbye.
```

**Root Cause:**

The helper path is computed at `termsurf-profile/src/main.rs:175-179`:

```rust
let app_contents = exe.parent().unwrap().parent().unwrap();
let helper_path = app_contents
    .join("Frameworks")
    .join("WezTerm Helper.app")
    .join("Contents/MacOS/WezTerm Helper");
```

This assumes the binary is 2 levels deep from `Contents/`:

- Old location: `Contents/MacOS/termsurf-profile` → `.parent().parent()` =
  `Contents/` ✓

But now the binary is 5 levels deep:

- New location:
  `Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/termsurf-profile`
- `.parent().parent()` = `TermSurf Profile Helper.app/Contents/` ✗

The code looks for `WezTerm Helper.app` inside the profile helper app instead of
at the main app's Frameworks level.

**Hypothesis for Fix:**

Change the path computation to go up 5 levels instead of 2:

```rust
let app_contents = exe
    .parent().unwrap()  // MacOS/
    .parent().unwrap()  // Contents/
    .parent().unwrap()  // TermSurf Profile Helper.app/
    .parent().unwrap()  // Frameworks/
    .parent().unwrap(); // Contents/ (of main app)
```

This is becoming complex. Moving a binary into a helper app bundle requires
updating every relative path calculation in the code.

---

### Experiment 3: Fix CEF Helper Subprocess Path

**Goal:** Fix the WezTerm Helper path resolution so CEF can launch its GPU and
network subprocesses.

**Problem:** Experiment 2 fixed CEF framework loading, but the helper subprocess
path is still computed assuming the binary is 2 levels from `Contents/`. Now
that it's 5 levels deep, the path resolves inside the profile helper app instead
of at the main app's Frameworks level.

**Hypothesis:** Changing `exe.parent().parent()` to go up 5 levels will
correctly resolve to the main app's `Contents/` directory.

#### Changes

**File: `ts3/termsurf-profile/src/main.rs`**

Change the `app_contents` computation (around line 175) from:

```rust
let app_contents = exe.parent().unwrap().parent().unwrap();
```

To:

```rust
// Navigate from helper app binary to main app's Contents/
// Binary is at: Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/termsurf-profile
let app_contents = exe
    .parent().unwrap()  // MacOS/
    .parent().unwrap()  // Contents/ (of helper app)
    .parent().unwrap()  // TermSurf Profile Helper.app/
    .parent().unwrap()  // Frameworks/
    .parent().unwrap(); // Contents/ (of main app)
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# 1. Verify helper app structure
ls -la target/debug/wezterm-gui.app/Contents/Frameworks/ | grep Profile

# 2. Check profile log for successful helper path
grep "Helper:" /tmp/termsurf-profile-default.log
# Expected: Path should end with ".app/Contents/Frameworks/WezTerm Helper.app/..." and (exists=true)

# 3. Check for CEF initialization success
grep "CEF initialized" /tmp/termsurf-profile-default.log

# 4. Check for NO GPU errors
grep -c "GPU process launch failed" /tmp/termsurf-profile-default.log
# Expected: 0

# 5. Test webview functionality
web google.com
# Expected: No dock icon, no focus stealing, webview renders

# 6. Test multiple webviews
web github.com
web apple.com

# 7. Test resize
# Drag window edge, verify no black borders
```

#### Success Criteria

1. [x] Helper app bundle exists at `Frameworks/TermSurf Profile Helper.app`
2. [x] `Info.plist` contains `LSUIElement=1`
3. [x] Binary located inside helper app, not in `Contents/MacOS/`
4. [x] Launcher finds and spawns the binary correctly
5. [x] CEF framework loads successfully
6. [x] WezTerm Helper path resolves correctly (exists=true)
7. [x] No GPU process launch failures
8. [x] No dock icon when opening webviews
9. [x] No focus stealing
10. [x] Webviews render correctly
11. [x] Resize still works (issue 311 fixes intact)
12. [x] `objc` dependency removed, workaround code deleted

#### Result

**Success.** All criteria met. The webview renders correctly, no dock icon
appears, and resize continues to work.

#### Conclusion

**What we did:**

We bundled `termsurf-profile` as a proper macOS helper app
(`TermSurf Profile Helper.app`) inside `Contents/Frameworks/`, with an
`Info.plist` declaring `LSUIElement=1`. This replaces the runtime workaround
from issue 312 (programmatically setting
`NSApplicationActivationPolicyProhibited` via the `objc` crate) with the
declarative macOS pattern.

**Why we did it:**

1. **Declarative vs Imperative** — `LSUIElement` is read by macOS at launch
   time, before any code runs. The previous workaround required code to execute
   before CEF initialized, which was fragile.

2. **Code signing compatibility** — Proper helper app bundles are the expected
   structure for notarization and distribution. Apple's tooling assumes this
   layout.

3. **Consistency** — CEF's own helpers (GPU, Renderer, etc.) follow this exact
   pattern. Our profile server now matches their structure.

4. **Reduced dependencies** — Removed the `objc` crate dependency and ~20 lines
   of unsafe FFI workaround code.

**How it worked:**

The implementation required three experiments to get all the path resolutions
correct:

| Experiment | Problem                                  | Fix                                                  |
| ---------- | ---------------------------------------- | ---------------------------------------------------- |
| 1          | CEF framework not found                  | — (identified root cause)                            |
| 2          | `LibraryLoader` used wrong path resolver | Changed `helper: false` → `helper: true`             |
| 3          | Helper subprocess path wrong             | Changed `app_contents` from 2 to 5 `.parent()` calls |

The key insight is that moving a binary into a helper app bundle changes its
depth from 2 levels (`Contents/MacOS/`) to 5 levels
(`Contents/Frameworks/Helper.app/Contents/MacOS/`). Every relative path
calculation must account for this.

**Files modified:**

- `ts3/termsurf-profile/helper-app/Info.plist` — NEW: Declares LSUIElement=1
- `ts3/scripts/build-debug.sh` — Creates helper app bundle in Frameworks/
- `ts3/scripts/build-release.sh` — Same for release builds
- `ts3/termsurf-launcher/src/main.rs` — Updated path to find binary in helper
  app
- `ts3/termsurf-profile/src/main.rs` — Fixed LibraryLoader and app_contents
  paths
- `ts3/termsurf-profile/Cargo.toml` — Removed `objc` dependency

**Next steps:**

This issue is complete. The remaining ts3 work (from the "Next Steps" section
above) includes:

1. **Input forwarding** — Keyboard and mouse events to webviews
2. **Profile process reuse** — Reuse existing profile process for same-profile
   webviews
3. **Dynamic resize** — Send resize commands when panes change size
4. **Webview lifecycle** — Clean up resources when panes close
