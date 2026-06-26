# Chromium Workspace

This file is the agent-facing source of truth for TermSurf's Chromium workspace.
Read it before modifying or building anything under `chromium/`.

## Purpose

`chromium/` contains the local Chromium fork used by Roamium. The source
checkout and build tools are local workspace state; TermSurf tracks
documentation and patch archives in the main repo.

## Layout

```text
chromium/
├── AGENTS.md        # agent workflow instructions
├── README.md        # human workspace overview and branch table
├── depot_tools/     # local Chromium tools, ignored by main repo
├── patches/         # tracked TermSurf patch archives
└── src/             # Chromium git checkout, ignored by main repo
```

`chromium/src/` must be named `src` because Chromium tooling expects that name.
Do not move or rename it.

## Current State

- Current fully archived build baseline: `148.0.7778.97-issue-794-exp19`
- Latest documented Chromium branch: `148.0.7778.97-issue-843`
- Base version: `148.0.7778.97`
- Version policy: track the Chromium version used by the latest stable Electron
  release unless an issue explicitly records a temporary exception.
- Build output: `chromium/src/out/Default/`
- Main build target: `libtermsurf_chromium`

`chromium/patches/issue-794-exp19/` is the current full-stack patch archive that
can reconstruct a buildable TermSurf Chromium checkout from the vanilla
`148.0.7778.97` tag. Later issue patch directories may be incremental, not
full-stack archives. Do not document an incremental patch directory as a fresh
setup path unless it has been regenerated and verified as cumulative from the
base tag.

Keep the current state and branch table in `chromium/README.md` current when
creating, switching, or publishing Chromium branches.

## Prerequisites

Chromium uses `depot_tools`, `gclient`, `gn`, and `autoninja`.

Before running Chromium tools:

```bash
cd /Users/astrohacker/dev/termsurf/chromium/src
export PATH="/Users/astrohacker/dev/termsurf/chromium/depot_tools:$PATH"
```

## Build Rules

Never run `ninja` directly in `chromium/src/out/Default`.

Always use:

```bash
autoninja -C out/Default libtermsurf_chromium
```

Direct `ninja` creates `.ninja_deps` state that makes future `autoninja` runs
fall back to Ninja. Recovery requires `gn clean out/Default`, which destroys the
large incremental build cache.

Do not delete `chromium/src/out/Default` unless the user explicitly approves.
That directory is the build cache.

## Setup From Scratch

From the repo root:

```bash
cd chromium
export PATH="/Users/astrohacker/dev/termsurf/chromium/depot_tools:$PATH"
gclient config --name=src https://chromium.googlesource.com/chromium/src.git
caffeinate gclient sync --revision src@148.0.7778.97 --no-history
cd src
git checkout -b 148.0.7778.97-issue-794-exp19 148.0.7778.97
git am ../../chromium/patches/issue-794-exp19/*.patch
gn gen out/Default --args='is_debug=false symbol_level=0 is_component_build=true enable_nacl=false'
autoninja -C out/Default libtermsurf_chromium
```

If a different issue branch is needed, first confirm whether its patch archive
is cumulative from `148.0.7778.97` or incremental on top of another TermSurf
branch. When in doubt, reconstruct the fully archived baseline above, then apply
or recreate the issue branch from the relevant local parent branch.

## Normal Build

From the Chromium source checkout:

```bash
cd /Users/astrohacker/dev/termsurf/chromium/src
export PATH="/Users/astrohacker/dev/termsurf/chromium/depot_tools:$PATH"
gn gen out/Default --args='is_debug=false symbol_level=0 is_component_build=true enable_nacl=false'
autoninja -C out/Default libtermsurf_chromium
```

`gn gen` is needed after changing GN args or build files. For ordinary source
edits, `autoninja` is enough.

## Branch Workflow

Every issue that modifies Chromium source gets its own Chromium branch. Do not
commit TermSurf changes directly to an existing issue branch unless the active
experiment explicitly says to continue that branch.

Branch names:

```text
{version}-termsurf
{version}-issue-{N}
{version}-issue-{N}-exp{M}
```

Typical workflow:

```bash
cd /Users/astrohacker/dev/termsurf/chromium/src
export PATH="/Users/astrohacker/dev/termsurf/chromium/depot_tools:$PATH"
git checkout 148.0.7778.97-issue-{parent}
git checkout -b 148.0.7778.97-issue-{N}
```

After creating a branch, update `chromium/README.md`:

- current branch if this becomes the active documented branch;
- Branches table with branch name, issue link, and description.

## Patch Archives

TermSurf tracks Chromium source changes as `git format-patch` archives under
`chromium/patches/`.

Patch directories should contain the complete patch stack from the vanilla
Chromium base tag to that issue branch tip:

```text
chromium/patches/issue-{N}/
```

Some historical patch directories after Issue 794 are incremental rather than
cumulative. Treat those as branch history records, not fresh setup recipes,
until they are regenerated as full-stack archives.

Generate cumulative patches after committing inside `chromium/src`:

```bash
cd /Users/astrohacker/dev/termsurf/chromium/src
rm -rf ../../chromium/patches/issue-{N}
git format-patch 148.0.7778.97..HEAD -o ../../chromium/patches/issue-{N}
```

Then return to the main repo and commit:

- `chromium/patches/issue-{N}/`;
- `chromium/README.md` branch table/current-state updates;
- issue experiment docs.

Apply a cumulative patch archive from a fresh checkout:

```bash
cd /Users/astrohacker/dev/termsurf/chromium/src
git checkout 148.0.7778.97
git checkout -b 148.0.7778.97-issue-{N}
git am ../../chromium/patches/issue-{N}/*.patch
```

## Verification Commands

Use these checks before recording a Chromium experiment result:

```bash
cd /Users/astrohacker/dev/termsurf
git status --short
git -C chromium/src status --short
git -C chromium/src rev-parse --abbrev-ref HEAD
git -C chromium/src rev-parse HEAD
git diff --check
```

When Chromium source changed, also verify:

```bash
cd /Users/astrohacker/dev/termsurf/chromium/src
export PATH="/Users/astrohacker/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default libtermsurf_chromium
```

Use issue-specific tests in addition to the build.

## Do Not

- Do not run `ninja` directly.
- Do not delete `out/Default` without explicit approval.
- Do not install Chromium build outputs outside the repo without explicit
  approval.
- Do not commit directly to a stale issue branch for a new issue.
- Do not change Chromium source without updating branch docs and patch archives.
- Do not rely on a separate Chromium skill for workflow instructions; this file
  is the workspace-local agent source of truth.
