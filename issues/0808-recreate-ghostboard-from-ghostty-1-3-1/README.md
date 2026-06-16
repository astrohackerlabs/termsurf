+++
status = "open"
opened = "2026-06-16"
+++

# Issue 808: Re-create Ghostboard from Ghostty 1.3.1

## Goal

Create a new `ghostboard/` GUI by importing Ghostty `v1.3.1` with full upstream
history preserved, then implement the current TermSurf protocol inside that new
app so `webtui` can run in Ghostboard without changes to `webtui` or `roamium`.

When solved, Ghostboard should be usable as a Ghostty-based alternative to
Wezboard.

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

In scope:

- Import Ghostty `v1.3.1` into `ghostboard/` with upstream history preserved.
- Keep Ghostty implementation names intact by default.
- Rename only the minimum user-facing and packaging surfaces needed for the new
  app identity.
- Make the CLI tool name `ghostboard`.
- Make the user-facing app name `TermSurf Ghostboard`.
- Make the dock name, menu bar, and about page say `TermSurf Ghostboard`.
- Use the current Wezboard app icon for Ghostboard, including the dock icon and
  about page icon.
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
- Using Ghostty's original icon as the new Ghostboard app icon.
- Supporting any protocol shape other than the current TermSurf protocol.

## Naming and Branding Requirements

The new port should be deliberately minimal:

| Surface                    | Required value                              |
| -------------------------- | ------------------------------------------- |
| Source directory           | `ghostboard/`                               |
| CLI command                | `ghostboard`                                |
| User-facing app name       | `TermSurf Ghostboard`                       |
| Dock/menu/about page name  | `TermSurf Ghostboard`                       |
| Config directory/file      | `~/.config/termsurf/config`                 |
| App icon                   | Same icon currently used by Wezboard        |
| Internal implementation    | Keep upstream Ghostty names unless required |
| Legacy reference directory | `ghostboard-legacy/`                        |

The config path intentionally does not include `ghostboard`.

## Protocol Requirements

Ghostboard must implement the current TermSurf protocol in the GUI. It should be
able to accept the existing `webtui` process as a client and coordinate browser
engine processes such as Roamium using the current protobuf/Unix-socket message
set.

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
- The app builds in the local development environment.
- The app can be launched as `TermSurf Ghostboard`.
- The CLI command is `ghostboard`.
- The app uses `~/.config/termsurf/config`.
- Dock, menu, and about page branding say `TermSurf Ghostboard`.
- The Ghostboard icon matches the current Wezboard icon.
- `webtui` can run inside Ghostboard without changes.
- Roamium can be launched and controlled by Ghostboard without changes.
- The current TermSurf protocol is implemented well enough for Ghostboard to
  replace Wezboard for ordinary browsing workflows.
- The issue records experiments one at a time, with each experiment documenting
  design, changes, verification, result, and conclusion.

## Notes

This issue should not start by trying to port every historical Ghostboard change
blindly. Start with a clean Ghostty `v1.3.1` subtree, establish the smallest
renaming and packaging surface needed for `TermSurf Ghostboard`, then add the
TermSurf protocol implementation incrementally.

Each experiment should preserve the ability to compare against upstream Ghostty
and against `ghostboard-legacy/` where useful.
