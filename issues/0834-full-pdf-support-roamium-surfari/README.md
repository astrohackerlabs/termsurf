+++
status = "open"
opened = "2026-06-22"
+++

# Issue 834: Full PDF Support Across Roamium and Surfari

## Goal

TermSurf should provide complete, regression-tested PDF support across both
browser engines: Roamium/Chromium and Surfari/WebKit.

This issue replaces the narrower open PDF follow-ups:

- Issue 795: PDF Native Print;
- Issue 797: PDF Core Workflow Coverage;
- Issue 798: PDF Advanced Features.

## Background

Roamium already has a working Chromium PDF viewer path. Issues 776, 789, 790,
791, 792, 793, 794, and 796 took the Chromium path from a blank white pane to a
usable in-pane PDF viewer with rendering, normal sizing, scroll, resize, mouse
input, keyboard input, text selection/copy, toolbar controls, save/download,
title propagation, local-file parity, security hardening, and non-PDF regression
coverage.

Issue 796 then audited the remaining PDF work. It found that the core Roamium
PDF viewer works, but several workflows are either unproven or intentionally
deferred:

- native print;
- keyboard page/scroll navigation;
- toolbar page selector coverage;
- internal and external PDF links;
- find/search;
- copy/save restrictions and disabled toolbar states;
- password-protected PDFs;
- malformed/error PDFs;
- forms;
- annotations;
- context menus;
- accessibility/searchify.

Those items were split into Issues 795, 797, and 798. This issue supersedes that
split because the product goal is now cross-engine PDF support, not only Roamium
completion.

Surfari has not yet been tested for PDF support in TermSurf. WebKit has native
PDF support through its PDF plugin/PDFKit path on Cocoa, so Surfari should not
need Chromium's extension-based PDF viewer architecture. However, TermSurf still
needs to prove Surfari PDF behavior through the same user-visible matrix:
rendering, input, geometry, links, search, restrictions, print, and advanced
surfaces where supported.

## Strategy

This is a program issue. The checklist below is the overall strategy and should
be checked off as each part is completed. Experiments must still be designed and
completed one at a time. Do not pre-create the experiment list; each experiment
should be informed by the previous result.

- [x] Preserve the Issue 795, 797, and 798 scopes by closing them as superseded
      and pointing them here.
- [x] Define the full cross-engine PDF feature matrix.
- [ ] Finish Roamium PDF support.
- [x] Add durable Roamium regression guards for every completed PDF workflow.
- [ ] Audit WebKit/Safari PDF capabilities relevant to Surfari.
- [ ] Prove basic Surfari PDF rendering in the real TermSurf app.
- [ ] Implement or fix Surfari PDF behavior where probes expose TermSurf
      integration gaps.
- [ ] Add durable Surfari regression guards for every completed PDF workflow.
- [ ] Run the cross-engine PDF regression matrix.
- [ ] Document intentional engine-specific differences, if any.
- [ ] Close this issue only when every required PDF workflow is proven for both
      engines or explicitly documented as unsupported by design.

## Feature Matrix

The full PDF matrix should cover at least:

- full-page PDF rendering;
- embedded PDF rendering;
- HTTP and HTTPS PDFs;
- `file://` PDFs;
- extensionless PDFs;
- scroll wheel;
- keyboard navigation;
- mouse click and focus;
- text selection and copy;
- internal PDF links;
- external PDF links;
- find/search;
- toolbar page navigation;
- zoom in and zoom out;
- fit modes;
- rotate;
- save/download;
- title propagation;
- copy-restricted PDFs;
- save/download-restricted PDFs;
- disabled toolbar states for document restrictions;
- password-protected PDFs;
- malformed/error PDFs;
- native print;
- forms;
- annotations;
- context menus;
- accessibility/searchify;
- split, tab, window, and resize behavior;
- non-PDF regression smoke.

Each feature row should ultimately record:

- Roamium status;
- Surfari status;
- automation coverage;
- known engine-specific differences;
- links to the experiment(s) and logs proving the status.

## Experiments

- [Experiment 1: Define the cross-engine PDF matrix](01-define-cross-engine-pdf-matrix.md)
  — **Pass**
- [Experiment 2: Rerun the Roamium PDF baseline](02-rerun-roamium-pdf-baseline.md)
  — **Pass**
- [Experiment 3: Prove Roamium keyboard and page navigation](03-prove-roamium-keyboard-page-navigation.md)
  — **Pass**
- [Experiment 4: Prove Roamium PDF links](04-prove-roamium-pdf-links.md) —
  **Pass**
- [Experiment 5: Prove Roamium PDF find/search](05-prove-roamium-pdf-find-search.md)
  — **Pass**
- [Experiment 6: Prove Roamium PDF document restrictions](06-prove-roamium-pdf-document-restrictions.md)
  — **Partial**
- [Experiment 7: Prove Roamium password-protected PDFs](07-prove-roamium-password-pdfs.md)
  — **Partial**
