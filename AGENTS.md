# TermSurf

TermSurf is a protocol for embedding web browsers inside terminal emulators. Any
terminal, any browser engine, any TUI — connected by a protobuf/Unix socket
protocol. Users type `web localhost:3000` and see their work without ever
leaving the terminal. No alt+tab, no context switch.

[Agent development guide](https://agents.md/).

## Rules

Do exactly what your user says. No more, no less. NEVER assume they want
something they didn't ask for. NEVER change code unless explicitly asked.

When editing Rust code, always run `cargo fmt`. Accept the formatter output as
the source of truth. Do not manually undo, minimize, or selectively revert
`cargo fmt` formatting changes, including import ordering or wrapping changes.

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

TermSurf's Chromium fork tracks the Chromium version used by the latest stable
Electron release. When upgrading Chromium, target latest stable Electron's
Chromium version — not Chromium stable, beta, tip-of-tree, or Electron
prerelease/nightly — unless the user explicitly says otherwise. If a temporary
exception is necessary, record it in the issue before implementing it.

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

### Debug Testing Without Installing

For local testing, prefer debug builds and run binaries directly from the repo.
Do not install over the stable Homebrew/app version unless the user explicitly
asks.

Build debug components:

```bash
./scripts/build.sh chromium
./scripts/build.sh roamium
./scripts/build.sh wezboard
./scripts/build.sh webtui
```

Run Wezboard with the debug GUI binary directly. The binary is `wezboard-gui`
under `wezboard/target/debug/`; it is not named `wezterm-gui`.

```bash
./wezboard/target/debug/wezboard-gui
```

Inside that Wezboard window, run the debug `web` binary directly:

```bash
/Users/ryan/dev/termsurf/target/debug/web \
  --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
  https://example.com
```

The `--browser` argument is required for testing Chromium/Roamium changes. If it
is omitted, `web` asks Wezboard to resolve the default browser name, and
Wezboard may spawn an installed stable Roamium from `/usr/local/roamium` or
Homebrew instead of the repo-built binary.

This flow uses debug builds only, does not launch the `.app` bundle, and does
not overwrite the installed reliable version.

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

Each issue is a **folder**. The `README.md` is the issue **spine** (frontmatter,
goal, background, analysis, an ordered index of experiments, and the final
conclusion). **Every experiment is its own numbered file** in the same folder —
the README never contains experiment bodies, only links to them.

```
issues/0792-pdf-support/
├── README.md                     ← spine: frontmatter, goal, background,
│                                    the ordered Experiments index, conclusion
├── 01-stand-up-extensions.md     ← Experiment 1 (full body in its own file)
├── 02-wire-stream-manager.md     ← Experiment 2
└── 03-...                        ← one file per experiment, in sequence
```

The folder name is `{NNNN}-{slug}`. The number is zero-padded to 4 digits and
globally sequential across all generations (ts1–ts5). The slug is lowercase,
hyphenated, and describes the topic.

**Why one file per experiment:** it keeps experiments ordered and easy to read,
access, and organize (up to ~100 per issue with clean `NN-` filenames), and —
critically — it makes experiments easy to **automate**: each experiment is a
discrete file created and tracked from the README, rather than ever-growing
edits to one monolithic document.

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

Issues may add their own TOML frontmatter keys — to `README.md`, experiment
files, or other issue docs — for issue-specific metadata such as per-experiment
agent provenance, as long as:

- the reserved workflow keys are preserved: `README.md` always carries `status`
  and `opened` (plus `closed` when closed), unchanged in name and meaning;
- additive keys are valid TOML between the `+++` delimiters and do not contradict
  the reserved keys or the index tooling — `scripts/build-issues-index.sh` reads
  only the reserved README keys and ignores the rest;
- the issue documents its own added schema in its `README.md`.

#### README.md structure

After the frontmatter, a new issue's `README.md` has these sections:

1. **Title** (H1) — `# Issue {N}: {descriptive title}`
2. **Goal** — One or two sentences describing the desired outcome.
3. **Background** — Context, prior work, constraints.
4. **Architecture** / **Analysis** / **Proposed Solutions** — Technical details.

A new issue's README has **no experiments listed yet**.

As experiments are created, the README grows an **`## Experiments`** section: an
ordered list linking to each experiment file, one per line, with a one-line
status. The README holds the links and statuses only — never the experiment
bodies. Example:

```markdown
## Experiments

- [Experiment 1: Stand up the extensions system](01-stand-up-extensions.md) —
  **Pass**
- [Experiment 2: Wire PdfViewerStreamManager](02-wire-stream-manager.md) —
  **Partial** (needs a Profile-less stream delegate)
- [Experiment 3: …](03-….md) — **Designed**
```

Keep each status to one of: `Designed`, `In progress`, `Pass`, `Partial`,
`Fail`. Update the line when the experiment's result is recorded, so the README
doubles as an at-a-glance progress tracker.

When the issue is solved or abandoned, add the **`## Conclusion`** section to
the README (see "Closing an Issue").

#### Experiment files

Each experiment lives in its **own file** `NN-{slug}.md` in the issue folder,
where `NN` is a zero-padded two-digit number in creation order (`01`, `02`, …,
up to `99`). The slug is lowercase-hyphenated and describes the experiment.

An experiment file may begin with an optional TOML frontmatter block
(`+++ … +++`) before its H1 title — for issue-specific metadata such as agent
provenance. Experiment frontmatter is optional and must not replace the required
H1 title and H2 sections below it.

Each experiment file contains:

1. **Title** (H1) — `# Experiment {N}: {descriptive title}`
2. **Description** — What and why.
3. **Changes** — Specific code changes, listed by file.
4. **Verification** — How to test. Concrete steps and pass/fail criteria.
5. **Result** and **Conclusion** — added after the experiment runs (see
   "Recording results").

Keep each file focused; if one grows past ~1000 lines, that is a sign the
experiment is too big and should be split into the next numbered experiment.

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

Each experiment is its own file `NN-{slug}.md` (see "Experiment files" above),
and is added as a new link in the README's `## Experiments` index the moment it
is created. Inside the file, use an H1 title and H2 sections:

1. **Title** (H1) — `# Experiment {N}: {descriptive title}`
2. **Description** (H2) — What and why.
3. **Changes** (H2) — Specific code changes, listed by file.
4. **Verification** (H2) — How to test. Concrete steps and pass/fail criteria.
5. **Result** / **Conclusion** (H2) — added after it runs.

#### Chromium branches

If an experiment modifies Chromium code, it MUST create a new branch:
`{version}-issue-{N}`. Fork the most relevant recent branch. Add it to the table
in `chromium/README.md`.

#### One at a time

Design and implement one experiment at a time. The result of Experiment 1
directly informs what Experiment 2 should be.

#### AI review gate

Every experiment must be reviewed by another AI agent before moving to the next
stage.

1. **Design review before implementation**
   - After writing the experiment design, ask another AI agent to review it.
   - Fix all real issues found by the review.
   - Record the review result in the experiment file.
   - Do not implement the experiment until the reviewing agent approves the
     design.

2. **Result review before the next experiment**
   - After implementation, verification, and result recording, ask another AI
     agent to review the completed experiment and result.
   - Fix all real issues found by the review.
   - Record the completion-review result in the experiment file.
   - Do not design or implement the next experiment until the reviewing agent
     approves the completed output.

The reviewing agent may be Codex, Claude, or another explicitly requested agent,
but it must be separate from the implementation pass.

#### Experiment commits

Every experiment has two required commit points:

1. **Plan commit** — after the experiment design is written, reviewed, fixed,
   approved, and linked from the issue README, commit the experiment plan before
   implementation begins.
2. **Result commit** — after implementation, verification, result recording,
   completion review, and any required fixes, commit the experiment result
   before designing the next experiment.

These commits must be separate. Do not combine an experiment plan and its result
in the same commit, and do not start the next experiment before the previous
experiment's result commit exists.

#### Recording results

After testing, append the result **inside the experiment's own file**, below
Verification:

```markdown
## Result

**Result:** Pass / Partial / Fail

{description}

## Conclusion

{what we learned, what the next experiment should be}
```

Then update that experiment's status on its line in the README's
`## Experiments` index (`Designed` → `Pass`/`Partial`/`Fail`). All three
outcomes are valuable — failed experiments eliminate dead ends.

### Closing an Issue

When the issue is solved or abandoned, add a `## Conclusion` section to the
**`README.md`** (after the `## Experiments` index), summarizing what was learned
and the outcome. Update the frontmatter to `status = "closed"` with a `closed`
date. Regenerate the index:

```bash
scripts/build-issues-index.sh
```

### Immutability

Closed issues are historical records. They are **immutable** and must NEVER be
modified. History stays as it was written.

The one-file-per-experiment structure applies to **issues created from now on**.
Earlier issues that recorded all experiments inline in a single `README.md`
(e.g. Issues 789–791) keep their original form as historical records — do not
retrofit them.

### Process Summary

1. **Create the issue** — `issues/{NNNN}-{slug}/README.md` with frontmatter,
   goal, background, analysis. No experiments yet.
2. **Design Experiment 1** — Create `01-{slug}.md` with the experiment body, and
   add a link to it under `## Experiments` in the README (status `Designed`).
3. **Review and commit the plan** — Get another AI agent to approve the design,
   fix real findings, record the review result, and commit the experiment plan.
4. **Implement Experiment 1** — Write the code.
5. **Record the result** — Append `## Result` / `## Conclusion` inside
   `01-{slug}.md`, and update its status on the README index line.
6. **Review and commit the result** — Get another AI agent to approve the
   completed output, fix real findings, record the completion review, and commit
   the experiment result.
7. **Repeat** — Create `02-{slug}.md` for the next experiment (the prior result
   informs it), link it from the README, and continue until the goal is met.
8. **Close the issue** — Write the `## Conclusion` in the README, update
   frontmatter, rebuild the index.

## Remember

NEVER change code unless explicitly asked. NEVER make unrequested changes.
Always do EXACTLY what your user asks — no more, no less.
