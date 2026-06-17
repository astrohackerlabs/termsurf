# Experiment 6: Batch G PDF and Browser API Audit

## Description

Classify Batch G from Experiment 4: issues `0789`-`0799`. This batch covers PDF
viewer infrastructure, PDF viewer interactions, native print, PDF workflow
coverage, advanced PDF features, Chromium/app-shell embedding research, and
browser API automation triage.

This experiment should read every Batch G issue and map each durable lesson to
current Ghostboard risk using the schema defined in Experiment 4. The output is
a classification table, not fixes.

This is an audit/documentation experiment only. It must not change application
code, generated code, historical issue files, closed issue files, scripts, test
harnesses, or PDF assets.

## Changes

Planned files:

- `issues/0810-ghostboard-preventive-parity-audit/06-batch-g-pdf-browser-api.md`
  - record this experiment design, design review, Batch G classification result,
    completion review, and conclusion;
  - classify every issue in Batch G using the Experiment 4 historical audit row
    schema.
- `issues/0810-ghostboard-preventive-parity-audit/README.md`
  - add Experiment 6 to the `## Experiments` index with status `Designed`, then
    update status after the result.

No application code, generated protobuf code, historical issue files, closed
issue files, scripts, test harnesses, or PDF assets should be edited.

## Verification

Design-gate pass criteria:

- The issue README links this experiment as `Designed`.
- A fresh-context adversarial design review approves the plan.
- The plan commit exists before implementation begins.

Implementation pass criteria:

- The result audits every Batch G issue exactly once:
  - `0789-electron-style-pdf-viewer`
  - `0790-pdf-viewer-mojo-bindings`
  - `0791-app-shell-foundation`
  - `0792-pdf-support`
  - `0793-pdf-iframe-size`
  - `0794-pdf-viewer-interactions`
  - `0795-pdf-native-print`
  - `0796-pdf-implementation-audit`
  - `0797-pdf-core-workflow-coverage`
  - `0798-pdf-advanced-features`
  - `0799-browser-api-automation-triage`
- The result uses the Experiment 4 row schema for every classification: source
  issue, batch, subsystem, durable lesson, current Ghostboard relevance,
  evidence paths, likelihood, risk or impact, recommended follow-up, and
  historical classification note.
- The result classifies each row as `Highly likely`, `Maybe`, or `No`, and
  explains the classification from issue evidence plus current code/test/doc
  evidence.
- The result treats open issues `0795`, `0797`, and `0798` as open historical
  evidence without trying to close or modify them.
- The result distinguishes PDF/Roamium/browser-engine risk from Ghostboard GUI
  risk. A PDF feature can be important without being a Ghostboard bug if it is
  owned by Roamium, webtui, Chromium, or PDF extension code.
- The result distinguishes current Ghostboard ordinary browsing evidence from
  unproven PDF-specific and browser-API workflows.
- The result carries forward relevant Issue 810 findings where they affect Batch
  G, especially GUI-responsibility messages that direct webtui/Roamium paths
  cannot cover.
- The result identifies the next audit slice after Batch G.
- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0810-ghostboard-preventive-parity-audit/README.md \
    issues/0810-ghostboard-preventive-parity-audit/06-batch-g-pdf-browser-api.md
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

- Any Batch G issue is omitted or classified more than once.
- The experiment edits historical issue files, application code, scripts, tests,
  or PDF assets.
- The result treats open PDF issues as closed or solved.
- The result labels an engine-owned PDF gap as a Ghostboard GUI gap without
  evidence of Ghostboard involvement.
- The result expands into other historical batches before Batch G is concluded.

## Design Review

Fresh-context adversarial design review returned **APPROVED**.

Reviewer checks confirmed:

- The README links Experiment 6 as `Designed`.
- The design has `Description`, `Changes`, and `Verification`.
- Scope is audit-only and excludes code, generated code, historical issue files,
  scripts, test harnesses, and PDF assets.
- Batch G is exactly `0789`-`0799`, each listed once.
- Verification requires the Experiment 4 schema, open issue handling for `0795`,
  `0797`, and `0798`, PDF/Roamium/browser-engine versus Ghostboard GUI
  separation, and carried-forward GUI-responsibility findings.
- `git diff --check` passed.
- The plan commit had not yet been made before review.

Findings: none.

## Result

**Result:** Pass

