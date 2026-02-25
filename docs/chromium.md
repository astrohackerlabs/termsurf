# Chromium Fork

## Repository

| Remote   | URL                                                |
| -------- | -------------------------------------------------- |
| upstream | https://chromium.googlesource.com/chromium/src.git |

No `origin` remote for now. Remote hosting TBD (likely patch set distribution).

## Current State

- Branch: `146.0.7650.0-issue-639`
- Base version: `146.0.7650.0` (tracking Electron's Chromium version)

## Branch Strategy

Track the same Chromium version as Electron. Branches are named
`{version}-termsurf` for the main working branch and `{version}-issue-{N}` for
issue-specific branches.

**Every issue gets its own branch.** When modifying Chromium for a new issue,
find the most relevant recent branch, create a new branch from it
(`{version}-issue-{N}`), and add it to the Branches table below.

## Branches

| Branch                   | Issue                                                 | Description                                 |
| ------------------------ | ----------------------------------------------------- | ------------------------------------------- |
| `146.0.7650.0-termsurf`  | —                                                     | Main working branch for v146                |
| `146.0.7650.0-electron`  | —                                                     | Electron's v146 base                        |
| `146.0.7650.0-issue-411` | [Issue 411](issues/411-two-profiles-3.md)             | Two profiles experiment 3                   |
| `146.0.7650.0-issue-412` | [Issue 412](issues/412-one-profile.md)                | One profile                                 |
| `146.0.7650.0-issue-413` | [Issue 413](issues/413-one-profile-2.md)              | One profile experiment 2                    |
| `146.0.7650.0-issue-414` | [Issue 414](issues/414-two-profiles-xpc.md)           | Two profiles via XPC                        |
| `146.0.7650.0-issue-415` | [Issue 415](issues/415-swift-receiver.md)             | Swift receiver                              |
| `146.0.7650.0-issue-416` | [Issue 416](issues/416-rust-receiver.md)              | Rust receiver                               |
| `146.0.7650.0-issue-501` | [Issue 501](issues/501-two-profiles-ts5.md)           | Two profiles in ts5                         |
| `146.0.7650.0-issue-502` | [Issue 502](issues/502-attach-delay.md)               | Attach delay fix                            |
| `146.0.7650.0-issue-503` | [Issue 503](issues/503-one-two-three.md)              | Dynamic multi-tab protocol                  |
| `146.0.7650.0-issue-507` | [Issue 507](issues/507-chromium.md)                   | Chromium integration                        |
| `146.0.7650.0-issue-509` | [Issue 509](issues/509-chromium.md)                   | Chromium streaming (retry)                  |
| `146.0.7650.0-issue-511` | [Issue 511](issues/511-three-profiles.md)             | Per-tab pane routing                        |
| `146.0.7650.0-issue-512` | [Issue 512](issues/512-vsync.md)                      | 120fps oversampling                         |
| `146.0.7650.0-issue-514` | [Issue 514](issues/514-mouse.md)                      | Mouse clicks + URL sync                     |
| `146.0.7650.0-issue-515` | [Issue 515](issues/515-drag.md)                       | Focus state + text selection                |
| `146.0.7650.0-issue-603` | [Issue 603](issues/603-box-demo.md)                   | Box demo                                    |
| `146.0.7650.0-issue-607` | [Issue 607](issues/607-keyboard-input.md)             | Keyboard input                              |
| `146.0.7650.0-issue-608` | [Issue 608](issues/608-search-input.md)               | Search input                                |
| `146.0.7650.0-issue-609` | [Issue 609](issues/609-keyboard-input-2.md)           | Keyboard input (continued)                  |
| `146.0.7650.0-issue-616` | [Issue 616](issues/616-web-features.md)               | Web features (loading, nav, context menu)   |
| `146.0.7650.0-issue-620` | [Issue 620](issues/620-zig-content-shell.md)          | Zig Content Shell                           |
| `146.0.7650.0-issue-621` | [Issue 621](issues/621-single-process.md)             | Single process multi-profile performance    |
| `146.0.7650.0-issue-625` | [Issue 625](issues/625-calayerhost.md)                | CALayerHost                                 |
| `146.0.7650.0-issue-627` | [Issue 627](issues/627-resize-calayerhost.md)         | CALayerHost resize                          |
| `146.0.7650.0-issue-628` | [Issue 628](issues/628-navigation-calayerhost.md)     | CALayerHost navigation                      |
| `146.0.7650.0-issue-629` | [Issue 629](issues/629-understand-nav-calayerhost.md) | Understand CALayerHost navigation blank     |
| `146.0.7650.0-issue-630` | [Issue 630](issues/630-nav-calayerhost-6.md)          | Fix navigation blank                        |
| `146.0.7650.0-issue-631` | [Issue 631](issues/631-continue-nav-calayerhost.md)   | Disable compositor recycling                |
| `146.0.7650.0-issue-633` | [Issue 633](issues/633-persistent-compositor.md)      | Persistent compositor for stable CAContext  |
| `146.0.7650.0-issue-635` | [Issue 635](issues/635-multi-pane-calayerhost.md)     | Multi-pane persistent compositor regression |
| `146.0.7650.0-issue-637` | [Issue 637](issues/637-editable-url-bar.md)           | Navigate XPC action                         |
| `146.0.7650.0-issue-638` | [Issue 638](issues/638-page-title.md)                 | Page title sync                             |
| `146.0.7650.0-issue-639` | [Issue 639](issues/639-open-in-same-tab.md)           | Open new-tab links in same tab              |

## Patches

Every TermSurf branch is archived as `git format-patch` output in
`chromium/patches/`. Each subdirectory contains the complete patch set from the
`146.0.7650.0` tag to the branch tip.

To reconstruct a branch:

```bash
cd chromium/src
git checkout 146.0.7650.0
git checkout -b 146.0.7650.0-issue-{N}
git am ../../chromium/patches/issue-{N}/*.patch
```

After committing to a Chromium branch, regenerate its patches:

```bash
cd chromium/src
rm -rf ../../chromium/patches/issue-{N}/
git format-patch 146.0.7650.0..HEAD -o ../../chromium/patches/issue-{N}/
```

Then commit the updated patches in the main repo.

See [chromium/README.md](../chromium/README.md) for the full directory layout.

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
