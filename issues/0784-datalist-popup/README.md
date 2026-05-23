+++
status = "open"
opened = "2026-05-23"
+++

# Issue 784: Datalist suggestions do not open

## Goal

Make `<input list="...">` datalist suggestions work in TermSurf's Chromium
engine, without regressing the native popup fixes completed in the preceding
native-popup issues.

After the datalist bug is fixed, perform the remaining native-popup diagnostic
log cleanup that is safe to remove.

## Background

This is the final known native popup bug from the current series.

The earlier work established and fixed several distinct native-popup failure
modes:

- [Issue 779](../0779-date-picker-popup-position/README.md) fixed PagePopup
  placement for date/time/color-style controls. The critical invariant from that
  issue is the `WebPagePopupImpl::SetWindowRect` y-axis correction: when Blink
  asks to place a PagePopup at the input's bottom edge, TermSurf corrects the
  popup y back to the input anchor y before passing the rect downstream.
- [Issue 782](../0782-native-popup-followups/README.md) fixed native widgets
  stopping after `<select>` interactions. The root cause was an invisible
  Chromium Shell window overlapping Wezboard while still accepting AppKit mouse
  events. The fix made TermSurf-managed Shell windows consistently
  mouse-transparent with `setIgnoresMouseEvents:YES`.
- [Issue 783](../0783-native-popup-remainders/README.md) fixed PagePopup
  dismissal on Cmd-Tab and the `<select>` x-position bug. Cmd-Tab dismissal now
  flows through `SetGuiActive`, and selects use direct `NSMenu` placement rather
  than the `NSPopUpButtonCell` path that shifted the menu left.

The native popup test page still includes one failing control: the datalist
input. The field accepts text, but the browser suggestions do not appear. This
appears to be a different widget family than the fixed PagePopup and select-menu
paths, so it should be investigated independently.

## Known Good Invariants

Do not regress these while fixing datalist:

- date/time/date-time/color PagePopup y-position remains correct;
- date/time/date-time/color PagePopups dismiss on Cmd-Tab;
- native widgets still open after a select interaction;
- select dropdown x-position remains correct with direct `NSMenu`;
- Shell windows remain mouse-transparent with `setIgnoresMouseEvents:YES`;
- `SetGuiActive` continues to restore page focus on app reactivation.

If any of these regress, stop and fix the regression before continuing.

## Initial Analysis

Datalist suggestions may use a path that differs from both:

- Blink PagePopup controls, such as `DateTimeChooserImpl` and related
  `WebPagePopupImpl` flows; and
- `<select>` menus, which flow through `RenderFrameHostImpl::ShowPopupMenu`,
  `PopupMenuHelper`, `RenderWidgetHostNSViewBridge::DisplayPopupMenu`, and
  `WebMenuRunner`.

The first experiment should identify which Chromium path a datalist suggestion
attempt takes on macOS in TermSurf:

- whether Blink attempts to open a PagePopup;
- whether it uses an Autofill-style popup;
- whether it sends a browser-side popup request that is suppressed;
- whether input/focus state prevents the datalist trigger from reaching the
  suggestion-open path;
- whether a popup opens but is hidden, offscreen, transparent, behind another
  window, or immediately dismissed.

Only after that path is known should the issue attempt a fix.

## Experiments

### Experiment 1: Code analysis of the datalist popup path

#### Description

Before adding more logs, analyze Chromium's datalist implementation to identify
which subsystem owns `<input list>` suggestions and where Roamium/content_shell
is likely missing support.

This experiment is code analysis only. It must not modify Chromium, Roamium,
Wezboard, the protocol, or the test page.

#### Changes

No code changes.

Read the relevant Chromium paths:

- Blink form-control trigger:
  `third_party/blink/renderer/core/html/forms/text_field_input_type.cc`
- Blink chrome client bridge:
  `third_party/blink/renderer/core/page/chrome_client_impl.cc`
- Blink Autofill client interface:
  `third_party/blink/public/web/web_autofill_client.h`
- Renderer Autofill implementation:
  `components/autofill/content/renderer/autofill_agent.cc`
- Browser Autofill suggestion path:
  `components/autofill/core/browser/ui/autofill_external_delegate.cc`
- Chrome renderer setup: `chrome/renderer/chrome_content_renderer_client.cc`
- content_shell renderer setup:
  `content/shell/renderer/shell_content_renderer_client.cc`
- content_shell main delegate: `content/shell/app/shell_main_delegate.cc`

#### Verification

The analysis is complete when the issue records:

- which Chromium subsystem owns datalist suggestions;
- whether datalist uses PagePopup, `<select>` menu plumbing, or another popup
  family;
- the first likely missing link in Roamium/content_shell;
- whether existing native-popup logs are expected to fire for datalist;
- the smallest useful logging plan for the next experiment.

