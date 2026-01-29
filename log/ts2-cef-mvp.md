# CEF Integration: Incremental Plan

## Purpose

We're trying to get CEF (Chromium Embedded Framework) to load inside WezTerm as
the foundation for TermSurf 2.0.

**The bigger picture:** TermSurf is a terminal emulator with browser/webview
support. TermSurf 1.x uses WKWebView, which is macOS-only. TermSurf 2.0 uses CEF
(Chromium) to enable cross-platform browser panes on Linux, Windows, and macOS.

**Our immediate goal:** The absolute minimum - get CEF to build and load inside
WezTerm with no errors. Not render a web page. Not integrate with the UI. Not do
anything useful. Just:

1. Compile with CEF linked in
2. Load the CEF framework at runtime
3. Initialize CEF successfully
4. Shut down cleanly

This proves CEF can coexist with WezTerm. Everything else builds on top of that
foundation.

**Why we're being so careful:** The previous attempt failed completely. We spent
a day writing integration code without first verifying CEF could even load.
We're now doing the simplest possible thing first - just load CEF - before
writing any real integration code.

---

## Critical: Risk Monitoring

**This plan includes known risks for each step.** As we execute each step, we
must compare what actually happens against the anticipated risks.

**If we encounter an error or behavior that is NOT listed in the known risks for
that step, STOP IMMEDIATELY.** This indicates a fundamental gap in our
understanding. We must:

1. Document the unexpected issue
2. Investigate the root cause
3. Decide whether to revise the plan or scrap it entirely

Do not proceed to the next step if an unanticipated issue occurs. The previous
CEF integration attempt failed because we pushed forward despite unexpected
errors, wasting an entire day. We will not repeat that mistake.

---

## Overview

**Two WezTerm.app locations:**

- `assets/macos/WezTerm.app/` → Template (checked into repo, read-only)
- `target/release/WezTerm.app/` → Built bundle (created during build)

---

## Step 1: Add CEF Dependency (Compile Only) ✅

**Goal:** Verify CEF links correctly with wezterm-gui.

**Changes:**

1. Edit `wezterm-gui/Cargo.toml` - add feature and dependency:

```toml
[features]
cef = ["dep:cef"]

[target.'cfg(target_os = "macos")'.dependencies]
cef = { path = "../../cef-rs/cef", optional = true }
```

2. Edit `wezterm-gui/src/main.rs` - add minimal CEF reference:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
fn cef_compiled() {
    let _ = cef::api_hash;
}
```

**Test:**

```bash
cargo build -p wezterm-gui --features cef
```

**Success criteria:**

- Build completes with no errors
- Binary exists at `target/debug/wezterm-gui`

**Known risks:**

- **Low: Build time.** First build will download CEF (~400MB) and compile
  `cef_dll_wrapper`. This can take 5-10 minutes. This is expected, not a
  failure.
- **Low: cmake/ninja missing.** If cmake or ninja isn't installed, the build
  will fail with a clear error message. Fix: install them via homebrew.

**Unanticipated issues (STOP if these occur):**

- Linking errors mentioning CEF symbols
- Rust compiler errors in cef crate code
- Path resolution errors for the cef dependency

**Results:**

- ✅ Build completed in ~1 minute (CEF was already cached from previous attempts)
- ✅ Binary exists at `target/debug/wezterm-gui` (170MB)
- ✅ No linking errors
- ✅ No unanticipated issues occurred

---

## Step 2: Create Helper Binary (Compile Only) ✅

**Goal:** Verify helper binary compiles.

**Changes:**

1. Create `wezterm-gui/src/bin/wezterm-cef-helper.rs`:

```rust
use cef::{args::Args, execute_process, library_loader, App};

