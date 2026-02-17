# Chromium Fork

## Repository

| Remote   | URL                                                |
| -------- | -------------------------------------------------- |
| upstream | https://chromium.googlesource.com/chromium/src.git |

No `origin` remote for now. Remote hosting TBD (likely patch set distribution).

## Current State

- Branch: `146.0.7650.0-issue-503`
- Base version: `146.0.7650.0` (tracking Electron's Chromium version)

## Branch Strategy

Track the same Chromium version as Electron. Branches are named
`{version}-termsurf` for the main working branch and `{version}-issue-{N}` for
issue-specific branches.

## Branches

| Branch                   | Issue                                       | Description                  |
| ------------------------ | ------------------------------------------- | ---------------------------- |
| `146.0.7650.0-termsurf`  | —                                           | Main working branch for v146 |
| `146.0.7650.0-electron`  | —                                           | Electron's v146 base         |
| `146.0.7650.0-issue-411` | [Issue 411](issues/411-two-profiles-3.md)   | Two profiles experiment 3    |
| `146.0.7650.0-issue-412` | [Issue 412](issues/412-one-profile.md)      | One profile                  |
| `146.0.7650.0-issue-413` | [Issue 413](issues/413-one-profile-2.md)    | One profile experiment 2     |
| `146.0.7650.0-issue-414` | [Issue 414](issues/414-two-profiles-xpc.md) | Two profiles via XPC         |
| `146.0.7650.0-issue-415` | [Issue 415](issues/415-swift-receiver.md)   | Swift receiver               |
| `146.0.7650.0-issue-416` | [Issue 416](issues/416-rust-receiver.md)    | Rust receiver                |
| `146.0.7650.0-issue-501` | [Issue 501](issues/501-two-profiles-ts5.md) | Two profiles in ts5          |
| `146.0.7650.0-issue-502` | [Issue 502](issues/502-attach-delay.md)     | Attach delay fix             |
| `146.0.7650.0-issue-503` | [Issue 503](issues/503-one-two-three.md)    | Dynamic multi-tab protocol   |
| `146.0.7650.0-issue-509` | [Issue 509](issues/509-chromium.md)         | Chromium streaming (retry)   |
| `146.0.7650.0-issue-511` | [Issue 511](issues/511-three-profiles.md)   | Per-tab pane routing         |
| `146.0.7650.0-issue-512` | [Issue 512](issues/512-vsync.md)            | 120fps oversampling          |
| `146.0.7650.0-issue-514` | [Issue 514](issues/514-mouse.md)            | Mouse clicks + URL sync      |

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

> You're still using Ninja. Please run 'gn clean out/Default' when convenient
> to upgrade this output directory to Siso (Chromium's Ninja replacement).

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