**Result:** Pass

Datalist suggestions use Chromium's Autofill suggestion infrastructure, not the
PagePopup path used by date/time/color controls and not the AppKit menu path
used by `<select>`.

The normal Blink trigger is:

1. `DataListIndicatorElement::DefaultEventHandler(...)` or
   `TextFieldInputType::OpenPopupView()` decides the datalist suggestions should
   open.
2. Blink calls
   `ChromeClientImpl::OpenTextDataListChooser(HTMLInputElement& input)`.
3. `ChromeClientImpl` calls `AutofillClientFromFrame(...)`.
4. If a `WebAutofillClient` exists, Blink calls
   `fill_client->OpenTextDataListChooser(WebInputElement(&input))`.
5. Chromium's `AutofillAgent::OpenTextDataListChooser(...)` calls
   `ShowSuggestions(...)` with trigger source `kOpenTextDataListChooser`.
6. The Autofill browser side eventually reaches
   `AutofillExternalDelegate::OnQuery(...)` and
   `AutofillExternalDelegate::ShowSuggestions(...)`, where datalist options are
   inserted into the suggestion list and shown through the Autofill popup UI.

The important difference is the renderer setup. Chrome installs Autofill support
in `ChromeContentRendererClient::RenderFrameCreated(...)`:

- it creates a `PasswordAutofillAgent`;
- it creates a `PasswordGenerationAgent`;
- it constructs `new AutofillAgent(...)`;
- the `AutofillAgent` constructor calls
  `render_frame->GetWebFrame()->SetAutofillClient(this)`.

Roamium is based on content_shell, not Chrome. content_shell's
`ShellMainDelegate::CreateContentRendererClient()` creates a
`ShellContentRendererClient`, and
`ShellContentRendererClient::RenderFrameCreated(...)` only installs
`ShellRenderFrameObserver`. It does not install `AutofillAgent`, and therefore
does not appear to install a `WebAutofillClient` on the Blink frame.

That makes the most likely failure boundary:

```text
ChromeClientImpl::OpenTextDataListChooser(...)
  -> AutofillClientFromFrame(frame) returns null
  -> no AutofillAgent::OpenTextDataListChooser(...)
  -> no browser-side Autofill query
  -> no visible datalist suggestions
```

This also explains why the existing native-popup logs did not settle the issue.
The PagePopup logs from Issue 779 and the `<select>` menu logs from Issues 782
and 783 are on the wrong widget families. There is an existing
`[issue-779-trace] AutofillExternalDelegate::ShowSuggestions` log in the browser
Autofill path, but if the renderer has no `WebAutofillClient`, execution never
reaches it.

#### Conclusion

The datalist bug is most likely missing Autofill plumbing in the content_shell
embedding used by Roamium. This is not a geometry bug, an AppKit popup placement
bug, or a PagePopup lifecycle bug.

The next experiment should be a narrow logging pass that proves or disproves the
missing-client boundary:

- log `ChromeClientImpl::OpenTextDataListChooser(...)` with whether
  `AutofillClientFromFrame(...)` is null;
- log `ChromeClientImpl::TextFieldDataListChanged(...)` with the same client
  presence check;
- log `ShellContentRendererClient::RenderFrameCreated(...)` so the trace proves
  the content_shell renderer client is the active renderer client;
- log `AutofillAgent::OpenTextDataListChooser(...)` and
  `AutofillAgent::ShowSuggestions(...)`, if reached;
- log whether a browser-side `ContentAutofillClient` exists for the Shell
  `WebContents`, if a renderer Autofill query reaches the browser.

Expected result: the Blink datalist trigger fires, but
`AutofillClientFromFrame(...)` is null. If confirmed, the fix should be designed
around adding the minimal Autofill/datalist support required by the
content_shell/Roamium embedding, without importing the full Chrome browser UI.

### Experiment 2: Trace the datalist Autofill boundary

#### Description

Add a small, read-only trace to prove exactly where the datalist open request
stops.

Experiment 1 found that datalist suggestions should flow through Blink's
Autofill client:

```text
ChromeClientImpl::OpenTextDataListChooser
  -> AutofillClientFromFrame
  -> AutofillAgent::OpenTextDataListChooser
  -> AutofillAgent::ShowSuggestions
  -> browser Autofill query
  -> AutofillExternalDelegate::ShowSuggestions
```

The current hypothesis is that Roamium/content_shell does not install
`AutofillAgent`, so `AutofillClientFromFrame(...)` returns null and the request
becomes a no-op before any browser-side Autofill code runs.

This experiment must only add logs. Do not install Autofill, do not change popup
behavior, do not change focus behavior, and do not clean up unrelated logs.

#### Non-Negotiable Invariants

Do not touch the existing native-popup fixes:

- do not modify `WebPagePopupImpl::SetWindowRect` or the PagePopup y-axis
  correction;
