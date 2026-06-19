# Experiment 2: Apply the Full Upstream Subtree Merge

## Description

Apply the full upstream Ghostty range selected by Experiment 1 to `ghostboard/`
using a history-preserving subtree pull, then resolve the known conflicts into a
coherent working tree.

The selected range is:

```text
332b2aefc6e72d363aa93ab6ecfc86eeeeb5ed28..5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff
```

This experiment is the real merge implementation, not another dry run. Its goal
is to make the full upstream update present in `ghostboard/` with no unresolved
merge conflicts while preserving required TermSurf-specific behavior.

Build, launch, and runtime parity are separate gates. This experiment should not
expand into broad build debugging unless a minimal build-file conflict
resolution requires it.

## Changes

- `ghostboard/`
  - Run the history-preserving subtree pull:

```bash
git subtree pull --prefix=ghostboard ghostty \
  5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff \
  -m "Merge upstream Ghostty into ghostboard"
```

- Resolve the expected 17 conflicts from Experiment 1.
- Preserve upstream Ghostty changes by default.
- Preserve TermSurf-specific behavior where required for the app identity,
  protocol exports, socket/protobuf integration, browser overlay lifecycle,
  browser input forwarding, config path, CLI name, and poisoned-file cleanup.
- `issues/0826-update-ghostboard-to-latest-ghostty/README.md`
  - Link this experiment with status `Designed`, then update the status after
    the result is known.
- `issues/0826-update-ghostboard-to-latest-ghostty/02-apply-full-upstream-subtree-merge.md`
  - Record the plan, conflict-resolution result, verification, review, and
    conclusion.

Do not modify `webtui/` or `roamium/` in this experiment.

## Conflict Resolution Strategy

Resolve the known conflict groups as follows:

- `.agents/commands/gh-issue`
  - Delete this file. It is an upstream agent-helper command, upstream deleted
    it, and TermSurf has no current requirement to keep it inside `ghostboard/`.
- `.github/VOUCHED.td` and `.github/workflows/vouch-*`
  - Preserve TermSurf's poisoned-file cleanup. Do not reintroduce upstream
    poisoned/vouch files that were intentionally removed.
- `.github/workflows/test.yml`
  - Preserve any TermSurf-required CI behavior and adopt upstream path-filter
    improvements when they do not reintroduce poisoned/vouch behavior.
- `CONTRIBUTING.md`, `HACKING.md`, `README.md`
  - Keep the current TermSurf/Ghostboard-local versions as the baseline.
  - Do not import upstream agent instructions, contribution policy, or README
    text that conflicts with root `AGENTS.md`, TermSurf's frontend status, or
    poisoned-file cleanup.
  - Only retain upstream doc content if it is factual project/build information
    needed by the merged source tree, contains no active agent/developer
    instructions, and does not contradict TermSurf's current Ghostboard role.
- `build.zig`
  - Combine upstream `emit_lib_vt` behavior with TermSurf's existing build
    semantics.
  - Do not install the CLI merely because an executable is emitted.
- `include/ghostty.h`
  - Keep upstream `GHOSTTY_API` usage and new upstream declarations.
  - Preserve TermSurf protocol C exports.
- `TerminalController.swift`
  - Preserve TermSurf pane cleanup.
  - Preserve upstream pending-initial-presentation cleanup.
- `SurfaceView_AppKit.swift`
  - Preserve upstream copy/action behavior.
  - Preserve TermSurf copy-current-URL feedback and any TermSurf state needed by
    browser overlay behavior.
- `src/build/SharedDeps.zig`
  - Preserve TermSurf protobuf C sources.
  - Preserve upstream MSVC sanitizer handling for `stb.c`.
- `src/main_c.zig`
  - Preserve TermSurf exported protocol functions.
  - Preserve upstream Windows `DllMain` support.

If a conflict has an unexpected shape during the real merge, resolve it using
the same rule: upstream wins by default, TermSurf wins only for documented
TermSurf-specific behavior.

## Verification

Before implementation:

```bash
git status --short
git rev-parse ghostty/main
test "$(git rev-parse ghostty/main)" = \
  "5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff"
git rev-list --count 332b2aefc6e72d363aa93ab6ecfc86eeeeb5ed28..ghostty/main
```

During implementation:

```bash
git diff --name-only --diff-filter=U
rg -n '^(<<<<<<<|=======|>>>>>>>)' ghostboard
```

After resolving conflicts, before result review:

```bash
test -z "$(git diff --name-only --diff-filter=U)"
test "$(git rev-parse MERGE_HEAD)" = \
  "5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff"
! rg -n '^(<<<<<<<|=======|>>>>>>>)' ghostboard
zig fmt ghostboard/build.zig ghostboard/src/build/SharedDeps.zig ghostboard/src/main_c.zig
(cd ghostboard && swiftlint lint --strict --fix)
git diff --check
```

