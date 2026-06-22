# Experiment 1: Define the Cross-Engine PDF Matrix

## Description

This experiment creates the evidence-backed PDF feature matrix that will govern
the rest of Issue 834. It is documentation-only: no Roamium, Surfari,
Ghostboard, Chromium, WebKit, protocol, fixture, script, or runtime behavior may
change.

Issue 834 spans two engines and a large PDF surface. Before implementing fixes,
we need a concrete matrix that separates:

- product requirements from optional or engine-specific behavior;
- existing Roamium evidence from stale, weak, or missing evidence;
- Surfari/WebKit unknowns from known TermSurf integration gaps;
- feature gaps from automation gaps;
- cheap regression guards from expensive/manual workflows.

The output of this experiment is the first authoritative matrix for Issue 834
and the immediate Roamium-first backlog for Experiment 2.

## Changes

1. Audit current issue records and existing PDF automation.

   Read at least:

   - Issue 834 README;
   - Issues 776, 789, 790, 791, 792, 793, 794, and 796 for existing Roamium PDF
     evidence;
   - superseded Issues 795, 797, and 798 for remaining print, core workflow, and
     advanced-feature scope;
   - Issue 756, `surfari/`, `surfari/libtermsurf_webkit/`, `webkit/README.md`,
     and `webkit/AGENTS.md` for Surfari capability context.

   A deep WebKit source audit is deferred to the Surfari phase. Experiment 1 may
   inspect targeted WebKit source when it is cheap and directly clarifies a
   matrix row, but Surfari rows must remain `Unknown` or `Likely but unverified`
   unless current TermSurf/Surfari evidence proves the behavior.

   Inspect existing scripts and logs, including:

   - `scripts/test-issue-794-pdf-toolbar.py`;
   - `scripts/test-issue-794-protocol-scroll.py`;
   - `scripts/test-issue-794-protocol-resize.py`;
   - `scripts/test-issue-794-protocol-mouse.py`;
   - `scripts/test-issue-796-pdf-security.py`;
   - `scripts/probe-pdf-save-print-title-local.mjs`;
   - `scripts/probe-pdf-toolbar-events.mjs`;
   - `scripts/termsurf_pdf_protocol_harness.py`;
   - relevant `logs/issue-794-*` and `logs/issue-796-*` summaries when present.

2. Add a matrix section to this experiment's `## Result`.

   The matrix must include every feature listed in the Issue 834 README and any
   additional PDF workflow discovered during the audit. Each row must include:

   - feature/workflow name;
   - requirement level: `Required`, `Engine-specific acceptable`, `Optional`, or
     `Out of scope`;
   - Roamium status;
   - Surfari status;
   - existing evidence;
   - automation coverage;
   - missing fixtures or probes;
   - known engine-specific difference, if any;
   - next action.

   Status values must be one of:

   - `Proven`;
   - `Likely but unverified`;
   - `Weak evidence`;
   - `Missing`;
   - `Blocked by fixture/probe gap`;
   - `Unsupported by design`;
   - `Unknown`.

3. Classify existing Roamium evidence.

   For Roamium, distinguish evidence that is still strong enough for Issue 834
   from evidence that must be rerun on the current tree. Older issue results may
   be cited, but they must be marked weak when the current code or test harness
   has materially changed.

4. Classify Surfari unknowns.

   For Surfari, do not assume WebKit-native PDF support is sufficient. Mark a
   feature as proven only if TermSurf/Surfari evidence exists. Otherwise
   classify it as `Unknown`, `Likely but unverified`, or
   `Blocked by fixture/probe gap` with a concrete next probe.

5. Define regression tiers.

   Split the future regression strategy into:

   - fast smoke checks suitable for frequent development;
   - focused feature probes used while fixing a row;
   - full matrix checks for issue completion or release confidence;
   - manual or OS-contained checks for features that cannot safely be fully
     automated, especially native print.

6. Produce the Experiment 2 recommendation.

   The conclusion must identify the next experiment. Expected default:
   Roamium-first verification of the current baseline using existing probes,
   because Issue 834 says Surfari begins after Roamium's matrix is complete and
   protected. If the audit proves a different ordering is safer, explain why.

