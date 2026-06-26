# TermSurf Public Source

This repository contains the open source TermSurf client: Ghostboard, the `web`
TUI, the TermSurf protocol, and browser engine integration code.

## Rules

Do exactly what the user asks. Do not change code unless explicitly asked.

When editing Rust code, run `cargo fmt`. Accept formatter output as the source
of truth.

When editing Markdown, run:

```bash
prettier --write --prose-wrap always --print-width 80 <file>
```

## Build

Common build commands:

```bash
./scripts/build.sh chromium
./scripts/build.sh roamium
./scripts/build.sh webkit
./scripts/build.sh surfari-lib
./scripts/build.sh surfari
./scripts/build.sh webtui
./scripts/build.sh ghostboard
./scripts/build.sh all --release
```

Ghostboard development runs from `ghostboard/`:

```bash
cd ghostboard
zig build run
```

## Engine Workspaces

Before modifying or building engine workspaces, read the local instructions:

- `chromium/AGENTS.md`
- `webkit/AGENTS.md`

The main repository tracks engine workspace instructions and patch archives. The
large upstream engine checkouts live outside Git history and are created locally
under `chromium/src` and `webkit/src`.

## Project Layout

- `ghostboard/` — primary terminal frontend.
- `webtui/` — `web` TUI.
- `roamium/` — Chromium-backed engine process.
- `surfari/` — WebKit-backed engine process.
- `proto/` — TermSurf protobuf protocol.
- `chromium/` — Chromium workspace docs and patches.
- `webkit/` — WebKit workspace docs and patches.
- `docs/` — public client documentation.

This public repository intentionally excludes private issue records, TermSurf
Cloud work, internal release orchestration, and other non-client material.
