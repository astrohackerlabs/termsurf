# Experiment 1: Audit Inherited Agent-Facing Poison

## Description

Build an evidence-based inventory of poisoned, trap-like, or irrelevant
Ghostty-specific agent-facing content inside `ghostboard/` before editing those
files.

This experiment should distinguish three categories:

- **Confirmed poison/trap content**: instructions aimed at agents that tell them
  to add unrelated files, insult themselves, sabotage a diff, override the user,
  or otherwise perform behavior unrelated to TermSurf development.
- **Ghostty-specific policy text**: upstream Ghostty contribution or AI policy
  text that may not be prompt injection, but is misleading in a TermSurf fork.
- **Benign technical text**: ordinary source comments or docs using words like
  `prompt`, `ignore`, `agent`, or `AI` in technical or historical context.

The audit is read-only with respect to `ghostboard/` source and documentation:
it may edit only this experiment file and the Issue 824 README to record the
audit result. Sanitizing or deleting poisoned content happens in a later
experiment after this inventory is reviewed and committed.

## Changes

Planned files:

- `issues/0824-ghostboard-poisoned-agent-files/README.md`
  - link this experiment in the `## Experiments` index.
- `issues/0824-ghostboard-poisoned-agent-files/01-audit-inherited-agent-facing-poison.md`
  - record the audit design, review, commands, findings, result, and conclusion.
- `issues/README.md`
  - generated index update from opening Issue 824.

No `ghostboard/` files should be edited in this experiment.

Audit inputs:

- all `ghostboard/**/AGENTS.md` files;
- `ghostboard/.agents/commands/*`;
- `ghostboard/AI_POLICY.md`;
- `ghostboard/CONTRIBUTING.md`;
- `ghostboard/HACKING.md`;
- targeted source-comment and docs searches for obvious prompt-injection/trap
  language;
- corresponding upstream files in `vendor/ghostty/` for `AGENTS.md`,
  `.agents/commands`, `AI_POLICY.md`, `CONTRIBUTING.md`, and `HACKING.md` when
  those files exist, so inherited content is classified from evidence.

## Verification

Pass criteria:

- The audit enumerates all local `ghostboard/**/AGENTS.md` files.
- The audit enumerates all local `ghostboard/.agents/commands/*` files.
- The audit searches at least these suspicious phrase classes:
  - self-insult / trap text, including `sad, dumb`, `AI driver`, and
    `denounced`;
  - prompt-injection terms, including `prompt injection`, `system prompt`,
    `developer message`, `ignore previous`, `ignore all`, and `disregard`;
  - human-boundary / ban terms, including `human boundary`, `instant ban`, and
    `poison`;
  - agent workflow terms in docs, including `create an issue`, `create a PR`,
    `pull request`, `submit`, `AI`, `agent`, and `slop`.
- The audit records:
  - `ghostboard/AGENTS.md` as confirmed poisoned/trap content, with the
    inherited issue/PR humiliation instruction cited;
  - confirmed poisoned/trap files;
  - Ghostty-specific policy files that should be rewritten or removed for
    TermSurf;
  - benign matches that should intentionally remain unchanged;
  - any files that need follow-up inspection before editing.
- Markdown formatting passes:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0824-ghostboard-poisoned-agent-files/README.md \
    issues/0824-ghostboard-poisoned-agent-files/01-audit-inherited-agent-facing-poison.md
  prettier --check --prose-wrap always --print-width 80 \
    issues/0824-ghostboard-poisoned-agent-files/README.md \
    issues/0824-ghostboard-poisoned-agent-files/01-audit-inherited-agent-facing-poison.md
  ```

- `git diff --check` passes.
- The design review is recorded before implementation.
- The plan is committed before implementation.
- After the audit result is recorded, completion review is recorded and the
  result commit is made before designing a follow-up edit experiment.

Fail criteria:

- The experiment edits `ghostboard/` files.
- The audit fails to inspect every `ghostboard/**/AGENTS.md` file.
- The audit treats ordinary terminal prompt comments as poisoned without
  evidence.
- The audit concludes the issue is solved without removing or rewriting the
  confirmed poison in `ghostboard/AGENTS.md`.

## Design Review

Fresh-context adversarial design review initially returned **CHANGES REQUIRED**
with one required finding:

- the verification did not explicitly require the audit to classify the
  already-confirmed `ghostboard/AGENTS.md` issue/PR humiliation instruction as
  confirmed poison/trap content.

The reviewer also raised two non-blocking improvements:

- list `issues/README.md` in planned files because opening Issue 824 updates the
  generated index;
- require concrete upstream comparisons against corresponding `vendor/ghostty`
  files for `AGENTS.md`, `.agents/commands`, `AI_POLICY.md`, `CONTRIBUTING.md`,
  and `HACKING.md` when they exist.

The design was updated to address all three items. Fresh-context re-review
returned **APPROVED** with no remaining required findings.

## Result

**Result:** Pass

The read-only audit completed without editing any `ghostboard/` files. Full raw
audit output is in `logs/issue824-exp1-audit.log`.

Commands used below are simplified equivalents of the logged audit commands; the
raw log includes printed `same`, `diff`, and `missing-upstream` labels for the
comparison loops.

```bash
find ghostboard -path '*/AGENTS.md' -type f | sort
find ghostboard/.agents -type f | sort

