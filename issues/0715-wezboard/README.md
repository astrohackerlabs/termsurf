+++
status = "closed"
opened = "2026-03-06"
closed = "2026-03-06"
+++

# Issue 715: Wezboard

## Goal

Build Wezboard — a WezTerm fork that speaks the TermSurf protocol. This is the
second "board" (terminal emulator with browser integration), proving the
protocol is board-agnostic. When complete, users can choose between Ghostboard
(Ghostty fork) and Wezboard (WezTerm fork) as their terminal, with identical
browser integration in both.

Three milestones:

1. **Fork** — Merge upstream WezTerm into the monorepo as `wezboard/`.
2. **Rename** — Rebrand to Wezboard, update XDG paths to
   `~/.config/termsurf/wezboard/`, verify it builds and runs.
3. **Protocol** — Implement full TermSurf protocol support (all 30 message
   types), matching Ghostboard's capabilities: Unix socket server, protobuf IPC,
   CALayerHost compositing, BrowserPane, input routing, process spawning.

## Background

Issue 709 researched WezTerm's architecture and mapped all 13 TermSurf protocol
requirements to specific code locations. The conclusion: WezTerm is a strong
architectural match. It already has Unix socket servers, per-connection async
dispatch, a pluggable `Pane` trait, binary tree pane layout, process spawning
with env vars, and a macOS layer tree with `CAMetalLayer` sublayers.

The three hardest problems identified:

1. **CALayerHost + ANGLE coexistence** — Making `CALayerHost` render on top of
   ANGLE's `CAMetalLayer`.
2. **Transparent pane rendering** — Ensuring the terminal renderer doesn't draw
   over browser content.
3. **Mode switching UX** — Integrating browse/control mode without conflicting
   with WezTerm's existing modals.

### Repo state

- WezTerm is tracked as a git remote (`wezterm` → `github.com/wezterm/wezterm`)
- The `wezterm-ts2` branch has an old subtree merge from ts2 (moved to `ts2/`)
- The remote `wezterm/main` is at commit `05343b3` (latest upstream)
- Ghostboard was merged via `git subtree add --prefix=ghost/` then renamed to
  `ghostboard/`. We follow the same pattern for Wezboard.

### XDG conventions

TermSurf uses `~/.config/termsurf/{board}/` for per-board configuration:

- Ghostboard: `~/.config/termsurf/ghostboard/`
- Wezboard: `~/.config/termsurf/wezboard/`

### Protocol reference

The full TermSurf protocol (30 messages) is documented in Issue 709. The proto
file is at `proto/termsurf.proto`. Ghostboard's implementation is in
`ghostboard/src/apprt/xpc.zig` (~3000 lines).

## Architecture

### BrowserPane approach (from Issue 709)

Implement the WezTerm `Pane` trait for browser panes. This integrates browser
content natively into WezTerm's tab/split/focus system:

- `BrowserPane` — Implements `Pane` trait. `key_down()`/`mouse_event()` forward
  to Chromium. `get_lines()` returns empty lines. `resize()` sends `Resize` to
  Chromium. `get_title()` returns page title.
- `BrowserDomain` — Implements `Domain` trait. `spawn_pane()` returns a
  `BrowserPane`. Manages Roamium process lifecycle.
- `TermSurfState` — Registries for servers, tabs, browsers, pending tabs. Stored
  alongside the global `Mux`.

### Socket server

A `UnixListener` on `$TMPDIR/termsurf/wezboard-{pid}.sock`. Uses WezTerm's
existing async task infrastructure (`smol`/`SimpleExecutor`). One task per
connection. First message determines connection type: `ServerRegister` =
Chromium, anything else = TUI.

### CALayerHost compositing (macOS)

Create a `CALayerHost` sublayer on the view's backing layer, positioned at the
`BrowserPane`'s pixel rect. The `CALayerHost` must have higher `zPosition` than
ANGLE's `CAMetalLayer` sublayer. Position/resize when the pane moves. Remove
when the tab closes.

