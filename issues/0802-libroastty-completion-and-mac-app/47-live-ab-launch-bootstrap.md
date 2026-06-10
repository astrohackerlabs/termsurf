+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex-adversarial"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 47: Phase D — launch-time live A/B recipe bootstrap

## Description

Experiment 46 proved that synthetic UI command delivery is the wrong foundation
for the live A/B harness right now: Command-V terminates Roastty, and both
AppleScript and CGEvent keyboard text can leave the recipe unexecuted while the
permissive screenshot diff still exits `0`. The next narrow step is to stop
typing recipe commands through the UI.

This experiment changes the live A/B harness to launch each app binary directly
with a per-run shell bootstrap directory. The bootstrap writes the selected
recipe command into the shell startup path for that launched process only, so
the terminal executes the recipe as it starts. This keeps the test in the real
app and real terminal surface, but removes the unreliable synthetic keyboard
layer from recipe setup.

Direct app-binary launch was pre-checked locally with:

```bash
/Users/ryan/dev/termsurf/roastty/macos/build/Debug/Roastty.app/Contents/MacOS/roastty &
```

The process stayed alive as the normal debug Roastty app and was killed by the
existing scoped stop helper.

## Changes

- `scripts/roastty-app/live-ab-smoke.sh`
  - Add a per-run temporary bootstrap directory outside the repo.
  - Launch Ghostty and Roastty by invoking their app binaries directly with
    per-process environment, rather than `open`, when running live A/B recipes.
  - Set the shell environment for each launched app so the startup shell reads a
    generated recipe file:
    - prefer `ZDOTDIR=<tmp>` with a generated `.zshrc`, matching the observed
      zsh prompt in current debug runs;
    - keep the shell alive after the recipe so screenshots and later manual
      inspection still work.
  - Preserve exact launched PID-tree cleanup.
  - Preserve window sizing, capture, diff, recipe names, matrix composition, and
    screenshot-outside-repo policy.
  - Remove or bypass the synthetic command-entry path for recipe execution in
    the live A/B flow.
- `scripts/roastty-app/README.md`
  - Document that live A/B recipes use launch-time shell bootstrap instead of UI
    typing.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add this experiment to the index as `Designed`.
  - After implementation, update the Operating notes with the bootstrap command
    delivery result.

## Verification

- Run shell syntax checks:
  - `bash -n scripts/roastty-app/live-ab-smoke.sh`
  - `bash -n scripts/roastty-app/live-ab-matrix.sh`
- Run non-GUI recipe discovery:
  - `scripts/roastty-app/live-ab-smoke.sh --list-recipes`
- Run the default one-recipe live A/B smoke with permissive thresholds:
  - `scripts/roastty-app/live-ab-smoke.sh --recipe smoke --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  - Confirm both captures visibly contain the smoke marker or otherwise provide
    direct evidence that the recipe executed in both apps.
  - Confirm the JSON summary is emitted and only launched PID trees are killed.
- Run the recipe that exposed the `%`/escaping problem:
  - `scripts/roastty-app/live-ab-smoke.sh --recipe ascii-grid --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  - Confirm both captures visibly contain the marker, uppercase row, lowercase
    row, digit row, and punctuation row without `printf` errors or shell quote
    prompts.
- If those pass, run the full default matrix:
  - `scripts/roastty-app/live-ab-matrix.sh`
  - Confirm it exits `0` with permissive thresholds and emits one JSON Lines
    object for every recipe reported by `live-ab-smoke.sh --list-recipes`.
  - Confirm every recipe has direct execution evidence in both apps, either from
    visible capture inspection of that recipe's marker/expected rows or from a
    harness-emitted per-app marker/recipe-executed field that fails the recipe
    before the matrix can report success.
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/47-live-ab-launch-bootstrap.md scripts/roastty-app/README.md`
- Run `git diff --check`.
- Run
  `pgrep -fl '[G]hostty.app/Contents/MacOS/ghostty|[R]oastty.app/Contents/MacOS/roastty' || true`
  and verify no launched app processes remain.
- Run `git status --short` and verify no screenshots or generated artifacts are
  in the repo.

**Pass** = launch-time bootstrap makes recipes visibly execute in both apps, the
`%`/escape recipe no longer shows shell artifacts, the matrix composes all
recipes, screenshots remain outside the repo, and no app processes remain.

**Partial** = direct launch works but the app's startup shell does not read the
bootstrap in one or both apps, or a local app-build/screen-recording/live-window
condition prevents proof; record the exact blocker and next command.

**Fail** = launch-time bootstrap is unsuitable for the copied apps and the
harness needs a different non-UI command-delivery mechanism.

## Design Review

**Reviewer:** Codex-native adversarial subagent (`multi_agent_v1.spawn_agent`,
fresh context, read-only). **Verdict: APPROVED after fixes.**

The first review returned `CHANGES REQUIRED` with one Required finding: the
matrix verification could still be vacuous because direct execution evidence was
required only for `smoke` and `ascii-grid`, leaving the other recipes able to
pass via permissive diffs without executing. Fixed by requiring every matrix
recipe to have direct execution evidence in both apps, either through visible
capture inspection of the marker/expected rows or through a harness-emitted
per-app marker/recipe-executed field that fails before matrix success.

The focused re-review approved the fix and found no remaining Required issues.
