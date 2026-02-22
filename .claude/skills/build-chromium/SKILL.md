---
name: build-chromium
description: "Build the Chromium fork for TermSurf. Use when building, rebuilding, or troubleshooting the Chromium build."
---

# Build Chromium

Build the Chromium fork for TermSurf. Everything stays inside the repo.

## Paths

| What | Path |
|------|------|
| depot_tools | `chromium/depot_tools` |
| Source root | `chromium/src` |
| Build output | `chromium/src/out/Default/` |
| Built app | `chromium/src/out/Default/One Profile.app/` |
| GN args | `chromium/src/out/Default/args.gn` |

All paths are relative to `~/dev/termsurf`.

## PATH Setup

depot_tools must be on PATH before running any build tool (`autoninja`, `gn`,
`gclient`, etc.):

```bash
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
```

## CRITICAL: Never Use `ninja` Directly

**ALWAYS use `autoninja`.** NEVER run `ninja -C out/Default ...` or invoke
`ninja` directly in any form.

`autoninja` uses Siso (Chromium's Ninja replacement) to execute builds. If
`ninja` is invoked directly even once, it creates `.ninja_deps` state files in
the output directory. Once those files exist, `autoninja` detects them and falls
back to Ninja for all subsequent builds — permanently downgrading the build
system. The only recovery is `gn clean out/Default`, which **deletes the entire
build cache** and forces a full rebuild (~42,000 steps, ~1.5 hours).

**Wrong:**
```bash
ninja -C out/Default chromium_profile_server    # DO NOT DO THIS
```

**Right:**
```bash
autoninja -C out/Default chromium_profile_server  # Always use autoninja
```

If the build ever prints "You're still using Ninja", the directory is already
contaminated. Run `gn clean out/Default` to fix it, then rebuild with
`autoninja`.

## Build Commands

```bash
# Navigate to source root
cd ~/dev/termsurf/chromium/src

# Add depot_tools to PATH
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"

# Generate build files (only needed once, or after changing args.gn)
gn gen out/Default

# Build
autoninja -C out/Default chromium_profile_server
```

The current build target is `chromium_profile_server`. This builds the
`Chromium Profile Server.app` bundle in `out/Default/`.

## GN Args

Current `out/Default/args.gn`:

```
is_debug = false
symbol_level = 0
is_component_build = true
```

Regenerate with `gn gen out/Default` after editing `args.gn`.

## Cache Preservation

The `out/Default/` directory IS the build cache. Never delete it. `autoninja`
does incremental builds automatically — only recompiling changed files. A full
build is ~42,000 steps; an incremental build after small changes is typically
50-200 steps.

## The Rule

All binaries, `.app` bundles, and build artifacts stay inside `~/dev/termsurf`.
Nothing gets installed to `/usr/local`, `/usr/bin`, `~/Library`, or anywhere
outside the repo without explicit user approval.

## Branches

The Chromium fork at `chromium/src` uses branches named
`{version}-termsurf` or `{version}-issue-{N}` (e.g.,
`146.0.7650.0-issue-414`). These are built as commits on top of the vanilla
Chromium version tag.

**ALWAYS create a new branch for every issue.** Never commit to an existing
issue's branch. Find the most relevant recent branch, fork it to the new issue
number, and add the new branch to the Branches table in `docs/chromium.md`.

## Branch and Version Tracking

`docs/chromium.md` tracks the current branch, commit, and a complete list of all
branches with links to their issue docs. Any time this skill switches branches,
updates the Chromium version, or creates a new branch, it must also update
`docs/chromium.md` accordingly.