### Input routing

Automatic via `BrowserPane`. When a browser pane is the active pane, all
keyboard/mouse events dispatch to it through the `Pane` trait. Mode switching
(browse ↔ control) uses WezTerm's key table system — activate a "browse" key
table, Esc pops back.

### Modifier translation

WezTerm and TermSurf use different bit positions:

| Modifier | WezTerm | TermSurf |
| -------- | ------- | -------- |
| Shift    | `1<<1`  | `1<<0`   |
| Ctrl     | `1<<3`  | `1<<1`   |
| Alt      | `1<<2`  | `1<<2`   |
| Super    | `1<<4`  | `1<<3`   |

## Experiments

### Experiment 1: Fork WezTerm into wezboard/

#### Goal

Merge upstream WezTerm into the monorepo as `wezboard/` using `git subtree add`,
the same method used for Ghostboard (Issue 600) and earlier WezTerm forks (Issue
418 Experiment 3).

#### Context

The WezTerm remote already exists:

- Remote: `wezterm` → `github.com/wezterm/wezterm`
- Latest upstream: `wezterm/main` at `05343b3`

Almost all WezTerm commits are already in the repo's history (from the ts2-era
subtree merge). This experiment adds WezTerm at the current upstream HEAD into a
new `wezboard/` prefix.

#### Steps

1. Fetch latest from the wezterm remote:

   ```bash
   git fetch wezterm
   ```

2. Subtree-add WezTerm into `wezboard/`:

   ```bash
   git subtree add --prefix=wezboard wezterm main
   ```

   This creates a merge commit that places all WezTerm files under `wezboard/`.
   The merge commit message will be the standard subtree format:
   `Add 'wezboard/' from commit '{hash}'`.

3. Verify the directory exists and contains expected files:

   ```bash
   ls wezboard/Cargo.toml wezboard/wezterm-gui/ wezboard/mux/
   ```

#### Verification

1. `ls wezboard/Cargo.toml` — workspace manifest exists
2. `ls wezboard/wezterm-gui/src/main.rs` — GUI entry point exists
3. `ls wezboard/mux/src/pane.rs` — Pane trait source exists
4. `wc -l wezboard/Cargo.toml` — non-trivial file (workspace with many members)

#### What this does NOT include

- No renaming (Experiment 2)
- No building (Experiment 2)
- No protocol integration (later experiments)

#### Result

Pass. Forked WezTerm into `wezboard/` via
`git subtree add --prefix=wezboard wezterm main`. The subtree merge commit
(`0bf1e8a`) places all of upstream WezTerm (at commit `05343b3`) under
`wezboard/`. All four verification checks pass: workspace manifest (282 lines),
GUI entry point, Pane trait source, and the standard subtree merge commit
format.

This is the same fork pattern used for Ghostboard (Issue 600) —
`git subtree add` preserves the full upstream commit history while placing files
under a subdirectory. Future upstream merges use
`git subtree pull --prefix=wezboard wezterm main`.

### Experiment 2: Build unmodified WezTerm

#### Goal

Build WezTerm as-is from `wezboard/` to establish a working baseline. If it
compiles and runs unmodified, we know any future build failures are caused by
our changes, not by the upstream code or missing dependencies.

#### Steps

1. Build the GUI binary (debug mode):

   ```bash
   cd wezboard && cargo build -p wezterm-gui
   ```

2. Run it briefly to confirm it launches:

   ```bash
   ./target/debug/wezterm-gui
   ```

   Verify a terminal window appears. Close it manually.

#### Verification

1. `cargo build -p wezterm-gui` exits with status 0
2. `ls wezboard/target/debug/wezterm-gui` — binary exists
3. The app launches and displays a terminal window

#### What this does NOT include

- No renaming (Experiment 3)
- No XDG path changes (Experiment 3)
- No protocol integration (later experiments)

#### Result

