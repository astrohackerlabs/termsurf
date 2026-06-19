+++
status = "open"
opened = "2026-06-19"
+++

# Issue 826: Update Ghostboard to Latest Ghostty

## Goal

Update `ghostboard/` so it is based on the latest available upstream Ghostty
commit while preserving Ghostty history and preserving TermSurf's Ghostboard
functionality.

When solved, Ghostboard should contain the current Ghostty implementation plus
the TermSurf-specific app identity, config path, protocol integration, browser
overlay behavior, and regression coverage needed for Ghostboard to remain the
primary TermSurf front-end.

## Background

Issue 808 recreated `ghostboard/` from Ghostty `v1.3.1` using a history-
preserving subtree import. That baseline is:

```text
Ghostty v1.3.1 tag object: 22efb0be2bbea73e5339f5426fa3b20edabcaa11
Ghostty v1.3.1 commit:     332b2aefc6e72d363aa93ab6ecfc86eeeeb5ed28
TermSurf import commit:    493817fd94ee3bc6bdefb24274132e7862378226
```

The current local upstream comparison target is Ghostty `origin/main`:

```text
5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff
5d0a82ba3 Remove unintended reasoning for user 'qappell' in vouched list (#13050)
```

The comparison from Ghostty `v1.3.1` to that upstream commit found:

```text
1159 commits
575 files changed
100078 insertions
44847 deletions
```

This is a large upstream update, not a small patch. The update must be treated
as an upstream merge, conflict-resolution, build, runtime, and parity effort.

## Constraints

- Do not rewrite Ghostty history.
- Preserve the history-preserving subtree relationship for `ghostboard/`.
- Do not copy files over the tree in a way that loses upstream commit ancestry.
- Do not begin by blindly reapplying historical Ghostboard patches.
- Do not modify `webtui` or `roamium` to compensate for Ghostboard regressions.
- Do not change the TermSurf protocol shape unless a separate issue explicitly
  decides to change the protocol.
- Keep Ghostty internal implementation names unless changing them is required
  for the app identity or build output.
- Keep the user-facing app identity as `TermSurf`.
- Keep the CLI command as `termsurf`.
- Keep the config path as `~/.config/termsurf/config`.
- Keep Ghostboard as the primary TermSurf front-end.

## Analysis

The update will likely touch the same files TermSurf has already modified in
Ghostboard. The highest-risk overlap files from the comparison are:

```text
ghostboard/build.zig
ghostboard/include/ghostty.h
ghostboard/macos/Ghostty.xcodeproj/project.pbxproj
ghostboard/macos/Sources/App/macOS/AppDelegate.swift
ghostboard/macos/Sources/Features/Terminal/BaseTerminalController.swift
ghostboard/macos/Sources/Features/Terminal/TerminalController.swift
ghostboard/macos/Sources/Ghostty/Ghostty.Config.swift
ghostboard/src/build/GhosttyExe.zig
ghostboard/src/build/GhosttyXCFramework.zig
ghostboard/src/build/LibtoolStep.zig
ghostboard/src/build/SharedDeps.zig
ghostboard/src/config/CApi.zig
ghostboard/src/config/Config.zig
ghostboard/src/main_c.zig
```

The largest upstream feature and fix clusters to account for are:

- terminal and `libghostty-vt` C API changes;
- selection and mouse gesture behavior;
- glyph glossary / APC protocol work;
- terminal correctness fixes for resizing, prompts, cursor state, links,
  variation selectors, and preedit text;
- macOS fixes for IME, search, global keybinds, focus, App Intents, window
  restoration, quick terminal behavior, config refresh, and surface state;
- CLI and configuration additions;
- build system and packaging changes.

The update strategy should therefore be ambitious, incremental, and
evidence-driven:

1. Record the exact upstream target commit and current Ghostboard subtree base.
2. Reproduce a clean upstream Ghostty build and run from the target commit in
   `vendor/ghostty` or another clean upstream checkout.