Batch G was audited as the PDF and browser-API automation slice. The
classification unit is each historical issue folder, so the table below has
exactly eleven rows: one for every issue from `0789` through `0799`.

### Classification Table

| Source issue                         | Batch | Subsystem                        | Durable lesson                                                                                                                                                                         | Current Ghostboard relevance                                                                                                                                                                                                                                                     | Evidence paths                                                                                                                                                                                                                                                                                                                                                                                                                                   | Likelihood | Risk or impact                                                                                                                                                                  | Recommended follow-up                                                                                                                                                                                              | Historical classification note                                                                                                                                  |
| ------------------------------------ | ----- | -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `0789-electron-style-pdf-viewer`     | G     | Roamium / Chromium PDF embedding | Inline PDF viewer work needs narrow Electron-style embedder layers, and `chrome://resources` requires both a browser URL-loader factory and renderer origin-access grant.              | This was Roamium/Chromium engine plumbing, not Ghostboard GUI behavior. Later issues superseded this blocked state by continuing the PDF stack.                                                                                                                                  | Issue 789 conclusion records the stream handoff and two-layer WebUI resource lesson, then stops at missing Mojo bindings in `issues/0789-electron-style-pdf-viewer/README.md:3830`; Issue 792 later proves inline PDF rendering in Roamium in `issues/0792-pdf-support/README.md:247`.                                                                                                                                                           | `No`       | Low Ghostboard risk. Regressions here would be Roamium/Chromium PDF regressions, not missing Ghostboard protocol or GUI behavior.                                               | No Ghostboard follow-up. Keep the WebUI resource lesson as Roamium/Chromium reference material.                                                                                                                    | Classified `No` because the historical failure was engine-owned and later resolved by the PDF stack, not by GUI changes.                                        |
| `0790-pdf-viewer-mojo-bindings`      | G     | Roamium / Chromium PDF embedding | A partial PDF shim is insufficient; completing inline PDF required canonical extensions, guest-view, and PDF stream-manager infrastructure.                                            | This is still engine/embedder architecture. It informs why PDF behavior should be audited at the Roamium layer first, but it does not imply a current Ghostboard bug.                                                                                                            | Issue 790 conclusion records the canonical-stack finding and restored non-PDF baseline in `issues/0790-pdf-viewer-mojo-bindings/README.md:1246`; Issue 791 rejects full app_shell rebasing in `issues/0791-app-shell-foundation/README.md:328`; Issue 792 later closes working inline PDF.                                                                                                                                                       | `No`       | Low Ghostboard risk. The danger is choosing the wrong Chromium foundation, but current Ghostboard only hosts Roamium output.                                                    | No Ghostboard follow-up. Keep using Issues 790-792 as the PDF engine lineage if PDF work resumes.                                                                                                                  | Classified `No` because Ghostboard is not responsible for Mojo JS bindings or PDF renderer process classification.                                              |
| `0791-app-shell-foundation`          | G     | Chromium embedder architecture   | app_shell has useful PDF wiring, but its single-window model conflicts with TermSurf's per-tab CALayerHost architecture; stay on content_shell and add separable extension layers.     | Current Ghostboard is a GUI host for Roamium and benefits from the decision not to impose app_shell's window model. There is no current evidence that Ghostboard should change because of this issue.                                                                            | Issue 791 conclusion says app_shell is not the right foundation and recommends separable extension layers in `issues/0791-app-shell-foundation/README.md:328`; Ghostboard's active protocol host handles overlay lifecycle and native presentation separately in `ghostboard/src/apprt/termsurf.zig:536`.                                                                                                                                        | `No`       | Low Ghostboard risk. Reopening app_shell would risk mismatching TermSurf's multi-pane overlay architecture.                                                                     | No Ghostboard follow-up. Preserve the architecture decision unless a future Chromium upgrade invalidates the content_shell-based extension layer.                                                                  | Classified `No` because the issue is an embedder-architecture decision already aligned with current Ghostboard.                                                 |
| `0792-pdf-support`                   | G     | PDF rendering / Roamium          | Working inline PDF rendering requires the full TermSurf extension/PDF layer and localized resource plumbing; basic PDF viewing is separate from print and polish.                      | Roamium now renders PDFs, but this issue's proof predates restored Ghostboard. Current Ghostboard ordinary browsing and geometry are proven, yet there is no Batch H or Issue 810 runtime proof that a PDF URL renders inside restored Ghostboard specifically.                  | Issue 792 proves recognizable PDF content in Roamium in `issues/0792-pdf-support/README.md:247`; Issue 809 proves current Ghostboard overlay geometry and browser input generally in `issues/0809-ghostboard-viewport-geometry/README.md:231`; current Ghostboard handles overlay lifecycle in `ghostboard/src/apprt/termsurf.zig:536`.                                                                                                          | `Maybe`    | Medium risk as a coverage gap, not a proven bug. A user could hit a restored-Ghostboard-specific PDF display regression that Roamium-only proof would not catch.                | Add a focused Ghostboard PDF smoke that opens a known PDF through the current debug `web` + Roamium path and checks screenshot/log evidence.                                                                       | Classified `Maybe` because the feature is engine-owned but needs one restored-Ghostboard integration proof.                                                     |
| `0793-pdf-iframe-size`               | G     | PDF layout / extension resources | PDF wrapper layout depends on preserving `pdf_embedder.css` as a web-accessible resource; otherwise the viewer iframe falls back to default size.                                      | The root cause is Roamium/Chromium extension-resource metadata. Ghostboard viewport geometry is now well covered, so there is no specific reason to suspect the GUI would recreate the tiny-iframe bug.                                                                          | Issue 793 proves local and remote PDFs render full-pane after preserving `pdf_embedder.css` in `issues/0793-pdf-iframe-size/README.md:102`; Issue 809 proves Ghostboard browser overlays fill/follow panes in `issues/0809-ghostboard-viewport-geometry/README.md:231`.                                                                                                                                                                          | `No`       | Low Ghostboard risk. If it regresses, the likely owner is PDF extension resource loading in Chromium/Roamium.                                                                   | Cover this indirectly in the focused Ghostboard PDF smoke from Issue 792 by checking that the PDF viewer fills the visible browser viewport.                                                                       | Classified `No` for Ghostboard because both known failure mechanism and fix are engine-side.                                                                    |
| `0794-pdf-viewer-interactions`       | G     | PDF interactions / input routing | Usable PDF viewing requires scroll, resize, mouse, keyboard, selection/copy, toolbar controls, save/download, title propagation, and local-file parity; native print remains separate. | Current Ghostboard has general keyboard, mouse, scroll, resize, DevTools, and navigation evidence, but PDF-specific interaction behavior has not been replayed under restored Ghostboard. Browser-title/loading/console paths remain `Maybe` in Experiment 3.                    | Issue 794 proves PDF interactions and defers print in `issues/0794-pdf-viewer-interactions/README.md:290`; Issue 809 proves general Ghostboard input/geometry in `issues/0809-ghostboard-viewport-geometry/README.md:231`; Experiment 3 keeps broader browser-state rows as `Maybe` in `issues/0810-ghostboard-preventive-parity-audit/03-direct-browser-paths.md:141`.                                                                          | `Maybe`    | Medium risk. The likely code owner is still Roamium/Chromium for PDF internals, but Ghostboard should prove that its input and geometry bridge does not break PDF workflows.    | Extend the focused Ghostboard PDF smoke into a small PDF interaction sweep: scroll, click toolbar, keyboard page navigation, text selection/copy, and title propagation.                                           | Classified `Maybe` because Ghostboard has adjacent general proof but no PDF-specific restored-Ghostboard proof.                                                 |
| `0795-pdf-native-print`              | G     | PDF native print / browser UI    | The PDF toolbar print path reaches Chromium's PDF/plugin side, but native print UI still needs browser-side printing infrastructure.                                                   | This issue is still open and is explicitly Roamium/content-shell browser infrastructure, not Ghostboard GUI protocol. Ghostboard could host the resulting native UI, but the known failure is before that boundary.                                                              | Issue 795 remains open and names missing `PrintManagerHost` / `PrintViewManager` browser-side infrastructure in `issues/0795-pdf-native-print/README.md:1`; Issue 794 says native print is intentionally not solved in `issues/0794-pdf-viewer-interactions/README.md:298`.                                                                                                                                                                      | `No`       | High product risk for PDF print, but low evidence that Ghostboard is the owner. Users cannot rely on PDF native print until Issue 795 is solved.                                | Do not create a Ghostboard fix from this audit. Keep Issue 795 open as the Roamium/Chromium owner; after it works in Roamium, add a Ghostboard-hosted print smoke if native window focus matters.                  | Classified `No` for Ghostboard while preserving the open product gap in Issue 795.                                                                              |
| `0796-pdf-implementation-audit`      | G     | PDF organization/security/audit  | The PDF implementation needs organized helpers, hardened extension boundaries, explicit invariants, and honest non-print scope boundaries.                                             | This issue closed the PDF audit tracks and explicitly moved remaining work to Issues 795, 797, and 798. Its direct lessons are engine/security owned; current Ghostboard has no identified GUI gap from this audit itself.                                                       | Issue 796 conclusion records organization, security hardening, coverage boundaries, and follow-up issues in `issues/0796-pdf-implementation-audit/README.md:201`.                                                                                                                                                                                                                                                                                | `No`       | Low Ghostboard risk. The main value is preventing PDF follow-up scope from being misfiled.                                                                                      | No Ghostboard follow-up. Respect the existing open follow-ups: Issue 795 for print, Issue 797 for core workflows, Issue 798 for advanced features.                                                                 | Classified `No` because the audit already decomposed remaining PDF risk and none is specifically Ghostboard-owned.                                              |
| `0797-pdf-core-workflow-coverage`    | G     | PDF core workflows               | Common non-print PDF workflows need focused fixtures and probes: keyboard navigation, links, search, restrictions, password/error PDFs, and toolbar states.                            | This issue is still open. Most work is PDF/Roamium-owned, but several workflows overlap with Ghostboard's GUI bridge: keyboard navigation, external links, title/status changes, and possibly download/save behavior inside the hosted browser.                                  | Issue 797 remains open and lists unproven non-print workflows in `issues/0797-pdf-core-workflow-coverage/README.md:1`; Issue 809 proves general Ghostboard keyboard/mouse/geometry only, not PDF workflow coverage, in `issues/0809-ghostboard-viewport-geometry/README.md:231`; Experiment 3 marks browser state paths as `Maybe`.                                                                                                              | `Maybe`    | Medium risk. Important PDF user workflows remain unproven under current Ghostboard, even if most fixes would likely land in Roamium/Chromium.                                   | Keep Issue 797 as the feature owner, but add restored-Ghostboard as a required host in at least one future core workflow sweep.                                                                                    | Classified `Maybe` because open PDF workflow coverage intersects with Ghostboard-hosted input/status behavior, but no concrete Ghostboard bug is proven.        |
| `0798-pdf-advanced-features`         | G     | PDF advanced workflows           | Advanced PDF features require separate diagnostics for forms, annotations, context menus, accessibility, and searchify behavior.                                                       | This issue is still open and is mostly PDFium/Chromium UI/accessibility infrastructure. Ghostboard may affect context menus or native accessibility integration, but there is no evidence yet.                                                                                   | Issue 798 remains open and scopes forms, annotations, context menus, accessibility, and searchify in `issues/0798-pdf-advanced-features/README.md:1`; current Ghostboard has no PDF-specific advanced-feature evidence in Issue 809.                                                                                                                                                                                                             | `Maybe`    | Medium-low risk. Advanced PDF gaps are plausible product gaps, but Ghostboard ownership is uncertain and likely secondary to engine support.                                    | Keep Issue 798 as the owner. When advanced PDF work starts, include one restored-Ghostboard runtime check for context menu/input/accessibility surfaces that cross the GUI boundary.                               | Classified `Maybe` because the issue is open and some advanced surfaces can cross native GUI boundaries, but evidence is not strong enough for `Highly likely`. |
| `0799-browser-api-automation-triage` | G     | Browser APIs / automation        | Missing browser APIs should be triaged automation-first; implement narrow embedder plumbing for automatable surfaces and explicitly defer broad product/platform surfaces.             | Several solved API surfaces use direct webtui/Roamium protocol paths that should work under Ghostboard after `BrowserReady`, but Experiment 3 shows Ghostboard-specific runtime evidence is still missing for dialogs/auth/crash and that compositor fallback cases are ignored. | Issue 799 completed dialog, auth, crash, console, downloads, file upload, WebAuthn, and session probes in `issues/0799-browser-api-automation-triage/README.md:190`; Experiment 3 classifies dialogs/auth/crash/browser-state under Ghostboard as `Maybe` in `issues/0810-ghostboard-preventive-parity-audit/03-direct-browser-paths.md:141`; current Ghostboard ignores unhandled protocol messages in `ghostboard/src/apprt/termsurf.zig:557`. | `Maybe`    | Medium risk. The direct path likely covers normal post-ready behavior, but Ghostboard lacks its own regression proof for several browser-interruption flows and fallback paths. | Add focused Ghostboard browser API runtime checks for JavaScript dialogs, HTTP auth, renderer crash recovery, console/title/loading state, and downloads/file upload only where the flow crosses the GUI boundary. | Classified `Maybe` because Issue 799 proves Roamium/webtui behavior, while Issue 810 already found narrower Ghostboard evidence gaps.                           |

