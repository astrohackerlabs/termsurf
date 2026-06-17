+++
status = "open"
opened = "2026-06-17"
+++

# Issue 810: Ghostboard Preventive Parity Audit

## Goal

Identify likely Ghostboard gaps before they are encountered during ordinary app
usage by auditing Wezboard protocol behavior and all historical TermSurf issues.

This issue is audit-only. It must not change application code. The output is a
ranked list of likely follow-up issues, not fixes.

## Background

Issue 809 proved Ghostboard viewport geometry across a full automated matrix.
The next risk is broader feature parity: Ghostboard may still be missing
protocol behaviors, user-visible features, cleanup paths, input paths, or
historical fixes that were already solved in Wezboard, Roamium, webtui, older
Ghostboard generations, or other TermSurf subprojects.

The purpose of this issue is to find those gaps analytically. Instead of waiting
for manual usage to reveal missing behavior, this audit will map known protocol
and historical behavior to the current Ghostboard implementation and classify
the likelihood that each item represents a real Ghostboard issue.

## Scope

In scope:

- Audit Wezboard as the current mature GUI reference.
- Audit `termsurf.proto` and infer the logical feature represented by each
  protobuf message or message group.
- Map each inferred feature to Wezboard behavior and current Ghostboard
  evidence.
- Audit all historical issues, including issues from older prototypes and
  subprojects that may not directly target Ghostboard.
- Classify each mapped item by likelihood:
  - `Highly likely`
  - `Maybe`
  - `No`
- Produce a prioritized list of follow-up candidates for deeper investigation or
  later fixing.

Out of scope:

- Application code changes.
- Fixing any discovered gaps.
- Closing historical issues or rewriting closed issue history.
- Treating an item as proven broken without evidence. This issue ranks
  likelihood; later focused issues can prove and fix specific findings.

## Audit Epics

### Epic 1: Wezboard Protocol and Feature Audit

This epic starts from the protocol and current mature GUI behavior.

For every relevant protobuf message or message group:

1. Identify the message name and fields.
2. Infer the logical feature it represents.
3. Find the Wezboard behavior that implements or depends on that feature.
4. Find the Ghostboard implementation evidence, if any.
5. Classify Ghostboard risk:
   - `Highly likely` if the feature appears absent or clearly incomplete.
   - `Maybe` if evidence is partial, ambiguous, or behavior depends on an
     untested path.
   - `No` if Ghostboard has convincing implementation or test evidence.
6. Record source references and the reason for the classification.

Example:

- Protocol signal: URL update messages.
- Inferred feature: webtui displays the updated browser URL after navigation.
- Reference behavior: Wezboard forwards or handles the URL update path.
- Ghostboard audit question: does Ghostboard forward the same message and does
  webtui receive/display it?
- Classification: `Highly likely`, `Maybe`, or `No`, depending on evidence.

### Epic 2: Historical Issue Audit

This epic treats the issue archive as a source of previously discovered product
requirements, edge cases, regressions, and implementation lessons.

For each historical issue:

1. Identify the subsystem and durable lesson.
2. Decide whether the issue can plausibly affect Ghostboard.
3. Map the historical behavior to current Ghostboard, Wezboard, Roamium, webtui,
   or protocol code as appropriate.
4. Classify Ghostboard risk as `Highly likely`, `Maybe`, or `No`.
5. Record evidence and recommended follow-up.

Historical issues that target unrelated subsystems should still be reviewed.
They may classify as `No`, but the audit should explain why the lesson does not
apply to Ghostboard.

## Output Format

Each audit item should use a durable table or structured list with these fields:

- Source: protobuf message, Wezboard code path, issue number, or document.
- Inferred feature or durable lesson.
- Reference behavior.
- Ghostboard evidence.
- Likelihood: `Highly likely`, `Maybe`, or `No`.
- Risk or impact.
- Recommended follow-up.

The final issue conclusion should include:

- all `Highly likely` findings, ordered by risk;
- all `Maybe` findings, grouped by subsystem;
- a summary of `No` findings sufficient to show they were actually audited;
- recommended next issue or issues for proving and fixing the highest-risk
  findings.

## Constraints

- No application code changes are allowed in this issue.
- Experiments should be audit slices, not fixes.
- Do not list every experiment upfront. Design one experiment at a time, and let
  each result inform the next audit slice.
- Closed historical issues are immutable; read them as evidence, but do not edit
  them.
- If an audit finding appears urgent, record it here and open or design a later
  focused issue before changing code.

## Acceptance Criteria

- The Wezboard/protobuf epic maps all TermSurf protocol message groups to
  inferred features and Ghostboard evidence.
- The historical issue epic reviews all historical issues and classifies their
  Ghostboard relevance.
- Every `Highly likely` and `Maybe` finding includes enough evidence for a later
  focused issue to verify or reject it.
- The final conclusion ranks follow-up candidates by likelihood and impact.
- No application code is changed while solving this issue.

## Experiments

- [Experiment 1: Protocol message inventory](01-protocol-message-inventory.md) —
  **Pass**
- [Experiment 2: Protocol feature parity](02-protocol-feature-parity.md) —
  **Pass**
- [Experiment 3: Direct browser paths](03-direct-browser-paths.md) — **Pass**
- [Experiment 4: Historical issue inventory](04-historical-issue-inventory.md) —
  **Pass**
- [Experiment 5: Batch H restored Ghostboard audit](05-batch-h-restored-ghostboard.md)
  — **Pass**
- [Experiment 6: Batch G PDF and browser API audit](06-batch-g-pdf-browser-api.md)
  — **Pass**
- [Experiment 7: Batch F Wezboard UX regression audit](07-batch-f-wezboard-ux-regressions.md)
  — **Pass**
- [Experiment 8: Batch E Wezboard implementation audit](08-batch-e-wezboard-implementation.md)
  — **Pass**
- [Experiment 9: Batch D direct browser and protocol audit](09-batch-d-direct-browser-protocol.md)
  — **Pass**
- [Experiment 10: Batch C product hardening audit](10-batch-c-product-hardening.md)
  — **Pass**
- [Experiment 11: Batch B feasibility and Ghostboard iterations audit](11-batch-b-feasibility-and-iterations.md)
  — **Designed**
