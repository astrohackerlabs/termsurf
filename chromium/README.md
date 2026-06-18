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

- Branch: `148.0.7778.97-issue-816`
- Base version: `148.0.7778.97` (tracking Electron's Chromium version)

> **Note:** The `…-issue-789-exp*` and `…-issue-790-exp*` branches are
> experimental inline-PDF work, **parked** (Issue 790 Exp 7). They are preserved
> as history. The current active/buildable inline-PDF foundation branch is
> `148.0.7778.97-issue-793-exp1`, forked from the protected
> `148.0.7778.97-issue-784` baseline. See
> [Issue 792](../issues/0792-pdf-support/README.md).

## Branch Strategy

Track the Chromium version used by the latest stable Electron release. Do not
target Chromium stable, beta, tip-of-tree, or Electron prerelease/nightly unless
an issue explicitly records a temporary exception.

Branches are named `{version}-termsurf` for the main working branch and
`{version}-issue-{N}` for issue-specific branches. Follow-up Chromium
experiments within an already-open issue may use `{version}-issue-{N}-exp{M}`.

**Every issue gets its own branch.** When modifying Chromium for a new issue,
find the most relevant recent branch, create a new branch from it
(`{version}-issue-{N}`), and add it to the Branches table below.

## Branches

| Branch                          | Issue                                                                        | Description                                 |
| ------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------- |
| `146.0.7650.0-termsurf`         | —                                                                            | Main working branch for v146                |
| `146.0.7650.0-electron`         | —                                                                            | Electron's v146 base                        |
| `146.0.7650.0-issue-411`        | [Issue 411](../issues/0411-two-profiles-3.md)                                | Two profiles experiment 3                   |
| `146.0.7650.0-issue-412`        | [Issue 412](../issues/0412-one-profile.md)                                   | One profile                                 |
| `146.0.7650.0-issue-413`        | [Issue 413](../issues/0413-one-profile-2.md)                                 | One profile experiment 2                    |
| `146.0.7650.0-issue-414`        | [Issue 414](../issues/0414-two-profiles-xpc.md)                              | Two profiles via XPC                        |
| `146.0.7650.0-issue-415`        | [Issue 415](../issues/0415-swift-receiver.md)                                | Swift receiver                              |
| `146.0.7650.0-issue-416`        | [Issue 416](../issues/0416-rust-receiver.md)                                 | Rust receiver                               |
| `146.0.7650.0-issue-501`        | [Issue 501](../issues/0501-two-profiles-ts5.md)                              | Two profiles in ts5                         |
| `146.0.7650.0-issue-502`        | [Issue 502](../issues/0502-attach-delay.md)                                  | Attach delay fix                            |
| `146.0.7650.0-issue-503`        | [Issue 503](../issues/0503-one-two-three.md)                                 | Dynamic multi-tab protocol                  |
| `146.0.7650.0-issue-507`        | [Issue 507](../issues/0507-chromium.md)                                      | Chromium integration                        |
| `146.0.7650.0-issue-509`        | [Issue 509](../issues/0509-chromium.md)                                      | Chromium streaming (retry)                  |
| `146.0.7650.0-issue-511`        | [Issue 511](../issues/0511-three-profiles.md)                                | Per-tab pane routing                        |
| `146.0.7650.0-issue-512`        | [Issue 512](../issues/0512-vsync.md)                                         | 120fps oversampling                         |
| `146.0.7650.0-issue-514`        | [Issue 514](../issues/0514-mouse.md)                                         | Mouse clicks + URL sync                     |
| `146.0.7650.0-issue-515`        | [Issue 515](../issues/0515-drag.md)                                          | Focus state + text selection                |
| `146.0.7650.0-issue-603`        | [Issue 603](../issues/0603-box-demo.md)                                      | Box demo                                    |
| `146.0.7650.0-issue-607`        | [Issue 607](../issues/0607-keyboard-input.md)                                | Keyboard input                              |
| `146.0.7650.0-issue-608`        | [Issue 608](../issues/0608-search-input.md)                                  | Search input                                |
| `146.0.7650.0-issue-609`        | [Issue 609](../issues/0609-keyboard-input-2.md)                              | Keyboard input (continued)                  |
| `146.0.7650.0-issue-616`        | [Issue 616](../issues/0616-web-features.md)                                  | Web features (loading, nav, context menu)   |
| `146.0.7650.0-issue-620`        | [Issue 620](../issues/0620-zig-content-shell.md)                             | Zig Content Shell                           |
| `146.0.7650.0-issue-621`        | [Issue 621](../issues/0621-single-process.md)                                | Single process multi-profile performance    |
| `146.0.7650.0-issue-625`        | [Issue 625](../issues/0625-calayerhost.md)                                   | CALayerHost                                 |
| `146.0.7650.0-issue-627`        | [Issue 627](../issues/0627-resize-calayerhost.md)                            | CALayerHost resize                          |
| `146.0.7650.0-issue-628`        | [Issue 628](../issues/0628-navigation-calayerhost.md)                        | CALayerHost navigation                      |
| `146.0.7650.0-issue-629`        | [Issue 629](../issues/0629-understand-nav-calayerhost.md)                    | Understand CALayerHost navigation blank     |
| `146.0.7650.0-issue-630`        | [Issue 630](../issues/0630-nav-calayerhost-6.md)                             | Fix navigation blank                        |
| `146.0.7650.0-issue-631`        | [Issue 631](../issues/0631-continue-nav-calayerhost.md)                      | Disable compositor recycling                |
| `146.0.7650.0-issue-633`        | [Issue 633](../issues/0633-persistent-compositor.md)                         | Persistent compositor for stable CAContext  |
| `146.0.7650.0-issue-635`        | [Issue 635](../issues/0635-multi-pane-calayerhost.md)                        | Multi-pane persistent compositor regression |
| `146.0.7650.0-issue-637`        | [Issue 637](../issues/0637-editable-url-bar.md)                              | Navigate XPC action                         |
| `146.0.7650.0-issue-638`        | [Issue 638](../issues/0638-page-title.md)                                    | Page title sync                             |
| `146.0.7650.0-issue-639`        | [Issue 639](../issues/0639-open-in-same-tab.md)                              | Open new-tab links in same tab              |
| `146.0.7650.0-issue-642`        | [Issue 642](../issues/0642-zig-profile-server.md)                            | Zig Profile Server                          |
| `146.0.7650.0-issue-643`        | [Issue 643](../issues/0643-zig-profile-server-2.md)                          | Zig Profile Server (Take 2)                 |
| `146.0.7650.0-issue-644`        | [Issue 644](../issues/0644-simplified-cpp.md)                                | Simplified C++ profile server               |
| `146.0.7650.0-issue-644-exp3`   | [Issue 644](../issues/0644-simplified-cpp.md)                                | Simplified C++ profile server (Exp 3)       |
| `146.0.7650.0-issue-655`        | [Issue 655](../issues/0655-substack-blank.md)                                | Stub BadgeService binder                    |
| `146.0.7650.0-issue-680`        | [Issue 680](../issues/0680-dark-mode.md)                                     | Dark mode via XPC                           |
| `146.0.7650.0-issue-684`        | [Issue 684](../issues/0684-devtools.md)                                      | DevTools via devtools:// URL                |
| `146.0.7650.0-issue-689`        | [Issue 689](../issues/0689-tab-lifecycle.md)                                 | Tab lifecycle                               |
| `146.0.7650.0-issue-689-exp3`   | [Issue 689](../issues/0689-tab-lifecycle.md)                                 | Close tab teardown order                    |
| `146.0.7650.0-issue-694`        | [Issue 694](../issues/0694-tab-id-chromium.md)                               | Replace pane_id with tab_id                 |
| `146.0.7650.0-issue-701`        | [Issue 701](../issues/0701-chromium-sockets.md)                              | Replace GUI↔Chromium XPC with Unix sockets  |
| `146.0.7650.0-issue-702`        | [Issue 702](../issues/0702-socket-cleanup.md)                                | Remove dead XPC code                        |
| `146.0.7650.0-issue-704`        | [Issue 704](../issues/0704-browser-bindings.md)                              | Browser bindings (libtermsurf_content)      |
| `146.0.7650.0-issue-705`        | [Issue 705](../issues/0705-browser-bindings.md)                              | Browser bindings continued (DevTools fix)   |
| `146.0.7650.0-issue-706`        | [Issue 706](../issues/0706-plusium-devtools.md)                              | Plusium DevTools crash fix                  |
| `146.0.7650.0-issue-707`        | [Issue 707](../issues/0707-roamium.md)                                       | Roamium (shared lib + Rust rewrite)         |
| `146.0.7650.0-issue-708`        | [Issue 708](../issues/0708-roamium-only.md)                                  | Roamium-only (clean fork, renamed lib)      |
| `146.0.7650.0-issue-750`        | [Issue 750](../issues/0750-target-blank.md)                                  | Suppress new-window, navigate same tab      |
| `146.0.7650.0-issue-759`        | [Issue 759](../issues/0759-link-hover-url/README.md)                         | UpdateTargetURL for link hover              |
| `146.0.7650.0-issue-762`        | [Issue 762](../issues/0762-persistent-cookies/README.md)                     | Persist cookies via NetworkContextFilePaths |
| `148.0.7778.97-issue-779`       | [Issue 779](../issues/0779-date-picker-popup-position/README.md)             | Native popup position tracing               |
| `148.0.7778.97-issue-782`       | [Issue 782](../issues/0782-native-popup-followups/README.md)                 | Native popup follow-up tracing              |
| `148.0.7778.97-issue-783`       | [Issue 783](../issues/0783-native-popup-remainders/README.md)                | PagePopup alt-tab fixes                     |
| `148.0.7778.97-issue-784`       | [Issue 784](../issues/0784-datalist-popup/README.md)                         | Datalist popup fix and cleanup              |
| `148.0.7778.97-issue-792-exp2`  | [Issue 792](../issues/0792-pdf-support/README.md)                            | Stand up extension foundation               |
| `148.0.7778.97-issue-792-exp3`  | [Issue 792](../issues/0792-pdf-support/README.md)                            | Register PDF component extension            |
| `148.0.7778.97-issue-792-exp4`  | [Issue 792](../issues/0792-pdf-support/README.md)                            | Load PDF viewer resource bytes              |
| `148.0.7778.97-issue-792-exp5`  | [Issue 792](../issues/0792-pdf-support/README.md)                            | Serve PDF extension resources               |
| `148.0.7778.97-issue-792-exp6`  | [Issue 792](../issues/0792-pdf-support/README.md)                            | Register extension renderer processes       |
| `148.0.7778.97-issue-792-exp7`  | [Issue 792](../issues/0792-pdf-support/README.md)                            | Serve PDF viewer chrome resources           |
| `148.0.7778.97-issue-792-exp8`  | [Issue 792](../issues/0792-pdf-support/README.md)                            | Expose PDF viewer private API surface       |
| `148.0.7778.97-issue-792-exp9`  | [Issue 792](../issues/0792-pdf-support/README.md)                            | Activate PDF extension pages                |
| `148.0.7778.97-issue-792-exp10` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Register PDF viewer Mojo binders            |
| `148.0.7778.97-issue-792-exp11` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Load extension renderer resources           |
| `148.0.7778.97-issue-792-exp12` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Register MIME-handler binders               |
| `148.0.7778.97-issue-792-exp13` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Populate PDF extension frame binders        |
| `148.0.7778.97-issue-792-exp14` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Wire PDF stream entry hooks                 |
| `148.0.7778.97-issue-792-exp15` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Probe PDF attach path                       |
| `148.0.7778.97-issue-792-exp16` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Trace PDF stream claim lifecycle            |
| `148.0.7778.97-issue-792-exp17` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Mark PDF wrapper responses                  |
| `148.0.7778.97-issue-792-exp18` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Wire PDF stream-info probes                 |
| `148.0.7778.97-issue-792-exp19` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Trace PDF wrapper startup                   |
| `148.0.7778.97-issue-792-exp20` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Load PDF wrapper resource pak               |
| `148.0.7778.97-issue-792-exp21` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Trace PDF wrapper parsing                   |
| `148.0.7778.97-issue-792-exp22` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Trace PDF wrapper body bytes                |
| `148.0.7778.97-issue-792-exp23` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Register PDF document MIME                  |
| `148.0.7778.97-issue-792-exp26` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Route internal PDF plugin gate              |
| `148.0.7778.97-issue-792-exp27` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Externalize internal PDF plugin embeds      |
| `148.0.7778.97-issue-792-exp28` | [Issue 792](../issues/0792-pdf-support/README.md)                            | Load PDF localized strings                  |
| `148.0.7778.97-issue-793-exp1`  | [Issue 793](../issues/0793-pdf-iframe-size/README.md)                        | Restore PDF embedder CSS access             |
| `148.0.7778.97-issue-794-exp4`  | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Route PDF wheel input through Chromium      |
| `148.0.7778.97-issue-794-exp5`  | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Route PDF mouse input through Chromium      |
| `148.0.7778.97-issue-794-exp6`  | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Route PDF keyboard input through Chromium   |
| `148.0.7778.97-issue-794-exp7`  | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Trace PDF drag selection                    |
| `148.0.7778.97-issue-794-exp8`  | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Trace PDF resize and reflow                 |
| `148.0.7778.97-issue-794-exp12` | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Wire PDF resourcesPrivate strings for zoom  |
| `148.0.7778.97-issue-794-exp14` | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Complete PDF viewer strings                 |
| `148.0.7778.97-issue-794-exp15` | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Wire PDF title propagation                  |
| `148.0.7778.97-issue-794-exp16` | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Probe contained PDF print activation        |
| `148.0.7778.97-issue-794-exp17` | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Trace PDF print message bridge              |
| `148.0.7778.97-issue-794-exp18` | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Probe PDF print intercept guard             |
| `148.0.7778.97-issue-794-exp19` | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Enable PDF print control                    |
| `148.0.7778.97-issue-794-exp20` | [Issue 794](../issues/0794-pdf-viewer-interactions/README.md)                | Install renderer print helper               |
| `148.0.7778.97-issue-796-exp2`  | [Issue 796](../issues/0796-pdf-implementation-audit/README.md)               | PDF organization cleanup                    |
| `148.0.7778.97-issue-796-exp4`  | [Issue 796](../issues/0796-pdf-implementation-audit/README.md)               | Harden PDF extension security boundary      |
| `148.0.7778.97-issue-799`       | [Issue 799](../issues/0799-browser-api-automation-triage/README.md)          | Default-deny PaymentRequest binder          |
| `148.0.7778.97-issue-799-exp4`  | [Issue 799](../issues/0799-browser-api-automation-triage/README.md)          | Enable contained generic downloads          |
| `148.0.7778.97-issue-799-exp5`  | [Issue 799](../issues/0799-browser-api-automation-triage/README.md)          | Add protocol-mediated JavaScript dialogs    |
| `148.0.7778.97-issue-799-exp6`  | [Issue 799](../issues/0799-browser-api-automation-triage/README.md)          | Add automated page zoom shortcuts           |
| `148.0.7778.97-issue-799-exp7`  | [Issue 799](../issues/0799-browser-api-automation-triage/README.md)          | Add protocol console capture                |
| `148.0.7778.97-issue-799-exp8`  | [Issue 799](../issues/0799-browser-api-automation-triage/README.md)          | Add protocol HTTP Basic Auth                |
| `148.0.7778.97-issue-799-exp9`  | [Issue 799](../issues/0799-browser-api-automation-triage/README.md)          | Add renderer crash recovery event           |
| `148.0.7778.97-issue-799-exp10` | [Issue 799](../issues/0799-browser-api-automation-triage/README.md)          | Add explicit default-deny permissions       |
| `148.0.7778.97-issue-799-exp12` | [Issue 799](../issues/0799-browser-api-automation-triage/README.md)          | Add contained file upload selection         |
| `148.0.7778.97-issue-799-exp13` | [Issue 799](../issues/0799-browser-api-automation-triage/README.md)          | Add session isolation and incognito checks  |
| `148.0.7778.97-issue-816`       | [Issue 816](../issues/0816-ghostboard-browser-state-interruptions/README.md) | Emit deterministic initial loading state    |
| `148.0.7778.97-issue-781`       | [Issue 781](../issues/0781-chromium-upgrade/README.md)                       | Chromium 148 migration                      |
| `148.0.7778.97-issue-780`       | [Issue 780](../issues/0780-link-drag-freeze/README.md)                       | Suppress native link drag in Roamium        |
| `148.0.7778.97-issue-778`       | [Issue 778](../issues/0778-back-nav-title-stale/README.md)                   | Re-emit titles on navigation commit         |
| `148.0.7778.97-issue-776`       | [Issue 776](../issues/0776-pdf-not-loading/README.md)                        | Probe PDF viewer plumbing                   |
| `148.0.7778.97-issue-776-exp2`  | [Issue 776](../issues/0776-pdf-not-loading/README.md)                        | Route PDF navigations to wrapper            |
| `148.0.7778.97-issue-776-exp3`  | [Issue 776](../issues/0776-pdf-not-loading/README.md)                        | Diagnose PDF renderer process gate          |
| `148.0.7778.97-issue-776-exp4`  | [Issue 776](../issues/0776-pdf-not-loading/README.md)                        | Trace PDF renderer routing                  |
| `148.0.7778.97-issue-776-exp6`  | [Issue 776](../issues/0776-pdf-not-loading/README.md)                        | Register PDF viewer component resources     |
| `148.0.7778.97-issue-776-exp7`  | [Issue 776](../issues/0776-pdf-not-loading/README.md)                        | Serve PDF viewer shell resources            |
| `148.0.7778.97-issue-776-exp8`  | [Issue 776](../issues/0776-pdf-not-loading/README.md)                        | Probe PDF stream handoff                    |
| `148.0.7778.97-issue-789-exp2`  | [Issue 789](../issues/0789-electron-style-pdf-viewer/README.md)              | Build TermSurf PDF stream handoff           |
| `148.0.7778.97-issue-789-exp3`  | [Issue 789](../issues/0789-electron-style-pdf-viewer/README.md)              | Implement PDF stream delegate               |
| `148.0.7778.97-issue-789-exp4`  | [Issue 789](../issues/0789-electron-style-pdf-viewer/README.md)              | Attach PDF extension viewer frame           |
| `148.0.7778.97-issue-789-exp5`  | [Issue 789](../issues/0789-electron-style-pdf-viewer/README.md)              | Add PDF viewer stream-info shim             |
| `148.0.7778.97-issue-789-exp6`  | [Issue 789](../issues/0789-electron-style-pdf-viewer/README.md)              | Serve PDF viewer chrome://resources         |
| `148.0.7778.97-issue-789-exp7`  | [Issue 789](../issues/0789-electron-style-pdf-viewer/README.md)              | Grant PDF viewer chrome://resources access  |
| `148.0.7778.97-issue-790-exp1`  | [Issue 790](../issues/0790-pdf-viewer-mojo-bindings/README.md)               | Enable Mojo JS on PDF viewer frame (broker) |
| `148.0.7778.97-issue-790-exp2`  | [Issue 790](../issues/0790-pdf-viewer-mojo-bindings/README.md)               | OOPIF PDF state diagnostic                  |
| `148.0.7778.97-issue-790-exp3`  | [Issue 790](../issues/0790-pdf-viewer-mojo-bindings/README.md)               | Flip PDF viewer to OOPIF mode               |
| `148.0.7778.97-issue-790-exp4`  | [Issue 790](../issues/0790-pdf-viewer-mojo-bindings/README.md)               | Probe external PDF plugin routing           |
| `148.0.7778.97-issue-790-exp5`  | [Issue 790](../issues/0790-pdf-viewer-mojo-bindings/README.md)               | Link canonical PDF stack (deps)             |

## Patches

`patches/` contains `git format-patch` output for every TermSurf branch. Each
subdirectory holds the complete patch set needed to reconstruct that branch from
its vanilla Chromium base tag.

```
patches/
├── termsurf/          — Base TermSurf modifications (5 patches)
├── issue-411/         — Two profiles experiment 3
├── issue-412/         — One profile
├── ...
└── issue-{N}/         — Issue branch patch archive
```

Each patch set is cumulative — it contains all commits from the base tag to the
branch tip, including inherited commits from parent branches.

### Applying patches

To reconstruct a branch from a fresh Chromium checkout:

```bash
cd chromium/src
git checkout 148.0.7778.97
git checkout -b 148.0.7778.97-issue-{N}
git am ../../chromium/patches/issue-{N}/*.patch
```

For the current TermSurf Chromium branch, use:

```bash
git checkout -b 148.0.7778.97-issue-794-exp19 148.0.7778.97
git am ../../chromium/patches/issue-794-exp19/*.patch
```

### Generating patches

After committing to a Chromium branch, regenerate its patch set:

```bash
cd chromium/src
rm -rf ../../chromium/patches/issue-{N}/
git format-patch 148.0.7778.97..HEAD -o ../../chromium/patches/issue-{N}/
```

Then commit the updated patches in the main repo.

## Local Setup

The `chromium/` directory at the repo root is a Chromium build workspace,
gitignored from the main repo. The `src/` subdirectory is the Chromium git repo
(Chromium requires it to be named `src/`). `depot_tools/` lives at
`chromium/depot_tools/`.

To set up from scratch:

```bash
cd chromium
export PATH="$(pwd)/depot_tools:$PATH"
gclient config --name=src https://chromium.googlesource.com/chromium/src.git
caffeinate gclient sync --revision src@148.0.7778.97 --no-history
cd src
git checkout -b 148.0.7778.97-issue-776-exp2 148.0.7778.97
git am ../../chromium/patches/issue-776-exp2/*.patch
```

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
gn gen out/Default --args='is_debug=false symbol_level=0 is_component_build=true enable_nacl=false'
```

Build a target:

```bash
autoninja -C out/Default libtermsurf_chromium
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
autoninja -C out/Default libtermsurf_chromium
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
