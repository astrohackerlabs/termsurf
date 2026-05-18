+++
status = "closed"
opened = "2026-01-30"
closed = "2026-01-30"
+++

# Issue 312: Profile Process Steals Focus on macOS

## Problem Statement

When opening a webview with the `web ...` command, a second instance of
wezterm-gui appears in the macOS dock. This second instance:

1. Has no visible window
2. Appears as a separate dock icon
3. Steals keyboard focus from the original terminal window
4. Forces the user to click back on the original window to continue typing

This makes the webview feature extremely frustrating to use, as every `web`
command interrupts the user's workflow.

## Research Findings

### App Bundle Structure

The built app has this structure:

```
wezterm-gui.app/
└── Contents/
    ├── MacOS/
    │   ├── wezterm-gui        ← Main executable
    │   ├── termsurf-profile   ← CEF browser process (PROBLEM)
    │   ├── web
    │   └── wezterm
    ├── Frameworks/
    │   ├── Chromium Embedded Framework.framework/
    │   ├── WezTerm Helper.app/           ← Has LSUIElement=1
    │   ├── WezTerm Helper (GPU).app/     ← Has LSUIElement=1
    │   ├── WezTerm Helper (Renderer).app/
    │   ├── WezTerm Helper (Plugin).app/
    │   └── WezTerm Helper (Alerts).app/
    └── XPCServices/
        └── com.termsurf.launcher.xpc/
```

### Key Discovery: LSUIElement

The WezTerm Helper apps include `LSUIElement=1` in their Info.plist:

```xml
<key>LSUIElement</key>
<string>1</string>
```

This tells macOS the process is a "UI Element" — a background process that
should not appear in the dock or receive focus.

### How termsurf-profile Gets Launched

The launcher (`termsurf-launcher/src/main.rs`) spawns termsurf-profile via
`std::process::Command`:

```rust
let mut cmd = Command::new(&profile_bin_path);
cmd.args(["--session-id", &session_id])
   .args(["--url", &url])
   // ...
cmd.spawn()
```

The path resolves to `wezterm-gui.app/Contents/MacOS/termsurf-profile`.

### Why Focus Gets Stolen

1. `termsurf-profile` is spawned as a child process
2. CEF initializes and creates an `NSApplication` instance internally
3. macOS sees a new process from inside an app bundle starting up
4. Without `LSUIElement=1` or explicit activation policy, macOS treats it as a
   regular app
5. macOS gives the new "app" focus (standard behavior for newly launched apps)
6. The original terminal window loses keyboard focus

### NSApplication Activation Policies

macOS supports three activation policies:

| Policy                                    | Dock Icon | Can Activate     | Menu Bar |
| ----------------------------------------- | --------- | ---------------- | -------- |
| `NSApplicationActivationPolicyRegular`    | Yes       | Yes              | Yes      |
| `NSApplicationActivationPolicyAccessory`  | No        | Yes (if clicked) | No       |
| `NSApplicationActivationPolicyProhibited` | No        | No               | No       |

The main wezterm-gui uses `NSApplicationActivationPolicyRegular` (in
`window/src/os/macos/connection.rs:37`):

```rust
ns_app.setActivationPolicy_(NSApplicationActivationPolicyRegular);
```

termsurf-profile has no activation policy code — CEF sets up NSApplication with
default behavior.

## Hypotheses

### Hypothesis A: Missing Activation Policy

termsurf-profile relies on CEF to initialize NSApplication, but CEF doesn't set
an appropriate activation policy for a headless/background renderer. Without
explicit configuration, macOS defaults to regular app behavior.

### Hypothesis B: Bundle Association

When a process runs from inside an app bundle's `Contents/MacOS/` directory,
macOS may associate it with that bundle. However, since termsurf-profile is a
separate binary (not the CFBundleExecutable), macOS might treat it as a new app
instance rather than a helper.

### Hypothesis C: CEF's NSApplication Initialization

CEF requires NSApplication for Cocoa event handling even in off-screen rendering
mode. When CEF calls `[NSApplication sharedApplication]` and runs its event
loop, macOS may interpret this as a new foreground application launching.

## Solution Options