## Verification

This is a documentation-only design/audit experiment. Verification for the
completed result will be:

- no product/runtime source files changed;
- this experiment contains `## Result` and `## Conclusion`;
- the Issue 834 README experiment index is updated from `Designed` to the final
  result status;
- every matrix row has a requirement level, Roamium status, Surfari status,
  evidence, automation coverage, and next action;
- no row claims `Proven` without concrete evidence;
- native print is classified with an OS-contained test strategy and no real
  print submission;
- Roamium and Surfari statuses are evaluated independently;
- automation gaps are separated from product behavior gaps;
- the next experiment recommendation is concrete enough to design without
  redoing this audit;
- markdown is formatted with:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0834-full-pdf-support-roamium-surfari/README.md \
    issues/0834-full-pdf-support-roamium-surfari/01-define-cross-engine-pdf-matrix.md
  ```

- `git diff --check` passes;
- required design and completion reviews are recorded in this file.

No Chromium, WebKit, Rust, Zig, Swift, Python, or JavaScript build is required
unless this experiment accidentally changes code. It must not change code.

## Design Review

Fresh-context adversarial review by Codex subagent `Helmholtz`: **Approved**.

Findings:

- Optional: Surfari/WebKit evidence sources were underspecified. Fixed by naming
  the local Surfari/WebKit docs and source areas to inspect, while explicitly
  deferring deep WebKit source audit to the Surfari phase.
- Nit: Prettier verification was implicit. Fixed by adding the exact Prettier
  command for the edited issue files.

## Pass Criteria

This experiment passes if it produces a complete, evidence-backed cross-engine
PDF matrix and a concrete next experiment recommendation.

## Partial Criteria

This experiment is partial if it improves the matrix but leaves major Issue 834
feature areas without evidence classification or next actions.

## Failure Criteria

This experiment fails if it changes product code, treats old Roamium evidence as
current proof without justification, assumes Surfari parity without TermSurf
evidence, or leaves native print without a contained testing strategy.

## Result

**Result:** Pass

Experiment 1 produced the initial cross-engine PDF matrix for Issue 834. No
product code, runtime scripts, engine source, fixtures, or protocol files were
changed.

### Evidence Rules Used

The audit intentionally distinguishes historical evidence from current proof:

- Issue 794 and Issue 796 contain strong historical Roamium evidence for the
  core Chromium PDF viewer, but the referenced log directories are not present
  in this checkout. Those rows are therefore marked `Weak evidence` until rerun
  on the current tree.
- Surfari Issue 756 proves broad real-app WebKit behavior: navigation, keyboard,
  click, drag, wheel, resize, splits, tabs, windows, profiles, and crash
  recovery. It does not prove PDF-specific WebKit behavior. Surfari PDF rows
  remain `Unknown` unless there is direct PDF evidence.
- WebKit source exposes APIs relevant to PDF-adjacent workflows, including
  `WKWebView.createPDFWithConfiguration`, `findString`, download APIs, and
  `printOperationWithPrintInfo`, but that only proves possible integration
  surfaces. It does not prove TermSurf/Surfari PDF support.
- Native print must never be tested by submitting a real job. Automation may
  only use contained intercepts, mocked/fake destinations, or a manual
  click/cancel smoke explicitly scoped to not submit.

### Cross-Engine PDF Matrix

| Feature / workflow                 | Requirement level | Roamium status | Surfari status | Existing evidence                                                                                              | Automation coverage                                                                                    | Missing fixtures / probes                                  | Engine-specific difference                                                | Next action                                                                 |
| ---------------------------------- | ----------------- | -------------- | -------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------- | ------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| Full-page PDF rendering            | Required          | Weak evidence  | Unknown        | Issues 792-794 and 796 say full-page Bitcoin PDF rendering worked.                                             | `test-issue-794-pdf-toolbar.py`, screenshot/devtools probes.                                           | Current rerun logs for Roamium; Surfari PDF fixture probe. | Chromium uses extension/PDFium viewer; WebKit should use native PDF.      | Experiment 2 reruns Roamium; Surfari phase adds first PDF load probe.       |
| Embedded PDF rendering             | Required          | Weak evidence  | Unknown        | Issue 796 says embedded rendering was covered; Issue 794 Exp 19/20 preserved embedded host-title safety.       | `probe-pdf-save-print-title-local.mjs` has embedded fixture handling.                                  | Current rerun; Surfari embedded fixture.                   | WebKit embedded PDF may render via native plugin path.                    | Include in Roamium rerun; defer Surfari direct proof to Surfari phase.      |
| HTTP PDF                           | Required          | Weak evidence  | Unknown        | Issue 794/796 HTTP Bitcoin fixture evidence.                                                                   | Existing Python harness can serve `bitcoin.pdf`.                                                       | Current rerun; Surfari HTTP PDF probe.                     | None acceptable beyond viewer UI differences.                             | Rerun Roamium HTTP fixture first.                                           |
| HTTPS PDF                          | Required          | Unknown        | Unknown        | No specific HTTPS PDF proof found in current audit.                                                            | Existing harness is local HTTP; no HTTPS fixture identified.                                           | Deterministic local HTTPS fixture or accepted HTTP proxy.  | TLS stack differs by engine, but user workflow should work.               | Add HTTPS row to later fixture work after Roamium baseline rerun.           |
| `file://` PDF                      | Required          | Weak evidence  | Unknown        | Issue 794 Exp 13/14/18/19/20 report local `file://` rendering/parity.                                          | `probe-pdf-save-print-title-local.mjs` has `--file-pdf-url`.                                           | Current rerun; Surfari file URL probe.                     | WebKit file access policy may differ.                                     | Rerun Roamium local parity; Surfari phase tests file URL explicitly.        |
| Extensionless PDF                  | Required          | Weak evidence  | Unknown        | Issue 794 Exp 13/14/18/19/20 report HTTP and `file://` extensionless PDF rendering.                            | `probe-pdf-save-print-title-local.mjs` has extensionless URL inputs.                                   | Current rerun; Surfari extensionless fixture.              | MIME sniffing/content-type behavior may differ.                           | Rerun current Roamium local parity.                                         |
| Scroll wheel                       | Required          | Weak evidence  | Unknown        | Issue 794 Exp 14 and Issue 796 Exp 6 report protocol scroll pass.                                              | `test-issue-794-protocol-scroll.py`. Surfari generic wheel proven in Issue 756, not PDF-specific.      | Current Roamium rerun; Surfari PDF wheel probe.            | WebKit native PDF scroll target may not expose the same DOM state.        | Rerun Roamium scroll; design Surfari-specific observable later.             |
| Keyboard page/scroll navigation    | Required          | Missing        | Unknown        | Issue 797 lists keyboard scroll/page navigation as still required. Issue 794 key select/copy is separate.      | Existing PDF mouse script has `key-select-copy`; no current page/scroll key assertion found.           | Keyboard navigation probe.                                 | Keybindings may differ between Chromium PDF viewer and WebKit PDF view.   | After Roamium baseline, add focused Roamium keyboard navigation experiment. |
| Mouse click and focus              | Required          | Weak evidence  | Unknown        | Issue 794 click/focus path was part of interaction work; Issue 796 protocol mouse click passed.                | `test-issue-794-protocol-mouse.py --action click`. Surfari generic click proven in Issue 756, not PDF. | Current Roamium rerun; Surfari PDF click/focus probe.      | WebKit PDF hit testing may be native/plugin-specific.                     | Rerun Roamium click first.                                                  |
| Text selection and copy            | Required          | Weak evidence  | Unknown        | Issue 794 Exp 7/12/14 report PDF drag/key selection and clipboard copy evidence.                               | `test-issue-794-protocol-mouse.py --action key-select-copy`; drag paths in historical harnesses.       | Current rerun; Surfari PDF selection/copy probe.           | WebKit PDF selection/copy may use AppKit/PDFKit surfaces.                 | Rerun Roamium selection/copy; later prove Surfari selection separately.     |
| Internal PDF links                 | Required          | Missing        | Unknown        | Issue 797 tracks internal links as required and unproven.                                                      | No deterministic link fixture identified.                                                              | PDF fixture with internal named/page link.                 | Link navigation observables may differ.                                   | Create link fixture/probe after baseline rerun.                             |
| External PDF links                 | Required          | Missing        | Unknown        | Issue 797 tracks external links as required and unproven.                                                      | No deterministic external-link PDF probe identified.                                                   | PDF fixture with external URL link.                        | Chromium may route via PDF extension; WebKit may route native viewer.     | Create link fixture/probe after baseline rerun.                             |
| Find/search                        | Required          | Missing        | Unknown        | Issue 797 tracks find/search as required and unproven.                                                         | WebKit exposes `WKWebView.findString`; Roamium PDF viewer path not yet probed.                         | Search probe and stable observable.                        | WebKit may expose app-level find instead of PDF toolbar find.             | Design after link/keyboard fixture inventory.                               |
| Toolbar page navigation / selector | Required          | Weak evidence  | Unknown        | Issue 794 local parity reported page changes; Issue 796 says page selector remains tracked by Issue 797.       | `probe-pdf-toolbar-events.mjs` covers fit/zoom/rotate, not page selector as separate current feature.  | Current page selector/page navigation assertion.           | WebKit PDF UI may not match Chromium toolbar.                             | Add focused Roamium page navigation probe after baseline.                   |
| Zoom in / zoom out                 | Required          | Weak evidence  | Unknown        | Issue 794 Exp 12/14 and Issue 796 Exp 6 toolbar events report pass.                                            | `probe-pdf-toolbar-events.mjs` covers zoom in/out.                                                     | Current rerun; Surfari equivalent workflow probe.          | UI controls may differ; workflow parity is required.                      | Rerun Roamium toolbar events.                                               |
| Fit modes                          | Required          | Weak evidence  | Unknown        | Issue 794 Exp 12/14 and Issue 796 Exp 6 toolbar events report fit pass.                                        | `probe-pdf-toolbar-events.mjs` covers fit.                                                             | Current rerun; Surfari equivalent workflow probe.          | Fit control names/state may differ.                                       | Rerun Roamium toolbar events.                                               |
| Rotate                             | Required          | Weak evidence  | Unknown        | Issue 794 Exp 12/14/17/18 and Issue 796 Exp 6 report rotate event/control evidence.                            | `probe-pdf-toolbar-events.mjs`; print bridge comparison also used rotate as a control.                 | Current rerun; Surfari equivalent workflow probe.          | WebKit native PDF rotation UI may differ or be absent.                    | Rerun Roamium toolbar events.                                               |
| Save/download                      | Required          | Weak evidence  | Unknown        | Issue 794 Exp 13/14/18/19/20 report controlled `bitcoin.pdf` download creation.                                | `probe-pdf-save-print-title-local.mjs` with controlled downloads directory.                            | Current rerun; Surfari download destination/intercept.     | WebKit download APIs differ and may require `_WKDownloadDelegate`.        | Rerun Roamium save; defer Surfari download proof.                           |
| Title propagation                  | Required          | Weak evidence  | Unknown        | Issue 794 Exp 15 fixed title propagation; Exp 18-20 report title propagation passed.                           | `probe-pdf-save-print-title-local.mjs` reads trace/title evidence.                                     | Current rerun; Surfari PDF title behavior probe.           | WebKit title may be URL/filename/PDF metadata depending native viewer.    | Rerun Roamium title.                                                        |
| Copy-restricted PDFs               | Required          | Missing        | Unknown        | Issue 797 tracks copy restrictions and disabled states as required.                                            | No fixture/probe identified.                                                                           | Restricted PDF fixture.                                    | Permission model may differ by PDF engine.                                | Create restricted fixture/probe after core reruns.                          |
| Save/download-restricted PDFs      | Required          | Missing        | Unknown        | Issue 797 tracks save/download restrictions as required.                                                       | No fixture/probe identified.                                                                           | Restricted PDF fixture.                                    | WebKit may expose fewer toolbar states.                                   | Create restricted fixture/probe after core reruns.                          |
| Disabled toolbar states            | Required          | Missing        | Unknown        | Issue 797 tracks disabled toolbar states for restrictions.                                                     | No fixture/probe identified.                                                                           | Restricted fixture plus toolbar-state assertion.           | WebKit UI may not expose Chromium-equivalent disabled controls.           | Audit acceptable engine-specific UI difference after fixture exists.        |
| Password-protected PDFs            | Required          | Missing        | Unknown        | Issue 797 tracks password PDFs as required.                                                                    | No fixture/probe identified.                                                                           | Password-protected fixture and prompt handling probe.      | Prompt/UI flows likely differ.                                            | Add fixture/probe later.                                                    |
| Malformed/error PDFs               | Required          | Missing        | Unknown        | Issue 797 tracks malformed/error PDFs as required.                                                             | No fixture/probe identified.                                                                           | Malformed fixture and error-state observable.              | Error UI will likely differ.                                              | Add fixture/probe later.                                                    |
| Native print                       | Required          | Missing        | Unknown        | Issue 794 Exp 20 says native print UI still does not appear; Issue 795 superseded into Issue 834.              | Contained intercept exists historically; production print must not be clicked by automation.           | Safe native-print strategy: fake/mock/cancel-only.         | WebKit has `printOperationWithPrintInfo`; Chromium path needs host glue.  | Do not test until contained strategy is designed.                           |
| Forms                              | Required          | Missing        | Unknown        | Issue 798 tracks forms as advanced work; later work may document engine-specific unsupported behavior.         | No fixture/probe identified.                                                                           | Form PDF fixture and input/copy/save assertions.           | Engine support may differ.                                                | Add advanced fixture tranche after core workflows.                          |
| Annotations                        | Required          | Missing        | Unknown        | Issue 798 tracks annotations as advanced work; later work may document engine-specific unsupported behavior.   | No fixture/probe identified.                                                                           | Annotated PDF fixture and UI/state assertions.             | Chromium and WebKit annotation editing support may differ.                | Audit support before implementation.                                        |
| Context menus                      | Required          | Missing        | Unknown        | Issue 798 tracks context menus as advanced work; later work may document engine-specific unsupported behavior. | No fixture/probe identified.                                                                           | Context-click automation and menu observable.              | Native context menu APIs differ.                                          | Audit after core pointer proof.                                             |
| Accessibility/searchify            | Optional          | Missing        | Unknown        | Issue 798 tracks accessibility/searchify; Issue 796 treated large accessibility work as out of audit scope.    | No fixture/probe identified.                                                                           | Decide product requirement and observables.                | Chromium Searchify may not map to WebKit.                                 | Product decision before implementation.                                     |
| Split/tab/window/resize with PDFs  | Required          | Weak evidence  | Unknown        | Issue 794/796 prove Roamium PDF resize only; Issue 756 proves generic Surfari split/tab/window geometry.       | Roamium resize harness; Surfari generic geometry harnesses.                                            | PDF-specific split/tab/window matrix for both engines.     | Overlay geometry should be GUI-level, but PDF internals must resize too.  | After core PDF reruns, add geometry matrix with PDFs open.                  |
| Profile/lifecycle with PDFs        | Required          | Unknown        | Unknown        | Generic profile/lifecycle behavior exists, but PDF-specific persistence/lifecycle is not proven.               | Generic profile/lifecycle harnesses, no PDF row.                                                       | PDF opened in named profiles and across close/reopen.      | Storage semantics differ by engine profile implementation.                | Add after core rendering and profile-sensitive fixtures.                    |
| Non-PDF regression smoke           | Required          | Weak evidence  | Proven         | Issue 794/796 report HTML smoke after Roamium PDF work; Issue 756 proves generic Surfari real-app parity.      | Roamium historical HTML click/resize smoke; Surfari final comparison harness.                          | Current paired smoke during every fix.                     | None.                                                                     | Include current non-PDF smoke in Experiment 2 and later Surfari runs.       |
| PDF security boundary              | Required          | Weak evidence  | Unknown        | Issue 796 security track passed for Roamium's Chromium extension boundary.                                     | `test-issue-796-pdf-security.py`.                                                                      | Current rerun; Surfari-specific threat model.              | WebKit has no Chromium PDF extension boundary, so it needs its own model. | Rerun Roamium security; define Surfari security rows during WebKit audit.   |