### Findings Summary

`Highly likely` findings:

- None from Batch G. The open and deferred PDF/browser API gaps are real, but
  the evidence points primarily to Roamium/Chromium, PDF extension, or webtui
  ownership rather than a proven Ghostboard GUI defect.

`Maybe` findings:

- Restored Ghostboard should run at least one PDF smoke proving current
  Ghostboard + current Roamium renders a known PDF in-pane.
- PDF interaction workflows should eventually be replayed under Ghostboard for
  the GUI-crossing subset: scroll, keyboard navigation, toolbar click, selection
  and copy, title/status propagation, and save/download.
- Open Issues 797 and 798 should remain PDF owners, but their future
  verification should include restored-Ghostboard host coverage where native GUI
  behavior is involved.
- Issue 799 browser API surfaces should get Ghostboard-specific runtime checks
  for dialogs/auth/crash/state/download/file-upload flows that cross the GUI or
  rely on compositor fallback behavior.

`No` findings:

- Issues 789-791 are PDF-engine lineage and app-shell architecture decisions,
  not current Ghostboard bugs.
- Issue 793's tiny-iframe bug is extension-resource owned and should be covered
  by the PDF smoke, not a separate Ghostboard finding.
- Issue 795 is an important open print feature, but its known missing layer is
  browser-side printing infrastructure, not Ghostboard.