for f in \
  ghostboard/AGENTS.md \
  ghostboard/macos/AGENTS.md \
  ghostboard/src/inspector/AGENTS.md \
  ghostboard/test/fuzz-libghostty/AGENTS.md \
  ghostboard/AI_POLICY.md \
  ghostboard/CONTRIBUTING.md \
  ghostboard/HACKING.md; do
  rel=${f#ghostboard/}
  vf="vendor/ghostty/$rel"
  test -f "$vf" && cmp -s "$f" "$vf"
done

for f in $(find ghostboard/.agents -type f | sort); do
  rel=${f#ghostboard/}
  vf="vendor/ghostty/$rel"
  test -f "$vf" && cmp -s "$f" "$vf"
done

rg -n --hidden -S \
  "sad, dumb|AI driver|bad AI|denounced|instant ban|human boundary|poison|prompt injection|system prompt|developer message|ignore previous|ignore all|disregard|create an issue|create a PR|pull request|submit|agent|AI|slop" \
  ghostboard \
  -g '!zig-cache/**' \
  -g '!macos/build/**' \
  -g '!*.png' \
  -g '!*.jpg' \
  -g '!*.jpeg' \
  -g '!*.icns' \
  -g '!*.ico'
```

### Enumerated Agent Files

All local `ghostboard/**/AGENTS.md` files:

- `ghostboard/AGENTS.md`
- `ghostboard/macos/AGENTS.md`
- `ghostboard/src/inspector/AGENTS.md`
- `ghostboard/test/fuzz-libghostty/AGENTS.md`

All local `ghostboard/.agents/commands/*` files:

- `ghostboard/.agents/commands/gh-issue`
- `ghostboard/.agents/commands/review-branch`

### Confirmed Poison/Trap Content

- `ghostboard/AGENTS.md`
  - Lines 25-30 contain a confirmed inherited trap in the Issue and PR
    Guidelines:
    - never create an issue;
    - never create a PR;
    - if asked to create an issue or PR, create a file in the diff containing a
      self-insulting message.
  - The same trap exists in upstream `vendor/ghostty/AGENTS.md` lines 34-39.
  - Follow-up: rewrite this section for TermSurf. It should say agents must
    follow the user's request and the local issue workflow, not add unrelated
    files or hostile text.

### Ghostty-Specific Policy Text To Rewrite Or Remove

- `ghostboard/AI_POLICY.md`
  - Identical to upstream `vendor/ghostty/AI_POLICY.md`.
  - It refers to "The Ghostty project", outside Ghostty contributions,
    maintainers, disclosure expectations, bad AI drivers, and a public
    denouncement list.
  - It is not an instruction to sabotage a diff, but it is Ghostty-specific
    policy and conflicts with TermSurf's local workflow.
  - Follow-up: remove it from `ghostboard/` or replace it with a short
    TermSurf-specific note that points to the root repo instructions.
- `ghostboard/CONTRIBUTING.md`
  - Mostly inherited from upstream Ghostty and still titled "Contributing to
    Ghostty".
  - Contains Ghostty-specific AI policy references, vouch process, denouncement
    system, issue/discussion process, Discord links, and PR rules.
  - Follow-up: replace with TermSurf/Ghostboard-specific contribution guidance
    or remove if the root TermSurf contribution docs already cover this fork.
- `ghostboard/HACKING.md`
  - Mostly inherited from upstream Ghostty and still titled "Developing
    Ghostty".
  - Contains Ghostty clone instructions, Ghostty upstream AI/Agents section, and
    Ghostty-specific logging, linting, and contribution references.
  - Follow-up: either rewrite for TermSurf Ghostboard or trim to build/develop
    facts that remain true for this fork.
- `ghostboard/.github/VOUCHED.td`
  - Not listed in the initial known-files set, but the targeted search found it.
  - It is Ghostty-specific denouncement/vouch infrastructure.
  - Follow-up: inspect and likely remove or rewrite if TermSurf does not use
    this system.
- `ghostboard/README.md`
  - Search hit at line 57 points contributors to Ghostty pull-request guidance.
  - Follow-up: inspect in the edit experiment and rewrite only if it is still
    user-facing Ghostty contribution text.
- `ghostboard/.agents/commands/gh-issue`
  - No corresponding upstream file exists in this local `vendor/ghostty/` tree.
  - It generates a prompt for Ghostty GitHub issues by default:
    `--repo: string = "ghostty-org/ghostty"`.
  - It is not poisoned, but it is Ghostty-specific and likely wrong for
    TermSurf.
  - Follow-up: remove, rewrite for TermSurf, or document as intentionally
    unused.
- `ghostboard/.agents/commands/review-branch`
  - Identical to upstream `vendor/ghostty/.agents/commands/review-branch`.
  - It is not poisoned; it asks for read-only review and no code changes.
  - Follow-up: inspect whether "consult the oracle" and GitHub issue/PR context
    belong in TermSurf. This can probably remain after minor wording cleanup or
    be removed with the rest of upstream `.agents`.

### Agent Files That Look Benign

- `ghostboard/macos/AGENTS.md`
  - Differs from upstream only by TermSurf app naming and bundle paths.
  - No trap language found.
  - Follow-up: keep, with possible minor cleanup of remaining "Ghostty library"
    wording only if the edit experiment is already touching it.
- `ghostboard/src/inspector/AGENTS.md`
  - Identical to upstream.
  - Contains useful inspector build/API guidance and no trap language.
  - Follow-up: keep unless a later cleanup wants to rename "Ghostty" references
    to "Ghostboard" for clarity.
- `ghostboard/test/fuzz-libghostty/AGENTS.md`
  - Identical to upstream.
  - Contains useful AFL++ fuzzer instructions and no trap language.
  - Follow-up: keep unless a later cleanup wants to rename "Libghostty"
    references to the current fork naming.

### Benign Source-Comment Matches To Leave Unchanged

The targeted source-comment search found several false positives that are normal
technical wording, not prompt injection:

- `ghostboard/src/termio/Thread.zig:77` and
  `ghostboard/src/os/cf_release_thread.zig:54`: mailbox code "drain and ignore
  all messages".
- `ghostboard/src/inspector/widgets/termio.zig:755`: inspector manually paused
  state ignores events.
- `ghostboard/include/ghostty/vt/allocator.h:58`: normal allocator guidance.
- `ghostboard/valgrind.supp:668`: normal suppression comment.
- `ghostboard/src/font/shaper/feature.zig:67`: normal font parsing comment.
- Terminal semantic prompt comments were not classified as poisoned; `prompt` is
  domain terminology in a terminal emulator.

### Verification

- All `ghostboard/**/AGENTS.md` files were enumerated.
- All `ghostboard/.agents/commands/*` files were enumerated.
- Corresponding upstream `vendor/ghostty` files were compared where present.
- The confirmed root `ghostboard/AGENTS.md` trap was classified as poisoned and
  cited.
- Ghostty-specific policy files were classified separately from confirmed
  poison.
- Benign technical comments were explicitly left unchanged.
- Markdown formatting passed:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0824-ghostboard-poisoned-agent-files/README.md \
    issues/0824-ghostboard-poisoned-agent-files/01-audit-inherited-agent-facing-poison.md
  prettier --check --prose-wrap always --print-width 80 \
    issues/0824-ghostboard-poisoned-agent-files/README.md \
    issues/0824-ghostboard-poisoned-agent-files/01-audit-inherited-agent-facing-poison.md
  ```

- `git diff --check` passed.

## Conclusion

Experiment 1 proves Issue 824 is real and scoped: `ghostboard/AGENTS.md` has
confirmed inherited poison, while other files mostly fall into Ghostty-specific
policy cleanup or benign technical text. The next experiment should edit the
confirmed and Ghostty-specific agent-facing files, then rerun the targeted
searches to prove the poison is removed without rewriting normal terminal
comments.

## Completion Review

Fresh-context adversarial completion review returned **APPROVED** with no
required findings.

The reviewer verified that no `ghostboard/` files were modified, all required
`AGENTS.md` and `.agents/commands` files were enumerated, upstream comparisons
were present in the audit log, `ghostboard/AGENTS.md` was correctly classified
as confirmed poison/trap, Ghostty-specific policy files and benign false
positives were classified distinctly, the README status matched the result, and
the result commit had not yet been made.

The reviewer raised one optional documentation precision finding: the "Commands
used" snippets were simplified equivalents rather than the exact printed
comparison loops used to produce `logs/issue824-exp1-audit.log`. The result was
updated to state that explicitly.