### Option A: Set Activation Policy Programmatically (Recommended)

Add code to `termsurf-profile/src/main.rs` to set
`NSApplicationActivationPolicyProhibited` before CEF initializes:

```rust
#[cfg(target_os = "macos")]
fn set_background_activation_policy() {
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let ns_app: *mut objc::runtime::Object =
            msg_send![class!(NSApplication), sharedApplication];
        // NSApplicationActivationPolicyProhibited = 2
        let _: () = msg_send![ns_app, setActivationPolicy: 2i64];
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    set_background_activation_policy();

    // ... rest of main
}
```

**Pros:**

- Simple code change
- No build system modifications
- Immediate effect

**Cons:**

- Must be called before CEF's NSApplication initialization
- Relies on objc crate (may need to add dependency)

### Option B: Bundle as Separate Helper App

Create `TermSurf Profile Helper.app` in the Frameworks folder with its own
Info.plist:

```
wezterm-gui.app/
└── Contents/
    └── Frameworks/
        └── TermSurf Profile Helper.app/
            └── Contents/
                ├── MacOS/
                │   └── termsurf-profile
                └── Info.plist  ← Contains LSUIElement=1
```

**Pros:**

- Follows macOS app bundle conventions
- LSUIElement is the "proper" way to declare background processes
- Works regardless of CEF's NSApplication behavior

**Cons:**

- Requires build script modifications
- More complex bundle structure
- Need to update launcher's path resolution

### Option C: Use LSBackgroundOnly in Main App

Add `LSBackgroundOnly=1` to the main app's Info.plist... but this would affect
wezterm-gui itself, making the terminal a background-only app. **Not viable.**

### Option D: Prevent CEF's NSApplication from Activating

CEF may have settings to control its NSApplication behavior. Research needed:

- `CefSettings` flags for background mode
- `--disable-features` command line flags
- Custom `CefApp` implementation that overrides activation

**Pros:**

- Addresses root cause within CEF

**Cons:**

- May not be possible with CEF's architecture
- Could break other CEF functionality

## Recommended Approach

**Start with Option A** (programmatic activation policy) as it's the simplest
fix with the least risk. If that doesn't work or has issues, fall back to
**Option B** (proper helper app bundling).

## Success Criteria

1. [x] Opening a webview with `web ...` does not create a new dock icon
2. [x] Opening a webview does not steal focus from the terminal window
3. [x] Keyboard input continues working in the terminal after opening a webview
4. [x] The webview still renders correctly (CEF functionality not broken)

## Files to Modify

| File                               | Changes                                    |
| ---------------------------------- | ------------------------------------------ |
| `ts3/termsurf-profile/src/main.rs` | Add activation policy code before CEF init |
| `ts3/termsurf-profile/Cargo.toml`  | Add objc/cocoa dependencies if needed      |

## References

