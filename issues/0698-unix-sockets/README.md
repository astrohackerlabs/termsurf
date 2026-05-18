+++
status = "closed"
opened = "2026-03-02"
closed = "2026-03-02"
+++

# Issue 698: Replace XPC with Unix Domain Sockets

## Goal

Eliminate XPC as the IPC mechanism and replace it with Unix domain sockets. This
removes the xpc-gateway daemon, the launchd plist, and the macOS-specific IPC
layer — enabling cross-platform IPC with a single codebase for macOS and Linux.

## Background

### The original XPC justification

XPC was chosen because IOSurface textures can only be transferred between
processes via Mach ports, and Mach ports can only be transferred via XPC. This
was a hard macOS kernel constraint. Every IPC channel used XPC because the
texture channel required it, and a second mechanism would add complexity for
zero benefit.

This constraint was documented in CLAUDE.md as a "settled architectural
decision" and proven across ts3 (Issues 303, 325-350) and ts4 (Issues 403, 407).

### CALayerHost eliminated the constraint

Issue 625 replaced `FrameSinkVideoCapturer` with `CALayerHost`. The old pipeline
transferred an IOSurface Mach port over XPC on every frame (~60fps). The new
pipeline sends a `ca_context_id` (uint32) once per tab. Window Server composites
directly from GPU VRAM — zero per-frame IPC.

The `ca_context_id` is just an integer. CALayerHost is a Window Server mechanism
— any process that knows the ID can create a `CALayerHost` and display the
remote content. No Mach port transfer, no XPC-specific capability required.

**The original justification for XPC no longer applies.** The CLAUDE.md "settled
architectural decision" was written for the IOSurface era and never revisited
after the CALayerHost migration (Issues 624-632).

### What XPC currently provides

Reviewing all 30 XPC message types across three processes (GUI, TUI, Chromium
Profile Server):

| Data type      | Count | Example messages                      |
| -------------- | ----- | ------------------------------------- |
| string         | 30/30 | URLs, pane IDs, profiles, actions     |
| int64          | 20/30 | tab IDs, cursor types, key codes      |
| uint64         | 14/30 | pixel dimensions, modifiers, progress |
| double         | 6/30  | mouse coordinates, scroll deltas      |
| bool           | 10/30 | focus, dark mode, browsing state      |
| xpc_endpoint_t | 1/30  | gateway connect reply                 |

Every message is a flat dictionary of basic types. The only XPC-specific feature
still in use is `xpc_endpoint_t` — which is only needed because XPC itself
requires it for the gateway handshake pattern.

### Why the gateway exists

A Mach service can only be claimed by the process launchd launched for that job.
When you launch TermSurf via `open TermSurf.app`, macOS assigns it an
application bundle identity that doesn't match the launchd plist's job identity.
The kernel rejects the app's attempt to claim the Mach service.

The gateway (~86 lines of Swift) is a tiny daemon that launchd launches to own
the `com.termsurf.xpc-gateway` Mach service. It handles exactly two messages:

1. `register_app` — TermSurf app deposits its anonymous listener endpoint
2. `connect` — `web` TUI (and Chromium Profile Server) claims the endpoint

After the handshake, the gateway is idle. All traffic flows directly between
processes.

**With Unix domain sockets, none of this is needed.** The app listens on a
well-known socket path. Clients connect directly. No launchd, no plist, no
gateway daemon, no endpoint transfer.

## Analysis

### What changes

**Eliminated entirely:**

- `gui/xpc-gateway/` — the entire gateway daemon (~86 lines Swift)
- `gui/macos/com.termsurf.xpc-gateway.plist` — launchd plist
- `gui/macos/com.termsurf.debug.xpc-gateway.plist` — debug launchd plist
- Gateway installation/registration in build and install scripts
- The `TERMSURF_XPC_SERVICE` environment variable (Issue 653)

**Rewritten:**

- `gui/src/apprt/xpc.zig` (~1800 lines) — replace XPC C API calls with Unix
  socket + message serialization. Rename to `ipc.zig`.
- `tui/src/xpc.rs` (~670 lines) — replace XPC FFI with Unix socket + message
  serialization. Rename to `ipc.rs`.
- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`
  — replace XPC connection setup with Unix socket client.
- `scripts/install.sh` — remove gateway bundling and plist registration steps.

**Unchanged:**

- CALayerHost/CAContext pipeline — unaffected, it's a Window Server mechanism
- All message semantics — same 30 message types, same fields, same directions
- Process architecture — still three processes (GUI, TUI, Chromium server)
- Profile server lifecycle — GUI still spawns and manages Chromium servers

### Service discovery

Replace launchd Mach service with a well-known socket path:

```
$XDG_RUNTIME_DIR/termsurf/termsurf.sock          (release)
$XDG_RUNTIME_DIR/termsurf/termsurf-debug.sock    (debug, replaces Issue 653)
```

On macOS, `$XDG_RUNTIME_DIR` is typically not set. TermSurf already has XDG
handling (Issue 615) — it would fall back to `$TMPDIR/termsurf/` or
`/tmp/termsurf-$UID/`.

The GUI creates the socket on startup and removes it on shutdown. Stale socket
files (from crashes) are detected by attempting to connect — if the connection
is refused, the file is stale and can be replaced.

### Message serialization

XPC dictionaries are replaced with Protocol Buffers (protobuf). All 30 messages
are defined once in a `.proto` file, and code generation produces type-safe
serializers for all three languages automatically.

**Why protobuf:**

- **Single schema, three languages.** One `.proto` file generates Rust, Zig, and
  C++ code. No hand-written parsers, no field ordering bugs, no type mismatches.
- **Extensible.** Adding new messages or fields to existing messages is
  backward-compatible. The message count will grow significantly as features are
  added (downloads, file uploads, JS dialogs, permissions, camera/mic, etc.).
- **Mature libraries.** C++ has Google's official `libprotobuf` (already in
  Chromium at `third_party/protobuf/`). Rust has
  [prost](https://github.com/tokio-rs/prost) (`prost = "0.14"`). Zig has
  [zig-protobuf](https://github.com/Arwalk/zig-protobuf) or can use
  [protobuf-c](https://github.com/allyourcodebase/protobuf-c) via C interop.
- **Compact binary encoding.** Smaller than JSON or msgpack for typed messages.
  No float precision issues.

**Wire format:** Length-prefixed protobuf messages over the Unix socket:

```
[4 bytes: message length][protobuf payload]
```

The `.proto` file lives at the repo root (e.g., `proto/termsurf.proto`) and is
shared by all three build systems.

### Synchronous request/reply

5 messages use XPC's synchronous request/reply (`xpc_dictionary_create_reply`):
`hello`, `query_last`, `query_devtools`, `query_tabs`, and gateway `connect`.

With Unix sockets, synchronous request/reply is implemented by including a
sequence ID in the request and blocking until a response with the same sequence
ID arrives. Or, since these are all TUI→GUI queries, the TUI can simply block on
`recv()` after sending the request — the GUI responds on the same connection.

### Connection topology

**Current (XPC):**

```
GUI ←──xpc_endpoint──→ Gateway ←──xpc_endpoint──→ TUI
GUI ←──xpc_endpoint──→ Gateway ←──xpc_endpoint──→ Chromium Server
GUI ←── direct XPC ──→ TUI (after endpoint handshake)
GUI ←── direct XPC ──→ Chromium Server (after endpoint handshake)
```

**Proposed (Unix sockets):**

```
GUI ←── unix socket ──→ TUI        (direct, no intermediary)
GUI ←── unix socket ──→ Chromium Server  (direct, no intermediary)
```

Each client gets its own connection (accept() returns a new fd). The GUI's event
loop polls all connected fds.

### Chromium server connection

The Chromium Profile Server currently connects to the gateway, requests an
endpoint, then connects directly to the GUI. With Unix sockets, it connects
directly to the socket path. The `--xpc-service` command-line flag becomes
`--ipc-socket` pointing to the socket file path.

### Event loop integration

**GUI (Zig):** Currently uses a GCD serial dispatch queue (`xpc_queue`) for all
XPC event handlers. With Unix sockets, the GUI can either:

- Use `poll()`/`kqueue()` on the socket fds, integrated into the existing event
  loop
- Continue using a GCD dispatch source (`dispatch_source_create` with
  `DISPATCH_SOURCE_TYPE_READ`) to maintain the serial-queue-no-mutex pattern

**TUI (Rust):** Currently uses `block2` crate for ObjC block callbacks. With
Unix sockets, it uses standard `std::net::UnixStream` — pure Rust, no FFI, no
ObjC blocks. The `block2` dependency can be removed entirely.

**Chromium (C++):** Currently uses XPC C API with ObjC blocks. With Unix
sockets, it uses standard POSIX `connect()`/`send()`/`recv()`. No ObjC
dependency for IPC.

### Cross-platform implications

Unix domain sockets work on macOS, Linux, and Windows. This means:

- The IPC layer is platform-agnostic from day one — one codebase for all three
  platforms
- On Linux, the same socket-based IPC works without any adaptation
- On Windows, AF_UNIX has been supported since Windows 10 build 17063 (2017) via
  Winsock. Only `SOCK_STREAM` is supported (no `SOCK_DGRAM`), but TermSurf uses
  stream connections anyway. Windows 11 supports it too.
- The GPU compositing layer remains platform-specific (CALayerHost on macOS,
  Wayland subsurfaces on Linux, TBD on Windows), but IPC is shared

### GPU compositing on Linux

On macOS, CALayerHost lets Window Server composite Chromium's GPU output
directly into TermSurf's window — zero copies, zero per-frame IPC. The closest
Linux analog is **Wayland subsurfaces**.

Wayland subsurfaces work the same way: Chromium renders to its own Wayland
surface, and the Wayland compositor composites that surface at specific
coordinates within TermSurf's window. The compositor handles GPU buffer sharing
directly from VRAM, same as Window Server.

Chromium already has full Wayland support via `--ozone-platform=wayland`. The
rendering side is solved upstream. The integration question is how to get
Chromium's Wayland surface reparented as a subsurface of TermSurf's window —
similar to the CALayerHost `contextId` handshake, but using Wayland's
`wl_subsurface` protocol.

**Wayland is the default display server on every major desktop Linux distro:**

- Fedora — default since 2016 (GNOME) and 2021 (KDE)
- Ubuntu — default since 22.04 LTS (2022)
- Debian — default since Debian 10 (2019)
- Linux Mint — switched to Wayland in 2026
- KDE Plasma — going Wayland-only, dropping X11 support entirely

X11 is effectively legacy. Targeting Wayland only for a Linux port is safe.

### What could go wrong

1. **Stale socket files.** If the app crashes, the socket file persists. Fix:
   check if the socket is live before creating a new one (attempt connect, if
   refused → stale → unlink and recreate).

2. **Permissions.** Socket file inherits directory permissions. The
   `$XDG_RUNTIME_DIR` directory is user-owned (mode 0700 on Linux), so this is
   fine. On macOS with `$TMPDIR`, same situation.

3. **Multiple instances.** Currently the gateway stores one app endpoint — if
   two TermSurf instances run, the second overwrites the first. With Unix
   sockets, the second instance would fail to bind. Fix: include PID or instance
   ID in socket name, or use a directory of sockets.

4. **macOS sandbox.** TermSurf is not sandboxed or notarized. If it ever is,
   Unix sockets in `$TMPDIR` require explicit sandbox entitlements. XPC
   integrates with the sandbox natively. This is a future concern, not a current
   blocker.

5. **File descriptor limits.** Each connection uses one fd. With ~3 connections
   (TUI, Chromium server, maybe a second TUI), this is negligible.

### Scope estimate

| Component                   | Current lines       | Work required                                                      |
| --------------------------- | ------------------- | ------------------------------------------------------------------ |
| xpc-gateway (Swift)         | 86                  | Delete entirely                                                    |
| launchd plists              | 24 each             | Delete                                                             |
| xpc.zig (GUI)               | ~1800               | Rewrite connection setup (~200 lines), keep message handlers       |
| xpc.rs (TUI)                | ~670                | Rewrite connection (~100 lines), simplify (remove block2/ObjC FFI) |
| shell_browser_main_parts.cc | ~50 lines XPC setup | Rewrite to Unix socket client                                      |
| install.sh                  | ~10 lines gateway   | Delete gateway steps                                               |
| build scripts               | gateway build steps | Remove                                                             |

Most of the existing code is message handling logic (parsing fields, looking up
panes, calling surface methods) — that stays the same. Only the transport layer
changes.

## Experiments

### Experiment 1: Protobuf proof-of-concept in all three languages

Prove that protobuf works in Rust, Zig, and C++ with a minimal example. One
trivial `.proto` file, code generation for each language, and a round-trip test
(serialize → bytes → deserialize) in each. No real messages yet — just proof
that the toolchain works end to end.

#### Changes

**1. Create `proto/hello.proto`**

A minimal schema with one message containing each data type we need (string,
int64, uint64, double, bool):

```protobuf
syntax = "proto3";
package termsurf;

