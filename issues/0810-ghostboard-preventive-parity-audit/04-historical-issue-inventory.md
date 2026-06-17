# Experiment 4: Historical Issue Inventory

## Description

Start the historical issue epic with a complete inventory of the issue archive.
Issue 810 must audit all historical issues, including issues from older
prototypes and subprojects that may not directly target Ghostboard. Before
classifying individual historical lessons, this experiment builds the coverage
map that later audit slices will use.

The result should enumerate every prior issue folder, define audit batches, and
record the schema later experiments must use when mapping historical lessons to
current Ghostboard risk.

This is an audit/documentation experiment only. It must not change application
code, generated code, historical issue files, closed issue files, scripts, or
test harnesses.

## Changes

Planned files:

- `issues/0810-ghostboard-preventive-parity-audit/04-historical-issue-inventory.md`
  - record this experiment design, design review, result, completion review, and
    conclusion;
  - record the full prior-issue inventory and batch plan after implementation.
- `issues/0810-ghostboard-preventive-parity-audit/README.md`
  - add Experiment 4 to the `## Experiments` index with status `Designed`, then
    update status after the result.

No application code, generated protobuf code, historical issue files, closed
issue files, scripts, or test harnesses should be edited.

## Verification

Design-gate pass criteria:

- The issue README links this experiment as `Designed`.
- A fresh-context adversarial design review approves the plan.
- The plan commit exists before implementation begins.

Implementation pass criteria:

- The result computes the authoritative prior-issue set from the filesystem:
  every `issues/[0-9][0-9][0-9][0-9]-*/` folder with number lower than `0810`.
- The result records:
  - total prior issue count;
  - first and last issue numbers;
  - any numbering gaps or duplicate issue numbers;
  - which prior issues are currently open versus closed, using
    `issues/README.md` as the index evidence;
  - any mismatch between filesystem issue folders, `issues/README.md` index
    entries, and README frontmatter status;
  - audit batches suitable for later experiments, with every prior issue
    assigned to exactly one batch.
- The result defines the historical audit row schema for later experiments:
  source issue, subsystem, durable lesson, current Ghostboard relevance,
  evidence paths, likelihood (`Highly likely`, `Maybe`, `No`), risk or impact,
  and recommended follow-up.
- The result must not classify every issue yet. It may include sample
  classifications only to validate the schema, but the full classification work
  belongs in later audit-slice experiments.
- The result identifies the next audit slice. Expected next slice: classify the
  newest/highest-signal Ghostboard/Roastty/Wezboard era batch first, unless the
  inventory reveals a better batch boundary.
- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0810-ghostboard-preventive-parity-audit/README.md \
    issues/0810-ghostboard-preventive-parity-audit/04-historical-issue-inventory.md
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

- Any prior issue folder is omitted from the inventory.
- Any prior issue is assigned to zero batches or more than one batch.
- The experiment edits historical issue files or application code.
- The result presents a partial inventory as complete.
- The result performs the full historical classification instead of creating the
  coverage map and batch plan.

## Design Review

Fresh-context adversarial design review returned **APPROVED**.

Optional finding:

- The status source could be more robust. The design used filesystem folders as
  the authoritative prior-issue set but used `issues/README.md` as status
  evidence; if the index is stale or omits a folder, the inventory should report
  that mismatch.

Fix:

- Added a requirement to report any mismatch between filesystem issue folders,
  `issues/README.md` index entries, and README frontmatter status.