### Automation Tiers

Fast development smoke:

- current Roamium full-page HTTP PDF render;
- Roamium protocol scroll;
- Roamium protocol resize;
- Roamium selection/copy;
- Roamium save/title/local parity;
- one non-PDF HTML smoke;
- later, Surfari full-page PDF render plus one input and one resize check.

Focused feature probes:

- toolbar events and page selector;
- keyboard page/scroll navigation;
- internal/external links;
- find/search;
- restricted, password, and malformed fixtures;
- forms, annotations, and context menus;
- PDF-specific split/tab/window/profile/lifecycle behavior.

Full matrix checks:

- all Required rows for Roamium;
- all Required rows for Surfari;
- paired non-PDF regression smoke;
- split, tab, window, resize, profile, and lifecycle checks with PDFs open.

Manual or OS-contained checks:

- native print only, unless a fake printer or contained intercept can prove the
  behavior without opening uncontrolled UI or submitting a job;
- any native context menu assertions that cannot be observed safely through logs
  or test hooks.

### Product Gaps vs. Automation Gaps

Likely product gaps:

- Roamium native print still does not open native print UI.
- Roamium keyboard page/scroll navigation, links, find/search, restrictions,
  password/error PDFs, forms, annotations, context menus, and
  accessibility/searchify are not implemented/proven.
