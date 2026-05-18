+++
status = "closed"
opened = "2026-01-25"
closed = "2026-01-25"
+++

# TermSurf 3.0 XPC Architecture

## Background

This document continues the work from [302-webview.md](./302-webview.md),
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

## Why Earlier Attempts Failed

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

## Cross-Platform Considerations

XPC is macOS-specific. Other platforms have their own GPU texture sharing
mechanisms, and importantly, they're often simpler.

### Platform-Specific Transfer Mechanisms

| Platform    | Handle Type               | Transfer Mechanism         | Complexity |
| ----------- | ------------------------- | -------------------------- | ---------- |
| **macOS**   | IOSurface (`*mut c_void`) | XPC + Mach ports           | High       |
| **Linux**   | DMA-BUF (file descriptor) | Unix socket + `SCM_RIGHTS` | Low        |
| **Windows** | DXGI handle (`HANDLE`)    | `DuplicateHandle`          | Medium     |

### Linux Is Easier

On Linux, CEF renders to DMA-BUF textures, which are represented as file
descriptors. File descriptors can be sent over Unix domain sockets using
`SCM_RIGHTS` ancillary data. This means the existing Unix socket infrastructure
can carry texture handles directly — no new IPC mechanism needed.

From cef-rs `dmabuf.rs`:

```rust
pub struct DmaBufImporter {
    fds: Vec<std::os::fd::RawFd>,  // File descriptors - can use SCM_RIGHTS
    // ...
}
```

### Windows Is Medium Complexity

On Windows, DXGI shared handles can be duplicated into another process using
`DuplicateHandle()`. This requires the target process handle, which the GUI has
for profile servers it spawns.

### Architectural Consistency

Despite different transfer mechanisms, the overall architecture remains
identical across platforms:

```
Profile Server                      GUI
──────────────                      ───
on_accelerated_paint(handle)
        │
        ├── macOS: XPC + Mach port
        ├── Linux: Unix socket + SCM_RIGHTS
        └── Windows: DuplicateHandle
                                    │
                                    └── Import texture, render
```

The profile server sends; the GUI receives and imports. Only the transfer
mechanism differs.

### Implementation Strategy

1. **macOS first** — XPC/Mach ports are the most complex; solving this first
   de-risks the architecture
2. **Linux second** — Add `SCM_RIGHTS` support to existing Unix socket code
3. **Windows third** — `DuplicateHandle` is straightforward once we have the
   process handle

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

## Experiments

### Experiment 1: XPC Test Bundle

**Status:** PLANNED

**Goal:** Validate the `termsurf-xpc` Rust bindings and XPC service architecture
with a minimal standalone test before integrating with wezterm/CEF. This
isolates XPC complexity from GUI/rendering complexity.

**Critical validation:** The launcher must be able to **spawn child processes**.
This is the entire reason the launcher exists — if it can't spawn profile
servers, the architecture fails.

#### What This Tests

| API / Capability                        | Purpose                                  |
| --------------------------------------- | ---------------------------------------- |
| `xpc_connection_create_mach_service()`  | Connect to named XPC service             |
| `xpc_connection_set_event_handler()`    | Handle incoming messages (via blocks)    |
| `XpcListener::new_anonymous()`          | Create endpoint for peer-to-peer         |
| `xpc_endpoint_create()`                 | Extract endpoint from listener           |
| `xpc_dictionary_set_endpoint()`         | Send endpoint in message                 |
| `xpc_connection_create_from_endpoint()` | Connect to anonymous listener            |
| `xpc_dictionary_set_mach_send()`        | Send Mach port in message                |
| `xpc_dictionary_copy_mach_send()`       | Receive Mach port from message           |
| `IOSurfaceCreateMachPort()`             | Create sendable port from IOSurface      |
| `IOSurfaceLookupFromMachPort()`         | Reconstruct IOSurface from received port |
| **Process spawning from XPC service**   | Launcher spawns sender (critical!)       |
| **Sandbox compatibility**               | XPC service can spawn with entitlements  |

#### Architecture

This test mirrors the production architecture:

| Test Component | Production Equivalent |
| -------------- | --------------------- |
| Receiver       | wezterm-gui           |
| Launcher       | termsurf-launcher     |
| Sender         | profile server        |

```
┌────────────────────────────────────────────────────────────────────────┐
│                     TermSurf XPC Test Bundle                           │
│                     (ts3/termsurf-xpc/)                                │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│  ┌─────────────┐          ┌─────────────┐          ┌─────────────┐    │
│  │  Receiver   │          │   Launcher  │ ──spawn──>   Sender    │    │
│  │   (Rust)    │          │   (XPC Svc) │          │   (Rust)    │    │
│  └──────┬──────┘          └──────┬──────┘          └──────┬──────┘    │
│         │                        │                        │           │
│    1. Create              2. Spawn sender            3. Create        │
│       anonymous              + pass endpoint            IOSurface     │
│       listener               + session ID               + send port   │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
```

#### Test Flow

**Key difference from naive approach:** The launcher spawns the sender. This
validates that XPC services can spawn child processes (required for production).

```
Receiver                      Launcher                     Sender
────────                      ────────                     ──────
    │                            │
    │── connect ────────────────>│
    │                            │
    │── spawn_sender ───────────>│
    │   + my endpoint            │
    │   + session_id: "test-1"   │
    │                            │
    │                            │── spawn ──────────────────>│
    │                            │   args: --session test-1   │
    │                            │                            │
    │                            │<──────── connect ──────────│
    │                            │                            │
    │                            │<── claim_session ──────────│
    │                            │    session_id: "test-1"    │
    │                            │                            │
    │                            │── receiver endpoint ──────>│
    │                            │                            │
    │<════════════ direct XPC connection ════════════════════>│
    │                            │                            │
    │                            │    (IOSurface Mach port)   │
    │<─────────────────────────────────── send_surface ───────│
    │                            │                            │
    │   IOSurfaceLookupFromMachPort() → handle                │
    │   Verify: width=100, height=100, pixel[0,0]=0xFF69B4    │
    │                            │                            │
    │   print "SUCCESS"          │                            │
    │                            │                            │
```

#### Components

##### 1. XPC Service (Launcher)

**Location:** `ts3/termsurf-xpc/examples/launcher.rs`

XPC service that spawns child processes and relays endpoints. Written in pure
Rust using the `termsurf-xpc` bindings:

```rust
// examples/launcher.rs
use termsurf_xpc::*;
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::env;

fn main() -> Result<()> {
    println!("Launcher: Starting...");

    // Session storage: session_id -> receiver endpoint
    let sessions: Arc<Mutex<HashMap<String, XpcEndpoint>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // CRITICAL: Store client connections to keep them alive!
    // Without this, connections are canceled when the handler returns.
    let clients: Arc<Mutex<Vec<XpcConnection>>> =
        Arc::new(Mutex::new(Vec::new()));

    // Path to sender binary (sibling in same directory)
    let exe_path = env::current_exe().expect("Failed to get exe path");
    let exe_dir = exe_path.parent().expect("Failed to get exe directory");
    let sender_path = exe_dir.join("sender");

    // Create listener for this XPC service
    let listener = XpcListener::new_mach_service("com.termsurf.xpc-test")?;

    // Handle incoming connections
    let sessions_clone = sessions.clone();
    let clients_clone = clients.clone();
    let sender_path_clone = sender_path.clone();

    set_new_connection_handler(&listener, move |conn| {
        println!("Launcher: New connection");

        let sessions = sessions_clone.clone();
        let sender_path = sender_path_clone.clone();

        set_event_handler(&conn, move |event| {
            match event {
                Ok(msg) => {
                    let action = msg.get_string("action").unwrap_or_default();

                    match action.as_str() {
                        "spawn_sender" => {
                            let session_id = msg.get_string("session_id")
                                .expect("Missing session_id");
                            let endpoint = msg.get_endpoint("receiver_endpoint")
                                .expect("Missing receiver_endpoint");

                            // Store endpoint for sender to claim
                            {
                                let mut sessions = sessions.lock().unwrap();
                                sessions.insert(session_id.clone(), endpoint);
                            }

                            // Spawn sender as child process
                            match Command::new(&sender_path)
                                .args(["--session", &session_id])
                                .spawn()
                            {
                                Ok(_) => println!("Launcher: Spawned sender for {}", session_id),
                                Err(e) => eprintln!("Launcher: Failed to spawn: {}", e),
                            }
                        }

                        "claim_session" => {
                            let session_id = msg.get_string("session_id")
                                .expect("Missing session_id");

                            let endpoint = {
                                let mut sessions = sessions.lock().unwrap();
                                sessions.remove(&session_id)
                            };

                            // Send reply with endpoint
                            let reply = XpcDictionary::create_reply(&msg)
                                .expect("Failed to create reply");

                            if let Some(ep) = endpoint {
                                reply.set_endpoint("endpoint", ep);
                                println!("Launcher: Session {} claimed", session_id);
                            } else {
                                reply.set_string("error", "session not found");
                                eprintln!("Launcher: Session {} not found", session_id);
                            }

                            conn.send(&reply);
                        }

                        _ => {
                            eprintln!("Launcher: Unknown action: {}", action);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Launcher: Connection error: {}", e);
                }
            }
        });

        conn.resume();

        // CRITICAL: Store the connection to keep it alive!
        clients_clone.lock().unwrap().push(conn);
    });

    listener.resume();

    println!("Launcher: Running...");
    run_loop();
}
```

**Info.plist** (required for XPC service registration):

```xml
<!-- xpc-service/Info.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.termsurf.xpc-test</string>
    <key>CFBundleName</key>
    <string>termsurf-xpc-test</string>
    <key>CFBundleExecutable</key>
    <string>launcher</string>
    <key>XPCService</key>
    <dict>
        <key>ServiceType</key>
        <string>Application</string>
    </dict>
</dict>
</plist>
```

**Note on sandboxing:** XPC services are sandboxed by default. To spawn child
processes, either disable the sandbox via entitlements or restructure so the
main app spawns processes instead. For this test, we run unsigned/unsandboxed.

##### 2. Receiver (Rust)

**Location:** `ts3/termsurf-xpc/examples/receiver.rs`

Tests the "GUI side" — creates anonymous listener, asks launcher to spawn
sender:

```rust
// examples/receiver.rs
use termsurf_xpc::*;
use std::sync::{Arc, Mutex};

fn main() -> Result<()> {
    println!("Receiver: Starting...");

    // CRITICAL: Storage for peer connections - must keep them alive!
    let peers: Arc<Mutex<Vec<XpcConnection>>> = Arc::new(Mutex::new(Vec::new()));
    let peers_clone = peers.clone();

    // 1. Connect to launcher
    let launcher = XpcConnection::connect_mach_service("com.termsurf.xpc-test")?;
    set_event_handler(&launcher, |event| {
        // Handle any errors from launcher
        if let Err(e) = event {
            eprintln!("Receiver: Launcher error: {}", e);
        }
    });
    launcher.resume();
    println!("Receiver: Connected to launcher");

    // 2. Create anonymous listener for sender to connect
    let listener = XpcListener::new_anonymous()?;
    let endpoint = listener.get_endpoint()?;
    println!("Receiver: Created anonymous listener");

    // 3. Set up handler for incoming peer connections
    set_new_connection_handler(&listener, move |peer| {
        println!("Receiver: Sender connected!");

        set_event_handler(&peer, |event| {
            match event {
                Ok(msg) => {
                    let action = msg.get_string("action").unwrap_or_default();
                    if action == "send_surface" {
                        // Receive IOSurface Mach port
                        let port = msg.copy_mach_send("iosurface_port");
                        let handle = iosurface::lookup_from_mach_port(port);

                        let handle = match handle {
                            Some(h) => h,
                            None => {
                                eprintln!("FAILED: lookup_from_mach_port returned None");
                                std::process::exit(1);
                            }
                        };

                        // Verify dimensions
                        let width = iosurface::get_width(handle);
                        let height = iosurface::get_height(handle);

                        if width != 100 || height != 100 {
                            eprintln!("FAILED: Expected 100x100, got {}x{}", width, height);
                            std::process::exit(1);
                        }

                        // Verify pixel color (hot pink: 0xFF69B4)
                        let pixel = iosurface::read_pixel(handle, 0, 0);
                        let expected = 0xFF69B4FF_u32; // RGBA

                        if pixel != expected {
                            eprintln!("FAILED: Expected pixel 0x{:08X}, got 0x{:08X}",
                                     expected, pixel);
                            std::process::exit(1);
                        }

                        println!("SUCCESS: Received 100x100 pink IOSurface via XPC!");
                        std::process::exit(0);
                    }
                }
                Err(e) => {
                    eprintln!("Receiver: Peer error: {}", e);
                }
            }
        });
        peer.resume();

        // CRITICAL: Store the peer to keep connection alive!
        peers_clone.lock().unwrap().push(peer);
    });
    listener.resume();

    // 4. Ask launcher to spawn sender with our endpoint
    let msg = XpcDictionary::new();
    msg.set_string("action", "spawn_sender");
    msg.set_string("session_id", "test-1");
    msg.set_endpoint("receiver_endpoint", endpoint);
    launcher.send(&msg);
    println!("Receiver: Requested sender spawn");

    // 5. Run event loop (keep peers in scope!)
    println!("Receiver: Waiting for sender...");
    let _keep_alive = peers;
    run_loop();
}
```