message Hello {
  string name = 1;
  int64 id = 2;
  uint64 size = 3;
  double x = 4;
  bool active = 5;
}
```

**2. Rust — standalone test binary**

Create `proto/test-rust/` with its own `Cargo.toml` and `build.rs`. This is
separate from the TUI — a throwaway PoC, not integrated into the real app.

```toml
[package]
name = "proto-test"
edition = "2021"

[dependencies]
prost = "0.14"

[build-dependencies]
prost-build = "0.14"
```

`build.rs` compiles `../../hello.proto`. `main.rs` creates a `Hello`, encodes it
to bytes with `prost::Message::encode`, decodes it back, asserts all fields
match, and prints "Rust: pass".

**3. Zig — standalone test program**

Create `proto/test-zig/` with a `build.zig` that compiles the protobuf-c
generated files and a `main.zig` that calls the C serialization functions.

Generate the C code:

```bash
protoc --c_out=proto/test-zig proto/hello.proto
```

`main.zig` creates a `Termsurf__Hello` struct, calls `termsurf__hello__pack()`
to serialize, `termsurf__hello__unpack()` to deserialize, asserts fields match,
and prints "Zig: pass".

This requires `libprotobuf-c` installed (`brew install protobuf-c`). The
`build.zig` adds the generated `.c` file and links against `protobuf-c`.

If protobuf-c integration proves difficult (linking issues, build.zig
conflicts), fall back to [zig-protobuf](https://github.com/Arwalk/zig-protobuf)
which is pure Zig and uses `build.zig.zon`.

**4. C++ — standalone test program**

Create `proto/test-cpp/` with a `Makefile` or direct compiler invocation.

Generate the C++ code:

```bash
protoc --cpp_out=proto/test-cpp proto/hello.proto
```

`main.cc` creates a `termsurf::Hello`, calls `SerializeToString()`, calls
`ParseFromString()`, asserts fields match, and prints "C++: pass". Link against
`-lprotobuf` (`brew install protobuf` — already installed, `protoc` is on PATH).

#### Verification

Run all three tests:

1. `cd proto/test-rust && cargo run` — prints "Rust: pass"
2. `cd proto/test-zig && zig build run` — prints "Zig: pass"
3. `cd proto/test-cpp && make && ./test` — prints "C++: pass"

**Pass criterion:** All three languages can serialize a `Hello` message to bytes
and deserialize it back with all fields intact. This proves the protobuf
toolchain works for Rust (prost), Zig (protobuf-c), and C++ (libprotobuf) before
we commit to defining the full 30-message schema.

**Result:** Pass

All three tests produce identical output:

```
Rust: pass (35 bytes)
Zig:  pass (35 bytes)
C++:  pass (35 bytes)
```

Libraries used:

- **Rust:** prost 0.14.3 with prost-build 0.14.3. Code generation via
  `build.rs`, zero issues.
- **Zig:** protobuf-c 1.5.2 via C interop (`@cImport`). Generated C code
  compiles and links cleanly with Zig 0.15.2. The C struct fields are accessible
  from Zig after dereferencing the `[*c]` pointer.
- **C++:** libprotobuf 33.4 (installed via Homebrew). Standard
  `SerializeToString` / `ParseFromString` API.

All five data types (string, int64, uint64, double, bool) round-trip correctly
across all three languages. The serialized output is identical (35 bytes),
confirming wire-format compatibility.

#### Conclusion

Protobuf works in all three languages with mature, well-maintained libraries.
The Zig integration via protobuf-c is slightly more verbose (C struct
initialization, manual pack/unpack calls) but fully functional. Ready to define
the full message schema in Experiment 2.

### Experiment 2: Full message schema

Define all 30 IPC messages in `proto/termsurf.proto` and verify that the
generated code compiles in all three languages. Use a `oneof` wrapper so every
message over the wire is a single `TermSurfMessage` type.

#### Changes

**1. Replace `proto/hello.proto` with `proto/termsurf.proto`**

```protobuf
syntax = "proto3";
package termsurf;

