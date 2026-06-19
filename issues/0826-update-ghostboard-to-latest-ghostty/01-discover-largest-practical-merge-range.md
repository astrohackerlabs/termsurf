# Experiment 1: Discover the Largest Practical Merge Range

## Description

Determine the largest practical upstream Ghostty commit range that Ghostboard
can merge first while preserving history and keeping the work reviewable.

This experiment is intentionally a dry-run discovery gate. It should not make
permanent `ghostboard/` source changes. It should use disposable branches or
worktrees to try the full upstream range first, then scale back by commit range
only if the full range produces an unmanageable conflict set.

The expected output is a documented recommendation for the first real upstream
merge experiment: either the full `v1.3.1` to latest Ghostty range, or a smaller
range justified by conflict evidence.

## Changes

- `issues/0826-update-ghostboard-to-latest-ghostty/README.md`
  - Link this experiment with status `Designed`.
- `issues/0826-update-ghostboard-to-latest-ghostty/01-discover-largest-practical-merge-range.md`
  - Define the dry-run range discovery plan and verification criteria.

No production code, build files, vendored source, `ghostboard/`, `webtui/`, or
`roamium/` files should be changed by this experiment plan.

## Verification

The implementation pass for this experiment should:

1. Confirm the working tree is clean before dry-run work begins.
2. Confirm the current Ghostboard subtree base:
   `332b2aefc6e72d363aa93ab6ecfc86eeeeb5ed28`.
3. Fetch or verify the latest Ghostty `origin/main` in `vendor/ghostty`, record
   the exact target commit, and note any fetch caveats.
4. Confirm a clean upstream Ghostty checkout at the target commit can at least
   report its build metadata and dependency expectations. If a full upstream
   build is practical within the experiment, run it; otherwise record why the
   build is deferred to a later gate.
5. Create a disposable branch or worktree for dry-run merge attempts.
6. Attempt the full upstream update range first using the same
   history-preserving mechanism intended for the real update:

```bash
git subtree pull --prefix=ghostboard ghostty <target-commit> \
  -m "Merge upstream Ghostty into ghostboard"
```

The dry run must not use `git merge -X subtree`, copy-over file replacement, or
any other non-history-preserving update mechanism.

7. If the full range is not practical, retry with smaller ranges selected from
   the upstream commit list, starting near the midpoint and adjusting based on
   observed conflict difficulty.
8. For every attempted range, record:
   - start commit;
   - end commit;
   - commit count;
   - command used;
   - whether the command completed cleanly;
   - conflicted files;
   - conflict classification: mechanical, semantic, build-system-specific,
     TermSurf-specific, or unknown;
   - unresolved conflict count from `git diff --name-only --diff-filter=U`;
   - whether a bounded inspection indicates the conflicts are likely resolvable
     within one real merge experiment;
   - whether the range is recommended for the first real merge.
9. Clean up or abandon disposable dry-run state so no dry-run `ghostboard/`
   changes remain in the main working tree.
10. Append `## Result` and `## Conclusion` to this file.
11. Update the experiment status in the issue README to `Pass`, `Partial`, or
    `Fail`.
12. Run:

```bash
prettier --write --prose-wrap always --print-width 80 \
  issues/0826-update-ghostboard-to-latest-ghostty/README.md \
  issues/0826-update-ghostboard-to-latest-ghostty/01-discover-largest-practical-merge-range.md
git diff --check
git status --short
git status --short -- ghostboard
test -z "$(git diff --name-only --diff-filter=U)"
```

Range selection rules:

- Attempt the full `v1.3.1` to latest Ghostty range first.
- Treat a range as practical if the dry run either merges cleanly or leaves a
  conflict set that is small enough to inspect file-by-file and classify during
  this experiment.
- Treat a range as not practical for the first real merge if the conflict set is
  too broad to classify file-by-file during this experiment, includes many
  unrelated conflict categories at once, or leaves no credible path to a
  buildable tree in one follow-up implementation experiment.
- If the full range is not practical, try a midpoint range. If the midpoint is
  still not practical, halve again. If the midpoint is practical, expand toward
  the latest upstream commit until the next attempted range stops being
  practical or the full range is reached.
- Select the largest attempted practical range. Do not claim a smaller range is
  largest practical unless at least one larger attempted range is documented as
  not practical.

Pass criteria:

- The full-range dry run was attempted first, or the experiment explains why it
  could not be attempted.
- The experiment identifies the largest practical first merge range from
  observed conflict data.
- The main working tree has no dry-run source changes.
- `git status --short -- ghostboard` is empty after cleanup.
- No unmerged paths remain in the main working tree after cleanup.
- The next experiment has enough evidence to perform the selected real merge
  range with history preserved.

Fail criteria:

- Dry-run state contaminates the main working tree.
- The experiment cannot identify any actionable next merge range.
- The recorded data is too vague to distinguish an easy merge range from an
  unreviewable one.
- The dry run uses `git merge -X subtree`, copy-over replacement, or another
  non-history-preserving update mechanism.

## Design Review

An adversarial Codex subagent reviewed the initial design with fresh context.

**Verdict:** Changes required.

Required findings and fixes:

- The subtree update mechanism was underspecified. Fixed by naming
  `git subtree pull --prefix=ghostboard ghostty <target-commit>` as the command
  shape and explicitly banning `git merge -X subtree`, copy-over replacement,
  and other non-history-preserving mechanisms.
- The pass criteria did not objectively prove "largest practical." Fixed by
  adding range selection rules, conflict counts, conflict classification, and a
  requirement to document at least one larger failed range before selecting a
  smaller range.
- Cleanup checks were too broad. Fixed by adding
  `git status --short -- ghostboard` and an unmerged-path check.

The optional note that the experiment file was untracked will be addressed at
the plan commit gate by staging this file with the issue README.

Re-review after those fixes:

**Verdict:** Approved.

The reviewer found no remaining findings. It confirmed the command shape,
non-history-preserving mechanism ban, objective range selection rules, focused
cleanup checks, issue README link, experiment structure, scope, and verification
criteria.