fn main() {
    let args = Args::new();

    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(
            &std::env::current_exe().unwrap(),
            true,
        );
        assert!(loader.load());
        loader
    };

    execute_process(
        Some(args.as_main_args()),
        None::<&mut App>,
        std::ptr::null_mut(),
    );
}
```

2. Edit `wezterm-gui/Cargo.toml` - add bin target:

```toml
[[bin]]
name = "wezterm-cef-helper"
path = "src/bin/wezterm-cef-helper.rs"
required-features = ["cef"]
```

**Test:**

```bash
cargo build -p wezterm-gui --features cef
```

**Success criteria:**

- Both binaries exist:
  - `target/debug/wezterm-gui`
  - `target/debug/wezterm-cef-helper`

**Known risks:**

- **None identified.** This step should work if Step 1 succeeded. The helper
  code is copied directly from the working cef-rs example.

**Unanticipated issues (STOP if these occur):**

- Import errors for cef types (Args, App, execute_process, library_loader)
- Compiler errors in the helper code
- Binary not being produced despite successful compilation

**Results:**

- ✅ Build completed in ~3 seconds (incremental)
- ✅ Both binaries exist:
  - `target/debug/wezterm-gui` (170MB)
  - `target/debug/wezterm-cef-helper` (570KB)
- ✅ No import errors, no compiler errors
- ✅ No unanticipated issues occurred

---

## Step 3: Manually Create Bundle ✅

**Goal:** Create `target/release/WezTerm.app/` by copying from the template and
cef-osr.

**Prerequisites:**

- cef-osr bundle must exist at `/Users/ryan/dev/termsurf/cef-rs/cef-osr.app/`
- If not, build it first:
  `cd /Users/ryan/dev/termsurf/cef-rs && cargo build -p cef-osr && cargo run -p bundle-cef-app -- cef-osr -o cef-osr.app`

**Actions:**

```bash
# 1. Build release binaries
cargo build -p wezterm-gui --features cef --release

# 2. Copy template to target
cp -R assets/macos/WezTerm.app target/release/WezTerm.app

# 3. Create missing directories
mkdir -p target/release/WezTerm.app/Contents/MacOS
mkdir -p target/release/WezTerm.app/Contents/Frameworks

# 4. Copy main executable
cp target/release/wezterm-gui target/release/WezTerm.app/Contents/MacOS/

# 5. Copy CEF framework from cef-osr (known working)
cp -R "/Users/ryan/dev/termsurf/cef-rs/cef-osr.app/Contents/Frameworks/Chromium Embedded Framework.framework" target/release/WezTerm.app/Contents/Frameworks/

# 6. Create helper bundles by copying from cef-osr and modifying
CEF_OSR_FRAMEWORKS="/Users/ryan/dev/termsurf/cef-rs/cef-osr.app/Contents/Frameworks"
for suffix in "Helper" "Helper (GPU)" "Helper (Renderer)" "Helper (Plugin)" "Helper (Alerts)"; do
    SRC_BUNDLE="${CEF_OSR_FRAMEWORKS}/cef-osr ${suffix}.app"
    DEST_BUNDLE="target/release/WezTerm.app/Contents/Frameworks/WezTerm ${suffix}.app"

    # Copy entire helper bundle structure from cef-osr
    cp -R "${SRC_BUNDLE}" "${DEST_BUNDLE}"

    # Rename the executable
    mv "${DEST_BUNDLE}/Contents/MacOS/cef-osr ${suffix}" "${DEST_BUNDLE}/Contents/MacOS/WezTerm ${suffix}"

    # Replace with our helper binary
    cp target/release/wezterm-cef-helper "${DEST_BUNDLE}/Contents/MacOS/WezTerm ${suffix}"

    # Update Info.plist: replace "cef-osr" with "WezTerm" and update bundle identifier
    sed -i '' 's/cef-osr/WezTerm/g' "${DEST_BUNDLE}/Contents/Info.plist"
    sed -i '' 's/apps.tauri.cef-rs.WezTerm/com.github.wez.wezterm.helper/g' "${DEST_BUNDLE}/Contents/Info.plist"
done

# 7. Add MallocNanoZone to main app Info.plist (required for CEF on macOS)
# Insert after the opening <dict> tag
sed -i '' 's/<dict>/<dict>\
	<key>LSEnvironment<\/key>\
	<dict>\
		<key>MallocNanoZone<\/key>\
		<string>0<\/string>\
	<\/dict>/' target/release/WezTerm.app/Contents/Info.plist
```

**Test:**

```bash
# Verify bundle structure
ls -la target/release/WezTerm.app/Contents/Frameworks/

# Verify MallocNanoZone is in main plist
grep -A3 MallocNanoZone target/release/WezTerm.app/Contents/Info.plist