If additional conflicted Zig files are introduced by the merge, run `zig fmt` on
those files as well. If additional Swift files are modified by conflict
resolution, they are covered by the `swiftlint lint --strict --fix` command from
`ghostboard/AGENTS.md`.

Result recording and review:

1. Append `## Result` and `## Conclusion` to this file.
2. Record each resolved conflict group and the chosen resolution.
3. Update the issue README status for this experiment to `Pass`, `Partial`, or
   `Fail`.
4. Run Prettier on this experiment file and the issue README:

```bash
prettier --write --prose-wrap always --print-width 80 \
  issues/0826-update-ghostboard-to-latest-ghostty/README.md \
  issues/0826-update-ghostboard-to-latest-ghostty/02-apply-full-upstream-subtree-merge.md
```

5. Request the required result review before completing the merge/result commit.
6. Commit the reviewed merge result.
7. After the merge/result commit, verify that the commit preserves the upstream
   parent:

```bash
git rev-list --parents -n 1 HEAD
git rev-list --parents -n 1 HEAD | rg '5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff'
```

Because this experiment is a real merge, the final result commit may be the
merge commit created after conflict resolution and result review. Use the
standard subtree merge message rather than a poetic commit message for that
merge commit.

Pass criteria:

- The full selected upstream range is applied with history preserved.
- Before the result review, `MERGE_HEAD` resolves to
  `5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff`.
- After the result commit, `HEAD` has `5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff`
  as a merge parent.
- No unmerged paths remain.
- No conflict markers remain under `ghostboard/`.
- The known 17 conflict files are resolved and documented.
- Required TermSurf-specific behavior is preserved in the resolved files.
- The result has been reviewed before the merge/result commit is completed.

Fail criteria:

- The real merge uses `git merge -X subtree`, copy-over replacement, or another
  non-history-preserving mechanism.
- The merge cannot be resolved into a coherent working tree.
- TermSurf-specific protocol, overlay, config, CLI, or poisoned-file cleanup
  behavior is knowingly dropped without a documented reason.
- The result attempts to absorb broad build, launch, or runtime parity work that
  should be handled by later experiments.

## Design Review

An adversarial Codex subagent reviewed the initial design with fresh context.

**Verdict:** Changes required.

Required findings and fixes:

- History preservation was not directly verified. Fixed by requiring
  `MERGE_HEAD` to equal the upstream target before result review, and requiring
  the final result commit's parents to include the upstream target.
- Swift formatting hygiene was omitted. Fixed by adding
  `(cd ghostboard && swiftlint lint --strict --fix)`, as required by
  `ghostboard/AGENTS.md`.
- Conflict strategy for `.agents/commands/gh-issue` and the doc files was too
  ambiguous. Fixed by explicitly deleting `.agents/commands/gh-issue`, keeping
  TermSurf/Ghostboard-local docs as the baseline, and allowing upstream doc
  content only when it is factual, non-instructional, and compatible with
  TermSurf's current Ghostboard direction.

The re-review approved the design with no required findings. The optional
suggestions were adopted by adding an explicit `ghostty/main` target assertion
and spelling out the Prettier command.

## Result

**Result:** Pass

The real full-range subtree pull was applied to `ghostboard/`:

```bash
git subtree pull --prefix=ghostboard ghostty \
  5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff \
  -m "Merge upstream Ghostty into ghostboard"
```

The command produced the expected 17 conflicts from Experiment 1. The merge was
resolved in the main worktree, and no unmerged paths or conflict markers remain.

Preflight and merge identity:

```text
git status --short                                  # clean before merge
git rev-parse ghostty/main                          # 5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff
git rev-list --count 332b2aef..ghostty/main         # 1159
git rev-parse MERGE_HEAD                            # 5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff
```

Resolved conflict groups:

- `.agents/commands/gh-issue`
  - Deleted. Upstream deleted it, and TermSurf has no current need to keep that
    upstream helper inside `ghostboard/`.
- `.github/VOUCHED.td` and `.github/workflows/vouch-*`
  - Kept absent. The merge did not reintroduce the poisoned/vouch files that
    TermSurf intentionally removed.
- `.github/workflows/test.yml`
  - Adopted upstream `dorny/paths-filter` `v4.0.1` pin and the upstream
    `.github/VOUCHED.td` exclusion while preserving the absence of the vouch
    files themselves.
- `CONTRIBUTING.md`, `HACKING.md`, `README.md`
  - Kept the TermSurf/Ghostboard-local versions as the baseline. Upstream
    contribution and README text was not imported as active local instruction.
- `build.zig`
  - Adopted upstream `emit_lib_vt` behavior and internal library naming.
  - Preserved the prior correction that an executable is not installed merely
    because `emit_exe` is true in `app_runtime == .none` mode.