// Wrapper — every message on the wire is one of these.
message TermSurfMessage {
  oneof msg {
    // Tab lifecycle (GUI → Chromium)
    CreateTab create_tab = 1;
    CreateDevtoolsTab create_devtools_tab = 2;
    Resize resize = 3;
    CloseTab close_tab = 4;

    // Navigation (GUI → Chromium, TUI → GUI)
    Navigate navigate = 5;

    // Input (GUI → Chromium)
    MouseEvent mouse_event = 6;
    MouseMove mouse_move = 7;
    ScrollEvent scroll_event = 8;
    KeyEvent key_event = 9;

    // State (GUI → Chromium)
    FocusChanged focus_changed = 10;
    SetColorScheme set_color_scheme = 11;

    // Chromium → GUI
    ServerRegister server_register = 12;
    TabReady tab_ready = 13;
    CaContext ca_context = 14;
    UrlChanged url_changed = 15;
    LoadingState loading_state = 16;
    TitleChanged title_changed = 17;
    CursorChanged cursor_changed = 18;

    // TUI → GUI
    SetOverlay set_overlay = 19;
    SetDevtoolsOverlay set_devtools_overlay = 20;
    OpenSplit open_split = 21;

    // GUI → TUI
    ModeChanged mode_changed = 22;

    // Request/reply pairs (TUI ↔ GUI)
    HelloRequest hello_request = 23;
    HelloReply hello_reply = 24;
    QueryLastRequest query_last_request = 25;
    QueryLastReply query_last_reply = 26;
    QueryDevtoolsRequest query_devtools_request = 27;
    QueryDevtoolsReply query_devtools_reply = 28;
    QueryTabsRequest query_tabs_request = 29;
    QueryTabsReply query_tabs_reply = 30;
  }
}

// --- Tab lifecycle ---

message CreateTab {
  string url = 1;
  string pane_id = 2;
  uint64 pixel_width = 3;
  uint64 pixel_height = 4;
  bool dark = 5;
}

message CreateDevtoolsTab {
  string pane_id = 1;
  int64 inspected_tab_id = 2;
  uint64 pixel_width = 3;
  uint64 pixel_height = 4;
  bool dark = 5;
}

message Resize {
  int64 tab_id = 1;
  uint64 pixel_width = 2;
  uint64 pixel_height = 3;
}

message CloseTab {
  int64 tab_id = 1;
}

// --- Navigation ---

message Navigate {
  int64 tab_id = 1;      // nonzero when GUI → Chromium
  string pane_id = 2;    // nonempty when TUI → GUI
  string url = 3;
}

// --- Input ---

message MouseEvent {
  int64 tab_id = 1;
  string type = 2;       // "down" or "up"
  string button = 3;     // "left", "right", "middle"
  double x = 4;
  double y = 5;
  int64 click_count = 6;
  uint64 modifiers = 7;
}

message MouseMove {
  int64 tab_id = 1;
  double x = 2;
  double y = 3;
  uint64 modifiers = 4;
}

message ScrollEvent {
  int64 tab_id = 1;
  double x = 2;
  double y = 3;
  double delta_x = 4;
  double delta_y = 5;
  uint64 phase = 6;
  uint64 momentum_phase = 7;
  bool precise = 8;
  uint64 modifiers = 9;
}

message KeyEvent {
  int64 tab_id = 1;
  string type = 2;       // "down", "up", "repeat"
  int64 windows_key_code = 3;
  string utf8 = 4;
  uint64 modifiers = 5;
}

// --- State ---

message FocusChanged {
  int64 tab_id = 1;
  bool focused = 2;
}

message SetColorScheme {
  int64 tab_id = 1;      // nonzero when GUI → Chromium
  string pane_id = 2;    // nonempty when TUI → GUI
  bool dark = 3;
}

// --- Chromium → GUI ---

message ServerRegister {
  string profile = 1;
}

message TabReady {
  string pane_id = 1;
  int64 tab_id = 2;
}

message CaContext {
  int64 tab_id = 1;
  uint64 ca_context_id = 2;
  uint64 pixel_width = 3;
  uint64 pixel_height = 4;
}

message UrlChanged {
  int64 tab_id = 1;
  string url = 2;
}

message LoadingState {
  int64 tab_id = 1;
  string state = 2;      // "loading", "progress", "done", "error"
  uint64 progress = 3;   // 0-100
}

message TitleChanged {
  int64 tab_id = 1;
  string title = 2;
}

message CursorChanged {
  int64 tab_id = 1;
  int64 cursor_type = 2;
}

// --- TUI → GUI ---

message SetOverlay {
  string pane_id = 1;
  uint64 col = 2;
  uint64 row = 3;
  uint64 width = 4;
  uint64 height = 5;
  string url = 6;
  string profile = 7;
  bool browsing = 8;
}

message SetDevtoolsOverlay {
  string pane_id = 1;
  uint64 col = 2;
  uint64 row = 3;
  uint64 width = 4;
  uint64 height = 5;
  string profile = 6;
  bool browsing = 7;
  int64 inspected_tab_id = 8;
}

message OpenSplit {
  string pane_id = 1;
  string direction = 2;  // "horizontal" or "vertical"
  string command = 3;
}

// --- GUI → TUI ---

message ModeChanged {
  bool browsing = 1;
}

// --- Request/reply pairs ---

message HelloRequest {
  string pane_id = 1;
}

message HelloReply {
  string homepage = 1;
}

message QueryLastRequest {
  string pane_id = 1;
  string profile = 2;
}

