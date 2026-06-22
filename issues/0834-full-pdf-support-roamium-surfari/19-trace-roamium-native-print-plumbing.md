# Experiment 19: Trace Roamium Native Print Plumbing

## Description

Experiment 18 safely clicked the real Roamium PDF print control. The PDF viewer
JavaScript emitted print records, Chromium reached
`pdf_view_web_plugin.cc event=handle-print`, and no print job was submitted, but
the macOS native Print/Printer dialog never appeared.

This experiment should identify the first missing native print hop inside
Roamium's Chromium embedding path. It is an audit/probe experiment, not a
product fix. The goal is to prove whether the missing integration is in:

- the PDF plugin `client_->Print()` path;
- `components/printing/renderer/PrintRenderFrameHelper`;
- browser-side `printing::PrintViewManager` ownership;
- print preview registration and WebContents delegate plumbing;
- macOS `PrintingContext` / system-dialog integration;
- build flags or command-line flags that disable the relevant path.

## Changes

1. Audit upstream Chromium's PDF print path.

   Read the current Chromium sources, especially:

   - `chrome/browser/pdf/pdf_extension_printing_test.cc`;
   - `pdf/pdf_view_web_plugin.cc`;
   - `components/printing/renderer/print_render_frame_helper.*`;
   - browser-side `printing::PrintViewManager` creation and ownership sites;
   - macOS printing context code under `printing/`.

   Record the expected upstream chain for the PDF toolbar Print button. The
   audit should explicitly distinguish print-preview mode from basic system
   dialog mode, because the upstream PDF tests exercise both.

2. Audit TermSurf's current Chromium embedding path.

   Read the local TermSurf-specific code, especially:

   - `content/libtermsurf_chromium/ts_pdf_renderer_support.cc`;
   - `content/libtermsurf_chromium/ts_browser_main_parts.cc`;
   - `content/libtermsurf_chromium/BUILD.gn`;
   - `chromium/src/out/Default/args.gn`;
   - the Issue 834 patch archive entries related to PDF print.

   Determine which expected upstream print objects are present, missing, or
   intentionally disabled in Roamium.

3. Add only diagnostic instrumentation if static audit is insufficient.

   If the missing hop cannot be proven by static audit and existing logs, add a
   narrow trace behind existing TermSurf print trace controls. The trace may
   record whether:

   - `PdfViewWebPlugin::OnInvokePrintDialog()` runs;
   - `client_->Print()` calls `PrintRenderFrameHelper::PrintNode()`;
   - `PrintRenderFrameHelper` exists for the relevant frame;
   - `PrintRenderFrameHelper` reaches its browser IPC call;
   - the browser has a `printing::PrintViewManager` for the WebContents;
   - print preview or basic print code is compiled/enabled.

   If Chromium code must be modified, create a fresh Chromium branch for Issue
   834 and update `chromium/README.md` according to the repo's Chromium branch
   rules.

4. Run the existing safe native print probe.

   Reuse Experiment 18's guarded command:

   ```bash
   python3 scripts/test-issue-834-pdf-native-print.py \
     --log-dir logs/issue-834-exp19-print-plumbing \
     --probe native-dialog \
     --allow-native-dialog-click
   ```

   The probe must still refuse unsafe native print attempts, must record the
   harmless preflight, must record print queue before/after state, and must not
   submit a print job.

5. Classify the first missing hop.

   Record the first objectively proven missing hop. Examples:

   - `plugin-print-not-invoked`;
   - `print-render-frame-helper-missing`;
   - `print-node-not-called`;
   - `print-render-frame-helper-stops-before-browser-ipc`;
   - `print-view-manager-missing`;
   - `print-preview-disabled`;
   - `basic-print-dialog-disabled`;
   - `mac-printing-context-not-reached`;
   - `native-dialog-observation-gap`;
   - `native-dialog-appears-and-cancels`.

   The classification must cite source lines and log evidence.

## Verification

Verification for the completed result is:

```bash
rm -rf scripts/__pycache__
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-pdf-native-print.py
rm -rf scripts/__pycache__

python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp19-print-plumbing \
  --probe native-dialog \
  --allow-native-dialog-click

git diff --check
```

If Chromium code is changed, also run the Chromium workspace verification from
`chromium/AGENTS.md`:

```bash
git status --short
git -C chromium/src status --short
git -C chromium/src rev-parse --abbrev-ref HEAD
git -C chromium/src rev-parse HEAD

cd chromium/src
export PATH="/Users/astrohacker/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default libtermsurf_chromium
```

Then regenerate the Issue 834 patch archive, update `chromium/README.md` if a
new Chromium branch was created, and return to the main repo before recording
the result.

Required evidence:

- the upstream expected print chain is documented;
- the TermSurf/Roamium actual print chain is documented;
- any added trace is gated and narrowly scoped;
- the guarded native print probe records its internal preflight;
- no print job is submitted;
- the first missing hop is classified from source and log evidence;
- the result explains whether the next experiment should implement a fix or
  improve observability;
- markdown is formatted with Prettier;
- Python bytecode cache is removed after compilation;
- `git diff --check` passes;
- Chromium status, branch, HEAD, build, branch table, and patch archive evidence
  is recorded if Chromium source is changed;
- design review is recorded, all real design-review findings are fixed, the
  design is approved, and the plan commit exists before implementation begins;
- completion review is recorded before the result commit.

## Pass Criteria

This experiment passes if it identifies the first missing native print hop with
source and runtime evidence, without submitting a print job.

## Partial Criteria

This experiment is partial if the audit narrows the problem but still cannot
identify the first missing hop without broader Chromium instrumentation or an
environment outside this VM.

## Failure Criteria

