+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# 326: Process Lifecycle

Profile server and launcher processes continue running after the GUI exits,
creating orphaned background processes.

## Status

**Resolved.** Profile server and launcher both exit gracefully when all GUI
connections close. Multi-pane support preserved.

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

**Status:** Failed.

**Result:** Profile server exits when GUI disconnects, but this **breaks
multi-pane support**. Opening two webviews for the same profile, then closing
one pane, kills the entire profile server — leaving the second pane unresponsive.

**Implementation notes:**

- Added global `QUIT_FLAG` static (couldn't use local variable in closure)
- Updated Ctrl+C handler and main loop to use `QUIT_FLAG`
- Event handler detects `ConnectionInterrupted`/`ConnectionInvalid` and sets flag
- Profile exits within ~1ms of GUI disconnect (polling loop detects flag)

**What worked:**

- Single-pane case: profile exits when GUI closes
- CEF shutdown is clean (no crashes)

**What broke:**

- **Multi-pane support is completely broken**
- Profile server exits on *any* disconnect, not just *last* disconnect
- A profile can serve multiple browsers (one per webview pane), each with its
  own GUI connection
- Closing one pane disconnects that connection → profile exits → other panes die

**Root cause:**

The implementation exits on the first disconnect instead of tracking connection
count and only exiting when all connections are closed.

**Next steps:**

Both profile server and launcher need **connection counting**:
1. Increment count when connection established
2. Decrement count on disconnect
3. Only exit when count reaches 0

### Experiment 2: Launcher Exits on GUI Disconnect

**Goal:** Make the launcher exit when all GUI connections disconnect.

**Hypothesis:** The launcher receives XPC connection events. When a GUI
disconnects and no other GUIs are connected, the launcher should exit.

**Approach:** Track active GUI connections in the launcher. On disconnect, check
if any connections remain. If not, exit.

**Changes made:**

1. Added `CFRunLoopStop` and `CFRunLoopGetMain` to `termsurf-xpc/src/ffi.rs`
2. Added `stop_run_loop()` function to `termsurf-xpc/src/runloop.rs`
3. Changed `run_loop()` return type from `-> !` to `()` (can now return)
4. Added `GUI_CONNECTION_COUNT` atomic counter to launcher
5. Increment on new connection, decrement on disconnect
6. Call `stop_run_loop()` when count reaches 0

**Status:** Failed (not tested due to Experiment 1 failure).

**Result:** Implementation was completed but could not be properly tested because
Experiment 1 broke multi-pane support. The launcher changes follow the correct
pattern (connection counting), but the profile server changes do not.

**Key learnings:**

1. **Both processes need connection counting** — A profile server can have
   multiple browsers (webview panes), each with its own GUI connection. The
   launcher can have multiple GUI clients. Both must track counts.

2. **Exit on last disconnect, not first** — The pattern must be:
   - Increment count when connection established
   - Decrement count when connection closes
   - Only exit when count reaches 0

3. **Profile server architecture:**
   - `create_browser_on_ui_thread` creates a new GUI connection per browser
   - Each browser has its own event handler
   - Need to increment count in `create_browser_on_ui_thread`
   - Need to decrement count in the error handler, exit only when count = 0

4. **Launcher architecture (correctly implemented):**
   - `set_new_connection_handler` fires for each GUI connection
   - Increment count there
   - Decrement in error handler, call `stop_run_loop()` when count = 0

### Experiment 3: Connection Counting for Both Processes

**Goal:** Fix experiment 1 by adding connection counting to the profile server,
matching the pattern used in the launcher.

**Approach:**

1. Add `GUI_CONNECTION_COUNT` atomic to profile server
2. Increment in `create_browser_on_ui_thread` when GUI connection established
3. Decrement in event handler on disconnect
4. Only set `QUIT_FLAG` when count reaches 0

**Status:** Success.

**Result:** Both profile server and launcher now exit gracefully when all GUI
connections close. Multi-pane support works correctly.

**Test results:**

1. **Single pane:** Open webview, close GUI → profile and launcher both exit ✓
2. **Multi-pane:** Open two webviews, close one → profile stays running, second
   pane remains responsive. Close second → profile exits ✓
3. **Multiple cycles:** Repeated open/close cycles → no orphaned processes ✓

**Files changed:**

- `ts3/termsurf-profile/src/main.rs` — Added `GUI_CONNECTION_COUNT`, increment
  on connect, decrement on disconnect, exit only when count = 0
- `ts3/termsurf-launcher/src/main.rs` — Added `GUI_CONNECTION_COUNT`, same pattern
- `ts3/termsurf-xpc/src/ffi.rs` — Added `CFRunLoopStop`, `CFRunLoopGetMain`
- `ts3/termsurf-xpc/src/runloop.rs` — Added `stop_run_loop()` function
- `ts3/termsurf-xpc/src/lib.rs` — Exported `stop_run_loop`

## References

- Issue 325 — Discovered this bug during frame rate testing
- `ts3/termsurf-profile/src/main.rs` — Profile server main loop and event
  handlers
- `ts3/termsurf-launcher/src/main.rs` — Launcher service
- `ts3/termsurf-xpc/src/error.rs` — XPC error types
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — GUI XPC manager
