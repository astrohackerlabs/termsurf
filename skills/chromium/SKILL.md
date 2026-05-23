---
name: chromium
description:
  "TermSurf's Chromium fork. Use when viewing, modifying, or building the
  Chromium source code."
---

# Build Chromium

Build the Chromium fork for TermSurf. Everything stays inside the repo.

## Paths

| What         | Path                               |
| ------------ | ---------------------------------- |
| depot_tools  | `chromium/depot_tools`             |
| Source root  | `chromium/src`                     |
| Build output | `chromium/src/out/Default/`        |
| Built target | `chromium/src/out/Default/*.dylib` |
| GN args      | `chromium/src/out/Default/args.gn` |

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
ninja -C out/Default libtermsurf_chromium    # DO NOT DO THIS
```

**Right:**

```bash
autoninja -C out/Default libtermsurf_chromium  # Always use autoninja
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
gn gen out/Default --args='is_debug=false symbol_level=0 is_component_build=true enable_nacl=false'

# Build
autoninja -C out/Default libtermsurf_chromium
```

The current build target is `libtermsurf_chromium`. This builds the TermSurf
Chromium shared library and its runtime resources in `out/Default/`.

## Fresh Setup

For a new checkout, configure Chromium with gclient, sync the current base
version, then apply the current TermSurf patch archive:

```bash
cd ~/dev/termsurf/chromium
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
gclient config --name=src https://chromium.googlesource.com/chromium/src.git
caffeinate gclient sync --revision src@148.0.7778.97 --no-history
cd src
git checkout -b 148.0.7778.97-issue-784 148.0.7778.97
git am ../../chromium/patches/issue-784/*.patch
gn gen out/Default --args='is_debug=false symbol_level=0 is_component_build=true enable_nacl=false'
autoninja -C out/Default libtermsurf_chromium
```

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

The Chromium fork at `chromium/src` uses branches named `{version}-termsurf` or
`{version}-issue-{N}` (e.g., `148.0.7778.97-issue-784`). These are built as
commits on top of the vanilla Chromium version tag.

**ALWAYS create a new branch for every issue.** Never commit to an existing
issue's branch. Find the most relevant recent branch, fork it to the new issue
number, and add the new branch to the Branches table in `chromium/README.md`.

## Patches

Every TermSurf Chromium branch is archived as `git format-patch` output in
`chromium/patches/`. After committing to a Chromium branch, always regenerate
its patch set:

```bash
cd ~/dev/termsurf/chromium/src
rm -rf ../../chromium/patches/issue-{N}/
git format-patch 148.0.7778.97..HEAD -o ../../chromium/patches/issue-{N}/
```

Then commit the updated patches in the main repo alongside the
`chromium/README.md` update.

## Full Workflow for a New Issue

1. Create a new issue doc in `issues/`.
2. Determine Chromium needs modification.
3. Fork the most relevant recent branch (not necessarily the immediate prior
   branch — that might have been a failed experiment):
   ```bash
   cd ~/dev/termsurf/chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   git checkout 148.0.7778.97-issue-{parent}
   git checkout -b 148.0.7778.97-issue-{N}
   ```
4. Make changes, build with `autoninja`, test.
5. Commit to the issue branch with git-poet.
6. Generate patches:
   ```bash
   rm -rf ../../chromium/patches/issue-{N}/
   git format-patch 148.0.7778.97..HEAD -o ../../chromium/patches/issue-{N}/
   ```
7. Return to the main repo.
8. Update `chromium/README.md` (current branch + Branches table).
9. Commit patches and docs in the main repo with git-poet.

## Branch and Version Tracking

`chromium/README.md` tracks the current branch, base version, and a complete
list of all branches with links to their issue docs. Any time this skill
switches branches, updates the Chromium version, or creates a new branch, it
must also update `chromium/README.md` accordingly.
