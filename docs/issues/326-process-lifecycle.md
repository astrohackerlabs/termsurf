# 326: Process Lifecycle

Profile server and launcher processes continue running after the GUI exits,
creating orphaned background processes.

## Status

Not started.

## Problem

When the GUI (wezterm-gui) closes, the profile server (termsurf-profile) and
launcher (termsurf-launcher) processes remain running indefinitely. Users must
manually kill them:

```bash
pkill -f termsurf-profile
pkill -f termsurf-launcher
```

This causes several issues:

1. **Stale processes** — Old code keeps running after rebuilds, confusing
   development and testing (discovered during Issue 325 experiments)
2. **Resource waste** — Orphaned CEF processes consume memory and CPU
3. **Port conflicts** — Stale launcher may interfere with new instances
4. **Unexpected behavior** — Users expect closing the app to close everything

## Architecture

```
GUI (wezterm-gui)
    │
    ├── XPC connection ──► Launcher (com.termsurf.launcher)
    │                           │
    │                           └── spawns ──► Profile Server (termsurf-profile)
    │                                              │
    └── XPC connection (anonymous endpoint) ◄──────┘
```

- GUI connects to launcher via Mach service
- GUI creates anonymous XPC listeners (one per webview pane)
- Launcher spawns profile servers and passes GUI endpoints to them
- Profile servers connect directly to GUI to send frames and receive input

## Root Cause

When the GUI exits, XPC connections are invalidated. The profile server receives
`XPC_ERROR_CONNECTION_INTERRUPTED` or `XPC_ERROR_CONNECTION_INVALID` errors, but
the event handler only logs them — it takes no action to shut down.

**Current code** (`ts3/termsurf-profile/src/main.rs`):

```rust
Err(e) => {
    eprintln!("Profile: GUI connection error: {}", e);
    // No shutdown logic — process continues running
}
```

The launcher has a similar issue — it doesn't track which GUI spawned which
profiles, so it can't coordinate cleanup.

## Proposed Solution

**Option 1: Profile detects disconnect and exits (Recommended)**

The profile server already has a `quit_flag` pattern from the polling loop
(Issue 325). When the GUI disconnects, set the flag to trigger graceful
shutdown.

```rust
Err(e) => {
    match e {
        XpcError::ConnectionInterrupted | XpcError::ConnectionInvalid => {
            eprintln!("Profile: GUI disconnected, exiting gracefully");
            quit_flag.store(true, Ordering::Relaxed);
        }
        _ => eprintln!("Profile: GUI connection error: {}", e),
    }
}
```

The 1ms polling loop already checks `quit_flag`, so the profile exits within
milliseconds.

**Complexity:** Low (5-10 lines)

**Option 2: Launcher coordinates shutdown**

Launcher tracks GUI→profile mappings. When GUI disconnects, launcher sends
shutdown signals to all profiles spawned by that GUI.

**Complexity:** Medium (requires bidirectional signaling, race condition
handling)

**Option 3: Hybrid monitoring**

Profile monitors both GUI and launcher connections. Exits if either disconnects.

**Complexity:** Medium (redundant monitoring, but more robust)

## Implementation Plan

### Phase 1: Profile server shutdown (Option 1)

1. Modify `ts3/termsurf-profile/src/main.rs` event handler
2. Detect `ConnectionInterrupted` and `ConnectionInvalid` errors
3. Set `quit_flag` to trigger graceful CEF shutdown
4. Test: Start GUI, open webview, close GUI, verify profile exits

### Phase 2: Launcher shutdown (if needed)

The launcher is a persistent Mach service. Options:

- **Keep running** — Launcher is lightweight, can serve multiple GUI instances
- **Exit when idle** — Exit after N seconds with no active connections
- **launchd management** — Let launchd handle lifecycle (KeepAlive=false)

For now, Phase 1 is sufficient. The launcher uses minimal resources when idle.

## Verification

```bash
# Before fix:
open wezterm-gui.app
web google.com
# Close GUI window
ps aux | grep termsurf
# Shows: termsurf-profile and termsurf-launcher still running

# After fix:
open wezterm-gui.app
web google.com
# Close GUI window
ps aux | grep termsurf
# Shows: Only termsurf-launcher (or nothing if idle-exit implemented)
```

## Edge Cases

1. **Multiple GUIs** — Each GUI has its own profile connections. Profile should
   only exit when its specific GUI disconnects, not when any GUI disconnects.

2. **Multiple webviews per GUI** — A GUI may have multiple panes connecting to
   one profile. Profile should exit when all connections from that GUI close.

3. **Profile crash** — GUI should handle profile disconnect gracefully (show
   error in pane, allow retry).

4. **GUI crash** — Profile should detect abnormal disconnect same as normal
   close.

## Experiments

### Experiment 1: Profile Exits on GUI Disconnect

**Goal:** Make the profile server exit gracefully when the GUI disconnects.

**Hypothesis:** The profile server already receives XPC disconnect errors. By
detecting these errors and setting the existing `quit_flag`, the profile will
exit within milliseconds via the 1ms polling loop.

**Approach:** Modify the GUI connection event handler to detect disconnect errors
and trigger shutdown using the existing `quit_flag` pattern from Issue 325.

**Changes:**

1. **`ts3/termsurf-profile/src/main.rs`** — In `create_browser_on_ui_thread`,
   modify the event handler's error case:

   Before:
   ```rust
   Err(e) => {
       eprintln!("Profile: GUI connection error: {}", e);
   }
   ```

   After:
   ```rust
   Err(e) => {
       match e {
           XpcError::ConnectionInterrupted | XpcError::ConnectionInvalid => {
               eprintln!("Profile: GUI disconnected, exiting gracefully");
               // Signal the main loop to exit
               quit_flag.store(true, std::sync::atomic::Ordering::Relaxed);
           }
           _ => eprintln!("Profile: GUI connection error: {}", e),
       }
   }
   ```

   Note: The `quit_flag` needs to be accessible from the event handler. This may
   require passing it through the handler closure or using a global atomic.

**Verification:**

```bash
# Kill any existing processes
pkill -f termsurf-profile
pkill -f termsurf-launcher

cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Normal close
web google.com
# Wait for page to load
# Close GUI window (Cmd+Q or click X)
sleep 1
ps aux | grep termsurf-profile
# Expected: No termsurf-profile process

# Test 2: Check logs for graceful shutdown
cat /tmp/termsurf-profile-*.log | tail -10
# Expected: "GUI disconnected, exiting gracefully" followed by "Shutting down..."

# Test 3: Multiple open/close cycles
# Repeat Test 1 several times
# Expected: No accumulation of orphaned processes
```

**Status:** Not started.

## References

- Issue 325 — Discovered this bug during frame rate testing
- `ts3/termsurf-profile/src/main.rs` — Profile server main loop and event
  handlers
- `ts3/termsurf-launcher/src/main.rs` — Launcher service
- `ts3/termsurf-xpc/src/error.rs` — XPC error types
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — GUI XPC manager
