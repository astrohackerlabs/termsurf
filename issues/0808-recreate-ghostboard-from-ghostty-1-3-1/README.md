+++
status = "open"
opened = "2026-06-16"
+++

# Issue 808: Re-create Ghostboard from Ghostty 1.3.1

## Goal

Create a new `ghostboard/` GUI by importing Ghostty `v1.3.1` with full upstream
history preserved, then implement the current TermSurf protocol inside that new
app so `webtui` can run in Ghostboard without changes to `webtui` or `roamium`.

When solved, the app built from `ghostboard/` should be the Ghostty-based
TermSurf app and should be usable as an alternative to Wezboard.

## Background

Ghostboard Legacy was restored in Issue 807 and now lives at
`ghostboard-legacy/`. That code is historical reference material only. The new
Ghostboard should be a fresh import from upstream Ghostty `v1.3.1`, not a
continuation of the legacy tree.

Prior Ghostty imports used `git subtree` so the upstream commit history remains
part of the TermSurf repository history. The relevant prior documentation is:

- `docs/early-prototypes.md` — records that ts5 used `git subtree`, not
  `git merge -X subtree`.
- `issues/0418-repo-restructure/README.md` — records that `git merge -X subtree`
  failed for this repo and `git subtree add/pull` worked.
- `issues/0600-termsurf-ghost/README.md` — records the old Ghostty import
  pattern:

  ```bash
  git fetch upstream
  git subtree add --prefix=ghost upstream main
  ```

- `docs/ghostty.md` — records later subtree merge guidance:

  ```bash
  git fetch upstream
  git subtree pull --prefix=gui upstream main -m "Merge upstream Ghostty into gui"
  ```

The current `upstream` remote points at TermSurf, not Ghostty, so this issue
should use a distinct Ghostty remote name. The upstream Ghostty release tag
exists:

```text
v1.3.1 -> 22efb0be2bbea73e5339f5426fa3b20edabcaa11
```

## Import Strategy

Use a history-preserving subtree import into `ghostboard/`, pinned to the exact
Ghostty release tag:

```bash
git remote add ghostty https://github.com/ghostty-org/ghostty.git
git fetch ghostty --tags
git subtree add --prefix=ghostboard ghostty v1.3.1 -m "Import Ghostty v1.3.1 into ghostboard"
```

If the `ghostty` remote already exists, verify it points to
`https://github.com/ghostty-org/ghostty.git` and reuse it.

Do not use `git merge -X subtree` for this import. Issue 418 established that
the subtree merge strategy is unreliable for this repo's Ghostty history.

## Scope

Before any Ghostboard porting modifications begin, the imported Ghostty `v1.3.1`
tree must build and run on macOS without errors. Experiments 2 through 4 proved
that a pristine build does not work on this VM's current Zig/Xcode/macOS SDK
combination, even though the environment has the documented Ghostty
dependencies.

Experiment 5 established the local baseline with two build-only deviations:

- build only the native macOS `GhosttyKit` slice instead of constructing iOS and
  iOS-simulator slices for the local app build;
- backport Ghostty's later Darwin `libtool` archive-normalization fix so newer
  Xcode does not drop static archive members while building `libghostty-fat.a`.

Until that build-only baseline is proven:

- make zero source changes under `ghostboard/`;
- do not change branding, config paths, CLI names, icons, protocol code, build
  scripts, Xcode project files, Zig build files, or vendored source;
- assume build, link, launch, or runtime failures are environment, toolchain,
  cache, permission, or invocation issues;
- fix failures by fixing the environment or the build invocation, not by
  modifying imported Ghostty code;
- only begin Ghostboard-specific modifications after the macOS build and launch
  baseline is verified and recorded in an experiment.

After Experiment 5, Ghostboard-specific modifications may begin, but the
build-only deviations must remain clearly separated from app/runtime, branding,
config, CLI, icon, protocol, `webtui`, and `roamium` changes.

In scope:

- Import Ghostty `v1.3.1` into `ghostboard/` with upstream history preserved.
- Keep Ghostty implementation names intact by default.
- Rename only the minimum user-facing and packaging surfaces needed for the new
  app identity.
