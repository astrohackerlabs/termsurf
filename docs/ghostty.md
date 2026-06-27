# Ghostty Fork

## Overview

This repo is a fork of [Ghostty](https://github.com/ghostty-org/ghostty). The
original Ghostty commit history is part of our git history — we forked, then
began modifying files in place.

The active Ghostty fork is `ghostboard/`. All browser integration logic lives
inside the Ghostboard fork, matching Ghostty's Zig/Swift architecture.
`ghostboard/` receives upstream Ghostty subtree merges.

Earlier Ghostty forks and prototypes (`ts1/`, `ts5/`, `gui/`, and `ghost/`) have
been archived from the working tree. See
[early-prototypes.md](early-prototypes.md) for their history.

## Remote

| Remote    | URL                                        | Branch |
| --------- | ------------------------------------------ | ------ |
| `ghostty` | https://github.com/ghostty-org/ghostty.git | main   |

The `ghostty` remote tracks upstream Ghostty for `ghostboard/` subtree merges.

## How ghostboard/ was created

`ghostboard/` was imported from upstream Ghostty with git subtree history. The
current subtree marker is:

```bash
git-subtree-dir: ghostboard
git-subtree-split: 332b2aefc6e72d363aa93ab6ecfc86eeeeb5ed28
```

Historical directories such as `ghost/` and `gui/` are no longer the active
frontend.

## Merging upstream into ghostboard/

To pull the latest upstream Ghostty changes into `ghostboard/`:

```bash
git fetch ghostty main
git subtree pull --prefix=ghostboard ghostty main \
  -m "Merge upstream Ghostty into ghostboard"
```

### Resolving conflicts

`ghostboard/` has TermSurf modifications in app identity, release/version
plumbing, browser process launch, browser overlay rendering, input forwarding,
focus handling, and webview mode/keybinding behavior. Upstream merges may
conflict with these. Key areas likely to require scrutiny:

- `ghostboard/src/Surface.zig` — browser state and input routing
- `ghostboard/src/renderer/` — overlay rendering and geometry
- `ghostboard/macos/Sources/Ghostty/Surface View/` — AppKit surface, focus, and
  input behavior
- `ghostboard/macos/Sources/Features/About/` — TermSurf About dialog identity
- `ghostboard/macos/Sources/App/` — app delegate, app identity, and lifecycle
- `ghostboard/build.zig` and `ghostboard/src/build/` — build and version
  metadata
- `scripts/release.sh` and Homebrew packaging docs — public release assumptions

### After merging

Verify the build:

```bash
./scripts/build.sh ghostboard
cd ghostboard
zig build -Demit-macos-app=false
zig build test-lib-vt
```

If the build fails, common causes are:

- Zig version mismatch (check `ghostboard/build.zig.zon` for the required
  version)
- New upstream dependencies or build system changes

Also run targeted TermSurf smoke checks after a Ghostty merge: launch the Debug
`TermSurf.app`, run the debug `web` client against a repo-built browser, verify
browser overlay click-to-browse behavior, verify back navigation does not break
window focus, and inspect app metadata/version output for TermSurf identity.