- [Experiment 8: Fix Roamium PDF password Enter submission](08-fix-roamium-password-enter-submit.md)
  — **Pass**
- [Experiment 9: Prove Roamium malformed PDF errors](09-prove-roamium-malformed-pdf-errors.md)
  — **Pass**
- [Experiment 10: Probe Roamium native print dialog safely](10-probe-roamium-native-print-dialog.md)
  — **Partial**
- [Experiment 11: Inventory Roamium advanced PDF surfaces](11-inventory-roamium-advanced-pdf-surfaces.md)
  — **Pass**
- [Experiment 12: Prove Roamium PDF forms](12-prove-roamium-pdf-forms.md) —
  **Partial**
- [Experiment 13: Fix Roamium PDF form sequences](13-fix-roamium-pdf-form-sequence.md)
  — **Partial**
- [Experiment 14: Compare Roamium PDF form input paths](14-compare-roamium-pdf-form-input-paths.md)
  — **Pass**
- [Experiment 15: Add Roamium PDF regression guard](15-add-roamium-pdf-regression-guard.md)
  — **Pass**
- [Experiment 16: Prove native dialog watcher preflight](16-prove-native-dialog-watcher-preflight.md)
  — **Partial**
- [Experiment 17: Prove native dialog cancellation](17-prove-native-dialog-cancellation.md)
  — **Pass**
- [Experiment 18: Probe Roamium native print dialog](18-probe-roamium-native-print-dialog.md)
  — **Partial**
- [Experiment 19: Trace Roamium native print plumbing](19-trace-roamium-native-print-plumbing.md)
  — **Pass**
- [Experiment 20: Wire Roamium browser print settings](20-wire-roamium-browser-print-settings.md)
  — **Partial**
- [Experiment 21: Diagnose Roamium macOS print dialog](21-diagnose-roamium-macos-print-dialog.md)
  — **Partial**
- [Experiment 22: Fix Roamium macOS print panel presentation](22-fix-roamium-macos-print-panel-presentation.md)
  — **Partial**
- [Experiment 23: Probe Roamium app-modal print presentation](23-probe-roamium-app-modal-print-presentation.md)
  — **Partial**
- [Experiment 24: Bridge GUI active state into Roamium app activation](24-bridge-gui-active-app-activation.md)
  — **Partial**
- [Experiment 25: Promote Roamium AppKit activation policy](25-promote-roamium-appkit-activation-policy.md)
  — **Partial**
- [Experiment 26: Retry parent-window print sheet after activation](26-retry-parent-window-print-sheet-after-activation.md)
  — **Partial**
- [Experiment 27: Inspect parent print sheet visibility](27-inspect-parent-print-sheet-visibility.md)
  — **Partial**
- [Experiment 28: Cancel parent print sheet through Accessibility](28-cancel-parent-print-sheet-accessibility.md)
  — **Pass**
- [Experiment 29: Add native print to Roamium regression guards](29-add-native-print-regression-guard.md)
  — **Pass**
- [Experiment 30: Prove Roamium PDF annotations](30-prove-roamium-pdf-annotations.md)
  — **Designed**

## Roamium Phase

Start with the evidence from Issues 794 and 796. Roamium's core PDF viewer is
already usable; the remaining work is to finish or prove the workflows that the
audit left unproven.

The initial Roamium work should:

1. Reconstruct the Issue 796 feature inventory as a current feature matrix.
2. Run probes before changing product code.
3. Fix only real TermSurf integration gaps found by those probes.
4. Keep native print contained during automation and never submit a real print
   job.
5. Add regression guards as each workflow becomes proven.

If Chromium is modified, create a fresh Chromium branch for this issue and add
it to `chromium/README.md`.

## Surfari Phase

Begin Surfari work only after Roamium's matrix is complete and protected by
regression tests.

The Surfari phase should:

1. Audit WebKit's PDF implementation and the local WebKit checkout.
2. Prove basic PDF load/render in Surfari with deterministic fixtures.
3. Walk the same PDF feature matrix used for Roamium.
4. Distinguish WebKit-native behavior from TermSurf integration gaps.
5. Fix TermSurf integration gaps in Surfari, `libtermsurf_webkit`, WebKit, or
   Ghostboard as appropriate.
6. Add Surfari-specific regression guards.

WebKit/Safari PDF UI does not have to look identical to Chromium's PDF viewer.
The required parity is user-visible workflow parity unless a difference is
explicitly documented as engine-specific and acceptable.

## Final Regression Phase

After both engines pass their own PDF matrices:

1. Run Roamium PDF regression guards.
2. Run Surfari PDF regression guards.
3. Run non-PDF browser regression smoke tests for both engines.
4. Verify split, tab, window, resize, profile, and lifecycle behavior with PDFs
   open.
5. Update the feature matrix with final evidence.
6. Record any intentional engine-specific differences.

This issue is complete only when the matrix proves the complete PDF workflow set
for both engines, or when an unsupported behavior is deliberately accepted and
documented with a product rationale.
