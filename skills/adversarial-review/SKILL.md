---
name: adversarial-review
description: "Run an in-session adversarial review of TermSurf work using the
  `adversarial-reviewer` subagent (fresh context, read-only, tries to reject on
  evidence). Use at an experiment's design gate and result gate, or whenever the
  user asks for an adversarial / skeptical / red-team review without calling an
  external reviewer CLI."
---

# Adversarial Review

Run a fresh-context, read-only adversarial review **inside this Claude session**
by delegating to the `adversarial-reviewer` subagent (defined in
`.claude/agents/adversarial-reviewer.md`). No subprocess, no session id, no logs
to manage — you spawn the subagent with the `Agent` tool and it returns its
verdict and findings to you.

This is the in-session counterpart of the `codex-review` and `claude-review`
skills, which shell out to a separate `codex exec` / `claude -p` process. Use
this skill when you want the review to run in the same session; use
`codex-review` when you specifically want a **different model's** independent
read (cross-model diversity — see "Self-review caveat" below).

## When this skill applies

- The user asks for an "adversarial review", "skeptical review", "red team",
  "try to break this", or similar.
- An experiment reaches its **design gate** (after the design is written, before
  implementation) or its **result gate** (after implementation + result
  recording, before the result commit). These are the two required AI review
  gates in `CLAUDE.md`'s experiment flow.
- A change is large, risky, or touches Chromium, protocol boundaries, browser
  process behavior, input/rendering, persistent state, or `unsafe` Rust.
- Before closing an issue after a complex series of experiments.

## How it works

The `adversarial-reviewer` subagent runs in **its own fresh context window** — it
does **not** see this conversation. It receives only what you put in the spawn
prompt plus whatever it reads itself with its read-only tools (Read, Grep, Glob,
Bash). It is prompted to try to reject the work on evidence, to verify claimed
gate results independently, and to return a structured verdict.

Because it starts blind, **you must hand it the artifacts** — point it at the
files; do not paraphrase them. Give it:

- the experiment file (`issues/<n>/NN-*.md`);
- the relevant diff (tell it the exact `git diff` / `git diff --staged` /
  `git show <ref>` command to run, or the changed file paths);
- the source files it should scrutinize;
- the upstream source to compare against, for ports
  (e.g. `vendor/ghostty/src/...`);
- `CLAUDE.md` and the issue `README.md` as the workflow contract;
- any command output whose truth matters (test counts, build logs).

## Invocation

Spawn the subagent with the `Agent` tool, `subagent_type: "adversarial-reviewer"`.
Put the review task and the artifact pointers in the prompt. Example:

> Use the **adversarial-reviewer** subagent to review the Experiment 620 design.
> Read `issues/0801-roastty-libghostty-rewrite/620-*.md`, `CLAUDE.md`, and the
> upstream `vendor/ghostty/src/config/url.zig`. Try to reject the design; return
> your verdict and findings.

The subagent's final message — its `VERDICT` plus findings — comes back to you as
the tool result. It is not shown to the user automatically; relay the high-signal
parts.

### Design-gate prompt template

```text
Review this TermSurf experiment DESIGN with fresh context. Do not edit anything.

Read:
- the experiment file: issues/<n>/NN-<slug>.md
- the workflow contract: CLAUDE.md and issues/<n>/README.md
- the upstream being ported (if any): vendor/ghostty/src/<path>

Try to reject this design. Check:
- the issue README links this experiment with status Designed;
- the experiment has Description, Changes, and Verification;
- scope is narrow enough for one experiment, and matches exactly what was asked;
- the technical plan is correct and faithful to upstream;
- verification has concrete pass/fail criteria that would actually prove the goal;
- required hygiene checks are present (fmt, build-no-warnings, tests, no-ghostty
  grep, git diff --check).

Return VERDICT (APPROVED | CHANGES REQUIRED) then findings (Required/Optional/Nit)
with file:line, evidence, and a concrete fix. Approve only if no Required remain.
```