##### 3. Sender (Rust)

**Location:** `ts3/termsurf-xpc/examples/sender.rs`

Tests the "profile server side" — spawned by launcher, claims endpoint, sends
IOSurface:

```rust
// examples/sender.rs
use termsurf_xpc::*;
use clap::Parser;
use std::time::Duration;
use std::thread;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    session: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Sender: Starting for session '{}'", args.session);

    // 1. Connect to launcher
    let launcher = XpcConnection::connect_mach_service("com.termsurf.xpc-test")?;
    launcher.resume();
    println!("Sender: Connected to launcher");

    // 2. Claim session with retry (session may not be registered yet)
    let receiver_endpoint = claim_session_with_retry(&launcher, &args.session)?;
    println!("Sender: Got receiver endpoint");

    // 3. Connect directly to receiver
    let receiver = XpcConnection::from_endpoint(receiver_endpoint)?;
    set_event_handler(&receiver, |event| {
        if let Err(e) = event {
            eprintln!("Sender: Receiver error: {}", e);
        }
    });
    receiver.resume();
    println!("Sender: Connected to receiver");

    // 4. Create pink IOSurface (100x100, hot pink 0xFF69B4)
    let surface = iosurface::create_iosurface(100, 100)?;
    iosurface::fill_with_color(surface, 0xFF, 0x69, 0xB4, 0xFF); // Hot pink
    println!("Sender: Created 100x100 pink IOSurface");

    // 5. Send Mach port to receiver
    let port = iosurface::create_mach_port(surface);
    if port == 0 {
        return Err(XpcError::Unknown("create_mach_port failed".into()));
    }

    let msg = XpcDictionary::new();
    msg.set_string("action", "send_surface");
    msg.set_mach_send("iosurface_port", port);
    msg.set_i64("width", 100);
    msg.set_i64("height", 100);
    receiver.send(&msg);
    println!("Sender: Sent IOSurface Mach port");

    // 6. Keep alive briefly to ensure message delivered
    thread::sleep(Duration::from_secs(2));
    println!("Sender: Done");

    Ok(())
}

/// Claim session with exponential backoff retry.
/// The session may not be registered yet if we start before the launcher
/// finishes processing the spawn request.
fn claim_session_with_retry(
    launcher: &XpcConnection,
    session_id: &str,
) -> Result<XpcEndpoint> {
    let max_retries = 5;
    let mut delay = Duration::from_millis(100);

    for attempt in 1..=max_retries {
        let msg = XpcDictionary::new();
        msg.set_string("action", "claim_session");
        msg.set_string("session_id", session_id);

        match launcher.send_with_reply_sync(&msg) {
            Ok(reply) => {
                // Check for error in reply
                if let Some(err) = reply.get_string("error") {
                    println!("Sender: Attempt {}/{}: {}", attempt, max_retries, err);
                    if attempt < max_retries {
                        thread::sleep(delay);
                        delay *= 2; // Exponential backoff
                        continue;
                    }
                    return Err(XpcError::Unknown(err));
                }

                // Success - get endpoint
                if let Some(endpoint) = reply.get_endpoint("endpoint") {
                    return Ok(endpoint);
                }
                return Err(XpcError::Unknown("No endpoint in reply".into()));
            }
            Err(e) => {
                println!("Sender: Attempt {}/{}: {:?}", attempt, max_retries, e);
                if attempt < max_retries {
                    thread::sleep(delay);
                    delay *= 2;
                    continue;
                }
                return Err(e);
            }
        }
    }

    Err(XpcError::Unknown("Max retries exceeded".into()))
}
```

#### Directory Structure

```
ts3/termsurf-xpc/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── connection.rs
│   ├── listener.rs
│   ├── dictionary.rs
│   ├── error.rs
│   ├── ffi.rs
│   ├── iosurface.rs              # IOSurface FFI bindings
│   ├── block.rs                  # Safe block wrappers (uses block2)
│   └── runloop.rs                # CFRunLoop wrapper
├── examples/
│   ├── launcher.rs               # XPC service (spawns sender)
│   ├── receiver.rs               # Test receiver (simulates GUI)
│   └── sender.rs                 # Test sender (simulates profile server)
├── xpc-service/
│   └── Info.plist                # XPC service bundle metadata
└── scripts/
    ├── build-test.sh             # Build everything
    └── run-test.sh               # Run the test
```

#### Build Script