# Verify helper plists have correct executable names
grep CFBundleExecutable target/release/WezTerm.app/Contents/Frameworks/*/Contents/Info.plist
```

**Success criteria:**

- `ls` output shows:
  - `Chromium Embedded Framework.framework/`
  - `WezTerm Helper.app/`
  - `WezTerm Helper (GPU).app/`
  - `WezTerm Helper (Renderer).app/`
  - `WezTerm Helper (Plugin).app/`
  - `WezTerm Helper (Alerts).app/`
- `grep MallocNanoZone` shows the key exists with value `0`
- `grep CFBundleExecutable` shows `WezTerm Helper`, `WezTerm Helper (GPU)`, etc.

**Known risks:**

- **Low: sed command differences.** The sed commands assume the cef-osr
  Info.plist format matches what we expect. If the format differs slightly, sed
  may not make the replacements correctly. Mitigated by verifying with grep
  commands in the test step.
- **Low: ~200MB framework copy is slow.** The `cp -R` for the CEF framework will
  take 10-30 seconds. This is expected.

**Unanticipated issues (STOP if these occur):**

- cef-osr.app doesn't exist or has different structure than expected
- Permission errors copying files
- sed commands corrupt the plist files (check with `plutil -lint` if unsure)
- Framework copy fails partway through

**Results:**

- ✅ Release build completed in ~1 minute
- ✅ Bundle created with all components:
  - `Chromium Embedded Framework.framework/`
  - `WezTerm Helper.app/` (and GPU, Renderer, Plugin, Alerts variants)
- ✅ MallocNanoZone in plist with value 0
- ✅ All CFBundleExecutable values correct
- ✅ `plutil -lint` confirms plist is valid
- ⚠️ Minor: sed inserted MallocNanoZone multiple times (multiple `<dict>` tags) -
  this is the known "sed command differences" risk. Plist still valid, will clean up
  in Step 7.
- ✅ No unanticipated issues occurred

---

## Step 4: Run Without CEF Init ✅

**Goal:** Verify the bundle structure works before adding CEF code.

**Test:**

```bash
./target/release/WezTerm.app/Contents/MacOS/wezterm-gui
```

**Success criteria:**

- WezTerm launches normally
- Terminal works as expected

**Known risks:**

- **None identified.** WezTerm should run normally since we haven't added any
  CEF code yet. The bundle structure is just a directory layout at this point.

**Unanticipated issues (STOP if these occur):**

- App crashes on launch (indicates bundle structure problem)
- App won't start at all (check Console.app for crash logs)
- "App is damaged" or Gatekeeper warnings (code signing issue - can bypass with
  `xattr -cr` for testing)

**Results:**

- ✅ WezTerm launched successfully from bundle
- ✅ App ran for 3 seconds without issues
- ✅ App exited cleanly when terminated
- ✅ No crash, no Gatekeeper warnings
- ✅ No unanticipated issues occurred

---

## Step 5: Add CEF Loading Code ✅

**Goal:** Load and initialize CEF.

**Changes to `wezterm-gui/src/main.rs`:**

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
fn init_cef() -> Result<(), String> {
    use cef::{args::Args, execute_process, initialize, library_loader, Settings, App};

    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let loader = library_loader::LibraryLoader::new(&exe, false);
    if !loader.load() {
        return Err("Failed to load CEF framework".into());
    }
    log::info!("CEF framework loaded");

    let args = Args::new();
    let ret = execute_process(
        Some(args.as_main_args()),
        None::<&mut App>,
        std::ptr::null_mut(),
    );
    if ret >= 0 {
        std::process::exit(ret);
    }
    log::info!("CEF execute_process returned {ret}");

    let settings = Settings {
        windowless_rendering_enabled: 1,
        external_message_pump: 1,
        no_sandbox: 1,
        ..Default::default()
    };

    if initialize(Some(args.as_main_args()), Some(&settings), None::<&mut App>, std::ptr::null_mut()) != 1 {
        return Err("CEF initialize failed".into());
    }

    log::info!("CEF initialized successfully");
    Ok(())
}
```

Add to `main()` after `notify_on_panic()`:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
match init_cef() {
    Ok(()) => {}
    Err(e) => log::error!("CEF init failed: {e}"),
}
```

**Test:**

```bash
cargo build -p wezterm-gui --features cef --release
cp target/release/wezterm-gui target/release/WezTerm.app/Contents/MacOS/
RUST_LOG=info ./target/release/WezTerm.app/Contents/MacOS/wezterm-gui 2>&1 | grep -i cef
```

**Success criteria:**

- Log shows: `CEF framework loaded`
- Log shows: `CEF initialized successfully`
- WezTerm launches normally

**Known risks:**

- **Low: Message pump not being called.** We set `external_message_pump: 1` but
  don't call `do_message_loop_work()`. For this MVP (just load CEF, don't use
  it), this should be fine since we're not creating any browsers. But if CEF
  tries to do background work, it might hang or behave unexpectedly.

**Unanticipated issues (STOP if these occur):**

- "Failed to load CEF framework" - indicates bundle path issue
- "CEF initialize failed" - indicates settings or resource problem
- Crash during initialization - check Console.app for details
- `icudtl.dat not found` or similar resource errors - bundle structure is wrong
- Helper processes spawning and crashing - helper bundle structure is wrong

**Results:**

- ✅ Build completed in ~14 seconds
- ✅ Log shows: `[CEF] Framework loaded`
- ✅ Log shows: `[CEF] Initialized successfully`
- ✅ WezTerm launches normally and runs
- ⚠️ Additional issue discovered: After copying binary to bundle, macOS killed the
  process with SIGKILL (exit code 137) due to invalid code signature. Fixed by:
  1. Moving ANGLE dylibs from bundle root to Contents/Frameworks/ (fixes
     "unsealed contents" error)
  2. Re-signing with `codesign --force --deep --sign - target/release/WezTerm.app`
- ⚠️ Note for Step 7: Bundle script must include codesign step and dylib move
- ✅ No unanticipated issues occurred (code signing is a known macOS requirement)

---

## Step 6: Add CEF Shutdown ✅

**Goal:** Clean shutdown.

**Changes to `wezterm-gui/src/main.rs`** - at end of `main()`:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
cef::shutdown();
```

**Test:**

- Run app, then Cmd+Q to quit
- Should exit cleanly with no crash

**Known risks:**

- **Low: Shutdown timing.** If `cef::shutdown()` is called while CEF is still
  processing something, it could crash. Since we're not creating any browsers,
  this should be safe.

**Unanticipated issues (STOP if these occur):**

- Crash on quit (check if it's in CEF shutdown or WezTerm's own cleanup)
- Hang on quit (CEF waiting for something that never completes)
- Error messages about CEF resources not being cleaned up

**Results:**

- ✅ App exits cleanly when terminated with SIGTERM (simulating Cmd+Q)
- ✅ Exit code 143 (128 + 15 = SIGTERM received) - expected behavior
- ✅ No crash on quit
- ✅ No hang on quit
- ✅ No error messages about CEF resources
- ✅ No unanticipated issues occurred

---

## Step 7: Automate Bundle Creation ✅

**Goal:** Script the manual steps from Step 3.

Create `scripts/bundle-cef.sh` containing the commands from Step 3.

**Test:**

```bash
rm -rf target/release/WezTerm.app
./scripts/bundle-cef.sh
./target/release/WezTerm.app/Contents/MacOS/wezterm-gui
```

**Known risks:**

- **Medium: Script diverges from manual steps.** The script must exactly match
  the manual commands from Step 3. Any difference could cause subtle failures.
  Always verify script output matches what manual execution produced.

**Unanticipated issues (STOP if these occur):**

- Script produces different bundle structure than manual steps
- Script fails on a command that worked manually
- App behavior differs when launched from script-built bundle vs manual bundle

**Results:**

- ✅ Script created at `scripts/bundle-cef.sh`
- ✅ Script includes all fixes discovered during testing:
  - Moves ANGLE dylibs to Contents/Frameworks/
  - Uses Python for reliable plist modification (instead of fragile sed)
  - Signs bundle with codesign
- ✅ Script-built bundle runs successfully
- ✅ CEF loads and initializes: `[CEF] Framework loaded`
- ✅ App exits cleanly
- ✅ No unanticipated issues occurred

---

## Summary

| Step | What               | Test                         | Pass                | Risk   | Status |
| ---- | ------------------ | ---------------------------- | ------------------- | ------ | ------ |
| 1    | Add CEF dependency | `cargo build --features cef` | Compiles            | Low    | ✅     |
| 2    | Add helper binary  | `cargo build --features cef` | Both binaries exist | None   | ✅     |
| 3    | Manual bundle      | `ls Frameworks/` + grep      | 6 items + plists ok | Low    | ✅     |
| 4    | Run without CEF    | Launch app                   | WezTerm works       | None   | ✅     |
| 5    | Add CEF init       | Check logs                   | "CEF initialized"   | Low    | ✅     |
| 6    | Add shutdown       | Quit app                     | Clean exit          | Low    | ✅     |
| 7    | Automate           | Run script                   | Same as step 5      | Medium | ✅     |

**🎉 All steps completed successfully!** CEF is now integrated into WezTerm and can be
built with `./scripts/bundle-cef.sh`.