### Result-gate prompt template

```text
Review this COMPLETED TermSurf experiment with fresh context. Do not edit anything.

Read:
- the experiment file (Description, Changes, Verification, Result): issues/<n>/NN-<slug>.md
- the implementation diff: run `git diff <plan-commit>..HEAD -- <paths>` (or the
  working tree if not yet committed)
- the changed source and the upstream it ports: vendor/ghostty/src/<path>
- the workflow contract: CLAUDE.md

Try to reject this result. Check:
- the implementation matches the approved scope — no unrequested changes;
- it is correct and faithful to upstream; find the specific divergence if any;
- the tests actually prove the claim (not vacuous, cover the interesting cases);
- independently verify the claimed gate results where feasible: run
  `cargo build -p <crate>`, `cargo test -p <crate>`, `cargo fmt -p <crate> -- --check`,
  and the no-ghostty grep; report any mismatch with the stated numbers;
- the experiment file has Result and Conclusion, and the README status matches;
- the result commit has NOT been made before this review.

Return VERDICT then findings (Required/Optional/Nit) with file:line, evidence, and
a concrete fix. Approve only if no Required remain.
```

### Re-review prompt template

```text
Re-review ONLY the fixes for your prior findings, with fresh context. Do not edit.
For each prior finding, confirm whether it is now resolved, citing the new
file:line. Report any new Required finding the fix introduced. Approve only if no
Required remain.
```

## After the review: lead-agent judgment

You (the implementing agent) stay responsible for the outcome. The review is
input, not a verdict you must obey blindly.

1. **Accept** findings that are real correctness, fidelity, verification, scope,
   or workflow issues. Fix them before proceeding.
2. **Reject** false positives explicitly, with a one-line reason — do not silently
   ignore a finding.
3. **Re-review** after non-trivial fixes (use the re-review template) until no
   Required findings remain.
4. **Record** the review in the experiment file: that it was the
   `adversarial-reviewer` subagent with fresh context, the findings, the fixes,
   and the final verdict — the same way Codex reviews are recorded today.
5. Respect the commit gates: do not implement after a design review until the
   plan commit exists; do not design the next experiment after a result review
   until the result commit exists.

## Self-review caveat (read this)

This subagent is the **same model family** as the implementer (Opus reviewing
Opus). That is convenient and fast, but a same-model reviewer shares blind spots
and can drift toward agreement. The subagent's design fights this with fresh
context, a hard "try to reject on evidence" mandate, read-only tools, independent
re-verification of claimed results, and a no-approval-with-Required-findings gate
— but it does not fully replace a genuinely different model.

Therefore:

- For routine gates, the `adversarial-reviewer` subagent is a reasonable default.
- For **high-risk** work (Chromium, protocol changes, tricky `unsafe`, anything
  that already failed once), prefer a **cross-model** check via `codex-review`,
  or run both and reconcile.
- You can raise rigor by spawning the subagent **two or three times in parallel**
  with different emphases (e.g. one on correctness, one on upstream fidelity, one
  on verification quality) and treating any Required finding from any pass as
  blocking. This breaks single-perspective blind spots without leaving the
  session.

## Notes

- The subagent is **read-only by discipline**, not by tool sandbox: it has `Bash`
  so it can run `git diff` and re-run builds/tests to verify claims. Its system
  prompt forbids any mutating command. If you want a hard guarantee that it
  cannot touch the tree, edit `.claude/agents/adversarial-reviewer.md` to drop
  `Bash` from `tools` (you lose independent test/build verification in exchange).
- `model` is set to `opus` in the agent file. Switch it to `sonnet` for cheaper,
  faster gates, or `inherit` to track the session model.
- Subagents are loaded at session start. After creating or editing the agent
  file directly on disk, restart the session (or use `/agents`) for it to load.