```bash
#!/bin/bash
# ts3/termsurf-xpc/scripts/build-test.sh

set -e
cd "$(dirname "$0")/.."

echo "=== Building XPC Test Bundle ==="

# Validate required files exist
echo "Checking prerequisites..."
if [ ! -f "xpc-service/Info.plist" ]; then
    echo "ERROR: xpc-service/Info.plist not found!"
    echo "Create this file with the XPC service configuration."
    exit 1
fi

# Validate Info.plist has required keys
if ! grep -q "com.termsurf.xpc-test" xpc-service/Info.plist; then
    echo "ERROR: Info.plist missing CFBundleIdentifier 'com.termsurf.xpc-test'"
    exit 1
fi
if ! grep -q "XPCService" xpc-service/Info.plist; then
    echo "ERROR: Info.plist missing XPCService dictionary"
    exit 1
fi
echo "Prerequisites OK"

# Build all Rust binaries
echo "Building Rust binaries..."
cargo build --release --example launcher
cargo build --release --example receiver
cargo build --release --example sender

# Verify binaries were created
for bin in launcher receiver sender; do
    if [ ! -f "../../target/release/examples/$bin" ]; then
        echo "ERROR: Failed to build $bin"
        exit 1
    fi
done
echo "Binaries built successfully"

# Create test app bundle structure
APP="TestXPC.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/XPCServices/com.termsurf.xpc-test.xpc/Contents/MacOS"

# Copy binaries
cp ../../target/release/examples/receiver "$APP/Contents/MacOS/"
cp ../../target/release/examples/sender "$APP/Contents/MacOS/"
cp ../../target/release/examples/launcher \
   "$APP/Contents/XPCServices/com.termsurf.xpc-test.xpc/Contents/MacOS/"

# Copy XPC service Info.plist
cp xpc-service/Info.plist \
   "$APP/Contents/XPCServices/com.termsurf.xpc-test.xpc/Contents/"

# Create app Info.plist
cat > "$APP/Contents/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.termsurf.xpc-test</string>
    <key>CFBundleExecutable</key>
    <string>receiver</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
</dict>
</plist>
EOF

# Sign the XPC service (required for launchd to load it)
echo "Signing XPC service..."
codesign --force --sign - \
    "$APP/Contents/XPCServices/com.termsurf.xpc-test.xpc"

# Sign the app
echo "Signing app bundle..."
codesign --force --sign - "$APP"

# Validate bundle structure
echo "Validating bundle structure..."
ERRORS=0

check_file() {
    if [ ! -f "$1" ]; then
        echo "  MISSING: $1"
        ERRORS=$((ERRORS + 1))
    else
        echo "  OK: $1"
    fi
}

check_file "$APP/Contents/Info.plist"
check_file "$APP/Contents/MacOS/receiver"
check_file "$APP/Contents/MacOS/sender"
check_file "$APP/Contents/XPCServices/com.termsurf.xpc-test.xpc/Contents/Info.plist"
check_file "$APP/Contents/XPCServices/com.termsurf.xpc-test.xpc/Contents/MacOS/launcher"

if [ $ERRORS -gt 0 ]; then
    echo "ERROR: Bundle validation failed with $ERRORS errors"
    exit 1
fi

# Verify code signing
echo "Verifying code signatures..."
codesign --verify --verbose "$APP" 2>&1 || {
    echo "ERROR: App signature verification failed"
    exit 1
}
codesign --verify --verbose "$APP/Contents/XPCServices/com.termsurf.xpc-test.xpc" 2>&1 || {
    echo "ERROR: XPC service signature verification failed"
    exit 1
}

echo ""
echo "=== Build complete: $APP ==="
echo ""
echo "Bundle structure:"
find "$APP" -type f | sed 's/^/  /'
```

#### Test Script

```bash
#!/bin/bash
# ts3/termsurf-xpc/scripts/run-test.sh

set -e
cd "$(dirname "$0")/.."

# Build first
./scripts/build-test.sh

echo ""
echo "=== Running XPC Test ==="
echo ""

# Run receiver (it will ask launcher to spawn sender)
# Timeout after 30 seconds
timeout 30 ./TestXPC.app/Contents/MacOS/receiver
EXIT_CODE=$?

echo ""
if [ $EXIT_CODE -eq 0 ]; then
    echo "=== TEST PASSED ==="
else
    echo "=== TEST FAILED (exit code: $EXIT_CODE) ==="
    exit 1
fi
```

#### Success Criteria

- [ ] XPC service starts when receiver connects (check Activity Monitor)
- [ ] Launcher successfully spawns sender as child process
- [ ] Sender connects to launcher and claims session
- [ ] Sender receives receiver's endpoint
- [ ] Sender connects directly to receiver via endpoint
- [ ] Sender creates IOSurface (100x100 pink)
- [ ] `IOSurfaceCreateMachPort()` succeeds
- [ ] Receiver receives Mach port
- [ ] `IOSurfaceLookupFromMachPort()` returns valid handle
- [ ] IOSurface dimensions are 100x100
- [ ] Pixel at (0,0) is hot pink (0xFF69B4)
- [ ] Test prints "SUCCESS" and exits 0

#### Failure Modes

| Symptom                               | Likely Cause                                           |
| ------------------------------------- | ------------------------------------------------------ |
| "Connection refused" on launcher      | XPC service not bundled correctly, check Console.app   |
| Sender never starts                   | Launcher can't spawn (sandbox?), check entitlements    |
| Sender starts but can't connect       | Timing issue, or XPC service crashed after spawn       |
| "No endpoint in reply"                | Session not registered, race condition                 |
| Direct connection fails               | Endpoint invalid, or anonymous listener pattern broken |
| `IOSurfaceCreateMachPort` returns 0   | IOSurface not created correctly                        |
| `IOSurfaceLookupFromMachPort` is NULL | Mach port not transferred, or wrong port type          |
| Wrong dimensions/color                | IOSurface creation bug (unrelated to XPC)              |
| Test hangs                            | Event handler not called, check block implementation   |

#### Debugging

**Console.app filters:**

- Process: `com.termsurf.xpc-test`
- Process: `receiver`
- Process: `sender`

**Check XPC service is running:**

```bash
launchctl list | grep termsurf
```

**Check code signing:**

```bash
codesign -dvv TestXPC.app
codesign -dvv TestXPC.app/Contents/XPCServices/com.termsurf.xpc-test.xpc
```

#### Dependencies

Before running this experiment, complete these in `termsurf-xpc`:

1. **Block-based event handlers** (`src/block.rs`)
   - Use `block2` crate to create Objective-C blocks from Rust closures
   - Implement `set_event_handler()` and `set_new_connection_handler()`
   - Handle block lifetime correctly (prevent use-after-free)

2. **IOSurface bindings** (`src/iosurface.rs`)
   - `IOSurfaceCreate()` — create new surface
   - `IOSurfaceGetBaseAddress()` / `IOSurfaceLock()` / `IOSurfaceUnlock()` —
     pixel access
   - `IOSurfaceCreateMachPort()` — create sendable port
   - `IOSurfaceLookupFromMachPort()` — reconstruct from port
   - `IOSurfaceGetWidth()` / `IOSurfaceGetHeight()` — dimension queries
   - Helper: `create_iosurface(w, h)`, `fill_with_color()`, `read_pixel()`

3. **Run loop** (`src/runloop.rs`)
   - `CFRunLoopRun()` wrapper
   - Or use `dispatch_main()` if using dispatch queues

4. **Verify anonymous listener pattern**
   - Test that `xpc_connection_create_mach_service(NULL, queue, LISTENER)` works
   - If not, research alternative approaches

