# Astrohacker TermSurf Public Source

This repository contains open source **Astrohacker TermSurf** client source
synced from the private Astrohacker monorepo.

## Rules

Do exactly what the user asks. Do not change code unless explicitly asked.

When editing Rust code, run `cargo fmt` from `rust/`. Accept formatter output as
the source of truth.

When editing Markdown, preserve the existing prose wrapping style.

## Build

Common build commands:

```bash
./scripts/build.sh chromium
./scripts/build.sh webkit
./scripts/build.sh webkit-lib
./scripts/build.sh ahweb
./scripts/build.sh ahterm
./scripts/build.sh all --release
```

## Engine Workspaces

Large upstream engine checkouts are local-only. Reconstruct them from the patch
records under `patches/`; do not commit engine working trees, build products, or
dependency caches.

Relevant public patch docs:

- `patches/chromium/README.md`
- `patches/webkit/README.md`
- `patches/ghostty/README.md`
- `patches/gecko/README.md`

## Project Layout

- `assets/` — TermSurf mark SVG and icon masters (`termsurf-icon.svg`,
  `termsurf-14-*.png`).
- `docs/` — product docs and public legal/records.
- `scripts/` — public build/install/test helpers.
- `rust/` — Astrohacker TermSurf Rust workspace.
- `patches/` — fork patch archives and reconstruction notes.

This public repository intentionally excludes private issue records, epics,
company operations, marketing plans, private release orchestration, local agent
metadata, ignored fork working trees, and build outputs.