This experiment fails if it submits a print job, performs an unsafe native print
click, makes product behavior changes before identifying the missing hop, or
claims a root cause without source and log evidence.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Initial verdict: **Changes Required**.

Required finding:

- If Chromium source instrumentation is needed, the design did not include the
  full Chromium verification hygiene required by `chromium/AGENTS.md`.

Fix:

- Added the conditional Chromium status, branch, HEAD, `autoninja`, patch
  archive, and `chromium/README.md` evidence requirements.

Re-review verdict: **Approved**.

The reviewer found no remaining Required findings.

## Result

**Result:** Pass

Experiment 19 identified the first missing Roamium native PDF print hop:
`browser-default-print-settings-null`.

The initial audit found that Experiment 18's production native-print probe was
still setting `TERMSURF_PDF_PRINT_BRIDGE_TRACE_FILE`. That bridge trace is not a
passive trace in the production native path. In
`chromium/src/pdf/pdf_view_web_plugin.cc`, `PdfViewWebPlugin::Print()` returns
early when `bridge_trace_path` exists and no contained print intercept is active
(`chromium/src/pdf/pdf_view_web_plugin.cc:1613`). That explained why Experiment
18 reached `handle-print` but did not display a dialog: the probe stopped before
`OnInvokePrintDialog()`.

Tooling changes:

- `scripts/test-issue-834-pdf-native-print.py` now uses
  `TERMSURF_PDF_NATIVE_PRINT_TRACE_FILE` for guarded production native clicks
  and keeps `TERMSURF_PDF_PRINT_BRIDGE_TRACE_FILE` for non-production bridge
  probing.
- `scripts/probe-pdf-save-print-title-local.mjs` now accepts
  `--native-print-trace-file` and reads native print trace lines for the
  production native-click summary.
- `scripts/test-issue-834-pdf-native-print.py` now classifies native print as
  `browser-default-print-settings-null` when the native trace reaches
  `get-default-print-settings-null`.

No Chromium source was changed in this experiment, so no Chromium branch,
`autoninja`, branch table update, or patch archive regeneration was required.

Static audit evidence:

- The upstream PDF print button path is exercised by
  `chromium/src/chrome/browser/pdf/pdf_extension_printing_test.cc`; the
  `PrintButton` test clicks the PDF toolbar print button and waits for print
  preview readiness.
- TermSurf creates a renderer-side `PrintRenderFrameHelper` in
  `chromium/src/content/libtermsurf_chromium/ts_pdf_renderer_support.cc:88`.
- TermSurf's renderer delegate disables print preview through
  `IsPrintPreviewEnabled() == false` in
  `chromium/src/content/libtermsurf_chromium/ts_pdf_renderer_support.cc:46`, so
  `PrintRenderFrameHelper::PrintNode()` takes the basic-print path instead of
  `print-node-preview`.
- In that basic-print path, `PrintRenderFrameHelper::InitPrintSettings()` calls
  `GetPrintManagerHost()->GetDefaultPrintSettings(&settings.params)` at
  `chromium/src/components/printing/renderer/print_render_frame_helper.cc:2486`.
  Chromium treats null `settings.params` as no available printer settings and
  aborts before showing a dialog at
  `chromium/src/components/printing/renderer/print_render_frame_helper.cc:2493`.

Runtime evidence from
`logs/issue-834-exp19-print-plumbing/pdf-native-print-summary.json`:

- `safety_gate_passed = true`;
- `probe_status = "ok"`;
- `probe_summary.print.status = "print-native-click-sent"`;
- `probe_summary.print.clicked = true`;
- `first_failing_hop = "browser-default-print-settings-null"`;
- `print_dialog_watch.dialog_observed = false`;
- `lpstat -o` and `lpstat -W completed -o` were empty before and after the
  probe, so no print job was submitted.

Native print trace evidence from
`logs/issue-834-exp19-print-plumbing/pdf-native-print.log`:

```text
post-invoke-print-dialog
invoke-print-dialog
client-print helper=present enable_printing=1
print-node
print-node-enter
print-node-call-print
print-init-settings-enter
get-default-print-settings-enter
get-default-print-settings-null
print-init-settings-failed
print-node-exit
```

This proves the path gets past the PDF plugin, past `client_->Print()`, past
`PrintRenderFrameHelper::PrintNode()`, and into the renderer print settings
initialization. It stops when the browser returns null default print settings.

Verification run:

```bash
node --check scripts/probe-pdf-save-print-title-local.mjs

rm -rf scripts/__pycache__
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-pdf-native-print.py
rm -rf scripts/__pycache__

python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp19-print-plumbing \
  --probe native-dialog \
  --allow-native-dialog-click

git diff --check
```

The guarded probe exited nonzero because it correctly classified
`browser-default-print-settings-null`, not because it submitted a print job.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

Verdict: **Approved**.

The reviewer found no Required, Optional, or Nit findings.

The reviewer verified that the plan commit was still `HEAD`, no result commit
had been made yet, the README marked Experiment 19 as `Pass`, the main diff was
limited to the two issue docs plus probe/harness scripts, `chromium/src` was
clean, and the logs/source supported `browser-default-print-settings-null` as
the first failing hop.

## Conclusion

Roamium native PDF print is blocked after renderer-side `PrintNode()` begins the
basic-print path. The first missing hop is browser default print settings:
`PrintRenderFrameHelper` asks the browser for default print settings, gets null,
and aborts before `DidShowPrintDialog()` or `ScriptedPrint()`.

The next experiment should fix browser-side print settings/manager integration
for Roamium. The likely implementation direction is to provide the browser-side
printing manager and default settings path expected by `PrintRenderFrameHelper`,
while preserving the safety-gated native print probe as the regression check.