#### After This Experiment

With all XPC primitives validated, Experiment 2 integrates with the real app:

1. Move launcher into `WezTerm.app/Contents/XPCServices/`
2. Replace receiver logic → `wezterm-gui` XPC client
3. Replace sender → profile server with CEF
4. Replace pink IOSurface → CEF's `on_accelerated_paint` surface
5. Render received texture in terminal pane
6. Replace pink IOSurface with CEF `on_accelerated_paint` surface
7. Render to actual terminal pane

---

### Experiment 2: XPC IOSurface Transfer with Test Texture

**Status:** SUCCESS

**Goal:** Validate the complete XPC architecture by displaying a test texture in
the terminal pane. Running `web google.com` will display a pink 100x100 texture
stretched to fill the pane, proving the entire IPC pipeline works before
integrating CEF.

**Result:** Running `web google.com` successfully displays a pink texture
stretched to fill the entire terminal window. The complete IPC pipeline works:
web CLI → Unix socket → GUI → XPC → launcher → test-sender → XPC Mach port → GUI
→ IOSurfaceLookupFromMachPort → wgpu texture import → render pipeline.

**Key Insight:** IOSurface IDs cannot be shared via Unix sockets on macOS.
`IOSurfaceLookupByID()` requires Mach port authorization. XPC with Mach port
transfer is the **only** viable approach for cross-process IOSurface sharing.

**Hybrid Architecture:** The existing `webview_socket.rs` handles control
messages (open/close/resize) via Unix socket. XPC is used **only** for Mach port
transfer. This minimizes changes to the existing codebase.

#### What the User Sees

```
$ web google.com
```

- Terminal pane fills with solid pink (stretched from 100x100 texture)
- Resizing the pane stretches the pink to fit
- Ctrl+C exits and restores the terminal

#### Why Pink?