- Make the CLI tool name `termsurf`.
- Make the user-facing app bundle `TermSurf.app`.
- Make the dock name, menu bar, and about page say `TermSurf`.
- Use the current Wezboard app icon for the TermSurf app built from
  `ghostboard/`, including the dock icon and about page icon.
- Make the configuration path `~/.config/termsurf/config`.
- Implement the current TermSurf protobuf/Unix-socket protocol inside
  Ghostboard.
- Support the existing `webtui` and `roamium` behavior without modifying those
  components.
- Use `ghostboard-legacy/` as a reference for prior Ghostboard protocol and GUI
  integration work.

Out of scope:

- Renaming all internal variables, modules, C symbols, comments, or build
  internals from `ghostty` to `termsurf`.
- Running the old wholesale `rename-ghostty.sh` flow.
- Modifying `webtui` or `roamium` to accommodate Ghostboard.
- Treating `ghostboard-legacy/` as active code.
- Using Ghostty's original icon as the new TermSurf app icon.
- Supporting any protocol shape other than the current TermSurf protocol.

## Naming and Branding Requirements

The new port should be deliberately minimal:

| Surface                    | Required value                              |
| -------------------------- | ------------------------------------------- |
| Source directory           | `ghostboard/`                               |
| CLI command                | `termsurf`                                  |
| User-facing app name       | `TermSurf`                                  |
| App bundle                 | `TermSurf.app`                              |
| Dock/menu/about page name  | `TermSurf`                                  |
| Config directory/file      | `~/.config/termsurf/config`                 |
| App icon                   | Same icon currently used by Wezboard        |
| Internal implementation    | Keep upstream Ghostty names unless required |
| Legacy reference directory | `ghostboard-legacy/`                        |

The source folder remains `ghostboard/`, but the shipped app identity is
`TermSurf`. The config path intentionally does not include `ghostboard`.

## Protocol Requirements

The app built from `ghostboard/` must implement the current TermSurf protocol in
the GUI. It should be able to accept the existing `webtui` process as a client
and coordinate browser engine processes such as Roamium using the current
protobuf/Unix-socket message set.

Expected protocol areas include, at minimum:

- GUI socket creation and `TERMSURF_SOCKET` propagation into terminal sessions.
- TUI connection lifecycle and message framing.
- Browser engine launch and socket connection lifecycle.
- Server registration and tab lifecycle.
- Overlay geometry and rendering lifecycle.
- CALayerHost or equivalent browser surface presentation.
- Keyboard and mouse forwarding in browser mode.
- Focus, pane, split, and resize synchronization.
- Navigation, loading, title, URL, and status updates.
- Shutdown and cleanup messages.
- Any current protocol messages that did not exist in Ghostboard Legacy.

The implementation should be guided by the current protocol definition and
Wezboard's active implementation, with `ghostboard-legacy/` used as a historical
reference.

## Acceptance Criteria

The issue is solved when:

- `ghostboard/` exists as a subtree import of Ghostty `v1.3.1`.
- The import preserves upstream Ghostty history.
- Before any Ghostboard-specific code changes, imported Ghostty `v1.3.1` builds
  and runs on macOS without errors, with any required build-only deviations
  explicitly documented.
- The app builds in the local development environment.
- The app can be launched as `TermSurf.app`.
- The CLI command is `termsurf`.
- The app uses `~/.config/termsurf/config`.
- Dock, menu, and about page branding say `TermSurf`.
- The TermSurf app icon matches the current Wezboard icon.
- `webtui` can run inside the app built from `ghostboard/` without changes.
- Roamium can be launched and controlled by the app built from `ghostboard/`
  without changes.
- The current TermSurf protocol is implemented well enough for the app built
  from `ghostboard/` to replace Wezboard for ordinary browsing workflows.
- The issue records experiments one at a time, with each experiment documenting
  design, changes, verification, result, and conclusion.

## Notes

This issue should not start by trying to port every historical Ghostboard change
blindly. Start with a clean Ghostty `v1.3.1` subtree, prove the pristine import
builds and runs on macOS with no source changes, establish the smallest renaming
and packaging surface needed for `TermSurf`, then add the TermSurf protocol
implementation incrementally.

Each experiment should preserve the ability to compare against upstream Ghostty
and against `ghostboard-legacy/` where useful.

## Experiments