Pass. `cargo build -p wezterm-gui` compiles successfully (2 harmless warnings).
The 159MB debug binary launches and displays a working terminal window.

One issue discovered: `git subtree add` does not populate git submodules.
WezTerm has four submodules (zlib, libpng, freetype2, harfbuzz) that needed to
be cloned manually at their pinned commits. The initial build failed because
`--depth 1` clones pulled the latest upstream (where file paths had changed).
Cloning at the exact commits from `git ls-tree wezterm/main` fixed it.

### Experiment 3: Register submodules properly

#### Goal

Register WezTerm's four submodules in the root `.gitmodules` with `wezboard/`
prefixed paths, so `git submodule update --init` works after cloning the repo.
Also remove dead submodule entries for ts1, ts2, and ts3 — those directories no
longer exist but their entries remain in `.gitmodules`.

#### Context

The root `.gitmodules` already has entries for `ts2/deps/...` and
`ts3/deps/...`. We add the same four submodules under `wezboard/deps/...`:

| Submodule                          | URL                             | Pinned commit |
| ---------------------------------- | ------------------------------- | ------------- |
| `wezboard/deps/freetype/zlib`      | `github.com/madler/zlib`        | `51b7f2a`     |
| `wezboard/deps/freetype/libpng`    | `github.com/glennrp/libpng`     | `f5e92d7`     |
| `wezboard/deps/freetype/freetype2` | `github.com/freetype/freetype2` | `42608f7`     |
| `wezboard/deps/harfbuzz/harfbuzz`  | `github.com/harfbuzz/harfbuzz`  | `33a3f8d`     |

#### Steps

1. Remove dead submodule entries for ts1 (8 entries), ts2 (4 entries), and ts3
   (4 entries) from `.gitmodules` via `git rm --cached` and editing
   `.gitmodules`.
2. Remove the manually cloned wezboard submodule directories (they're untracked
   git repos, not proper submodules).
3. Add all four wezboard submodules via `git submodule add` with the correct
   paths.
4. Verify `git submodule status` shows only the four `wezboard/deps/...` entries
   (no ts1/ts2/ts3 ghosts).
5. Rebuild to confirm nothing broke.

#### Verification

1. `git submodule status` lists exactly four `wezboard/deps/...` entries
2. `.gitmodules` contains only `wezboard/deps/...` entries (no ts1/ts2/ts3)
3. `cargo build -p wezterm-gui` from `wezboard/` still compiles

#### Result

Pass. Removed 16 dead submodule entries (8 ts1, 4 ts2, 4 ts3) from
`.gitmodules`. Registered the four wezboard submodules properly via
`git submodule add` at their pinned commits (zlib 1.3.1, libpng 1.6.44,
freetype2 2.13.3, harfbuzz 11.2.1). Build still compiles. `git submodule status`
now shows exactly four entries, all under `wezboard/deps/`.

### Experiment 4: Rename script + full rebrand

#### Goal

Create `scripts/rename-wezterm.sh [dir]` — a deterministic, re-runnable script
that renames all "wezterm" references in the given directory to "wezboard" (or
"termsurf wezboard" where appropriate). After running, zero instances of
"wezterm" remain. The directory defaults to `wezboard/` but accepts a custom
path so the script can be run on a fresh WezTerm checkout before merging
upstream.

**Upstream merge workflow:**

1. Clone upstream WezTerm to a temporary directory
2. Run `scripts/rename-wezterm.sh /path/to/fresh-wezterm`
3. Merge the pre-renamed tree into `wezboard/`
4. Conflicts are minimized because both sides already use "wezboard" naming

This follows the same pattern as `scripts/rename-ghostty.sh` (protect →
substitute → restore → file renames → verify), which also accepts a custom path
argument.

#### Branding rules

