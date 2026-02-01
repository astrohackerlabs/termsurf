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

- Profile already knows when connections drop: `[CONN-0] GUI disconnected (remaining: {})`
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
