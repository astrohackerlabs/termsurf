# TermSurf

TermSurf is a protocol for embedding web browsers inside terminal emulators. Any
terminal, any browser engine, any TUI — connected by a protobuf/Unix socket
protocol. Users type `web localhost:3000` and see their work without ever
leaving the terminal. No alt+tab, no context switch.

[Agent development guide](https://agents.md/).

## Rules

Do exactly what your user says. No more, no less. NEVER assume they want
something they didn't ask for. NEVER change code unless explicitly asked.

## Vision

TermSurf is a protocol, not just an app. It is a network of interchangeable
components — terminals, browser engines, and TUIs — all speaking the same
protobuf/Unix socket protocol (`termsurf.proto`).

### Cross-platform

TermSurf will work on macOS, Linux, and Windows. iOS and Android can be added
later.

### Every major browser engine

Each browser engine runs as a separate "profile server" process, communicating
with the GUI (terminal emulator) via the TermSurf protocol. One process per
profile.

| Engine   | C library              | Rust binary | Status     |
| -------- | ---------------------- | ----------- | ---------- |
| Chromium | `libtermsurf_chromium` | Roamium     | Done       |
| WebKit   | `libtermsurf_webkit`   | Surfari     | Planned    |
| Gecko    | `libtermsurf_gecko`    | Waterwolf   | Researched |
| Ladybird | `libtermsurf_ladybird` | Girlbat     | Researched |

Each engine follows the same pattern: a C shared library wrapping the engine's
embedding API (`ts_*` functions), linked by a Rust binary that handles Unix
socket IPC, protobuf parsing, and process lifecycle. The Rust binary (~400
lines) is almost entirely reusable across engines.

### Multiple GUIs

TermSurf currently ships as a WezTerm fork (`wezboard/`). We will implement
forks of all major terminal emulators:

- **Ghostboard** — Archived. Will return from a fresh Ghostty fork after the
  protocol stabilizes.
- **Wezboard** (wezboard/) — Active GUI. WezTerm fork, Rust. Full protocol
  support, CALayerHost rendering, input forwarding, DevTools, direct TUI↔Browser
  connection (Issues 715–741).
- **Kitty** — Planned.
- **Alacritty** — Planned.
- **iTerm2** — Planned.

Any terminal that implements the TermSurf protocol can host browser overlays. A
GUI is a terminal emulator that listens on a Unix socket, accepts connections
from TUIs and browser engines, and renders browser content as overlays at pixel
coordinates.

### Many TUIs

The first TUI, `web`, provides browser chrome (URL bar, navigation, modes) in
the terminal pane. But TermSurf is really a webview overlay protocol — many TUIs
can embed web browsers with any engine:

- `web` — General-purpose web browser TUI (current)
- Future TUIs could include: documentation viewers, API explorers, email
  clients, dashboard monitors, or any application that benefits from rendering
  web content inside a terminal.

### The protocol is the product

The TermSurf protocol (`termsurf.proto`) is the most important artifact. It
defines 30 message types covering tab lifecycle, navigation, input forwarding,
GPU compositing, state synchronization, and request/reply pairs. The protocol
will be extended to support:

- All common web browser features (bookmarks, history, downloads, etc.)
- Terminal-specific features (keyboard-based navigation, shrink/grow overlay,
  split management)
- New message types as needs arise

Care goes into the protocol first. Individual apps (boards, engines, TUIs) are
implementations of the protocol.

## Architectural Decisions

### Multi-process architecture

TermSurf is multi-process by necessity, not by choice. Each browser engine
process serves exactly one profile (one set of cookies, storage, and cache).
This is a hard constraint imposed by Chromium (one `BrowserContext` per
process), and Gecko and Ladybird have the same limitation. This constraint is
the defining architectural fact of TermSurf — it shaped every generation from
ts2 onward.

The multi-process design has a second benefit: it enables multi-engine support.
Because each browser process is an independent program speaking the TermSurf
protocol, the GUI doesn't care which engine is behind it. A user can have one
pane running Roamium (Chromium), another running Surfari (WebKit), and a third
running Girlbat (Ladybird) — all in the same terminal window, all speaking the
same protobuf messages.

WebKit is the exception — `WKWebsiteDataStore` supports multiple profiles in one
process — but the one-process-per-profile model still works for it, and keeping
the architecture uniform is more valuable than optimizing for one engine.

**Process topology:**

```
┌─────────┐  ┌─────────┐  ┌─────────┐
│  TUI 1  │  │  TUI 2  │  │  TUI N  │    N TUIs (e.g., `web`)
└────┬────┘  └────┬────┘  └────┬────┘
     │            │            │
     └────────────┼────────────┘
                  │  Unix socket
           ┌──────┴──────┐
           │     GUI     │                1 GUI (terminal emulator)
           │ (Wezboard)  │
           └──┬───┬───┬──┘
              │   │   │
              │   │   │  Unix sockets
              │   │   │
     ┌────────┘   │   └──────┐
     │            │          │
┌────┴────┐ ┌─────┴───┐ ┌────┴────┐
│ Roamium │ │ Surfari │ │ Roamium │    M engines (one per profile)
│ profile │ │ profile │ │ profile │
│   "A"   │ │   "B"   │ │   "C"   │
└─────────┘ └─────────┘ └─────────┘
```

N TUIs connect to 1 GUI. The GUI manages M browser engine processes, each
serving one profile. Different profiles can use different engines. The GUI is
the hub — it routes messages between TUIs and engines, manages pane layout, and
composites browser overlays into the terminal window.

### Unix sockets + protobuf for all IPC

All inter-process communication uses Unix domain sockets with length-prefixed
protobuf messages. The GUI (terminal) listens on a PID-scoped socket
(`$TMPDIR/termsurf/termsurf-wezboard-{pid}.sock`), and both TUIs and browser
engines connect to it as clients.

- **TUI → GUI:** The TUI reads the `TERMSURF_SOCKET` env var (set by the GUI) to
  discover the socket path.
- **GUI → Engine:** The GUI passes `--ipc-socket={path}` when launching browser
  engine processes.
- **Wire format:** 4-byte little-endian length prefix + serialized protobuf
  (`termsurf.proto`).
- **Serialization:** prost in Rust (GUI, TUI, and engines), C++ protobuf in
  Chromium.

Earlier generations (ts3–ts5) used XPC for IPC. Issues 698–701 replaced XPC with
sockets, and Issue 702 removed all dead XPC code. CALayerHost compositing
(zero-copy GPU rendering) does not require XPC — Window Server routes
`CAContext` layer IDs between processes natively.

- **Graceful shutdown:** The GUI sends a `Shutdown` protobuf message to browser
  engine processes before terminating them (Issue 732 added the message, Issue
  733 made Ghostboard use it instead of SIGKILL). This allows engines to clean
  up resources and exit gracefully.

### Every Chromium issue gets its own branch

When modifying the Chromium fork (`chromium/src/`), ALWAYS create a new branch
for the current issue. Never commit directly to an existing issue's branch.

1. Find the most relevant recent branch (usually the one with the latest
   TermSurf modifications).
2. Create a new branch from it: `{version}-issue-{N}` (e.g.,
   `146.0.7650.0-issue-625`).
3. Add the new branch to the Branches table in `chromium/README.md`.

This keeps every issue's Chromium changes isolated and traceable.

## Directory Structure

- `ghostboard/` — Archived. See docs/early-prototypes.md.
- `wezboard/` — Wezboard (WezTerm fork, Rust). **Active development.**
- `webtui/` — The `web` TUI (Rust/ratatui). Browser chrome in the terminal pane.
- `roamium/` — Roamium (Chromium browser binary, Rust).
- `chromium/` — Chromium fork build workspace (gitignored).
- `issues/` — Issue folders with README.md and TOML frontmatter. See
  `issues/README.md` for the full index.
- `website/` — termsurf.com project website.
- `docs/early-prototypes.md` — Archived prototype documentation (ts1–ts5,
  cef-rs, Ghostboard Legacy).

## Wezboard (wezboard/) — Active Development

### Current State

Wezboard is a WezTerm fork with browser integration built in Rust. Current
additions: WezTerm fork with rename script and initial build (Issue 715), build
warning cleanup (Issue 716), cocoa crate removal and objc2 migration (Issues
717–719), manual testing after migration (Issue 720), wgpu 25→28 upgrade (Issue
721), cargo dependency updates (Issue 722), focused/unfocused split pane borders
(Issue 723), TermSurf protocol implementation (Issue 724), CALayerHost browser
overlay rendering (Issue 725), overlay lifecycle and protocol (Issue 726),
second webview positioning (Issue 727), remaining protocol messages (Issues
728–729), Roamium standalone install (Issue 730), scroll crash fix (Issue 731),
Shutdown message and tab reopen fix (Issue 732).

### Source Layout

#### Wezboard

- `wezboard/wezboard/src/main.rs` — Main GUI application entry point
- `wezboard/wezboard-gui/` — GUI rendering and window management
- `wezboard/wezboard-surface/` — Surface/pane rendering
- `wezboard/mux/` — Terminal multiplexer core
- `wezboard/termwiz/` — Terminal widget library
- `wezboard/config/` — Configuration parsing

#### Roamium

- `roamium/src/main.rs` — Entry point, process initialization and lifecycle
- `roamium/src/dispatch.rs` — Message dispatch and routing (core IPC handler)
- `roamium/src/ipc.rs` — Unix socket IPC protocol (socket framing)
- `roamium/src/ffi.rs` — FFI bindings to libtermsurf_chromium C library
- `roamium/build.rs` — Build script for protobuf code generation

### Build & Install

All build scripts live in `scripts/`. They handle Wezboard, Chromium, TUI, and
Roamium together.

| Script                                                   | Purpose                                                                  |
| -------------------------------------------------------- | ------------------------------------------------------------------------ |
| `scripts/build.sh <comp> [--release] [--clean] [--open]` | Build a component. Components: wezboard, roamium, webtui, chromium, all. |
| `scripts/install.sh <comp>`                              | Install a component. Components: wezboard, roamium, webtui, all.         |
| `scripts/uninstall.sh <comp>`                            | Uninstall a component. Components: wezboard, roamium, webtui, all.       |
| `scripts/deploy.sh <comp>`                               | Deploy a component. Components: website.                                 |
| `scripts/release.sh [version]`                           | Package, upload to GitHub, and publish to Homebrew. Default: 0.1.0.      |
| `scripts/rename-wezterm.sh [dir]`                        | Rename all WezTerm references to Wezboard in `wezboard/`. Re-runnable.   |
| `scripts/nerd-font-test.sh`                              | Print Nerd Font test glyphs for visual verification.                     |

The build scripts auto-detect Chromium's `protoc` so you don't need a system
install.

### Homebrew Distribution

TermSurf is distributed via a Homebrew Cask in the `termsurf/homebrew-termsurf`
tap (submodule at `homebrew/`).

**User install:** `brew tap termsurf/termsurf && brew install --cask termsurf`

**Release workflow:**

1. Build all components: `scripts/build.sh all --release`
2. Run: `scripts/release.sh <version>`

The release script packages a tarball (binaries + Chromium dylibs + .app
bundle), uploads it to a GitHub Release on `termsurf/termsurf`, updates the
Homebrew cask SHA and version, and pushes to `termsurf/homebrew-termsurf`.

**Cask installs:**

- `.app` bundle → `/Applications/TermSurf Wezboard.app`
- `web`, `wezboard` CLIs → `/opt/homebrew/bin/`
- Roamium + Chromium dylibs → `/opt/homebrew/opt/termsurf-roamium/`

## Documentation

All documentation is in `docs/` or in `README.md` files throughout the codebase.
Issue docs for all prototype generations are indexed in
[docs/early-prototypes.md](docs/early-prototypes.md).

## Issues and Experiments

Every significant piece of work gets an issue in `issues/`. Issues describe the
problem, provide background, and propose solutions. Experiments are the
incremental steps that solve the problem.

### Issue Structure

Each issue is a **folder** containing a `README.md` with TOML frontmatter:

```
issues/0756-surfari/
├── README.md          ← main issue document with frontmatter
├── 01-build-webkit.md ← optional: additional files for long issues
└── 02-compositing.md
```

The folder name is `{NNNN}-{slug}`. The number is zero-padded to 4 digits and
globally sequential across all generations (ts1–ts5). The slug is lowercase,
hyphenated, and describes the topic.

The full index of all issues is at `issues/README.md`. Regenerate it with:

```bash
scripts/build-issues-index.sh
```

#### Frontmatter

Every `README.md` starts with TOML frontmatter:

```
+++
status = "open"
opened = "2026-03-16"
+++
```

Or for closed issues:

```
+++
status = "closed"
opened = "2026-03-16"
closed = "2026-03-16"
+++
```

#### README.md structure

After the frontmatter, a new issue has these sections:

1. **Title** (H1) — `# Issue {N}: {descriptive title}`
2. **Goal** — One or two sentences describing the desired outcome.
3. **Background** — Context, prior work, constraints.
4. **Architecture** / **Analysis** / **Proposed Solutions** — Technical details.

A new issue does **not** have an Experiments section yet.

#### Additional files

For long issues, split experiments or sub-topics into numbered files:
`01-name.md`, `02-name.md`, etc. Link them from the README.md. Keep each file
under ~1000 lines to fit in an AI agent's context window.

### Multiple Open Issues

Multiple issues can be open at the same time. This allows interleaving work — a
large issue like Surfari can stay open while smaller issues are opened and
closed alongside it.

### Experiments

#### When to create an experiment

Only after the issue's requirements are clear. Each experiment is designed,
implemented, and concluded before the next one is designed.

**Never list experiments upfront.** The outcome of each experiment informs what
comes next.

#### Experiment structure

Each experiment has:

1. **Title** (H3) — `### Experiment {N}: {descriptive title}`
2. **Description** — What and why.
3. **Changes** — Specific code changes, listed by file.
4. **Verification** — How to test. Concrete steps and pass/fail criteria.

#### Chromium branches

If an experiment modifies Chromium code, it MUST create a new branch:
`{version}-issue-{N}`. Fork the most relevant recent branch. Add it to the table
in `chromium/README.md`.

#### One at a time

Design and implement one experiment at a time. The result of Experiment 1
directly informs what Experiment 2 should be.

#### Recording results

After testing, add a result below the verification section:

```markdown
**Result:** Pass / Partial / Fail

{description}

#### Conclusion

{what we learned, what to do next}
```

All three outcomes are valuable. Failed experiments eliminate dead ends.

### Closing an Issue

Add a `## Conclusion` section after the last experiment. Update the frontmatter
to `status = "closed"` with a `closed` date. Regenerate the index:

```bash
scripts/build-issues-index.sh
```

### Immutability

Closed issues are historical records. They are **immutable** and must NEVER be
modified. History stays as it was written.

### Process Summary

1. **Create the issue** — `issues/{NNNN}-{slug}/README.md` with frontmatter,
   goal, background. No experiments yet.
2. **Design Experiment 1** — Add `## Experiments` and `### Experiment 1`.
3. **Implement Experiment 1** — Write the code.
4. **Record the result** — Pass, partial, or fail with a conclusion.
5. **Repeat** — Design the next experiment. Continue until the goal is met.
6. **Close the issue** — Write the `## Conclusion`, update frontmatter, rebuild
   index.

## Remember

NEVER change code unless explicitly asked. NEVER make unrequested changes.
Always do EXACTLY what your user asks — no more, no less.
