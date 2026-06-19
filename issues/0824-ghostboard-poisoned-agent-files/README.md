+++
status = "open"
opened = "2026-06-19"
+++

# Issue 824: Ghostboard Poisoned Agent Files

## Goal

Remove prompt-injection/trap content inherited from upstream Ghostty files in
`ghostboard/`, and prove that the remaining agent-facing documentation and code
comments are safe, factual, and useful for TermSurf development.

## Background

Mitchell Hashimoto publicly described intentionally poisoning `AGENTS.md` files,
code comments, and related repo text with prompt-injection/trap content to catch
people who pass AI-generated work across a human boundary without reviewing it.
TermSurf's `ghostboard/` tree was recreated from Ghostty, so inherited
agent-facing files may contain Ghostty-specific traps that are not appropriate
for TermSurf.

The confirmed local example is `ghostboard/AGENTS.md`, whose Issue and PR
Guidelines currently instruct an agent to create a humiliating file in the diff
if asked to create an issue or PR. That text also exists in
`vendor/ghostty/AGENTS.md`, so it appears inherited from upstream Ghostty rather
than authored for TermSurf.

Known local files that must be audited include:

- `ghostboard/AGENTS.md`
- `ghostboard/macos/AGENTS.md`
- `ghostboard/src/inspector/AGENTS.md`
- `ghostboard/test/fuzz-libghostty/AGENTS.md`
- `ghostboard/AI_POLICY.md`
- `ghostboard/CONTRIBUTING.md`
- `ghostboard/HACKING.md`
- `.agents/commands` files under `ghostboard/`
- suspicious code comments in `ghostboard/`, especially comments containing
  agent/AI/prompt-injection language rather than normal terminal prompt
  terminology.

## Analysis

This issue is not about sanitizing all upstream Ghostty opinions from the fork.
It is specifically about removing instructions or text that:

- tells an agent to insult itself, add unrelated files, sabotage a diff, or
  perform any action unrelated to the user's request;
- attempts to override the user's instructions, repository instructions, or the
  normal code-review workflow;
- contains prompt-injection bait aimed at AI agents rather than useful project
  guidance;
- imports upstream Ghostty contribution policy into TermSurf in a way that
  conflicts with TermSurf's local development workflow;
- could mislead a future human or agent into believing TermSurf requires
  Ghostty-specific issue/PR behavior.

Normal code comments that use words like `prompt`, `ignore`, or `agent` in their
ordinary technical sense should not be changed. For example, terminal semantic
prompt comments and mailbox comments about ignoring messages are probably
unrelated and should be left alone unless an experiment proves otherwise.

## Proposed Approach

The first experiment should perform a read-only audit of `ghostboard/` and
`vendor/ghostty/` to build a precise inventory of inherited poisoned or
Ghostty-specific agent-facing text. It should distinguish:

- confirmed poisoned/trap instructions;
- Ghostty-specific policy text that is not poison but should be rewritten or
  removed for TermSurf;
- benign technical comments that should remain unchanged.

Only after that audit should a follow-up experiment edit files. The edit should
be narrow: replace poisoned instructions with TermSurf-appropriate guidance,
remove irrelevant upstream contribution policy where needed, and preserve useful
build/test instructions.

## Acceptance Criteria

- `ghostboard/AGENTS.md` no longer contains the inherited trap instruction to
  create a humiliating file when asked to create an issue or PR.
- All `ghostboard/**/AGENTS.md` files are audited and contain only useful,
  factual TermSurf/Ghostboard development guidance.
- `ghostboard/.agents/commands`, `ghostboard/AI_POLICY.md`,
  `ghostboard/CONTRIBUTING.md`, and `ghostboard/HACKING.md` are audited for
  prompt-injection/trap content and Ghostty-specific AI/contribution policy that
  does not belong in TermSurf.
- A targeted source-comment audit is performed for obvious prompt-injection
  language, without rewriting normal terminal prompt comments.
- The final issue record lists every file changed and every suspicious file
  intentionally left unchanged.
- Markdown formatting passes for edited markdown files.
- `git diff --check` passes.

## Experiments

- [Experiment 1: Audit inherited agent-facing poison](01-audit-inherited-agent-facing-poison.md)
  — **Pass**