- Issue 796 already decomposed remaining PDF risk into open follow-ups and does
  not add a new Ghostboard-owned gap.

### Verification

Commands run:

```bash
for d in issues/0789-* issues/079{0,1,2,3,4,5,6,7,8,9}-*; do
  sed -n '/^## Conclusion/,$p' "$d/README.md" | sed -n '1,180p'
done

sed -n '1,220p' issues/0795-pdf-native-print/README.md
sed -n '1,220p' issues/0797-pdf-core-workflow-coverage/README.md
sed -n '1,220p' issues/0798-pdf-advanced-features/README.md

rg -n \
  "pdf|PDF|viewer|print|Mojo|WebUI|extension|browser api|Browser API|download|iframe|app_shell|Print|Save|find|zoom|outline" \
  roamium webtui chromium/README.md \
  issues/0810-ghostboard-preventive-parity-audit/0*.md proto

prettier --write --prose-wrap always --print-width 80 \
  issues/0810-ghostboard-preventive-parity-audit/README.md \
  issues/0810-ghostboard-preventive-parity-audit/06-batch-g-pdf-browser-api.md

git diff --check
```

Verification results:

- All eleven Batch G issues are represented exactly once in the classification
  table.
- Every row uses the Experiment 4 schema.
- Open issues `0795`, `0797`, and `0798` are treated as open historical evidence
  and were not modified.
- No historical issue files, application code, generated code, scripts, test
  harnesses, or PDF assets were edited.
- Markdown formatting passed.
- Whitespace check passed.

## Conclusion

Batch G does not add a new `Highly likely` Ghostboard-owned bug. It does add
important `Maybe` coverage gaps: restored Ghostboard needs a focused PDF smoke,
the GUI-crossing part of PDF interactions should be replayed under Ghostboard,
and browser API interruption flows from Issue 799 should get Ghostboard-specific
runtime proof where they cross the GUI boundary or depend on compositor
fallback.

The next audit slice should move backward to Batch F (`0743`-`0788`), because it
contains the dense Wezboard UX, overlay, popup, and regression work immediately
before the PDF/browser API batch.

## Completion Review

Fresh-context adversarial completion review returned **APPROVED**.

Reviewer checks confirmed:

- Batch G `0789`-`0799` appears exactly once.
- Rows follow the Experiment 4 schema.
- Open issues `0795`, `0797`, and `0798` remain open evidence only.
- Classifications are defensible.
- The README marks Experiment 6 as `Pass`.
- Only Issue 810 docs are changed.
- `git diff --check` passes.
- The result commit had not yet been made before review.

Findings: none.