- [Experiment 1: Import Ghostty 1.3.1 subtree](01-import-ghostty-1-3-1-subtree.md)
  — **Pass**
- [Experiment 2: Build the pristine Ghostty import](02-build-pristine-ghostty-import.md)
  — **Partial**
- [Experiment 3: Fix pristine macOS app link](03-fix-pristine-macos-app-link.md)
  — **Fail**
- [Experiment 4: Reproduce the upstream macOS baseline](04-reproduce-upstream-macos-baseline.md)
  — **Fail**
- [Experiment 5: Apply the macOS-only GhosttyKit build patch](05-apply-macos-only-ghosttykit-build-patch.md)
  — **Pass**
- [Experiment 6: Establish minimal TermSurf identity](06-minimal-ghostboard-identity.md)
  — **Pass**
- [Experiment 7: Start the TermSurf GUI socket](07-start-termsurf-gui-socket.md)
  — **Pass**
- [Experiment 8: Decode HelloRequest](08-decode-hello-request.md) — **Pass**
- [Experiment 9: Reply to QueryTabsRequest](09-query-tabs-reply.md) — **Pass**
- [Experiment 10: Reply to QueryLastRequest](10-query-last-reply.md) — **Pass**
- [Experiment 11: Reply to QueryDevtoolsRequest](11-query-devtools-reply.md) —
  **Pass**
- [Experiment 12: Classify TermSurf connections](12-classify-termsurf-connections.md)
  — **Pass**
- [Experiment 13: Handle unmatched ServerRegister](13-handle-unmatched-server-register.md)
  — **Pass**
- [Experiment 14: Track pending overlay servers](14-track-pending-overlay-servers.md)
  — **Pass**
- [Experiment 15: Flush pending CreateTab](15-flush-pending-create-tab.md) —
  **Pass**
- [Experiment 16: Record TabReady state](16-record-tab-ready-state.md) —
  **Pass**
- [Experiment 17: Reply to QueryLast from state](17-query-last-from-state.md) —
  **Pass**
- [Experiment 18: Count QueryTabs GUI panes from state](18-query-tabs-gui-pane-count.md)
  — **Pass**
- [Experiment 19: Reply to QueryDevtools from tab state](19-query-devtools-from-tab-state.md)
  — **Pass**
- [Experiment 20: Spawn browser process for SetOverlay](20-spawn-browser-process.md)
  — **Pass**
- [Experiment 21: Send BrowserReady after TabReady](21-send-browser-ready.md) —
  **Pass**
- [Experiment 22: Forward Resize on overlay updates](22-forward-resize-on-overlay-updates.md)
  — **Pass**
- [Experiment 23: Forward ModeChanged focus to browser](23-forward-tui-control-to-browser.md)
  — **Pass**
- [Experiment 24: Close browser tab on TUI disconnect](24-close-tab-on-tui-disconnect.md)
  — **Pass**
- [Experiment 25: Create DevTools browser tabs](25-create-devtools-browser-tabs.md)
  — **Pass**
- [Experiment 26: Propagate pane ids to surfaces](26-propagate-pane-ids-to-surfaces.md)
  — **Pass**
- [Experiment 27: Open native splits from protocol](27-open-native-splits-from-protocol.md)
  — **Pass**
- [Experiment 28: Validate webtui DevTools flow](28-validate-webtui-devtools-flow.md)
  — **Pass**
- [Experiment 29: Validate Roamium normal tab lifecycle](29-validate-roamium-normal-tab-lifecycle.md)
  — **Fail**
- [Experiment 30: Validate Chromium-output Roamium lifecycle](30-validate-chromium-output-roamium-lifecycle.md)
  — **Pass**
- [Experiment 31: Present the normal Roamium overlay](31-present-normal-roamium-overlay.md)
  — **Pass**
- [Experiment 32: Forward normal browser input](32-forward-normal-browser-input.md)
  — **Pass**
- [Experiment 33: Acceptance audit and walkthrough](33-acceptance-audit-and-walkthrough.md)
  — **Partial**
- [Experiment 34: Fix CLI and Zig app names](34-fix-cli-and-zig-app-names.md) —
  **Partial**
- [Experiment 35: Install macOS helper CLI](35-install-macos-helper-cli.md) —
  **Designed**