| Context                         | Before                                         | After                                             |
| ------------------------------- | ---------------------------------------------- | ------------------------------------------------- |
| App name (UI, About, title bar) | WezTerm                                        | TermSurf Wezboard                                 |
| macOS app bundle                | WezTerm.app                                    | TermSurf Wezboard.app                             |
| CLI binaries                    | `wezterm`, `wezterm-gui`, `wezterm-mux-server` | `wezboard`, `wezboard-gui`, `wezboard-mux-server` |
| Bundle ID                       | `org.wezfurlong.wezterm`                       | `com.termsurf.wezboard`                           |
| Environment variables           | `WEZTERM_*`                                    | `WEZBOARD_*`                                      |
| XDG config path                 | `~/.config/wezterm/`                           | `~/.config/termsurf/wezboard/`                    |
| Config file                     | `wezterm.lua`                                  | `wezboard.lua`                                    |
| Crate/package names             | `wezterm-*`                                    | `wezboard-*`                                      |
| Function/type names             | `wezterm_*` / `WezTerm*`                       | `wezboard_*` / `Wezboard*`                        |
| Lua module name                 | `require("wezterm")`                           | `require("wezboard")`                             |
| Lua API calls                   | `wezterm.action`, `wezterm.config_builder()`   | `wezboard.action`, `wezboard.config_builder()`    |
| README title                    | WezTerm                                        | Wezboard                                          |
| Author name                     | Wez Furlong                                    | Wez Longboard                                     |
| Author email                    | `wez@wezfurlong.org`                           | `wezboard@termsurf.com`                           |
| Author domain                   | `wezfurlong.org`                               | `termsurf.com/wezboard`                           |
| GitHub repo                     | `wez/wezterm`, `wezterm/wezterm`               | `termsurf/termsurf`                               |
| Crate registry                  | `crates.io/crates/wezterm`                     | `crates.io/crates/wezboard`                       |
| Docs URL                        | `docs.rs/wezterm`                              | `docs.rs/wezboard`                                |

#### Script structure

**Phase 1: Text substitutions (single sed pass)**

Unlike the Ghostty rename script, there are no protected patterns — everything
gets renamed. The sed script applies substitutions in order from most specific
to most generic, so longer matches are replaced before shorter ones can
interfere.

Substitute (order: specific before generic):

- `wez@wezfurlong.org` → `wezboard@termsurf.com`
- `Wez Furlong` → `Wez Longboard`
- `wezfurlong.org` → `termsurf.com/wezboard`
- `wez/wezterm` → `termsurf/termsurf`
- `wezterm/wezterm` → `termsurf/termsurf`
- `crates.io/crates/wezterm` → `crates.io/crates/wezboard`
- `docs.rs/wezterm` → `docs.rs/wezboard`
- `org.wezfurlong.wezterm` → `com.termsurf.wezboard`
- `~/.config/wezterm` → `~/.config/termsurf/wezboard`
- `XDG_CONFIG_HOME…wezterm` → `XDG_CONFIG_HOME…termsurf/wezboard`
- `wezterm contributors` → `wezboard contributors`
- `WEZTERM_` → `WEZBOARD_`
- `WEZTERM` → `WEZBOARD`
- `WezTerm` → `Wezboard`
- `wezterm` → `wezboard`

**Phase 2: File/directory renames (git mv, idempotent)**

Rename crate directories:

- `wezboard/wezterm/` → `wezboard/wezboard/`
- `wezboard/wezterm-blob-leases/` → `wezboard/wezboard-blob-leases/`
- `wezboard/wezterm-cell/` → `wezboard/wezboard-cell/`
- `wezboard/wezterm-char-props/` → `wezboard/wezboard-char-props/`
- `wezboard/wezterm-client/` → `wezboard/wezboard-client/`
- `wezboard/wezterm-dynamic/` → `wezboard/wezboard-dynamic/`
- `wezboard/wezterm-escape-parser/` → `wezboard/wezboard-escape-parser/`
- `wezboard/wezterm-font/` → `wezboard/wezboard-font/`
- `wezboard/wezterm-gui/` → `wezboard/wezboard-gui/`
- `wezboard/wezterm-gui-subcommands/` → `wezboard/wezboard-gui-subcommands/`
- `wezboard/wezterm-input-types/` → `wezboard/wezboard-input-types/`
- `wezboard/wezterm-mux-server/` → `wezboard/wezboard-mux-server/`
- `wezboard/wezterm-mux-server-impl/` → `wezboard/wezboard-mux-server-impl/`
- `wezboard/wezterm-open-url/` → `wezboard/wezboard-open-url/`
- `wezboard/wezterm-ssh/` → `wezboard/wezboard-ssh/`
- `wezboard/wezterm-surface/` → `wezboard/wezboard-surface/`
- `wezboard/wezterm-toast-notification/` →
  `wezboard/wezboard-toast-notification/`