- do not modify Shell window movement or any `setIgnoresMouseEvents:YES`
  reassertion;
- do not modify `SetGuiActive`;
- do not modify `WebMenuRunner` direct `NSMenu` select placement;
- do not modify the test page.

If the date/time/color/select invariants regress after this logging patch, the
experiment fails.

#### Changes

Create a new Chromium branch for Issue 784, branched from the current Issue 783
Chromium tip, and register it in `chromium/README.md`.

Add trace logs gated by the existing `TERMSURF_ISSUE_779_TRACE=1` gate and the
existing `[issue-779-trace]` prefix. Use a new event label such as
`datalist_autofill` so extraction is precise.

1. In `third_party/blink/renderer/core/page/chrome_client_impl.cc`, log at the
   top of `ChromeClientImpl::OpenTextDataListChooser(...)`:
   - input element pointer;
   - document/frame pointers;
   - whether `AutofillClientFromFrame(...)` is null;
   - whether the input has a datalist;
   - current input value length;
   - owner frame URL if cheaply available.

   This is the primary smoking-gun log. If it fires with
   `autofill_client_present=false`, the missing-client hypothesis is confirmed.

2. In the same file, log `ChromeClientImpl::TextFieldDataListChanged(...)` with
   the same `AutofillClientFromFrame(...)` presence check.

   This tells us whether datalist option changes are also being dropped because
   the frame has no Autofill client.

3. In `content/shell/renderer/shell_content_renderer_client.cc`, log
   `ShellContentRendererClient::RenderFrameCreated(...)`.

   This confirms that Roamium is using content_shell's renderer client path, not
   Chrome's renderer client path.

4. In `components/autofill/content/renderer/autofill_agent.cc`, log:
   - the `AutofillAgent` constructor after it calls `SetAutofillClient(this)`;
   - `AutofillAgent::OpenTextDataListChooser(...)`;
   - the beginning of `AutofillAgent::ShowSuggestions(...)`, including the
     trigger source and any early-return reason that prevents a browser query.

   Log the trigger source symbolically if Chromium already has a helper for
   `AutofillSuggestionTriggerSource`. If no helper exists, log the raw enum
   value and include enough context in the log label to make
   `kOpenTextDataListChooser` recognizable.

   Expected result for the current hypothesis: none of these logs fire in the
   datalist click run, including the constructor log. That non-appearance is
   itself confirmation that content_shell never installs `AutofillAgent`. If
   they do fire, the missing-client hypothesis is wrong and the trace should
   show the next suppression point.

5. In `components/autofill/core/browser/ui/autofill_external_delegate.cc`, keep
   the existing `AutofillExternalDelegate::ShowSuggestions` trace and add one
   lightweight log to `AutofillExternalDelegate::OnQuery(...)`:
   - trigger source;
   - `update_datalist`;
   - datalist option count;
   - caret bounds;
   - field bounds.

   Expected result for the current hypothesis: this log does not fire. If it
   does fire, browser-side Autofill is receiving the query and the bug is later
   in suggestion UI display.

6. Do not add high-volume per-mouse, input-router, or AppKit window logs. Those
   were useful for earlier issues but are not part of the datalist hypothesis.

#### Verification

1. Build Chromium with the project script:

   ```bash
   scripts/build.sh chromium
   ```

2. Build the other components normally if needed:

   ```bash
   scripts/build.sh roamium
   scripts/build.sh wezboard
   scripts/build.sh webtui
   ```

3. Run a quick invariant check without focusing datalist first:
   - open the native popup test page;
   - open a date picker and confirm the y-position is still correct;
   - with the date picker still open, Cmd-Tab to another app and confirm the
     picker dismisses; Cmd-Tab back and confirm the page is still usable;
   - open a select dropdown and confirm the x-position is still correct;
   - dismiss the select, then open the date picker again and confirm native
     widgets still work.

4. Run the datalist trace with:

   ```bash
   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-784-exp2-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-784-exp2-wezboard.log
   ```

5. In `web`, open the native popup test page.

6. Test the exact datalist control on the page:
   - the control is `input#browser`;
   - it has `list="browsers"`;
   - its initial value is `Roamium`;
   - valid options include `Roamium`, `Surfari`, `Waterwolf`, and `Girlbat`.

   Click into `input#browser`, select the existing text, type `S`, then perform
   the normal datalist-open action for the browser UI under test: click the
   datalist affordance if it is visible, or press ArrowDown while the caret is
   in the field. `S` should match `Surfari`, so the test is not blocked by an
   empty suggestion set.

7. Stop immediately after the datalist fails or succeeds. Do not continue with
   other controls after the datalist attempt.