- `include/ghostty.h`
  - Adopted upstream `GHOSTTY_API` declarations and new upstream config API.
  - Preserved TermSurf protocol C exports and repeatable config accessors.
- `TerminalController.swift`
  - Preserved TermSurf pane cleanup.
  - Preserved upstream pending-initial-presentation cleanup.
- `SurfaceView_AppKit.swift`
  - Preserved TermSurf copy-current-URL feedback and state.
  - Preserved successful copy-action behavior without logging a false failure.
- `src/build/SharedDeps.zig`
  - Preserved upstream MSVC sanitizer handling for `stb.c`.
  - Preserved TermSurf protobuf C include path and C sources.
- `src/main_c.zig`
  - Preserved TermSurf exported protocol functions.
  - Preserved upstream Windows `DllMain` support.

Additional poisoned-file cleanup:

- Upstream added `.agents/skills/writing-commit-messages/SKILL.md`. This was
  removed because TermSurf already has its own GitPoet commit workflow, and the
  upstream skill would be an active agent instruction inside `ghostboard/`.
- Upstream added `CLAUDE.md`. It was kept because it is identical to the
  sanitized `ghostboard/AGENTS.md` after the merge.

Verification run before result review:

```text
test -z "$(git diff --name-only --diff-filter=U)"        # pass
test "$(git rev-parse MERGE_HEAD)" = target              # pass
! rg -n '^(<<<<<<<|=======|>>>>>>>)' ghostboard          # pass
zig fmt ghostboard/build.zig \
  ghostboard/src/build/SharedDeps.zig \
  ghostboard/src/main_c.zig                              # pass
(cd ghostboard && swiftlint lint --strict --fix)         # pass
git diff --check                                         # pass
git diff --name-only                                     # no unstaged changes
```

SwiftLint printed:

```text
warning: The option --strict has no effect together with --fix.
```

It then completed successfully. The warning is from SwiftLint's option
combination, not from Ghostboard source. The command is the exact formatter
command required by `ghostboard/AGENTS.md`.

The staged whitespace check initially found trailing whitespace in a small set
of newly imported upstream C/header example files and one extra blank line at
EOF in an upstream Swift UI test. Those were mechanically cleaned and restaged:

```text
ghostboard/example/c-vt-kitty-graphics/src/main.c
ghostboard/example/c-vt-static/src/main.c
ghostboard/include/ghostty/vt/grid_ref.h
ghostboard/include/ghostty/vt/key.h
ghostboard/include/ghostty/vt/kitty_graphics.h
ghostboard/include/ghostty/vt/render.h
ghostboard/include/ghostty/vt/terminal.h
ghostboard/macos/GhosttyUITests/GhosttyCommandPaletteTests.swift
```

After that cleanup, `git diff --cached --check` passed.

Poisoned/vouch file checks:

```text
git ls-files ghostboard/.github/VOUCHED.td \
  ghostboard/.github/workflows/vouch-check-issue.yml \
  ghostboard/.github/workflows/vouch-check-pr.yml \
  ghostboard/.github/workflows/vouch-manage-by-discussion.yml \
  ghostboard/.github/workflows/vouch-manage-by-issue.yml \
  ghostboard/.github/workflows/vouch-sync-codeowners.yml \
  ghostboard/.agents/skills/writing-commit-messages/SKILL.md
```

produced no output.
`find ghostboard/.github/workflows -maxdepth 1 -name 'vouch-*' -print` also
produced no output, and `ghostboard/.github/VOUCHED.td` does not exist.

The final parent check cannot be run until after the reviewed merge/result
commit is created. It remains required immediately after the commit:

```bash
git rev-list --parents -n 1 HEAD
git rev-list --parents -n 1 HEAD | rg '5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff'
```

## Conclusion

Experiment 2 applied the full upstream Ghostty range selected by Experiment 1
and resolved the known conflict set without falling back to smaller ranges.

The merge is coherent enough to proceed to the next gate: build verification.
The next experiment should build the updated `ghostboard/` tree, treat failures
as build/toolchain/integration issues to be fixed or documented, and avoid
runtime parity work until the build gate is complete.

## Result Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context while the merge was still pending.

**Verdict:** Approved.

The reviewer found no required issues. It independently verified:

- `MERGE_HEAD` was `5d0a82ba337368f5632ffa6ce4d7c558fa2de9ff`;
- no unmerged paths remained;
- no conflict markers remained under `ghostboard/`;
- poisoned/vouch files and the upstream commit-message skill were absent from
  `git ls-files`;
- `ghostboard/AGENTS.md` and `ghostboard/CLAUDE.md` were identical;
- `git diff --check` and `git diff --cached --check` passed;
- `git diff --name-only` was empty, so all merge/result changes were staged;
- the result commit had not yet been made.

The reviewer did not rerun `zig fmt` or `swiftlint --fix` because those commands
may mutate files. The staged whitespace checks independently passed.