- [Apple: LSUIElement](https://developer.apple.com/documentation/bundleresources/information_property_list/lsuielement)
- [Apple: NSApplicationActivationPolicy](https://developer.apple.com/documentation/appkit/nsapplication/activationpolicy)
- [CEF Forum: Background/Headless Mode](https://magpcss.org/ceforum/)

---

## Experiments

### Experiment 1: Set NSApplicationActivationPolicyProhibited

**Goal:** Prevent termsurf-profile from appearing in the dock and stealing focus
by setting `NSApplicationActivationPolicyProhibited` before CEF initializes
NSApplication.

**Hypothesis:** If we set the activation policy to "Prohibited" before CEF
creates its NSApplication instance, macOS will treat termsurf-profile as a
background process that cannot activate or appear in the dock.

#### Changes

**File: `ts3/termsurf-profile/src/main.rs`**

Add a function to set the activation policy at the very start of `main()`,
before any CEF initialization:

```rust
/// Set NSApplication activation policy to Prohibited.
/// This prevents the process from appearing in the dock or stealing focus.
/// Must be called before CEF initializes NSApplication.
#[cfg(target_os = "macos")]
fn set_background_activation_policy() {
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let ns_app: *mut objc::runtime::Object =
            msg_send![class!(NSApplication), sharedApplication];
        // NSApplicationActivationPolicyProhibited = 2
        let _: () = msg_send![ns_app, setActivationPolicy: 2i64];
    }
}
```

Call it as the first thing in `main()`:

```rust
fn main() {
    #[cfg(target_os = "macos")]
    set_background_activation_policy();

    // ... existing code (redirect_output, args parsing, CEF init, etc.)
}
```

**File: `ts3/termsurf-profile/Cargo.toml`**

Add the `objc` dependency if not already present:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc = "0.2"
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Open a webview
web google.com

# Observe:
# - Does a second dock icon appear?
# - Does the terminal window lose focus?
# - Can you continue typing immediately?

# Test 2: Open multiple webviews
web github.com
web apple.com

# Observe:
# - Still no extra dock icons?
# - Focus remains on terminal?

# Test 3: Verify webview still works
# - Does the webpage render correctly?
# - Does resize still work?
```

#### Success Criteria

1. [x] No new dock icon appears when running `web ...`
2. [x] Terminal window retains keyboard focus after `web ...`
3. [x] User can continue typing immediately without clicking
4. [x] Webview renders correctly (no regression in CEF functionality)
5. [x] Resize behavior unchanged from issue 311 fixes

#### Result

**Success.** Setting `NSApplicationActivationPolicyProhibited` before CEF
initializes NSApplication completely eliminates the focus-stealing behavior.

The fix confirms Hypothesis A: the profile process was defaulting to regular app
activation policy, causing macOS to show it in the dock and give it focus. By
explicitly setting the policy to "Prohibited" at the very start of `main()`,
before any CEF or Cocoa initialization, macOS now treats termsurf-profile as a
true background process that cannot activate or appear in the dock.

This is a minimal, non-invasive fix that requires no build system changes or app
bundle restructuring.

---

## Conclusion

### Goal

The goal of this issue was to fix a critical UX problem: every time a user
opened a webview with `web ...`, the terminal window lost keyboard focus. This
forced users to click back on their terminal after every webview command,
breaking their workflow and making the feature frustrating to use.

### What We Did

1. **Diagnosed the root cause.** We traced the problem to macOS treating the
   `termsurf-profile` process as a regular application. When CEF initialized
   NSApplication without an explicit activation policy, macOS defaulted to
   showing a dock icon and giving the process focus.

2. **Researched solutions.** We identified four potential approaches:
   - Option A: Set activation policy programmatically (simplest)
   - Option B: Bundle as a separate helper app with LSUIElement
   - Option C: Modify main app Info.plist (not viable)
   - Option D: CEF-specific settings (uncertain feasibility)

3. **Implemented Option A.** We added a single function that sets
   `NSApplicationActivationPolicyProhibited` at the very start of `main()`,
   before CEF has a chance to initialize NSApplication. This required adding the
   `objc` crate as a dependency.

4. **Verified the fix.** All success criteria passed:
   - No dock icon appears for the profile process
   - Terminal retains keyboard focus after opening webviews
   - Users can continue typing immediately
   - Webview rendering and resize behavior remain unaffected

### Files Modified

| File                               | Change                                   |
| ---------------------------------- | ---------------------------------------- |
| `ts3/termsurf-profile/src/main.rs` | Added `set_background_activation_policy` |
| `ts3/termsurf-profile/Cargo.toml`  | Added `objc = "0.2"` dependency          |

### Next Steps

1. **Consider Option B for robustness.** While the programmatic fix works, the
   "proper" macOS pattern is to bundle helper processes as separate `.app`
   bundles with `LSUIElement=1` in their Info.plist. This would make the fix
   declarative and immune to any future changes in CEF's NSApplication handling.
   However, this requires build script modifications and is lower priority now
   that the issue is resolved.

2. **Test on different macOS versions.** The fix was tested on the current
   development machine. Verify behavior on older macOS versions (11, 12, 13) to
   ensure the activation policy API behaves consistently.

3. **Monitor for regressions.** If future CEF updates change how NSApplication
   is initialized (e.g., calling `sharedApplication` before our code runs), the
   fix may need adjustment. The current placement at the very start of `main()`
   should be robust, but it's worth keeping in mind.
