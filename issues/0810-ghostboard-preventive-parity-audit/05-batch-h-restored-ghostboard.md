# Experiment 5: Batch H Restored Ghostboard Audit

## Description

Classify the newest historical audit batch from Experiment 4: issues
`0800`-`0809`. This batch covers Roastty architecture, libroastty completion,
GUI automation, parity with the Ghostty base commit, Ghostboard restoration, and
Issue 809's viewport-geometry proof. It is the highest-signal historical slice
for current restored Ghostboard behavior.

This experiment should read every Batch H issue and map each durable lesson to
current Ghostboard risk using the schema defined in Experiment 4. The output is
a classification table, not fixes.

This is an audit/documentation experiment only. It must not change application
code, generated code, historical issue files, closed issue files, scripts, or
test harnesses.

## Changes

Planned files:

- `issues/0810-ghostboard-preventive-parity-audit/05-batch-h-restored-ghostboard.md`
  - record this experiment design, design review, Batch H classification result,
    completion review, and conclusion;
  - classify every issue in Batch H using the Experiment 4 historical audit row
    schema.
- `issues/0810-ghostboard-preventive-parity-audit/README.md`
  - add Experiment 5 to the `## Experiments` index with status `Designed`, then
    update status after the result.

No application code, generated protobuf code, historical issue files, closed
issue files, scripts, or test harnesses should be edited.

## Verification

Design-gate pass criteria:

- The issue README links this experiment as `Designed`.
- A fresh-context adversarial design review approves the plan.
- The plan commit exists before implementation begins.

Implementation pass criteria:

- The result audits every Batch H issue exactly once:
  - `0800-roastty-architecture`
  - `0801-roastty-libghostty-rewrite`
  - `0802-libroastty-completion-and-mac-app`
  - `0803-roastty-debug-overlay`
  - `0804-roastty-gui-automation-readiness`
  - `0805-roastty-ghostty-parity`
  - `0806-roastty-input-latency`
  - `0807-restore-ghostboard-code`
  - `0808-recreate-ghostboard-from-ghostty-1-3-1`
  - `0809-ghostboard-viewport-geometry`
- The result uses the Experiment 4 row schema for every classification: source
  issue, batch, subsystem, durable lesson, current Ghostboard relevance,
  evidence paths, likelihood, risk or impact, recommended follow-up, and
  historical classification note.
- The result classifies each row as `Highly likely`, `Maybe`, or `No`, and
  explains the classification from issue evidence plus current code/test/doc
  evidence.
- The result treats Issue 803 as open historical evidence without trying to
  close or modify it.
- The result incorporates the highest-signal protocol findings already learned
  in this issue where relevant, especially likely missing Ghostboard handling
  for `CursorChanged` and `SetGuiActive`.
- The result distinguishes proven current coverage from historical success. A
  closed historical issue is not enough by itself to classify current Ghostboard
  risk as `No`.
- The result identifies the next audit slice after Batch H.
- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0810-ghostboard-preventive-parity-audit/README.md \
    issues/0810-ghostboard-preventive-parity-audit/05-batch-h-restored-ghostboard.md
  ```

- Whitespace check passes:

  ```bash
  git diff --check
  ```

- A fresh-context completion review approves the completed result before the
  result commit.
- All real completion-review findings are fixed and recorded in this experiment
  file.
- The result commit is made after completion-review approval and before any next
  experiment is designed.

Fail criteria:

- Any Batch H issue is omitted or classified more than once.
- The experiment edits historical issue files or application code.
- The result treats historical completion as proof of current Ghostboard parity
  without current evidence.
- The result fixes a finding instead of recording it for follow-up.
- The result expands into other historical batches before Batch H is concluded.

## Design Review

Fresh-context adversarial design review returned **APPROVED**.

Reviewer checks confirmed:

- The README links Experiment 5 as `Designed`.
- The design has `Description`, `Changes`, and `Verification`.
- Scope is audit-only and excludes application code, generated code, historical
  issue files, scripts, and test harnesses.
- Batch H is exactly `0800`-`0809`; every listed issue appears once, and
  expansion to other batches is a fail condition.
- Verification requires the Experiment 4 schema, current evidence, Issue 803 as
  open evidence, and carried-forward `CursorChanged` / `SetGuiActive` findings.
- `git diff --check` passed.
- The plan commit had not yet been made before review.

Findings: none.