- Surfari PDF behavior is not proven at all inside TermSurf.

Automation gaps:

- Existing Roamium core evidence needs current reruns because old logs are not
  present in this checkout.
- There are no deterministic fixtures yet for links, restrictions, passwords,
  malformed PDFs, forms, or annotations.
- Surfari needs PDF-specific probes; generic Issue 756 WebKit browser evidence
  cannot be reused as PDF proof.

## Conclusion

Experiment 1 passes. The matrix is now concrete enough to drive the rest of
Issue 834 without guessing.

The next experiment should be Roamium-first current-baseline verification. It
should run the existing Roamium PDF probes on the current tree, record fresh log
directories, and update the matrix rows from `Weak evidence` to `Proven` only
where the current runs directly pass. It should include at least:

```bash
python3 scripts/test-issue-794-pdf-toolbar.py \
  --log-dir logs/issue-834-exp2-save-title-local \
  --serve-bitcoin-pdf \
  --probe save-print-title-local \
  --enable-pdf-print-intercept
python3 scripts/test-issue-794-pdf-toolbar.py \
  --log-dir logs/issue-834-exp2-toolbar-events \
  --serve-bitcoin-pdf \
  --probe events
python3 scripts/test-issue-794-protocol-scroll.py \
  --log-dir logs/issue-834-exp2-protocol-scroll \
  --serve-bitcoin-pdf
python3 scripts/test-issue-794-protocol-resize.py \
  --log-dir logs/issue-834-exp2-protocol-resize \
  --serve-bitcoin-pdf
python3 scripts/test-issue-794-protocol-mouse.py \
  --log-dir logs/issue-834-exp2-protocol-mouse-click \
  --serve-bitcoin-pdf \
  --action click
python3 scripts/test-issue-794-protocol-mouse.py \
  --log-dir logs/issue-834-exp2-protocol-select-copy \
  --serve-bitcoin-pdf \
  --action key-select-copy
python3 scripts/test-issue-796-pdf-security.py \
  --log-dir logs/issue-834-exp2-security
```

If any current Roamium baseline probe fails, Experiment 2 should record the
first failing layer and stop before designing new PDF features. Surfari should
begin only after Roamium's current baseline is proven and protected.

## Completion Review

Fresh-context adversarial review by Codex subagent `Poincare`: **Changes
required**, then **Approved** after fixes.

Required finding:

- The matrix used non-enum values in a few `Requirement level`,
  `Roamium status`, and `Surfari status` cells, violating the experiment's
  approved status contract.

Fix:

- Normalized those cells to the approved enum values and moved qualifiers such
  as contained print, PDF-specific scope, and WebKit's different security model
  into evidence, engine-specific difference, or next-action columns.

Re-review verdict: **Approved**.