- `wezboard/wezterm-uds/` → `wezboard/wezboard-uds/`
- `wezboard/wezterm-version/` → `wezboard/wezboard-version/`

Rename files with "wezterm" in the name (screenshots, configs, docs, CI
templates, etc.).

Rename `wezboard/README.md` title to "Wezboard".

**Phase 3: Verify**

- `grep -r wezterm wezboard/` shows only protected patterns (URLs, attribution)
- No leftover `__PROTECT_` placeholders
- `cargo build -p wezboard-gui` compiles

#### Steps

1. Write `scripts/rename-wezterm.sh` following the structure above.
2. Run the script.
3. Verify zero unprotected "wezterm" references remain.
4. Build to confirm compilation.

#### Verification

1. `grep -ri wezterm wezboard/` — only protected patterns (URLs, attribution)
2. `cargo build -p wezboard-gui` from `wezboard/` compiles
3. The app launches as "TermSurf Wezboard"

#### Result

Pass. Created `scripts/rename-wezterm.sh` — a deterministic, re-runnable rename
script that transforms all "wezterm" references to "wezboard" (or "termsurf
wezboard" where appropriate). The script processed 886 files across three
phases: text substitutions (sed), file/directory renames (git mv for 19 crate
dirs + 75 other files), and verification.

After running, only 2 references to "wezterm" remain — both are the
`github.com/wezterm/xcb-imdkit-rs` dependency URL, which is a real upstream repo
that must stay unchanged. The script protects this URL via a protect/restore
pattern.

Key discoveries during implementation:

- `Wezterm` (capital W, lowercase t) needed its own sed rule — `WezTerm` and
  `wezterm` didn't catch it.
- `WezFurlong` and `wezfurlong` as standalone account names (Patreon, Ko-Fi,
  Copr, Twitter) needed explicit substitutions beyond the `wezfurlong.org`
  domain rule.
- The `wezterm` GitHub org owns non-main repos (xcb-imdkit-rs) that must be
  protected from renaming.
- `cargo build -p wezboard-gui` compiles with only 2 harmless warnings (same as
  pre-rename). The 159MB debug binary builds successfully.
- The app launches, reads config from
  `~/.config/termsurf/wezboard/wezboard.lua`, and displays a working terminal
  window. XDG paths correctly nest under `termsurf/wezboard`.

### Experiment 5: TermSurf socket server

#### Goal

Add a second Unix socket listener to wezboard-gui that speaks the TermSurf
protocol (length-prefixed protobuf). This is the foundation for all protocol
integration — without it, no TUI or browser engine can connect.

The existing WezTerm mux socket (`gui-sock-{pid}`) stays untouched. The TermSurf
socket runs in parallel on a separate path.

#### Context

Wezboard already has a Unix socket server for its native mux protocol:

- `spawn_mux_server()` in `wezboard-gui/src/main.rs` creates a `LocalListener`
  on a dedicated thread
- `LocalListener::run()` uses a blocking `incoming()` iterator
- Each accepted connection spawns an async task via `spawn_into_main_thread()`
  using `smol`
- Per-connection dispatch multiplexes reads and mux notifications via
  `smol::channel` and `smol::future::or()`

We add a parallel TermSurf listener using the same patterns. The TermSurf
protocol uses 4-byte little-endian length prefix + protobuf (same wire format as
Ghostboard).

#### Socket path

`$TMPDIR/termsurf/wezboard-{pid}.sock`

Set `TERMSURF_SOCKET={path}` in child process environment so TUIs can discover
it.

#### Code location

All TermSurf code lives inside `wezboard-gui` as a module — no separate crate.
This matches Ghostboard's pattern (socket server in `xpc.zig`, part of the
apprt) and gives the module direct access to `Mux`, `Window`, the renderer, and
everything else it will need for future integration (BrowserPane, CALayerHost,
input routing).