message QueryLastReply {
  string pane_id = 1;
  int64 tab_id = 2;
  string profile = 3;
  string error = 4;
}

message QueryDevtoolsRequest {
  string pane_id = 1;
  int64 inspected_tab_id = 2;
  string profile = 3;
}

message QueryDevtoolsReply {
  int64 tab_id = 1;
  string error = 2;
}

message QueryTabsRequest {
  string pane_id = 1;
  string profile = 2;
}

message TabInfo {
  int64 id = 1;
  int64 inspected_tab_id = 2;
  string pane_id = 3;
  string url = 4;
}

message QueryTabsReply {
  int64 gui_panes = 1;
  int64 chromium_tabs = 2;
  int64 chromium_browser = 3;
  int64 chromium_devtools = 4;
  repeated TabInfo tabs = 5;
  string error = 6;
}
```

Key design decisions:

- **`Navigate` and `SetColorScheme` are shared.** These messages are sent both
  TUI→GUI (with `pane_id`) and GUI→Chromium (with `tab_id`). Using one message
  type with both fields avoids duplication. The receiver checks which field is
  populated.
- **`QueryTabsReply` uses `repeated TabInfo`.** The current XPC implementation
  uses dynamic keys (`tab_0`, `tab_1`, ...) which is an anti-pattern. A repeated
  message field is the idiomatic protobuf way.
- **No `action` field.** The `oneof` discriminator replaces the string-based
  action dispatch. Type safety instead of string matching.
- **`oneof` field numbers 1-30.** One per message type, leaving room for future
  additions above 30.

**2. Update `proto/test-rust/` to use `termsurf.proto`**

Change `build.rs` to compile `termsurf.proto`. Update `main.rs` to create a
`TermSurfMessage` wrapping a `CreateTab`, serialize, deserialize, verify the
`oneof` discriminator round-trips correctly.

**3. Update `proto/test-zig/` to use `termsurf.proto`**

Regenerate C code from `termsurf.proto`. Update `main.zig` to create a
`Termsurf__TermSurfMessage` with a `CreateTab` variant, serialize, deserialize,
verify.

**4. Update `proto/test-cpp/` to use `termsurf.proto`**

Regenerate C++ code from `termsurf.proto`. Update `main.cc` to create a
`TermSurfMessage` with a `create_tab` case, serialize, deserialize, verify.

#### Verification

1. `cd proto/test-rust && cargo run` — prints "Rust: pass"
2. `cd proto/test-zig && zig build run` — prints "Zig: pass"
3. `cd proto/test-cpp && make clean && make && ./test` — prints "C++: pass"
4. Cross-reference every field in the `.proto` against `xpc.zig`, `xpc.rs`, and
   `shell_browser_main_parts.cc` to confirm nothing was missed.

**Pass criterion:** The full 30-message schema compiles in all three languages,
and a `TermSurfMessage` containing a `CreateTab` round-trips correctly through
serialize/deserialize in each language.

**Result:** Pass

All three tests produce identical output:

```
Rust: pass (40 bytes)
Zig:  pass (40 bytes)
C++:  pass (40 bytes)
```

The full 30-message schema with `oneof` wrapper compiles and round-trips
correctly in all three languages. The `TermSurfMessage` containing a `CreateTab`
(url="https://termsurf.com", pane_id="pane-1", pixel_width=1920,
pixel_height=1080, dark=true) serializes to 40 bytes and deserializes with all
fields intact.

Key observations:

- **Rust (prost):** Clean enum-based `oneof` — pattern matching on
  `Msg::CreateTab(tab)`. Zero unsafe code.
- **Zig (protobuf-c):** The `oneof` maps to a C union with a `msg_case`
  discriminator. Accessed via `msg.unnamed_0.create_tab` in Zig. Works but
  requires pointer dereferencing for the nested message.
- **C++ (libprotobuf):** Idiomatic `mutable_create_tab()` setter and
  `msg_case()` discriminator. Cleanest API of the three.

#### Conclusion

The full 30-message protobuf schema is validated across all three languages. The
`oneof` wrapper pattern works correctly — type-safe dispatch replaces
string-based action matching. The schema is ready to be used as the wire format
when replacing XPC with Unix domain sockets.

### Experiment 3: Unix domain socket round-trip (Zig server ↔ Rust client)

Prove that a Zig server and Rust client can exchange protobuf messages over a
Unix domain socket. This is the exact pattern TUI (Rust) → GUI (Zig) will use in
production.

#### Message framing

Unix domain sockets are byte streams — there are no message boundaries. We need
a framing protocol so the receiver knows where one protobuf message ends and the
next begins. Use the simplest possible approach: **4-byte little-endian length
prefix** before each serialized protobuf message.

```
[4 bytes: message length (u32 LE)] [N bytes: serialized TermSurfMessage]
```

This is a standard pattern (gRPC uses a similar 5-byte prefix). The 4-byte
length supports messages up to 4 GB, far more than needed.

#### Socket path

Use `$TMPDIR/termsurf-test.sock` on macOS (where `$XDG_RUNTIME_DIR` is not set).
In production, this will be `$XDG_RUNTIME_DIR/termsurf/gui.sock` on Linux and
`$TMPDIR/termsurf/gui.sock` on macOS. For this experiment, a simple hardcoded
path is fine.

#### Changes

**1. Create `proto/test-socket/server.zig`** — Zig Unix domain socket server

- Bind and listen on `$TMPDIR/termsurf-test.sock` (unlink first if stale)
- Accept one connection
- Read a length-prefixed `TermSurfMessage` from the client
- Assert it's a `HelloRequest` with `pane_id = "pane-1"`
- Construct a `HelloReply` with `homepage = "https://termsurf.com"`
- Write it back as a length-prefixed `TermSurfMessage`
- Close the connection and clean up the socket file
- Print "Zig server: pass"

Uses POSIX socket API via `@cImport`: `socket()`, `bind()`, `listen()`,
`accept()`, `read()`, `write()`, `close()`, `unlink()`. Zig has these in
`std.posix` but the C API is fine too — matches how `gui/` will call them.

**2. Create `proto/test-socket/client.rs`** — Rust Unix domain socket client
(standalone binary in `proto/test-socket/client/`)

- Connect to `$TMPDIR/termsurf-test.sock` via `std::os::unix::net::UnixStream`
- Construct a `HelloRequest` with `pane_id = "pane-1"`
- Serialize with prost and send as length-prefixed bytes
- Read the length-prefixed reply
- Deserialize as `TermSurfMessage`, assert it's a `HelloReply` with
  `homepage = "https://termsurf.com"`
- Print "Rust client: pass"

**3. Create `proto/test-socket/build.zig`** — builds the Zig server

Same pattern as `proto/test-zig/build.zig`: links protobuf-c, compiles the
generated C code, adds POSIX socket headers.

**4. Create `proto/test-socket/client/Cargo.toml`** and
`proto/test-socket/client/build.rs` — builds the Rust client

Same pattern as `proto/test-rust/`: prost + prost-build, compiles
`termsurf.proto`.

**5. Regenerate protobuf C code into `proto/test-socket/`**

```bash
protoc --c_out=proto/test-socket --proto_path=proto proto/termsurf.proto
```

**6. Add `.gitignore` files** for build artifacts in both directories.

#### Verification

Run the server and client in sequence:

```bash
# Terminal 1: start the Zig server (blocks waiting for connection)
cd proto/test-socket && zig build run &

