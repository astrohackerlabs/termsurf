# TermSurf 3.0 XPC Architecture

## Background

This document continues the work from [ts3-2-webview.md](./ts3-2-webview.md),
which explored cross-process GPU texture sharing between the profile server
(CEF) and the GUI (wezterm-gui).

### The Problem

TermSurf 3.0 uses a multi-process architecture for browser isolation:

```
wezterm-gui (terminal + renderer)
    │
    └── profile server (CEF browser engine)
            │
            └── CEF GPU helper process
                    │
                    └── IOSurface (GPU texture)
```

The profile server renders web content to an IOSurface. The GUI needs to display
that texture. The challenge: how do you share a GPU texture between separate
processes on macOS?

## Why Earlier Attempst Failed

### Attempt 1: IOSurface Global ID Lookup

Our first approach used `IOSurfaceGetID()` and `IOSurfaceLookup()`:

```rust
// Profile server
let id = IOSurfaceGetID(handle);  // Get numeric ID
send_to_gui(id);                   // Send ID over socket

// GUI
let handle = IOSurfaceLookup(id);  // Look up by ID → NULL
```

**Result:** `IOSurfaceLookup()` returned NULL.

**Root cause:** Apple deprecated `kIOSurfaceIsGlobal` in macOS 10.11 (2015).
Without this flag, IOSurfaces are not globally registered and cannot be looked
up by ID across process boundaries. This flag was removed for security reasons —
globally accessible screen buffers are a security hole.

### Attempt 2: Process Ancestry

We hypothesized that `IOSurfaceLookup()` might work if the GUI was an ancestor
of the process that created the IOSurface (similar to how Chromium's browser
process can access IOSurfaces from its GPU subprocess).

We re-architected so the GUI spawns the profile server:

```
wezterm-gui (spawns profile server)
    └── profile server
            └── CEF GPU helper (creates IOSurface)
```

**Result:** `IOSurfaceLookup()` still returned NULL.

**Conclusion:** The `kIOSurfaceIsGlobal` deprecation is absolute. Process
ancestry does not enable IOSurface lookup. The "global ID" mechanism is
fundamentally broken for cross-process sharing on modern macOS.

### The Bootstrap API Dead End

After the global ID approach failed, we considered using Mach ports directly:

```rust
// Profile server
let port = IOSurfaceCreateMachPort(handle);
// Send port to GUI somehow...

// GUI
let handle = IOSurfaceLookupFromMachPort(port);  // Works!
```

The APIs exist and work. The problem: how do you send a Mach port between
processes?

**Attempt: Bootstrap server registration**

```rust
// GUI
bootstrap_register("com.termsurf.gui", port);  // Register port with name

// Profile server
bootstrap_look_up("com.termsurf.gui", &port);  // Look up by name
```

**Problem:** `bootstrap_register()` is deprecated (since macOS 10.5).
`bootstrap_check_in()` only works for launchd-managed services. There is no
non-deprecated way to register an arbitrary Mach port for lookup by name.

## Why XPC Is Required

XPC (Cross-Process Communication) is Apple's modern replacement for raw Mach IPC
and bootstrap server registration. It provides:

1. **Mach port transfer:** XPC handles Mach port passing transparently via
   `xpc_dictionary_set_mach_send()` and `xpc_dictionary_copy_mach_send()`

2. **Service discovery:** XPC services are registered with launchd and
   discoverable by name without deprecated bootstrap APIs

3. **Endpoint passing:** Anonymous XPC listeners can create endpoints that are
   passed over existing XPC connections, enabling dynamic peer-to-peer
   communication

4. **Process lifecycle:** launchd manages XPC service lifecycle (launch
   on-demand, restart on crash)

### How Chromium Solved This

Chromium faced the same problem internally. Their GPU process creates
IOSurfaces, and the browser process needs to access them.

From
[Chromium code review 1532813002](https://codereview.chromium.org/1532813002):

> **"Replace IOSurfaceManager by directly passing IOSurface Mach ports over
> Chrome IPC"**

Chromium switched from global IDs to Mach ports sent over their internal IPC
layer. Their IPC layer is analogous to XPC — it can transfer Mach port rights
between processes.

CEF inherits this mechanism internally. When your code receives an IOSurface
handle in `on_accelerated_paint`, CEF has already used Mach ports to transfer it
from the GPU process to your browser process. The handle is valid because you're
in the same process.

For TermSurf 3.0, we need to perform the same transfer again — from the profile
server process to the GUI process. XPC is Apple's supported mechanism for this.

### The XPC Requirement Is Fundamental

This is not a matter of preference or convenience. On modern macOS:

- **Global IOSurface IDs don't work** (deprecated 2015)
- **Bootstrap registration doesn't work** (deprecated 2005)
- **Task port insertion requires entitlements** (security hardening)
- **Unix sockets can't transfer Mach ports** (only file descriptors via
  SCM_RIGHTS)

XPC is the only non-deprecated, non-entitled mechanism for transferring Mach
port rights between unrelated processes on macOS.

## Summary

| Approach               | Status   | Reason                                 |
| ---------------------- | -------- | -------------------------------------- |
| IOSurface global ID    | Failed   | `kIOSurfaceIsGlobal` deprecated        |
| Process ancestry       | Failed   | Deprecation is absolute                |
| Bootstrap registration | Blocked  | `bootstrap_register()` deprecated      |
| Raw Mach IPC           | Blocked  | No way to establish initial connection |
| XPC                    | Required | Apple's supported mechanism            |

The next step is to design and implement XPC-based communication between the GUI
and profile servers.