#### New files

1. **`wezboard/wezboard-gui/build.rs`** — Compile `proto/termsurf.proto` via
   `prost-build`. The proto file path is `../../proto/termsurf.proto` relative
   to the crate root.
2. **`wezboard/wezboard-gui/src/termsurf/mod.rs`** — Module root. Re-exports
   `TermSurfListener`, generated protobuf types.
3. **`wezboard/wezboard-gui/src/termsurf/listener.rs`** — `TermSurfListener`
   struct with `run()` method (blocking accept loop, mirrors `LocalListener`).
4. **`wezboard/wezboard-gui/src/termsurf/conn.rs`** — Per-connection async
   handler. Reads length-prefixed messages, detects connection type on first
   message (`ServerRegister` = Chromium, else = TUI), dispatches to stub
   handlers.
5. **`wezboard/wezboard-gui/src/termsurf/state.rs`** — `TermSurfState` global
   (empty registries for now).

#### Modified files

1. **`wezboard/wezboard-gui/Cargo.toml`** — Add `prost` and `prost-build` deps.
2. **`wezboard/wezboard-gui/src/main.rs`** — Add `mod termsurf;`. In
   `async_run_terminal_gui()`, after `spawn_mux_server()`, call
   `spawn_termsurf_server()` which creates a `TermSurfListener` on a new thread.

#### Connection type detection

```rust
enum ConnType {
    Unknown,
    Tui,
    Chromium,
}
```

First message's `oneof` field determines type:

- `msg_case == server_register` → `Chromium`
- Anything else → `Tui`

Locked after first message (no switching).

#### Wire format

Identical to Ghostboard:

```
[4 bytes: little-endian u32 length] [N bytes: serialized TermSurfMessage]
```

Reading:

1. Read into buffer
2. Parse 4-byte length prefix
3. If buffer has enough bytes, decode protobuf
4. Shift buffer, repeat

Writing:

1. Serialize protobuf to get packed size
2. Write 4-byte LE length + packed bytes

#### Stub handlers

All message handlers log and return. No actual browser integration yet — that's
future experiments. The goal is just the socket + framing + dispatch skeleton.

```rust
fn handle_message(msg: TermSurfMessage, conn: &mut Connection) {
    match msg.msg {
        Some(Msg::ServerRegister(r)) => log::info!("ServerRegister: {}", r.profile),
        Some(Msg::SetOverlay(o)) => log::info!("SetOverlay: {}", o.pane_id),
        Some(Msg::HelloRequest(_)) => { /* send HelloReply */ },
        _ => log::debug!("unhandled message"),
    }
}
```

#### Steps

1. Add `prost` and `prost-build` to `wezboard-gui/Cargo.toml`, create `build.rs`
   to compile the proto.
2. Create `src/termsurf/` module with listener, connection handler, and state.
3. Wire `spawn_termsurf_server()` into `main.rs`.
4. Build and verify socket is created on startup.
5. Test with `socat` or a small script that sends a `HelloRequest`.

#### Verification

1. `cargo build -p wezboard-gui` compiles
2. Launch wezboard-gui, verify socket exists at
   `$TMPDIR/termsurf/wezboard-{pid}.sock`
3. `TERMSURF_SOCKET` env var is set in child shells
4. A test client can connect and send a `HelloRequest`, receiving a `HelloReply`

#### Result