- **Not purple** — Purple often indicates uninitialized GPU memory or errors
- **Not black** — Could be confused with "nothing rendered"
- **Not white** — Could be confused with a blank page
- **Bright pink (#FF69B4)** — Unmistakably intentional, clearly a test texture

#### Architecture

**Two IPC channels:**

- **Unix socket** (existing): Control messages between web CLI and GUI
- **XPC** (new): Mach port transfer between test sender and GUI

```
web CLI                    GUI                         Launcher (XPC)           Test Sender
───────                    ───                         ──────────────           ───────────
    │                       │                               │                        │
    │── open_webview ──────>│  (Unix socket, existing)      │                        │
    │   {pane_id, url}      │                               │                        │
    │                       │                               │                        │
    │                       │── spawn_profile ─────────────>│  (XPC)                 │
    │                       │   {session_id, endpoint}      │                        │
    │                       │                               │── spawn ──────────────>│
    │                       │                               │   --session-id UUID    │
    │                       │                               │                        │
    │                       │                               │<── claim_session ──────│
    │                       │                               │    {session_id}        │
    │                       │                               │                        │
    │                       │                               │── endpoint ───────────>│
    │                       │                               │                        │
    │                       │<══════════════════ XPC (direct) ═════════════════════>│
    │                       │                                                        │
    │                       │<── send_surface ──────────────────────────────────────│
    │                       │    {pane_id, mach_port, width, height}                 │
    │                       │                                                        │
    │                       │── IOSurfaceLookupFromMachPort()                        │
    │                       │── import as wgpu texture                               │
    │                       │── render stretched to pane                             │
    │                       │                                                        │
    │<── response ─────────│  (Unix socket)                                         │
    │   {webview_id}        │                                                        │
```

#### Components

##### 1. Launcher XPC Service (`ts3/termsurf-launcher/`) — Rust, macOS-only

**Why macOS-only?** The launcher exists solely because XPC is required for Mach
port transfer. Other platforms don't need it:

- **Linux:** DMA-BUF file descriptors pass over Unix sockets via `SCM_RIGHTS`
- **Windows:** `DuplicateHandle()` copies DXGI handles between processes

On Linux/Windows, the GUI spawns profile servers directly and passes texture
handles over existing IPC. No launcher needed.

**Why Rust?** Experiment 1 validates that Rust XPC bindings work correctly.
Using Rust for the launcher maintains consistency with the rest of the codebase
and avoids introducing a second language. The `termsurf-xpc` crate provides all
needed functionality.

The launcher is based on Experiment 1's working code, with production service
name (`com.termsurf.launcher` instead of `com.termsurf.xpc-test`):

```rust
// ts3/termsurf-launcher/src/main.rs
// Copy from ts3/termsurf-xpc/examples/launcher.rs with these changes:
// 1. Service name: "com.termsurf.launcher"
// 2. Sender binary: "termsurf-test-sender"
// 3. Action name: "spawn_profile" (not "spawn_sender")

use termsurf_xpc::*;
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::env;

fn main() {
    println!("Launcher: Starting...");

    let sessions: Arc<Mutex<HashMap<String, XpcEndpoint>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // CRITICAL: Store connections wrapped in Arc to share with event handler
    let clients: Arc<Mutex<Vec<Arc<XpcConnection>>>> =
        Arc::new(Mutex::new(Vec::new()));

    // Path to test sender binary
    // Launcher is at: .app/Contents/XPCServices/com.termsurf.launcher.xpc/Contents/MacOS/launcher
    // Sender is at:   .app/Contents/MacOS/termsurf-test-sender
    let exe_path = env::current_exe().expect("Failed to get exe path");
    let sender_path = exe_path
        .parent()                    // MacOS
        .and_then(|p| p.parent())    // Contents
        .and_then(|p| p.parent())    // com.termsurf.launcher.xpc
        .and_then(|p| p.parent())    // XPCServices
        .and_then(|p| p.parent())    // Contents
        .map(|p| p.join("MacOS").join("termsurf-test-sender"))
        .expect("Failed to compute sender path");

    let listener = XpcListener::new_mach_service("com.termsurf.launcher")
        .expect("Failed to create listener");

    let sessions_clone = sessions.clone();
    let clients_clone = clients.clone();

    set_new_connection_handler(&listener, move |conn| {
        println!("Launcher: New connection");

        // Wrap in Arc to share with event handler
        let conn = Arc::new(conn);
        let conn_for_handler = conn.clone();

        let sessions = sessions_clone.clone();
        let sender_path = sender_path.clone();
        let clients_inner = clients_clone.clone();

        set_event_handler(&*conn, move |event| {
            match event {
                Ok(msg) => {
                    let action = msg.get_string("action").unwrap_or_default();

                    match action.as_str() {
                        "spawn_profile" => {
                            let session_id = match msg.get_string("session_id") {
                                Some(id) => id,
                                None => { eprintln!("Missing session_id"); return; }
                            };
                            let endpoint = match msg.get_endpoint("gui_endpoint") {
                                Some(ep) => ep,
                                None => { eprintln!("Missing gui_endpoint"); return; }
                            };

                            sessions.lock().unwrap().insert(session_id.clone(), endpoint);

                            match Command::new(&sender_path)
                                .args(["--session-id", &session_id])
                                .spawn()
                            {
                                Ok(child) => println!("Launcher: Spawned sender {} (pid {})",
                                    session_id, child.id()),
                                Err(e) => eprintln!("Launcher: Failed to spawn: {}", e),
                            }
                        }

                        "claim_session" => {
                            let session_id = match msg.get_string("session_id") {
                                Some(id) => id,
                                None => { eprintln!("Missing session_id"); return; }
                            };

                            let endpoint = sessions.lock().unwrap().remove(&session_id);

                            let reply = match XpcDictionary::create_reply(&msg) {
                                Ok(r) => r,
                                Err(e) => { eprintln!("Failed to create reply: {}", e); return; }
                            };

                            if let Some(ep) = endpoint {
                                reply.set_endpoint("endpoint", ep);
                            } else {
                                reply.set_string("error", "session not found");
                            }

                            conn_for_handler.send(&reply);
                        }

                        _ => eprintln!("Launcher: Unknown action: {}", action),
                    }
                }
                Err(e) => eprintln!("Launcher: Connection error: {}", e),
            }
        });

        conn.resume();

        // CRITICAL: Store connection to keep it alive
        clients_inner.lock().unwrap().push(conn);
    });

    listener.resume();
    println!("Launcher: Running...");
    run_loop();
}
```

**Info.plist:** Registers as `com.termsurf.launcher`

**Sandbox Note:** XPC services are sandboxed by default. To spawn child
processes, disable the sandbox via entitlements:

```xml
<!-- termsurf-launcher.entitlements -->
<key>com.apple.security.app-sandbox</key>
<false/>
```

##### 2. Test Sender (`ts3/termsurf-test-sender/`)

Based on Experiment 1's working sender, creates and sends a test IOSurface:

```rust
// ts3/termsurf-test-sender/src/main.rs
use clap::Parser;
use std::thread;
use std::time::Duration;
use termsurf_xpc::*;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    session_id: String,
}

fn main() {
    let args = Args::parse();
    println!("TestSender: Starting for session '{}'", args.session_id);

    // 1. Connect to launcher
    let launcher = XpcConnection::connect_mach_service("com.termsurf.launcher")
        .expect("Failed to connect to launcher");
    set_event_handler(&launcher, |event| {
        if let Err(e) = event { eprintln!("Launcher error: {}", e); }
    });
    launcher.resume();
    thread::sleep(Duration::from_millis(100));

    // 2. Claim session with retry (launcher may not have stored endpoint yet)
    let gui_endpoint = claim_session_with_retry(&launcher, &args.session_id)
        .expect("Failed to claim session");
    println!("TestSender: Got GUI endpoint");

    // 3. Connect directly to GUI
    let gui = XpcConnection::from_endpoint(gui_endpoint)
        .expect("Failed to connect to GUI");
    set_event_handler(&gui, |event| {
        if let Err(e) = event {
            eprintln!("GUI disconnected: {}", e);
            std::process::exit(0);
        }
    });
    gui.resume();
    thread::sleep(Duration::from_millis(100));

    // 4. Create pink IOSurface (100x100, hot pink)
    let surface = iosurface::create_iosurface(100, 100)
        .expect("Failed to create IOSurface");
    iosurface::fill_with_color(surface, 255, 105, 180, 255); // Hot pink
    println!("TestSender: Created 100x100 hot pink IOSurface");

    // 5. Create Mach port and send to GUI
    let port = iosurface::create_mach_port(surface);
    let msg = XpcDictionary::new();
    msg.set_string("action", "send_surface");
    msg.set_mach_send("mach_port", port);
    msg.set_u64("width", 100);
    msg.set_u64("height", 100);
    gui.send(&msg);
    println!("TestSender: Sent Mach port to GUI");

    // 6. Keep alive until GUI disconnects
    println!("TestSender: Waiting for GUI to disconnect...");
    run_loop();
}

fn claim_session_with_retry(launcher: &XpcConnection, session_id: &str) -> Result<XpcEndpoint> {
    let max_retries = 10;
    let mut delay = Duration::from_millis(100);

    for attempt in 1..=max_retries {
        let msg = XpcDictionary::new();
        msg.set_string("action", "claim_session");
        msg.set_string("session_id", session_id);

        match launcher.send_with_reply_sync(&msg) {
            Ok(reply) => {
                if let Some(err) = reply.get_string("error") {
                    println!("TestSender: Attempt {}/{}: {}", attempt, max_retries, err);
                    if attempt < max_retries {
                        thread::sleep(delay);
                        delay = delay.min(Duration::from_secs(2)) * 2;
                        continue;
                    }
                    return Err(XpcError::Unknown(err));
                }
                if let Some(endpoint) = reply.get_endpoint("endpoint") {
                    return Ok(endpoint);
                }
                return Err(XpcError::Unknown("No endpoint in reply".into()));
            }
            Err(e) => {
                println!("TestSender: Attempt {}/{}: {:?}", attempt, max_retries, e);
                if attempt < max_retries {
                    thread::sleep(delay);
                    delay = delay.min(Duration::from_secs(2)) * 2;
                    continue;
                }
                return Err(e);
            }
        }
    }
    Err(XpcError::Unknown("Max retries exceeded".into()))
}
```

##### 3. GUI Modifications (`ts3/wezterm-gui/`)

**Existing:** `src/termwindow/webview_socket.rs` (keep for control messages)

The socket server continues to handle `open_webview`, `close_webview`, etc. When
it receives `open_webview`, it now also triggers XPC spawn via the new XPC
manager.

**New file:** `src/termwindow/webview_xpc.rs`

XPC client for receiving Mach ports from test sender (and later, profile
server):

```rust
use termsurf_xpc::*;
use std::sync::{Arc, Mutex};

pub struct XpcManager {
    // Anonymous listener for senders to connect
    listener: XpcListener,
    endpoint: XpcEndpoint,

    // Connection to launcher
    launcher: Option<XpcConnection>,

    // CRITICAL: Store peer connections to keep them alive
    peers: Arc<Mutex<Vec<XpcConnection>>>,

    // Callback when IOSurface received
    on_surface: Box<dyn Fn(PaneId, mach_port_t, u32, u32) + Send>,
}

impl XpcManager {
    pub fn new(on_surface: impl Fn(PaneId, mach_port_t, u32, u32) + Send + 'static) -> Result<Self> {
        let listener = XpcListener::new_anonymous()?;
        let endpoint = listener.get_endpoint()?;
        let peers: Arc<Mutex<Vec<XpcConnection>>> = Arc::new(Mutex::new(Vec::new()));

        // Set up handler for incoming connections
        let peers_clone = peers.clone();
        let on_surface = Arc::new(Mutex::new(on_surface));
        let on_surface_clone = on_surface.clone();

        set_new_connection_handler(&listener, move |peer| {
            let on_surface = on_surface_clone.clone();
            let peers_inner = peers_clone.clone();

            set_event_handler(&peer, move |event| {
                if let Ok(msg) = event {
                    if msg.get_string("action").as_deref() == Some("send_surface") {
                        let port = msg.copy_mach_send("mach_port");
                        let pane_id = msg.get_u64("pane_id") as PaneId;
                        let width = msg.get_u64("width") as u32;
                        let height = msg.get_u64("height") as u32;

                        let callback = on_surface.lock().unwrap();
                        callback(pane_id, port, width, height);
                    }
                }
            });
            peer.resume();

            // CRITICAL: Store peer to keep connection alive
            peers_inner.lock().unwrap().push(peer);
        });

        listener.resume();

        Ok(Self {
            listener,
            endpoint,
            launcher: None,
            peers,
            on_surface: Box::new(|_, _, _, _| {}),
        })
    }

    /// Spawn a profile via the launcher XPC service
    pub fn spawn_profile(&mut self, session_id: &str) -> Result<()> {
        // Lazy connect to launcher
        if self.launcher.is_none() {
            let launcher = XpcConnection::connect_mach_service("com.termsurf.launcher")?;
            set_event_handler(&launcher, |_| {});
            launcher.resume();
            self.launcher = Some(launcher);
        }

        let msg = XpcDictionary::new();
        msg.set_string("action", "spawn_profile");
        msg.set_string("session_id", session_id);
        msg.set_endpoint("gui_endpoint", self.endpoint);

        self.launcher.as_ref().unwrap().send(&msg);
        Ok(())
    }
}
```

**Render changes:** `src/termwindow/render/pane.rs`

After rendering terminal content, check for webview overlay:

```rust
// In paint_pane() or equivalent
if let Some(overlay) = webview_state.get_overlay(pane_id) {
    // Import IOSurface as wgpu texture
    let importer = IOSurfaceImporter::from_mach_port(
        overlay.mach_port,
        CEF_COLOR_TYPE_BGRA_8888,
        overlay.width,
        overlay.height,
    )?;

    let texture = importer.import_to_wgpu(device)?;

    // Render fullscreen quad stretched to pane dimensions
    render_textured_quad(texture, pane_rect);
}
```

##### 4. cef-rs Modification

**File:** `cef-rs/cef/src/osr_texture_import/iosurface.rs`

Add constructor for Mach port import:

```rust
impl IOSurfaceImporter {
    /// Create from a Mach port received via XPC.
    /// This is the only way to share IOSurfaces across processes on macOS.
    pub fn from_mach_port(
        port: mach_port_t,
        format: cef_color_type_t,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        let handle = unsafe { IOSurfaceLookupFromMachPort(port) };
        if handle.is_null() {
            return None;
        }
        Some(Self { handle, format, width, height })
    }
}
```

##### 5. Web CLI

Existing code already sends `open_webview` to GUI via socket. No changes needed.
The GUI socket server triggers XPC spawn internally.

#### Files to Create

| File                                                   | Purpose                              |
| ------------------------------------------------------ | ------------------------------------ |
| `ts3/termsurf-launcher/Cargo.toml`                     | Launcher crate manifest              |
| `ts3/termsurf-launcher/src/main.rs`                    | XPC service (copy from Experiment 1) |
| `ts3/termsurf-launcher/Info.plist`                     | XPC service registration             |
| `ts3/termsurf-launcher/termsurf-launcher.entitlements` | Sandbox disabled for spawn           |
| `ts3/termsurf-test-sender/Cargo.toml`                  | Test sender crate manifest           |
| `ts3/termsurf-test-sender/src/main.rs`                 | IOSurface creation + Mach port send  |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`        | XPC client for receiving Mach ports  |

#### Files to Modify

| File                                               | Changes                                     |
| -------------------------------------------------- | ------------------------------------------- |
| `cef-rs/cef/src/osr_texture_import/iosurface.rs`   | Add `from_mach_port()` constructor          |
| `ts3/wezterm-gui/src/termwindow/mod.rs`            | Initialize XPC manager                      |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Trigger XPC spawn on `open_webview`         |
| `ts3/wezterm-gui/src/termwindow/render/pane.rs`    | Render webview overlay texture              |
| `ts3/wezterm-gui/Cargo.toml`                       | Add termsurf-xpc, cef dependencies          |
| `ts3/Cargo.toml`                                   | Add launcher, test-sender workspace members |
| Build scripts                                      | Bundle XPC service in wezterm-gui.app       |

#### IOSurface → wgpu Import

The GUI receives a Mach port and must import it as a wgpu texture. This uses
cef-rs's IOSurface import code from `cef/src/osr_texture_import/`, extended with
the new `from_mach_port()` constructor.

**Step-by-step import:**

```rust
use cef::osr_texture_import::IOSurfaceImporter;
use cef::sys::CEF_COLOR_TYPE_BGRA_8888;

/// Import IOSurface from Mach port into wgpu texture
fn import_iosurface(
    device: &wgpu::Device,
    port: mach_port_t,
    width: u32,
    height: u32,
) -> Option<wgpu::Texture> {
    // 1. Create importer from Mach port (handles IOSurfaceLookupFromMachPort internally)
    let importer = IOSurfaceImporter::from_mach_port(
        port,
        CEF_COLOR_TYPE_BGRA_8888,
        width,
        height,
    )?;

    // 2. Import as wgpu texture via Metal
    importer.import_to_wgpu(device).ok()
}
```

**Color format:** For the test texture, use `CEF_COLOR_TYPE_BGRA_8888`. The
importer's `import_to_wgpu()` handles the Metal texture creation and format
conversion, including sRGB view formats for correct gamma.

**Metal interop:** The `IOSurfaceImporter` wraps Metal's
`newTextureWithDescriptor:iosurface:plane:` API, which creates a Metal texture
backed by the IOSurface memory. wgpu can then use this texture via its Metal
backend. No pixel copying occurs — it's zero-copy GPU-to-GPU sharing.

#### Texture Lifecycle

When a new IOSurface is received (e.g., on resize), the old texture must be
released before importing the new one. GPU resources aren't automatically
garbage collected.

**State to track:**

```rust
struct WebviewOverlay {
    pane_id: PaneId,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    importer: IOSurfaceImporter,  // Holds Metal texture reference
    width: u32,
    height: u32,
}

struct WebviewState {
    overlays: HashMap<PaneId, WebviewOverlay>,
}
```

**On new IOSurface received:**

```rust
fn update_overlay(
    &mut self,
    device: &wgpu::Device,
    pane_id: PaneId,
    port: mach_port_t,
    width: u32,
    height: u32,
) {
    // 1. Remove old overlay (drops texture, importer, frees GPU memory)
    if let Some(old) = self.overlays.remove(&pane_id) {
        drop(old);  // Explicit for clarity
    }

    // 2. Import new IOSurface
    if let Some(new_overlay) = self.create_overlay(device, pane_id, port, width, height) {
        self.overlays.insert(pane_id, new_overlay);
    }
}
```

**Why explicit drop matters:** The `IOSurfaceImporter` holds a reference to the
Metal texture, which holds a reference to the IOSurface. If you don't drop the
old importer before creating a new one, you may hit memory limits on systems
with limited VRAM.

**For this experiment:** Since we only send one test texture (no resize), this
is less critical. But the code should be correct from the start to avoid bugs
when CEF sends new textures on every paint.

#### Connection Management

The GUI must maintain XPC connections and state across multiple webview
lifecycles.

**State required:**

```rust
struct XpcManager {
    // Connection to launcher (persistent, reused for all webviews)
    launcher: Option<XpcConnection>,

    // Anonymous listener for profile servers to connect
    listener: XpcListener,
    listener_endpoint: XpcEndpoint,

    // CRITICAL: Store peer connections to keep them alive
    peers: Arc<Mutex<Vec<XpcConnection>>>,

    // Active overlays per pane
    overlays: HashMap<PaneId, WebviewOverlay>,

    // Pending sessions (waiting for profile server to connect)
    pending_sessions: HashMap<SessionId, PaneId>,
}
```

**Connection lifecycle:**

1. **GUI startup:**
   - Create anonymous XPC listener
   - Get endpoint from listener
   - Connect to launcher (lazy, on first webview request)

2. **`open_webview` request:**
   - Generate unique session ID
   - Store in `pending_sessions`
   - Send `spawn_profile` to launcher with endpoint + session ID

3. **Profile server connects:**
   - Received in `set_new_connection_handler` callback
   - **CRITICAL:** Store connection in `peers` to keep it alive
   - Wait for `session_id` message to match with `pending_sessions`
   - Remove from `pending_sessions`, associate connection with pane

4. **IOSurface received:**
   - Look up pane from connection
   - Import texture, store in `overlays`

5. **`close_webview` request:**
   - Remove from `overlays` (drops texture)
   - Drop peer connection (cancels XPC connection)
   - Profile server receives disconnect, exits

**Critical pattern reminder:** Every connection received in
`set_new_connection_handler` must be stored. If you don't store it, the
connection is canceled when the handler returns:

```rust
// WRONG - connection canceled immediately!
set_new_connection_handler(&listener, |peer| {
    set_event_handler(&peer, |event| { ... });
    peer.resume();
    // peer dropped here → connection canceled
});

// CORRECT - store peer to keep it alive
set_new_connection_handler(&listener, move |peer| {
    set_event_handler(&peer, |event| { ... });
    peer.resume();
    peers_clone.lock().unwrap().push(peer);  // Keep alive!
});
```

#### Success Criteria

- [x] `web google.com` displays solid pink in the pane
- [x] Pink fills entire pane (stretched from 100x100)
- [ ] Resizing pane re-stretches the pink (no flicker, no re-fetch)
- [x] Ctrl+C exits cleanly, terminal restored
- [x] Logs show: "IOSurfaceLookupFromMachPort returned valid handle"
- [x] Socket server still handles control messages correctly

#### What This Proves

1. **Hybrid architecture works** — Socket for control, XPC for Mach ports
2. **XPC service bundling works** — launchd finds and launches our service
3. **Endpoint relay works** — GUI endpoint successfully passed through launcher
4. **Direct XPC connection works** — Test sender connects to GUI via endpoint
5. **Mach port transfer works** — IOSurface port sent over XPC
6. **IOSurfaceLookupFromMachPort works** — Handle valid in receiving process
7. **from_mach_port() works** — New IOSurfaceImporter constructor functions
8. **Texture import works** — IOSurface → wgpu texture pipeline
9. **Rendering works** — Stretched quad displays correctly in pane

#### Failure Modes

| Symptom                     | Likely Cause                                            |
| --------------------------- | ------------------------------------------------------- |
| `web` hangs                 | Launcher not starting, check Console.app for XPC errors |
| Socket error                | webview_socket.rs not initialized properly              |
| XPC connection invalid      | Launcher not registered with launchd                    |
| Black pane                  | IOSurfaceLookupFromMachPort failed, check logs          |
| Purple pane                 | Texture import failed, uninitialized GPU memory         |
| Pink square (not stretched) | Render quad not using pane dimensions                   |
| No texture displayed        | XpcManager not receiving callbacks, check peer storage  |
| Crash on resize             | Texture lifecycle issue                                 |

#### After This Experiment

If successful, Experiment 2 replaces `termsurf-test-sender` with the real
profile server:

1. Profile server uses same XPC flow
2. Instead of pink IOSurface, uses `on_accelerated_paint` IOSurface
3. Sends Mach port on first paint and on resize
4. Real webpage appears in pane
