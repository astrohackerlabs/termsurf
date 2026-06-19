# Experiment 2: Sanitize Ghostboard Agent-Facing Files

## Description

Remove or rewrite the poisoned and Ghostty-specific agent-facing content found
by Experiment 1, while preserving useful build/test guidance and leaving benign
technical source comments unchanged.

This experiment should be a documentation-only cleanup. It should not change
Ghostboard source behavior.

## Changes

Planned files:

- `ghostboard/AGENTS.md`
  - remove the inherited Issue and PR Guidelines trap that tells agents to
    create a humiliating file;
  - replace it with TermSurf-appropriate guidance to follow the user's request
    and the root TermSurf issue workflow;
  - preserve useful build, test, formatting, and directory guidance.
- `ghostboard/AI_POLICY.md`
  - remove the inherited Ghostty AI policy or replace it with a short
    TermSurf-specific note;
  - do not retain Ghostty denouncement/vouch language.
- `ghostboard/CONTRIBUTING.md`
  - remove or rewrite Ghostty-specific contribution policy, vouch process,
    denouncement system, issue/discussion routing, and upstream Ghostty links;
  - point contributors/developers at the root TermSurf workflow instead.
- `ghostboard/HACKING.md`
  - rewrite Ghostty-specific development prose to describe local Ghostboard
    development facts only;
  - preserve useful build/dependency notes that remain true for Ghostboard.
- `ghostboard/.github/VOUCHED.td`
  - remove the inherited Ghostty vouch/denouncement list if TermSurf does not
    use it, or replace it with a short non-policy placeholder if deletion would
    leave broken references.
- `ghostboard/README.md`
  - inspect the Ghostty contribution guidance hit and rewrite only if it points
    readers to Ghostty-specific pull-request policy.
- `ghostboard/.agents/commands/gh-issue`
  - either remove it or rewrite defaults/text so it no longer targets
    `ghostty-org/ghostty`.
- `ghostboard/.agents/commands/review-branch`
  - remove upstream-specific wording if present; keep only if the command is
    useful and TermSurf-appropriate.
- `ghostboard/macos/AGENTS.md`
  - optional minor wording cleanup if it still says "Ghostty library" in a way
    that is misleading for TermSurf.
- `issues/0824-ghostboard-poisoned-agent-files/README.md`
  - update Experiment 2 status.
- `issues/0824-ghostboard-poisoned-agent-files/02-sanitize-ghostboard-agent-facing-files.md`
  - record design review, changes, verification, completion review, result, and
    conclusion.

Explicit non-changes:

- Do not edit normal terminal prompt comments.
- Do not edit benign source comments from Experiment 1, such as mailbox "ignore
  all messages" comments.
- Do not touch `vendor/ghostty/`; it is upstream reference material.

## Verification

Pass criteria:

- `ghostboard/AGENTS.md` no longer contains:
  - `I am a sad, dumb little AI driver with no real skills`;
  - instructions to create unrelated files when asked to create an issue or PR.
- No `ghostboard/` file contains high-signal trap phrases:

  ```bash
  rg -n --hidden -S \
    "sad, dumb|AI driver|instant ban|human boundary|poison|prompt injection|ignore previous|developer message|system prompt" \
    ghostboard \
    -g '!zig-cache/**' \
    -g '!macos/build/**' \
    -g '!*.png' \
    -g '!*.jpg' \
    -g '!*.jpeg' \
    -g '!*.icns' \
    -g '!*.ico'
  ```

  Any remaining matches must be explicitly listed as benign and justified.

- Ghostty-specific AI/contribution policy is gone from `ghostboard/`:
  - no Ghostty vouch process remains;
  - no Ghostty denouncement policy remains;
  - no `ghostty-org/ghostty` default remains in `.agents/commands`;
  - no local doc tells TermSurf contributors to follow Ghostty's issue or PR
    process.
- A targeted Ghostty-policy search is run:

  ```bash
  rg -n --hidden -S \
    "vouch|denounc|ghostty-org/ghostty|Ghostty.*pull request|AI_POLICY|Bad AI drivers|VOUCHED" \
    ghostboard \
    -g '!zig-cache/**' \
    -g '!macos/build/**' \
    -g '!*.png' \
    -g '!*.jpg' \
    -g '!*.jpeg' \
    -g '!*.icns' \
    -g '!*.ico'
  ```

  Any remaining matches must be explicitly listed as benign and justified.

- All remaining `ghostboard/**/AGENTS.md` files are useful and factual for
  TermSurf/Ghostboard development.
- All changed markdown files are formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    ghostboard/AGENTS.md \
    ghostboard/AI_POLICY.md \
    ghostboard/CONTRIBUTING.md \
    ghostboard/HACKING.md \
    ghostboard/README.md \
    ghostboard/macos/AGENTS.md \
    issues/0824-ghostboard-poisoned-agent-files/README.md \
    issues/0824-ghostboard-poisoned-agent-files/02-sanitize-ghostboard-agent-facing-files.md
  ```

  Only include files that still exist after the edit.

- `git diff --check` passes.
- Design review is recorded and approved before implementation.
- The Experiment 2 plan commit exists before any `ghostboard/` edits begin.
- The experiment result lists:
  - every file changed;
  - every audited suspicious file intentionally left unchanged;
  - every remaining suspicious search hit and why it is benign.
- Completion review approves before the result commit.

Fail criteria:

- Any poisoned/trap instruction remains in `ghostboard/AGENTS.md`.
- The edit rewrites normal terminal prompt comments.
- The edit changes runtime source behavior.
- The result claims Issue 824 is solved while Ghostty-specific vouch,
  denouncement, or trap instructions remain in active Ghostboard docs.

## Design Review

Fresh-context adversarial design review initially returned **CHANGES REQUIRED**
with one required finding:

- the verification did not explicitly require the design review to be recorded
  and approved, and the Experiment 2 plan commit to exist before any
  `ghostboard/` edits begin.

The reviewer also raised one optional improvement:

- add a concrete targeted search for Ghostty-specific policy terms such as
  `vouch`, `denounc`, `ghostty-org/ghostty`, `Ghostty.*pull request`, and
  `AI_POLICY`.

The design was updated to add the missing plan-commit gate and targeted
Ghostty-policy search. Fresh-context re-review returned **APPROVED** with no
remaining required findings.
