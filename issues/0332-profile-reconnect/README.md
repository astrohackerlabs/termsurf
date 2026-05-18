+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# Issue 332: Profile server reconnect fails after webview close

## Problem

Opening a webview, closing it, and then trying to open it again fails.

## Reproduction

1. Launch TermSurf
2. Run `web google.com` - webview opens successfully
3. Close webview with Ctrl+C twice
4. Run `web google.com` again - fails with "XPC connection invalid"

## Root Cause

When all GUI connections disconnect from the profile server, it exits
gracefully:

```
[CONN-0] No more GUI connections, exiting gracefully
Profile: Shutting down...
Profile: Done
```

However, the launcher still has the profile registered and tries to forward
subsequent requests to the dead process:

```
Launcher: Forwarding to existing profile 'default' (session=pane-0-81580, url=https://google.com)
Launcher: Profile 'default' connection error: XPC connection invalid
```

## Possible Solutions

1. **Profile server stays alive** - Don't exit when connections drop; wait for
   new connections
2. **Launcher detects dead profile** - Unregister profile when connection fails,
   spawn new one
3. **Heartbeat mechanism** - Launcher periodically checks if profile is alive
4. **Profile notifies launcher on exit** - Send unregister message before
   shutting down

## Analysis

**Option 1 is wrong.** Keeping profiles alive forever is bad because there could
be unlimited profiles. We need to close unused ones to free resources.

**Option 2: Respawn on failure**

- Launcher tries to forward, connection fails
- Launcher unregisters dead profile, spawns new one
- Pros: Simple, handles any unexpected death (crashes, etc.)
- Cons: Reactive - we hit an error before recovering

**Option 4: Profile notifies launcher (track connections)**

- Profile already knows when connections drop:
  `[CONN-0] GUI disconnected (remaining: {})`
- Profile sends "unregister_profile" message to launcher before exiting
- Launcher removes profile from registry
- Next request spawns fresh
- Pros: Clean, no error path
- Cons: Requires new IPC message from profile → launcher

## Recommended Fix

**Implement both Option 2 and Option 4:**

1. **Primary path (Option 4):** Profile notifies launcher before exiting - this
   is the clean path for normal shutdown
2. **Safety net (Option 2):** Launcher handles connection failures by
   unregistering and respawning - this catches crashes or unexpected deaths

## Files Involved

- `ts3/termsurf-profile/src/main.rs` - Profile server exit logic
- `ts3/termsurf-launcher/src/main.rs` - Profile registration and forwarding
  logic

---

## Experiment 1: Unregister dead profiles on connection error

**Status: Failed**

Implement the safety net (Option 2). When the launcher detects a profile
connection error, unregister it so the next request spawns a fresh process.

### Current Behavior

In `register_profile` handler (lines 228-235), the event handler logs the error
but doesn't remove the profile from `running_profiles`:

```rust
let profile_name = profile.to_string();
set_event_handler(&*profile_conn, move |event| {
    if let Err(e) = event {
        eprintln!(
            "Launcher: Profile '{}' connection error: {}",
            profile_name, e
        );
    }
});
```

### Fix

Pass `running_profiles` into the closure and remove the profile on error:

```rust
let profile_name = profile.to_string();
let running_profiles_for_handler = running_profiles.clone();
set_event_handler(&*profile_conn, move |event| {
    if let Err(e) = event {
        eprintln!(
            "Launcher: Profile '{}' connection error: {}",
            profile_name, e
        );
        // Unregister so next request spawns fresh
        running_profiles_for_handler
            .lock()
            .unwrap()
            .remove(&profile_name);
        println!("Launcher: Unregistered dead profile '{}'", profile_name);
    }
});
```

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web google.com   # Opens webview
# Close with Ctrl+C twice
web google.com   # Should spawn new profile and work
tail -f /tmp/termsurf-launcher.log
# Expected: "Unregistered dead profile 'default'" then "Spawning new profile"
```

### Conclusion

The unregistration works, but it's **too late**. The sequence is:

1. `spawn_profile` received for second webview
2. Launcher forwards to existing profile via `send()` (fire-and-forget)
3. Error handler fires asynchronously, unregisters profile ✓
4. But the GUI has already given up waiting and disconnected
5. Launcher exits because no GUI connections remain

The fix only helps _future_ requests, but the _current_ request is already lost.

**Hypothesis:** The safety net (Option 2) cannot work alone because the error is
detected too late. We must implement Option 4 first: the profile notifies the
launcher before exiting. This ensures the profile is already unregistered when
the second request arrives, so the launcher spawns fresh instead of forwarding
to a dead process.

---

## Experiment 2: Profile notifies launcher before exit

**Status: Failed**

Implement Option 4. When the profile server detects all GUI connections are
gone, it sends an `unregister_profile` message to the launcher before exiting.

### Current Behavior

In `create_browser_on_ui_thread` (lines 1037-1044), when all connections drop:

```rust
if conns.is_empty() {
    println!(
        "[CONN-{}] No more GUI connections, exiting gracefully",
        conn_id_for_handler
    );
    drop(conns); // Release lock before setting flag
    crate::QUIT_FLAG.store(true, std::sync::atomic::Ordering::Relaxed);
}
```

The profile exits without notifying the launcher.

### Fix

1. Store the launcher connection in a global (it's already an Arc)
2. Before setting QUIT_FLAG, send `unregister_profile` to the launcher

Add global for launcher connection:

```rust
static LAUNCHER_CONNECTION: OnceLock<Arc<XpcConnection>> = OnceLock::new();
```

Store after connecting (in `run_profile_server`, after line 165):

```rust
let launcher = Arc::new(launcher);
let _ = LAUNCHER_CONNECTION.set(Arc::clone(&launcher));
```

Modify shutdown logic (lines 1037-1044):

```rust
if conns.is_empty() {
    println!(
        "[CONN-{}] No more GUI connections, exiting gracefully",
        conn_id_for_handler
    );
    drop(conns); // Release lock before sending

    // Notify launcher to unregister this profile
    if let Some(launcher) = crate::LAUNCHER_CONNECTION.get() {
        if let Some(state) = crate::PROFILE_STATE.get() {
            let msg = XpcDictionary::new();
            msg.set_string("action", "unregister_profile");
            msg.set_string("profile", &state.profile);
            launcher.send(&msg);
            println!("[CONN-{}] Sent unregister_profile to launcher", conn_id_for_handler);
        }
    }

    crate::QUIT_FLAG.store(true, std::sync::atomic::Ordering::Relaxed);
}
```

Add handler in launcher (in the event handler for profile connections):

```rust
if action == "unregister_profile" {
    let profile = msg.get_string("profile").unwrap_or_default();
    running_profiles.lock().unwrap().remove(&profile);
    println!("Launcher: Profile '{}' unregistered (self-reported)", profile);
}
```

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web google.com   # Opens webview
# Close with Ctrl+C twice
# Check launcher log for "unregistered (self-reported)"
web google.com   # Should spawn new profile and work
tail -f /tmp/termsurf-launcher.log
# Expected: "Profile 'default' unregistered (self-reported)" then "Spawning new profile"
```

