+++
status = "closed"
opened = "2026-06-17"
closed = "2026-06-17"
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
  — **Pass**
- [Experiment 12: Batch A early prototypes audit](12-batch-a-early-prototypes.md)
  — **Pass**

## Conclusion

Issue 810 completed the preventive Ghostboard parity audit without changing
application code. It mapped the protocol surface, audited direct Roamium paths,
inventoried all prior issue folders, and classified every historical issue batch
from `0001` through `0809`.

### Highly Likely Findings

1. **Cursor feedback is missing or incomplete in Ghostboard.** Roamium emits
   `CursorChanged`, and Wezboard has GUI-side cursor handling, but Ghostboard
   appears to name the message without dispatching or applying it. This was
   independently reinforced by the protocol audit, restored-Ghostboard audit,
   and historical mouse/cursor issues.
2. **GUI active/inactive signaling is missing or incomplete.** `SetGuiActive` is
   GUI-owned state, so webtui's direct Roamium socket cannot replace it.
   Wezboard sends app/window activation state; Ghostboard does not show an
   equivalent runtime path.
3. **The one-DevTools-per-tab guard is likely missing.** Historical Issues 686
   and 687 show duplicate DevTools sessions for one inspected tab can recreate a
   Chromium crash class. Current Ghostboard evidence validates that an inspected
   tab exists, but does not show a guard that rejects a second DevTools frontend
   for the same tab.
4. **Restored Ghostboard build/install/browser-discovery workflow is
   incomplete.** The audit repeatedly found risk around named/default browser
   launch, installed-vs-debug binary selection, socket discovery, app identity,
   and config paths.
5. **`HelloReply` is likely incomplete.** Ghostboard replies to `HelloRequest`,
   but the audit found likely missing homepage and browser-list configuration
   needed by `web`.

### Maybe Findings

- **Browser state and interruption flows:** loading/title/hover target/console,
  dialogs, HTTP auth, renderer crash recovery, color scheme, target blank,
  refresh/reload, copy-current-URL, and default white background have static or
  partial evidence but need focused Ghostboard runtime proof.
- **Input and focus:** keyboard matrix, Cmd/menu shortcuts, clipboard behavior,
  mode transitions, focus stealing, dimming/inactive feedback, caret visibility,
  mouse click/hover/scroll, double/triple-click, modifier clicks, drag
  selection, and mouse performance need targeted regression coverage.
- **Profile, tab, and process lifecycle:** multi-profile isolation,
  multi-pane/multi-tab routing, warm reconnect, server reuse, close/reopen
  behavior, stale process cleanup, DevTools target lookup, and profile display
  have credible code shape but need focused runtime matrices.
- **Packaging and identity:** app bundle naming, config locations, release
  packaging, normal launch environment, and debug-vs-installed binary selection
  need a hardening pass.
- **Performance methodology:** old CEF/XPC performance bugs do not directly
  apply to CALayerHost/Roamium, but the historical issues justify later
  lightweight performance and repeated-run smoke tests after functional parity.

### No Findings Summary

Most historical CEF, Electron, XPC, WebView, IOSurface copy, benchmark-harness,
website, and exploratory architecture rows do not indicate current
Ghostboard-owned bugs. They were still audited as historical evidence; the
current architecture generally supersedes them with socket/protobuf IPC,
Roamium, direct webtui browser sockets, and CALayerHost presentation.

### Recommended Follow-Up Issues

1. Fix and test GUI-owned protocol gaps: `CursorChanged` and `SetGuiActive`.
2. Restore and test the one-DevTools-per-tab guard, including duplicate launch
   rejection and close/reopen behavior.
3. Harden Ghostboard launch/config/install workflow, including named/default
   browser launch and `HelloReply` homepage/browser-list data.
4. Build a focused input regression matrix for keyboard, mouse, scroll,
   clipboard, focus, caret, drag selection, and mode transitions.
5. Build a focused lifecycle matrix for multi-profile, multi-pane, multi-tab,
   DevTools, reconnect, close/reopen, and process cleanup behavior.
6. Add browser-state walkthrough coverage for title/loading/hover/console,
   dialogs/auth/crash, color scheme, target blank, reload, copy URL, and default
   page background.

The audit produced a ranked list of likely follow-up work and satisfied the
acceptance criteria. Future issues should prove or reject these findings with
runtime tests before making fixes.
