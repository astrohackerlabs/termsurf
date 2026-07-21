# WebKit Patches (tombstone)

WebKit is **not** a shipped Astrohacker Terminal engine (Issue 26072120115614).
Chromium is the supported product browser engine.

## Current State

- **Product pin:** none (removed from `patches/release-manifest.json` in Exp 1)
- **Live fork:** removed (operator deleted `forks/webkit` on close of Issue
  26072120115614; not required for release or agent workflows)
- **Historical archives:** remain under `patches/webkit/patches/` as immutable
  records of past product series. Do not mass-delete `issue-*` archives.

## Last product pin (before archive)

Frozen for reconstruction if a future issue reintroduces WebKit. Values from
the release-manifest / `patches/webkit/README.md` tip immediately before Exp 1
pin removal:

| Field | Value |
| --- | --- |
| Checkout (historical) | `forks/webkit/src` |
| Upstream base | `e0ee95bcafc0c470dfce6db7cfd8ce708c6e9e5e` |
| Branch | `issue-26072112084519-exp1-live-compositor-presentation` |
| HEAD | `bed48373fbdf1400bfbf4f8ecc2c96fb581455cc` |
| Tree | `547986ebaf3970020f4dc86325c20dc2fe5fa756` |
| Released series dirs | `issue-26071814115751` + `issue-26072112084519` (3 patches total) |
| Prior archive SHA-256 | `644ecfe100feb5c5449f9ef4a3a205e83422b4dbbdbea810bf79fb1bcf299596` |

Reconstruct by fetching WebKit at base (or tip), applying the archived series
under `patches/webkit/patches/`, and opening a **new** issue before any product
pin or Homebrew ship.

## Do not

- Re-add a WebKit release-manifest pin without a new issue
- Ship `ah-webkitd` in Homebrew packaging