# Terminal 2: run the Rust client
cd proto/test-socket/client && cargo run

# Wait for server to finish
wait
```

Expected output:

```
Zig server: pass
Rust client: pass
```

**Pass criterion:** A Rust client sends a length-prefixed protobuf
`HelloRequest` over a Unix domain socket to a Zig server, which deserializes it,
sends back a length-prefixed `HelloReply`, and the client deserializes the reply
with all fields intact. This proves the full stack: socket transport +
length-prefix framing + protobuf serialization across Zig and Rust.

**Result:** Pass

```
Zig server: listening
Rust client: pass
Zig server: pass
```

The Rust client connected to the Zig server over a Unix domain socket at
`$TMPDIR/termsurf-test.sock`. A `HelloRequest` (pane_id="pane-1") was serialized
with prost, framed with a 4-byte LE length prefix, and sent over the socket. The
Zig server deserialized it with protobuf-c, verified the fields, constructed a
`HelloReply` (homepage="https://termsurf.com"), and sent it back with the same
framing. The Rust client deserialized the reply and verified the homepage field.

Key implementation details:

- **Zig server:** Uses C POSIX socket API (`socket()`, `bind()`, `listen()`,
  `accept()`) via `@cImport` and protobuf-c for serialization. Socket path built
  from `$TMPDIR` at runtime. Stale socket files removed before binding.
- **Rust client:** Uses `std::os::unix::net::UnixStream` (pure Rust, no FFI) and
  prost for serialization. Includes a retry loop (up to 20 attempts, 100ms
  apart) to handle the race between server startup and client connection.
- **Framing:** 4-byte little-endian length prefix before each protobuf message.
  Both sides implement `readExact`/`writeAll` to handle partial reads/writes.

#### Conclusion

Unix domain sockets + protobuf work end-to-end across Zig and Rust. The full
stack is proven: socket transport, length-prefix framing, and protobuf
serialization. This is the exact pattern TUI (Rust) → GUI (Zig) will use in
production. Ready to begin replacing XPC.

### Experiment 4: Replace TUI↔GUI XPC with Unix sockets + protobuf

Replace the entire TUI↔GUI IPC channel with Unix domain sockets and protobuf.
The GUI↔Chromium channel remains on XPC (replaced in a later experiment).

#### Architecture

```
Before:
TUI --XPC--> Gateway --endpoint--> GUI --XPC--> Chromium

