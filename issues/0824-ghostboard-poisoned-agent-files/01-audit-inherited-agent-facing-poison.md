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