3. Try the full upstream update range first in a disposable branch or worktree.
4. If the full-range dry run is too large, scale back by commit range: try a
   midpoint, then halve or expand ranges based on observed merge difficulty.
5. Use the dry-run attempts to identify the largest practical first merge range,
   rather than assuming ahead of time that the update must be split into small
   chunks.
6. Plan the real subtree update mechanism and conflict strategy from the dry-run
   evidence.
7. Apply the selected upstream update range to `ghostboard/` with history
   preserved.
8. Resolve conflicts by keeping upstream behavior unless TermSurf-specific
   behavior is required.
9. Rebuild and run the updated app.
10. Reapply or adapt TermSurf-specific behavior only where the upstream update
    removed or conflicted with it.
11. Verify Ghostboard-specific browser overlay behavior, pane/tab/window
    geometry, input forwarding, config, CLI, app identity, and protocol
    behavior.
12. Add focused durable regression guards for behavior proven during the issue.

The first experiment should therefore determine the largest practical upstream
merge range. It should attempt the full `v1.3.1` to latest Ghostty range first
in a throwaway branch or worktree. If that attempt produces an unmanageable
conflict set, the experiment should retry with smaller commit ranges until it
finds a range that is large enough to make progress but small enough to resolve
and verify rigorously.

Each dry-run range attempt should record:

- the upstream start and end commits;
- the number of commits in the range;
- whether the subtree update command completed cleanly;
- the conflicted files;
- whether conflicts appear mechanical, semantic, build-system-specific, or
  TermSurf-specific;
- the recommended next real merge range.

Dry-run attempts must not leave permanent `ghostboard/` changes behind. Once a
range is selected, a later experiment should perform the real history-preserving
update for that range.

## Acceptance Criteria

- The issue records the exact upstream Ghostty commit used as the new Ghostboard
  base.
- `ghostboard/` is updated with Ghostty history preserved.
- The update path is documented well enough that a future upstream update can
  follow the same pattern.
- The updated Ghostboard builds on macOS.
- The updated Ghostboard runs on macOS.
- The app identity remains `TermSurf`.
- The CLI command remains `termsurf`.
- The config path remains `~/.config/termsurf/config`.
- The TermSurf protocol still works with the existing `webtui` and `roamium`
  binaries without requiring changes to either component.
- Browser overlays still attach to the correct pane, tab, and window.
- Browser overlays still resize and move correctly during pane resize, split,
  close, tab switch, and window changes.
- Keyboard and mouse forwarding still work in browser mode.
- Existing Ghostboard-specific configuration options still work or are
  intentionally updated with documented compatibility notes.
- Important newly inherited Ghostty features and fixes are identified and, where
  practical, smoke-tested.
- Any upstream changes that cannot be adopted are explicitly documented with a
  reason.
- Regression guards are added only for high-value behavior, avoiding broad slow
  test duplication.

## Notes

This issue should not start with code changes. The first experiment should
confirm the exact upstream target, try the full-range dry-run merge, scale back
only if needed, and design the first real update range from observed conflict
data. Implementation should begin only after the plan has passed the normal
experiment review gate.

Do not open experiments upfront. Each experiment should be designed after the
previous experiment has produced a result.

## Experiments

- [Experiment 1: Discover the largest practical merge range](01-discover-largest-practical-merge-range.md)
  — **Pass**
- [Experiment 2: Apply the full upstream subtree merge](02-apply-full-upstream-subtree-merge.md)
  — **Pass**
- [Experiment 3: Build the merged Ghostboard tree](03-build-merged-ghostboard-tree.md)
  — **Pass**
- [Experiment 4: Launch the merged Ghostboard app](04-launch-merged-ghostboard-app.md)
  — **Pass**
- [Experiment 5: Restore TermSurf identity surfaces](05-restore-termsurf-identity-surfaces.md)
  — **Designed**