Pass. Added a TermSurf protocol socket server to wezboard-gui as a module
(`src/termsurf/`), not a separate crate. The implementation mirrors WezTerm's
existing mux server pattern: blocking `UnixListener::incoming()` on a dedicated
thread, with each accepted connection spawning an async task via
`spawn_into_main_thread()`.

New files:

- `wezboard-gui/build.rs` — Extended to compile `proto/termsurf.proto` via
  `prost-build`.
- `wezboard-gui/src/termsurf/mod.rs` — Module root. Re-exports generated
  protobuf types and `spawn_termsurf_server`.
- `wezboard-gui/src/termsurf/listener.rs` — Creates the socket directory,
  removes stale sockets, binds, sets `TERMSURF_SOCKET` env var, spawns accept
  thread.
- `wezboard-gui/src/termsurf/conn.rs` — Per-connection async handler. Reads
  length-prefixed messages (4-byte LE u32 + protobuf), detects connection type
  on first message (`ServerRegister` = Chromium, else = TUI), dispatches to stub
  handlers. Responds to `HelloRequest` with `HelloReply`.
- `wezboard-gui/src/termsurf/state.rs` — Empty `TermSurfState` global
  (scaffolding for future experiments).

Verified:

1. `cargo build -p wezboard-gui` compiles (only pre-existing warnings from
   upstream code).
2. On launch, socket created at `$TMPDIR/termsurf/wezboard-{pid}.sock` —
   confirmed via `ls` and log output:
   `TermSurf socket listening on /var/folders/.../T/termsurf/wezboard-53930.sock`.
3. `TERMSURF_SOCKET` env var set in process environment.
4. HelloRequest/HelloReply not tested with external client yet (stub handlers
   are in place).

## Conclusion

Issue 715 established Wezboard — a WezTerm fork that speaks the TermSurf
protocol — as the second board in the TermSurf ecosystem.

**What we accomplished across 5 experiments:**

1. **Research** (Experiment 1) — Analyzed WezTerm's architecture: Rust/smol
   async runtime, `window` crate with macOS backend, Metal rendering via wgpu,
   mux server with Unix socket IPC. Confirmed it's a strong match for TermSurf
   integration.
2. **Fork** (Experiment 2) — Merged upstream WezTerm into the monorepo as a
   `wezboard/` subtree with submodules (harfbuzz, freetype2, libpng, zlib).
   Verified `cargo build -p wezterm-gui` compiles and launches.
3. **Build scripts** (Experiment 3) — Created `scripts/build-wezboard.sh` and
   updated `scripts/build-debug.sh` / `scripts/build-release.sh` to include
   Wezboard alongside Ghostboard, Chromium, TUI, and Roamium.
4. **Rename** (Experiment 4) — Created `scripts/rename-wezterm.sh`, a
   deterministic, re-runnable rename script. Processed 886 files: sed
   substitutions (wezterm → wezboard, wezfurlong → termsurf, XDG paths, env
   vars, bundle IDs), git mv for 19 crate directories + 75 other files, and
   verification. Only 2 references remain (upstream `xcb-imdkit-rs` URL,
   protected by design).
5. **TermSurf socket** (Experiment 5) — Added a second Unix socket listener at
   `$TMPDIR/termsurf/wezboard-{pid}.sock` speaking the TermSurf protocol
   (length-prefixed protobuf). Connection type detection, stub message dispatch,
   `HelloReply` response. The foundation for all future protocol integration.

**What comes next:**

Before continuing with Wezboard's protocol implementation (BrowserPane,
CALayerHost compositing, input routing, process spawning), we will first
eliminate all build warnings. The `cargo build -p wezboard-gui` currently
produces ~194 warnings — 188 from the legacy `objc` 0.2 crate's macro
expansions, 2 unnecessary `unsafe` blocks, 1 dead assignment, and 3 from our
scaffolding code. This will be tracked in a new issue focused on cleaning up
inherited WezTerm technical debt.