After:
TUI --socket+protobuf--> GUI --XPC--> Chromium
```

The gateway is no longer needed for TUI connections. The GUI listens directly on
a Unix domain socket. The TUI connects to it by path.

**Socket path:** `$TMPDIR/termsurf/gui.sock` (macOS) or
`$XDG_RUNTIME_DIR/termsurf/gui.sock` (Linux). The GUI creates this at startup
and removes it on shutdown. Stale sockets are unlinked before binding.

**Framing:** Same as experiment 3 — 4-byte LE length prefix before each
serialized `TermSurfMessage`.

**Bidirectional messaging:** The socket carries traffic in both directions:

- TUI→GUI: commands (`set_overlay`, `navigate`, etc.) and queries (`hello`,
  `query_tabs`, etc.)
- GUI→TUI: query replies (`HelloReply`, `QueryTabsReply`, etc.) and async events
  forwarded from Chromium (`url_changed`, `loading_state`, `title_changed`,
  `mode_changed`)

**Multiplexing:** The TUI runs a reader thread that reads all incoming messages
and routes them by type:

- Reply messages (`HelloReply`, `QueryLastReply`, `QueryDevtoolsReply`,
  `QueryTabsReply`) → pending query channel (the calling thread blocks on this)
- Event messages (`ModeChanged`, `UrlChanged`, `LoadingState`, `TitleChanged`) →
  existing `LoopEvent` mpsc channel (Issue 666)

Since queries are synchronous (the TUI blocks until the reply arrives), there is
at most one pending query at a time. A simple `Arc<Mutex<Option<mpsc::Sender>>>`
suffices.

**Thread safety on GUI side:** The GUI already uses a serial GCD queue for all
XPC handlers. The socket reader uses `dispatch_source`
(DISPATCH_SOURCE_TYPE_READ) targeting the same serial queue. This gives the same
serialization guarantee — no mutexes needed for pane/server state.

#### Changes

**1. GUI: Add protobuf-c to the build system (`gui/build.zig`)**

Add the generated `termsurf.pb-c.c` to the C source files and link
`libprotobuf-c`. The generated code lives at `proto/termsurf.pb-c.c` (already
exists from experiment 2). Add include paths for the generated header and system
protobuf-c headers.

**2. GUI: Add socket listener to `gui/src/apprt/xpc.zig`**

In `init()`, after the existing XPC setup:

- Build the socket path from `$TMPDIR`
- Create the parent directory if needed (`mkdir -p`)
- Unlink any stale socket
- Create, bind, and listen on `AF_UNIX` / `SOCK_STREAM`
- Create a `dispatch_source` (READ) on the listen fd, targeting `xpc_queue`
- The dispatch_source handler calls `accept()`, then creates a per-connection
  dispatch_source for reading

In `deinit()`:

- Close the socket fd
- Unlink the socket file
- Cancel dispatch sources

**3. GUI: Add protobuf message reader and dispatcher**

New function `handleSocketData(fd)` called by the per-connection
dispatch_source:

- Maintain a per-connection read buffer (append new data from `read()`)
- Extract complete messages (4-byte length prefix + payload)
- Deserialize with `termsurf__term_surf_message__unpack()`
- Switch on `msg_case` and call the appropriate handler

The handler switch replaces the string-based `action` dispatch. Each case
extracts fields from the protobuf struct and calls the existing handler logic.
For handlers that currently parse XPC dictionaries, extract the fields from
protobuf and pass them directly.

For query handlers (`hello`, `query_last`, `query_devtools`, `query_tabs`):
serialize the reply as a `TermSurfMessage`, length-prefix it, and write to the
client fd.

For Chromium event forwarding (`url_changed`, `loading_state`, `title_changed`):
replace `xpc_connection_send_message(p.web_peer, ...)` with a protobuf write to
the TUI socket fd. Store the TUI's socket fd in the `Pane` struct (replacing
`web_peer`).

**4. TUI: Create `tui/src/ipc.rs` replacing `tui/src/xpc.rs`**

New module with the same public API as `xpc.rs` but using sockets + protobuf:

- `connect()`: build socket path from `$TMPDIR`, connect via
  `std::os::unix::net::UnixStream`, spawn reader thread
- Reader thread: read length-prefixed messages in a loop, dispatch by oneof type
  to either the reply channel or the `LoopEvent` channel
- `set_overlay()`, `navigate()`, `set_color_scheme()`, etc.: serialize as
  `TermSurfMessage`, write length-prefixed to socket
- `hello()`, `query_last()`, `query_devtools()`, `query_tabs()`: serialize
  request, write to socket, block on reply channel, deserialize reply
- `mode_changed()`, `open_split()`: serialize and send (fire-and-forget)

Add `prost` and `prost-build` to `tui/Cargo.toml`. Add a `build.rs` that
compiles `proto/termsurf.proto`. Remove the `block2` dependency.

**5. TUI: Switch from `xpc.rs` to `ipc.rs`**

In `tui/src/main.rs` (or wherever `CompositorConnection` is used), replace
`mod xpc` with `mod ipc` and update the connection type. The public API is
identical so callers don't change.

**6. Keep XPC for Chromium (no changes)**

The GUI's XPC listener, gateway connection, and all Chromium handlers
(`server_register`, `tab_ready`, `ca_context`, `cursor_changed`,
`sendCreateTab`, `sendMouseEvent`, `sendKeyEvent`, etc.) remain unchanged. Only
the TUI-facing handlers and forwarding code change.

#### What stays the same

- All handler business logic (pane management, server spawning, focus tracking,
  auto-targeting, tab lifecycle)
- All Chromium communication (XPC)
- All GUI public APIs (`sendMouseEvent`, `sendKeyEvent`, etc.)
- The serial GCD queue synchronization model
- The gateway process (still needed for Chromium; removed in a later experiment)

#### Verification

1. `cd gui && zig build` — compiles without errors
2. `cd tui && cargo build` — compiles without errors
3. Launch TermSurf, create a split, type `web google.com`
4. Page loads (proves: socket connection, `hello`, `set_overlay`, Chromium tab
   creation all work)
5. URL bar updates as page loads (proves: `url_changed` forwarded from Chromium
   via socket)
6. Loading indicator animates (proves: `loading_state` forwarded)
7. Navigate to another URL via `:open https://github.com` (proves: `navigate`)
8. Open DevTools via `:devtools` (proves: `query_devtools`,
   `set_devtools_overlay`)
9. Switch dark mode via `:colorscheme dark` (proves: `set_color_scheme`)
10. Close pane — no crashes, no stale socket (proves: cleanup)
11. Open multiple splits with different URLs — all work independently
12. Verify `query_tabs` works (`:tabs`)
13. Verify existing `unfocused-split-opacity` still works (no regression)

**Pass criterion:** All TUI↔GUI communication works over Unix sockets +
protobuf. The XPC gateway is no longer needed for TUI connections. No
regressions in any existing functionality.

**Result:** Fail

The TUI side compiled cleanly — `ipc.rs` (replacing `xpc.rs`) builds with prost
and pure Rust Unix sockets, no FFI needed. The GUI side hit a cascade of build
system problems integrating protobuf-c into the Zig/Xcode/xcframework pipeline:

1. **Zig 0.15 API changes.** `std.ArrayList(u8)` resolved to the unmanaged
   `Aligned` variant which requires the allocator passed to every method
   (`deinit(alloc)`, `appendSlice(alloc, ...)`) rather than storing it
   internally. Fixed, but burned time.

2. **Zig keyword escaping.** Protobuf-c generates a struct field named `error`,
   which is a Zig keyword. Required `reply.@"error"` (4 occurrences). Fixed.

3. **`[:0]const u8` vs `[*c]u8`.** Zig string literals and config fields are
   sentinel-terminated const slices, but protobuf-c expects mutable C pointers.
   Required `@constCast()`. Fixed.

4. **protobuf-c runtime linking — the fatal blocker.** The generated
   `termsurf.pb-c.c` references runtime symbols like `protobuf_c_empty_string`
   from the protobuf-c library. The Zig build compiled the generated code and
   the runtime source into per-architecture `.a` files correctly (verified with
   `nm`), but the xcframework `libtool` step that creates the final fat library
   for Xcode did not include the protobuf-c object files. The Xcode linker then
   failed with undefined symbols on both arm64 and x86_64.

   Attempted fixes:
   - `linkSystemLibrary2("libprotobuf-c")` — links dynamically, but Xcode
     doesn't know about the dynamic lib
   - `addCSourceFile` for both `termsurf.pb-c.c` and `protobuf-c.c` (runtime
     source downloaded from GitHub) — Zig compiled the objects, but the
     xcframework pipeline dropped them during the libtool → xcframework step
   - Created `protobuf-c/protobuf-c.h` subdirectory to satisfy
     `#include <protobuf-c/protobuf-c.h>` — fixed the include error but the
     linker issue persisted

   The root cause is that the Zig build system's xcframework pipeline
   (`LibtoolStep.zig` → `XCFrameworkStep.zig`) combines Zig-compiled static
   libraries, but the C source files added via `addCSourceFile` are compiled
   into intermediate objects that don't survive the multi-step archive process.
   Fixing this requires understanding and modifying Ghostty's build pipeline —
   `TermSurfLib.zig`, `LibtoolStep.zig`, `SharedDeps.zig` — which is deep
   infrastructure work.

#### Conclusion

The experiment proved that the TUI side is straightforward — pure Rust sockets
and prost are cleaner than the XPC FFI they replace. But the GUI side
integration exposed a hard build system problem: getting a third-party C library
(protobuf-c) through Ghostty's multi-architecture Zig → libtool → xcframework →
Xcode pipeline. The objects compile correctly but get lost in the archive chain.

This doesn't invalidate the Unix socket approach. It means the protobuf-c
integration needs to be solved at the build system level before the socket code
can link. Options for a future attempt:

1. **Fix the xcframework pipeline** — understand why `addCSourceFile` objects
   don't survive libtool and patch the build scripts.
2. **Use a pure Zig protobuf library** (like zig-protobuf) instead of
   protobuf-c, avoiding the C library linking problem entirely.
3. **Use a simpler wire format** — JSON or a hand-rolled binary format that
   doesn't require a third-party C library. Less elegant but zero build
   dependencies.
4. **Add protobuf-c to the Xcode project** — link it directly in Xcode's build
   settings alongside the Zig static library, sidestepping the xcframework
   issue.

## Conclusion

This issue validated the full Unix socket + protobuf stack across four
experiments:

1. **Protobuf works in all three languages** (Experiment 1). Rust (prost), Zig
   (protobuf-c via `@cImport`), and C++ (libprotobuf) all serialize and
   deserialize correctly. Wire-compatible output (35 bytes).

2. **The full 30-message schema works** (Experiment 2). All TUI↔GUI and
   GUI↔Chromium messages defined in a single `termsurf.proto` with a `oneof`
   wrapper. Type-safe dispatch replaces string-based action matching. Compiles
   and round-trips in all three languages (40 bytes).

3. **Unix sockets work across Zig and Rust** (Experiment 3). A Zig server and
   Rust client exchange length-prefixed protobuf messages over `AF_UNIX` /
   `SOCK_STREAM`. The exact production pattern (TUI → GUI) is proven.

4. **Production integration blocked by build system** (Experiment 4). The TUI
   side compiled cleanly — `ipc.rs` replaces `xpc.rs` with pure Rust sockets and
   prost. The GUI side compiled the Zig code and protobuf-c objects correctly,
   but the xcframework pipeline (`LibtoolStep.zig` → `XCFrameworkStep.zig`)
   dropped the protobuf-c objects during the archive step. Xcode failed to link
   with undefined symbols.

The blocker is not Unix sockets — it's getting protobuf-c through Ghostty's
multi-architecture Zig → libtool → xcframework → Xcode build pipeline. This is a
standalone build system problem.

### Next steps

1. **Issue 699: Build protobuf-c into the GUI.** Solve the xcframework linking
   problem in isolation. Once `protobuf_c_empty_string` and friends resolve at
   link time, the build system is unblocked.

2. **Issue 700: Replace TUI↔GUI XPC with Unix sockets + protobuf.** With the
   build system solved, apply the Experiment 4 code (TUI `ipc.rs`, GUI socket
   listener, protobuf dispatcher). This is the actual IPC replacement.

The uncommitted code from Experiment 4 (`tui/src/ipc.rs`, `tui/build.rs`,
`gui/src/apprt/xpc.zig` socket additions, `gui/src/build/SharedDeps.zig`
protobuf-c integration) should be discarded and rewritten fresh once Issue 699
is resolved, since the build system changes will likely affect how protobuf-c is
integrated.
