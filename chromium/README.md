# Chromium Fork

TermSurf's Chromium fork. The full source tree (`src/`) and build tools
(`depot_tools/`) are gitignored — only this README and the `patches/` directory
are tracked.

## Repository

| Remote   | URL                                                |
| -------- | -------------------------------------------------- |
| upstream | https://chromium.googlesource.com/chromium/src.git |

No `origin` remote for now. Remote hosting TBD (likely patch set distribution).

## Current State

- Branch: `146.0.7650.0-issue-705`
- Base version: `146.0.7650.0` (tracking Electron's Chromium version)

## Branch Strategy

Track the same Chromium version as Electron. Branches are named
`{version}-termsurf` for the main working branch and `{version}-issue-{N}` for
issue-specific branches.

**Every issue gets its own branch.** When modifying Chromium for a new issue,
find the most relevant recent branch, create a new branch from it
(`{version}-issue-{N}`), and add it to the Branches table below.

## Branches

| Branch                        | Issue                                                         | Description                                 |
| ----------------------------- | ------------------------------------------------------------- | ------------------------------------------- |
| `146.0.7650.0-termsurf`       | —                                                             | Main working branch for v146                |
| `146.0.7650.0-electron`       | —                                                             | Electron's v146 base                        |
| `146.0.7650.0-issue-411`      | [Issue 411](../docs/issues/411-two-profiles-3.md)             | Two profiles experiment 3                   |
| `146.0.7650.0-issue-412`      | [Issue 412](../docs/issues/412-one-profile.md)                | One profile                                 |
| `146.0.7650.0-issue-413`      | [Issue 413](../docs/issues/413-one-profile-2.md)              | One profile experiment 2                    |
| `146.0.7650.0-issue-414`      | [Issue 414](../docs/issues/414-two-profiles-xpc.md)           | Two profiles via XPC                        |
| `146.0.7650.0-issue-415`      | [Issue 415](../docs/issues/415-swift-receiver.md)             | Swift receiver                              |
| `146.0.7650.0-issue-416`      | [Issue 416](../docs/issues/416-rust-receiver.md)              | Rust receiver                               |
| `146.0.7650.0-issue-501`      | [Issue 501](../docs/issues/501-two-profiles-ts5.md)           | Two profiles in ts5                         |
| `146.0.7650.0-issue-502`      | [Issue 502](../docs/issues/502-attach-delay.md)               | Attach delay fix                            |
| `146.0.7650.0-issue-503`      | [Issue 503](../docs/issues/503-one-two-three.md)              | Dynamic multi-tab protocol                  |
| `146.0.7650.0-issue-507`      | [Issue 507](../docs/issues/507-chromium.md)                   | Chromium integration                        |
| `146.0.7650.0-issue-509`      | [Issue 509](../docs/issues/509-chromium.md)                   | Chromium streaming (retry)                  |
| `146.0.7650.0-issue-511`      | [Issue 511](../docs/issues/511-three-profiles.md)             | Per-tab pane routing                        |
| `146.0.7650.0-issue-512`      | [Issue 512](../docs/issues/512-vsync.md)                      | 120fps oversampling                         |
| `146.0.7650.0-issue-514`      | [Issue 514](../docs/issues/514-mouse.md)                      | Mouse clicks + URL sync                     |
| `146.0.7650.0-issue-515`      | [Issue 515](../docs/issues/515-drag.md)                       | Focus state + text selection                |
| `146.0.7650.0-issue-603`      | [Issue 603](../docs/issues/603-box-demo.md)                   | Box demo                                    |
| `146.0.7650.0-issue-607`      | [Issue 607](../docs/issues/607-keyboard-input.md)             | Keyboard input                              |
| `146.0.7650.0-issue-608`      | [Issue 608](../docs/issues/608-search-input.md)               | Search input                                |
| `146.0.7650.0-issue-609`      | [Issue 609](../docs/issues/609-keyboard-input-2.md)           | Keyboard input (continued)                  |
| `146.0.7650.0-issue-616`      | [Issue 616](../docs/issues/616-web-features.md)               | Web features (loading, nav, context menu)   |
| `146.0.7650.0-issue-620`      | [Issue 620](../docs/issues/620-zig-content-shell.md)          | Zig Content Shell                           |
| `146.0.7650.0-issue-621`      | [Issue 621](../docs/issues/621-single-process.md)             | Single process multi-profile performance    |
| `146.0.7650.0-issue-625`      | [Issue 625](../docs/issues/625-calayerhost.md)                | CALayerHost                                 |
| `146.0.7650.0-issue-627`      | [Issue 627](../docs/issues/627-resize-calayerhost.md)         | CALayerHost resize                          |
| `146.0.7650.0-issue-628`      | [Issue 628](../docs/issues/628-navigation-calayerhost.md)     | CALayerHost navigation                      |
| `146.0.7650.0-issue-629`      | [Issue 629](../docs/issues/629-understand-nav-calayerhost.md) | Understand CALayerHost navigation blank     |
| `146.0.7650.0-issue-630`      | [Issue 630](../docs/issues/630-nav-calayerhost-6.md)          | Fix navigation blank                        |
| `146.0.7650.0-issue-631`      | [Issue 631](../docs/issues/631-continue-nav-calayerhost.md)   | Disable compositor recycling                |
| `146.0.7650.0-issue-633`      | [Issue 633](../docs/issues/633-persistent-compositor.md)      | Persistent compositor for stable CAContext  |
| `146.0.7650.0-issue-635`      | [Issue 635](../docs/issues/635-multi-pane-calayerhost.md)     | Multi-pane persistent compositor regression |
| `146.0.7650.0-issue-637`      | [Issue 637](../docs/issues/637-editable-url-bar.md)           | Navigate XPC action                         |
| `146.0.7650.0-issue-638`      | [Issue 638](../docs/issues/638-page-title.md)                 | Page title sync                             |
| `146.0.7650.0-issue-639`      | [Issue 639](../docs/issues/639-open-in-same-tab.md)           | Open new-tab links in same tab              |
| `146.0.7650.0-issue-642`      | [Issue 642](../docs/issues/642-zig-profile-server.md)         | Zig Profile Server                          |
| `146.0.7650.0-issue-643`      | [Issue 643](../docs/issues/643-zig-profile-server-2.md)       | Zig Profile Server (Take 2)                 |
| `146.0.7650.0-issue-644`      | [Issue 644](../docs/issues/644-simplified-cpp.md)             | Simplified C++ profile server               |
| `146.0.7650.0-issue-655`      | [Issue 655](../docs/issues/655-substack-blank.md)             | Stub BadgeService binder                    |
| `146.0.7650.0-issue-680`      | [Issue 680](../docs/issues/680-dark-mode.md)                  | Dark mode via XPC                           |
| `146.0.7650.0-issue-684`      | [Issue 684](../docs/issues/684-devtools.md)                   | DevTools via devtools:// URL                |
| `146.0.7650.0-issue-689-exp3` | [Issue 689](../docs/issues/689-tab-lifecycle.md)              | Close tab teardown order                    |
| `146.0.7650.0-issue-694`      | [Issue 694](../docs/issues/694-tab-id-chromium.md)            | Replace pane_id with tab_id                 |
| `146.0.7650.0-issue-701`      | [Issue 701](../docs/issues/701-chromium-sockets.md)           | Replace GUI↔Chromium XPC with Unix sockets  |
| `146.0.7650.0-issue-702`      | [Issue 702](../docs/issues/702-socket-cleanup.md)             | Remove dead XPC code                        |
| `146.0.7650.0-issue-704`      | [Issue 704](../docs/issues/704-browser-bindings.md)           | Browser bindings (libtermsurf_content)       |
| `146.0.7650.0-issue-705`      | [Issue 705](../docs/issues/705-browser-bindings.md)           | Browser bindings continued (DevTools fix)    |

## Patches

`patches/` contains `git format-patch` output for every TermSurf branch. Each
subdirectory holds the complete patch set needed to reconstruct that branch from
the vanilla `146.0.7650.0` tag.

```
patches/
├── termsurf/          — Base TermSurf modifications (5 patches)
├── issue-411/         — Two profiles experiment 3
├── issue-412/         — One profile
├── ...
└── issue-694/         — Replace pane_id with tab_id
```

Each patch set is cumulative — it contains all commits from the base tag to the
branch tip, including inherited commits from parent branches.

### Applying patches

To reconstruct a branch from a fresh Chromium checkout:

```bash
cd chromium/src
git checkout 146.0.7650.0
git checkout -b 146.0.7650.0-issue-{N}
git am ../../chromium/patches/issue-{N}/*.patch
```

### Generating patches

After committing to a Chromium branch, regenerate its patch set:

```bash
cd chromium/src
rm -rf ../../chromium/patches/issue-{N}/
git format-patch 146.0.7650.0..HEAD -o ../../chromium/patches/issue-{N}/
```

Then commit the updated patches in the main repo.

## Local Setup

The `chromium/` directory at the repo root is a Chromium build workspace,
gitignored from the main repo. The `src/` subdirectory is the Chromium git repo
(Chromium requires it to be named `src/`). `depot_tools/` lives at
`chromium/depot_tools/`. To set up from scratch, use `fetch chromium` from
depot_tools or clone from upstream and apply our patches (patch distribution
TBD).

```
chromium/
├── depot_tools/   — Chromium build tools (gclient, gn, autoninja, etc.)
└── src/           — Chromium source tree (git repo)
    ├── content/   — Content API (where our code lives)
    └── out/       — Build output
```

## Build

Set the PATH so that `gn` and `autoninja` are available:

```bash
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
```

Configure the build (one time):

```bash
gn gen out/Default --args='is_debug=false symbol_level=0 is_component_build=true'
```

Build a target:

```bash
autoninja -C out/Default chromium_profile_server
```

Build times: ~1.5 hours for a full build, 15–20 seconds incremental.

### Never use `ninja` directly

Always use `autoninja`, never `ninja`. Chromium's build system uses Siso (a
Ninja replacement). `autoninja` routes builds through Siso automatically. If
`ninja` is invoked directly — even once — it creates `.ninja_deps` state files
that permanently downgrade the build directory to Ninja. Every subsequent
`autoninja` invocation will detect the Ninja state and fall back to Ninja,
printing:

> You're still using Ninja. Please run 'gn clean out/Default' when convenient to
> upgrade this output directory to Siso (Chromium's Ninja replacement).

The only recovery is `gn clean out/Default`, which deletes the entire build
cache (preserving only `args.gn`) and forces a full rebuild (~1.5 hours).

### Recovery

If the build directory is contaminated with Ninja state:

```bash
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
gn clean out/Default
autoninja -C out/Default chromium_profile_server
```

This cleans the directory, and `autoninja` will use Siso from that point
forward.

## Directory Layout

| Path               | Tracked | Description                             |
| ------------------ | ------- | --------------------------------------- |
| `README.md`        | Yes     | This file                               |
| `patches/`         | Yes     | Patch archive for all TermSurf branches |
| `src/`             | No      | Chromium source tree (~100 GB)          |
| `depot_tools/`     | No      | Chromium build tools (647 MB)           |
| `.gclient`         | No      | gclient configuration                   |
| `.gclient_entries` | No      | gclient dependency map                  |
| `_bad_scm/`        | No      | Quarantined gclient artifacts           |
| `.cipd/`           | No      | CIPD package cache                      |