### Conclusion

The `unregister_profile` message was sent but handled by the **wrong handler**.

The launcher has two types of connections:

1. **Main connections** - from GUI/profile clients connecting to the Mach
   service (handles `spawn_profile`, `claim_session`, `register_profile`)
2. **Profile connections** - created when a profile registers its endpoint
   (handles errors from that specific profile)

The profile sends `unregister_profile` via its original launcher connection
(`LAUNCHER_CONNECTION`), which is a main connection. But the handler was added
to the profile connection event handler.

Log evidence:

```
Launcher: Received action: unregister_profile
Launcher: Unknown action: unregister_profile
```

**Fix:** Add the `unregister_profile` handler to the main connection handler
(alongside `spawn_profile`, `claim_session`, `register_profile`).

---

## Experiment 3: Fix unregister handler location

**Status: Success**

Move the `unregister_profile` handler from the profile connection event handler
to the main connection event handler.

### Current Code

The main event handler (lines 108-300) handles actions via
`match action.as_str()`:

- `"spawn_profile"` - spawn or forward to profile
- `"register_profile"` - register profile's command endpoint
- `"claim_session"` - profile claims a GUI endpoint

The `unregister_profile` action falls through to the `_` case which logs
"Unknown action".

### Fix

Add `unregister_profile` case to the main `match action.as_str()` block:

```rust
"unregister_profile" => {
    // Issue 332, Experiment 3: Profile self-reports shutdown
    let profile = msg.get_string("profile").unwrap_or_default();
    running_profiles.lock().unwrap().remove(&profile);
    println!("Launcher: Profile '{}' unregistered (self-reported)", profile);
}
```

Also remove the duplicate handler from the profile connection event handler
(added in experiment 2) since it's now handled in the main handler.

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web google.com   # Opens webview
# Close with Ctrl+C twice
tail -f /tmp/termsurf-launcher.log
# Expected: "Profile 'default' unregistered (self-reported)"
web google.com   # Should spawn new profile and work immediately
```

---

## Conclusion

### The Problem

When a webview closes, the profile server exits but the launcher keeps a stale
reference. The next `web` command forwards to the dead profile and fails.

### What We Learned

1. **Reactive error handling is too late (Experiment 1)**

   The "safety net" approach—detecting connection errors and unregistering dead
   profiles—doesn't help the _current_ request. XPC's `send()` is
   fire-and-forget; the error handler fires asynchronously after the GUI has
   already given up. This approach only helps _future_ requests, but by then the
   launcher may have exited.

2. **Proactive notification is the fix (Experiments 2-3)**

   The profile must notify the launcher _before_ exiting. This ensures the
   profile is already unregistered when the next request arrives, so the
   launcher spawns fresh instead of forwarding to a dead process.

3. **XPC connection topology matters (Experiment 2)**

   The launcher has two types of connections: main connections (from clients
   connecting to the Mach service) and profile connections (from registered
   profile endpoints). Messages sent via the original launcher connection arrive
   at the main handler, not the profile connection handler. Understanding this
   topology was key to placing the handler correctly.

### Final Implementation

| Component      | Change                                                                                      |
| -------------- | ------------------------------------------------------------------------------------------- |
| Profile server | Stores launcher connection in global; sends `unregister_profile` before setting `QUIT_FLAG` |
| Launcher       | Handles `unregister_profile` in main event handler; removes profile from `running_profiles` |
| Safety net     | Profile connection error handler also unregisters (catches crashes)                         |

The fix is both proactive (profile notifies on clean shutdown) and reactive
(launcher handles unexpected deaths), providing defense in depth.