8. Extract the relevant trace lines:

   ```bash
   rg "\\[issue-779-trace\\].*(datalist_autofill|OpenTextDataListChooser|TextFieldDataListChanged|AutofillAgent|AutofillExternalDelegate|ShellContentRendererClient)" \
     logs/issue-784-exp2-wezboard.log \
     logs/issue-784-exp2-state
   ```

9. After committing the Chromium trace patch, export the cumulative Issue 784
   patch archive to `chromium/patches/issue-784/`. The new trace patch should
   appear after the inherited Issue 783 patches, currently as
   `0019-Trace-datalist-Autofill.patch`. Verify that the new patch applies
   cleanly.

#### Pass Criteria

The experiment passes if the trace names the first missing boundary.

Expected pass shape:

- `ShellContentRendererClient::RenderFrameCreated(...)` fires;
- `ChromeClientImpl::OpenTextDataListChooser(...)` fires;
- that log says `autofill_client_present=false`;
- no `AutofillAgent::OpenTextDataListChooser(...)` log fires;
- no browser-side `AutofillExternalDelegate::OnQuery(...)` log fires.

That result would confirm that content_shell/Roamium lacks the renderer Autofill
client required to open datalist suggestions.

#### Partial Criteria

If `AutofillAgent::OpenTextDataListChooser(...)` fires, the missing-client
hypothesis is wrong. The result is still useful if the trace records the next
early-return reason in `AutofillAgent::ShowSuggestions(...)`.

If `AutofillExternalDelegate::OnQuery(...)` fires, the renderer and browser
Autofill query path is alive. The next experiment should target the Autofill
popup UI display path rather than client installation.

#### Failure Criteria

The experiment fails if:

- any non-log behavior changes are made;
- any known-good native popup invariant regresses;
- the trace does not show whether `AutofillClientFromFrame(...)` is null;
- broad mouse/input/AppKit logs are reintroduced and drown out the datalist
  signal.

#### Expected Interpretation

If the expected pass shape is observed, Experiment 3 should design the minimal
fix for installing datalist-capable Autofill support in Roamium/content_shell.
That fix should avoid importing Chrome browser UI wholesale. The first design
question will be whether to reuse Chromium's `AutofillAgent` plus a small
content-side `AutofillClient`, or to implement a smaller datalist-only
`WebAutofillClient` for TermSurf's embedding. Choose the fix direction in
Experiment 3 based on dependency cost and which popup UI surface is safer for
TermSurf.

**Result:** Pass

The trace confirmed the missing-client hypothesis.

The renderer path is definitely content_shell:

```text
datalist_autofill event=ShellContentRendererClient::RenderFrameCreated
```

Blink also definitely attempts to open the datalist chooser. The trace recorded
multiple calls to:

```text
datalist_autofill event=ChromeClientImpl::OpenTextDataListChooser
```

Each call identified the real datalist input on the test page:

```text
has_datalist=1
value_length=7
url="http://localhost:9616/test-native-popups.html"
```

But every open attempt reported:

```text
autofill_client_present=0
```

No downstream Autofill logs fired:

- no `AutofillAgent::AutofillAgent.after_set_client`;
- no `AutofillAgent::OpenTextDataListChooser`;
- no `AutofillAgent::ShowSuggestions`;
- no `AutofillExternalDelegate::OnQuery`.

This proves the datalist request reaches Blink's datalist-open hook, but dies
immediately because the frame has no `WebAutofillClient`.

#### Conclusion

The datalist bug is not caused by AppKit, PagePopup geometry, select-menu state,
Shell window mouse transparency, or Cmd-Tab focus handling.

The concrete bug is:

```text
content_shell/Roamium does not install Chromium's Autofill renderer plumbing.
ChromeClientImpl::OpenTextDataListChooser(...)
  -> AutofillClientFromFrame(frame) returns null
  -> no AutofillAgent::OpenTextDataListChooser(...)
  -> no browser-side Autofill query
  -> no datalist suggestions
```

Experiment 3 should design and implement the smallest viable datalist-capable
Autofill integration for Roamium/content_shell. The main design choice is
whether to reuse Chromium's existing `AutofillAgent` plus the browser-side
Autofill popup machinery, or to add a narrower TermSurf-specific
`WebAutofillClient` that only supports datalist suggestions.

## Cleanup Requirement

Do not perform broad log cleanup before the datalist fix. Some remaining
native-popup traces may still be useful for identifying the datalist path.

After datalist suggestions work and the known-good invariants above are
verified, perform a dedicated cleanup pass:

- remove obsolete diagnostic logs from Issues 779, 782, and 783 that no longer
  serve the datalist investigation;
- preserve low-volume logs only if they are still useful for future popup
  regression diagnosis;
- do not remove the behavioral fixes that were introduced in the same commits as
  trace code;
- regenerate the Chromium patch archive after any Chromium cleanup commit.

The cleanup must be done by reviewed hunks, not by blanket-reverting historical
trace commits.
