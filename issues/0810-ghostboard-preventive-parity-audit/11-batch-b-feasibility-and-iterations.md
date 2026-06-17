# Experiment 11: Batch B Feasibility and Ghostboard Iterations Audit

## Description

Classify Batch B from Experiment 4: issue folders `0400`-`0515`. This batch
covers Chromium/Electron feasibility research and Ghostboard iteration work:
browser-engine choice, programming-language choice, terminal-emulator choice,
Swift/Rust/C++ ownership boundaries, early Chromium proofs of concept, profile
isolation, Electron patch experiments, XPC receivers, Ghostty-vs-WezTerm
selection, repo restructuring, ts5 rename work, Web TUI foundations, pink
texture and checkerboard overlay demos, Chromium frame delivery, multi-profile
scaling, vsync, Ctrl+Esc, mouse input, and drag behavior.

This experiment should read every Batch B issue folder and map each durable
lesson to current Ghostboard risk using the schema defined in Experiment 4. The
output is a classification table, not fixes.

Batch B has duplicate issue numbers in the historical archive: `0401` and `0410`
each appear in two distinct folders. Those folders must be audited as distinct
rows.

This is an audit/documentation experiment only. It must not change application
code, generated code, historical issue files, closed issue files, scripts, test
harnesses, screenshots, website assets, or build configuration.

## Changes

Planned files:

- `issues/0810-ghostboard-preventive-parity-audit/11-batch-b-feasibility-and-iterations.md`
  - record this experiment design, design review, Batch B classification result,
    completion review, and conclusion;
  - classify every issue folder in Batch B using the Experiment 4 historical
    audit row schema.
- `issues/0810-ghostboard-preventive-parity-audit/README.md`
  - add Experiment 11 to the `## Experiments` index with status `Designed`, then
    update status after the result.

No application code, generated protobuf code, historical issue files, closed
issue files, scripts, test harnesses, screenshots, website assets, or build
configuration should be edited.

## Verification

Design-gate pass criteria:

- The issue README links this experiment as `Designed`.
- A fresh-context adversarial design review approves the plan.
- The plan commit exists before implementation begins.

Implementation pass criteria:

- The result audits every Batch B issue folder exactly once:
  - `0400-a-new-hope`
  - `0401-chromium-feasibility`
  - `0401-programming-language`
  - `0402-wezterm-vs-alacritty`
  - `0403-swift-rust-cpp`
  - `0404-terminal-emulator`
  - `0405-architecture-comparison`
  - `0406-chromium`
  - `0407-chromium-poc`
  - `0408-two-profiles`
  - `0409-electron-patch`
  - `0410-partial-electron`
  - `0410-two-profiles-2`
  - `0411-two-profiles-3`
  - `0412-one-profile`
  - `0413-one-profile-2`
  - `0414-two-profiles-xpc`
  - `0415-swift-receiver`
  - `0416-rust-receiver`
  - `0417-ghostty-vs-wezterm`
  - `0418-repo-restructure`
  - `0500-rename`
  - `0501-two-profiles`
  - `0502-attach-delay`
  - `0503-one-two-three`
  - `0504-web-tui`
  - `0505-pink-texture`
  - `0506-xpc-gateway`
  - `0507-chromium`
  - `0508-checkerboard`
  - `0509-chromium`
  - `0510-two-profiles`
  - `0511-three-profiles`
  - `0512-vsync`
  - `0513-ctrl-esc`
  - `0514-mouse`
  - `0515-drag`
- The result uses the Experiment 4 row schema for every classification: source
  issue, batch, subsystem, durable lesson, current Ghostboard relevance,
  evidence paths, likelihood, risk or impact, recommended follow-up, and
  historical classification note.
- The result classifies each row as `Highly likely`, `Maybe`, or `No`, and
  explains the classification from issue evidence plus current code/test/doc
  evidence.
- The result treats all Batch B issues as closed historical evidence and does
  not modify or reinterpret their closure state.
- The result distinguishes feasibility research and abandoned Electron/XPC
  mechanisms from current socket/protobuf, Roamium, and restored Ghostboard
  evidence.
- The result distinguishes Ghostboard GUI-owned parity findings from Roamium,
  Chromium, webtui, website, packaging, docs, and historical prototype findings.
- The result carries forward relevant Issue 810 findings where Batch B overlaps
  current Ghostboard risk, especially browser-engine selection, profile
  isolation, browser startup delays, Web TUI discovery, overlay geometry,
  input/mouse/drag behavior, vsync/latency, and old XPC receiver lessons.
- The result explicitly handles duplicate issue numbers by folder slug, while
  still classifying each issue folder exactly once.
- The result groups or summarizes related repeated findings after the table, but
  the table itself must still contain one row per Batch B issue folder.
- The result identifies the next audit slice after Batch B.
- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0810-ghostboard-preventive-parity-audit/README.md \
    issues/0810-ghostboard-preventive-parity-audit/11-batch-b-feasibility-and-iterations.md
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

- Any Batch B issue folder is omitted or classified more than once.
- Duplicate issue numbers are collapsed into one row instead of being audited by
  folder slug.
- The experiment edits historical issue files, application code, generated code,
  scripts, tests, screenshots, website assets, or build configuration.
- The result treats obsolete Electron or XPC implementation details as current
  Ghostboard requirements without mapping them to the current socket/protobuf
  architecture.
- The result treats Roamium, Chromium, webtui, website, packaging, docs, or
  prototype behavior as a Ghostboard GUI bug without a direct current Ghostboard
  ownership path.

## Design Review

Tesla reviewed the design and approved it with no findings.

The review verified that the plan is audit-only, the README links Experiment 11
as `Designed`, the Batch B list has thirty-seven rows matching Experiment 4 and
the filesystem, duplicate numeric prefixes `0401` and `0410` are preserved as
separate folder rows, the Experiment 4 row schema is required, closed historical
issue immutability is preserved, obsolete Electron/XPC mechanisms must be mapped
to the current architecture, non-Ghostboard ownership boundaries are explicit,
and the fail criteria are clear.
