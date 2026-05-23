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

### Experiment 3: Prototype minimal datalist Autofill plumbing

#### Description

Experiment 2 proved that the datalist request reaches Blink and then dies
because the frame has no `WebAutofillClient`.

This experiment should try the shortest plausible fix path: install Chromium's
existing Autofill renderer plumbing in the content_shell/Roamium embedding, then
add only the browser-side support required to let datalist suggestions reach a
popup boundary.

This is an implementation spike, not another geometry or AppKit experiment. The
goal is to make datalist work if the existing Autofill UI can be reused without
pulling in Chrome's full browser stack. If the implementation reaches a
dependency wall, stop at the smallest verified boundary and record that result.

This experiment tries the existing `AutofillAgent` path before a custom
TermSurf-only `WebAutofillClient` because it proves the standard Chromium
renderer-to-browser datalist data path with the least new UI code. Even if the
existing popup UI proves too Chrome-specific to reuse, a useful Partial result
will give the next experiment a known-good suggestion source instead of making
it rediscover datalist extraction from scratch.

The expected outcome is probably Partial, not necessarily Pass. Chromium's data
plumbing lives mostly under `components/autofill`, but the desktop popup UI is
Chrome UI code. If the renderer and browser receive datalist suggestions but the
visible popup would require `chrome/browser` profile services or Chrome-only
Views controllers, stop there and record the UI dependency boundary.

#### Non-Negotiable Invariants

Do not regress the native-popup fixes from Issues 779, 782, and 783:

- do not modify `WebPagePopupImpl::SetWindowRect` or the PagePopup y-axis
  correction;
- do not modify Shell window movement or any `setIgnoresMouseEvents:YES`
  reassertion;
- do not modify `SetGuiActive`;
- do not modify `WebMenuRunner` direct `NSMenu` select placement;
- do not modify the native popup test page;
- do not reintroduce broad per-mouse, input-router, or AppKit tracing.

If any invariant regresses, stop and fix that regression before continuing.

#### Changes

Continue on the Issue 784 Chromium branch, `148.0.7778.97-issue-784`.

1. Pre-code dependency check.

   Before writing implementation code, inspect the constructors and GN
   dependencies for:
   - `components/autofill/content/renderer/autofill_agent.*`;
   - `components/password_manager/content/renderer/password_autofill_agent.*`;
   - `components/password_manager/content/renderer/password_generation_agent.*`;
   - Chrome's `chrome/renderer/chrome_content_renderer_client.cc` Autofill setup
     block.

   Confirm whether `PasswordAutofillAgent` and `PasswordGenerationAgent` can be
   constructed in content_shell without enabling password storage or importing
   broad password-manager/browser infrastructure. If they cannot, do not force
   the `AutofillAgent` path. Record that as an immediate dependency wall and
   pivot the next experiment to a narrow datalist-only `WebAutofillClient`.

2. Renderer setup: install Autofill for content_shell frames.

   In `content/shell/renderer/shell_content_renderer_client.cc`, update
   `ShellContentRendererClient::RenderFrameCreated(...)` to install the same
   core renderer object that Chrome installs:
   - create the password Autofill support objects required by `AutofillAgent`;
   - construct `autofill::AutofillAgent`;
   - pass `render_frame->GetAssociatedInterfaceRegistry()` as the associated
     interface registry;
   - preserve the existing `ShellRenderFrameObserver` setup.

   Use Chrome's
   `chrome/renderer/chrome_content_renderer_client.cc::RenderFrameCreated(...)`
   as the reference shape, specifically the local block that constructs
   `PasswordAutofillAgent`, `PasswordGenerationAgent`, and `AutofillAgent`. Do
   not copy unrelated Chrome renderer behavior.

   Expected immediate effect: Experiment 2's
   `ChromeClientImpl::OpenTextDataListChooser(...)` log should change from
   `autofill_client_present=0` to `autofill_client_present=1`, and
   `AutofillAgent::OpenTextDataListChooser(...)` should fire.

3. Browser setup: attach the smallest content Autofill client that can receive
   datalist queries.

   `AutofillAgent` sends its browser query through
   `components/autofill/content/browser/ContentAutofillDriverFactory`, which
   requires a `ContentAutofillClient` attached to the `WebContents`. Add a
   content_shell/Roamium-specific client under `content/shell/browser/`, for
   example `ShellContentAutofillClient`, and create it for each Shell
   `WebContents`.

   The client must be datalist-focused:
   - implement unrelated Autofill services as safe no-op or null-returning
     methods;
   - do not enable address, card, payments, identity, sync, strike database, or
     password storage features;
   - implement enough of `CreateManager(...)`, `ShowAutofillSuggestions(...)`,
     `UpdateAutofillDataListValues(...)`, and `HideAutofillSuggestions(...)` for
     datalist suggestions to reach a visible or inspectable boundary;
   - reject or no-op non-datalist Autofill flows.

   Use `components/autofill/content/browser/test_content_autofill_client.*` only
   as a reference for how a `ContentAutofillClient` can be attached and stubbed.
   Do not add test-only dependencies to production code.

   Expect this class to be substantial even if most methods are stubs. That is
   acceptable only if the real behavior stays datalist-scoped. The methods that
   need meaningful behavior for this experiment are the datalist suggestion,
   datalist update, hide, and manager creation boundaries; unrelated services
   should remain null or no-op.

4. Popup display: prefer reusing existing Autofill suggestion UI, but do not
   import Chrome wholesale.

   If the content Autofill client can reuse Chromium's existing Autofill popup
   UI without importing `chrome/browser` profile services or Chrome-only UI
   infrastructure, wire it far enough that selecting `Surfari` from the datalist
   fills `input#browser`.

   If the existing UI requires broad Chrome dependencies, do not keep expanding
   the patch. Stop after the browser client receives the datalist suggestions
   and records the exact missing UI boundary. That result is Partial and should
   feed a narrower custom datalist popup experiment.

5. Build wiring.

   Update only the GN targets required for the renderer agent and the minimal
   browser-side datalist client. Keep dependency additions narrow. If adding one
   Autofill dependency pulls in large Chrome subsystems, stop and record the
   dependency boundary rather than forcing it.

   Expected narrow dependencies may include:
   - `//components/autofill/content/renderer`;
   - `//components/autofill/content/browser`;
   - `//components/autofill/core/browser`;
   - `//components/password_manager/content/renderer`, only if the pre-code
     check proves those renderer objects can be constructed without enabling
     password storage.

   Treat `chrome/browser`, Chrome profile services, sync, identity, payments,
   address storage, and Chrome Autofill Views controllers as the dependency wall
   unless a very small isolated dependency proves otherwise.

6. Keep the existing `datalist_autofill` trace lines from Experiment 2.

   Add only low-volume logs needed to interpret this experiment:
   - renderer installed Autofill client for a frame;
   - browser attached `ShellContentAutofillClient` to a `WebContents`;
   - datalist query reached the browser client;
   - suggestion count and first few suggestion labels/values;
   - visible popup shown, if a UI path is reached;
   - suggestion accepted, if selection is wired.

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

3. Run the invariant checks first:
   - open the native popup test page;
   - open a date picker and confirm the y-position is still correct;
   - with the date picker still open, Cmd-Tab to another app and confirm the
     picker dismisses; Cmd-Tab back and confirm the page is still usable;
   - open a select dropdown and confirm the x-position is still correct;
   - dismiss the select, then open the date picker again and confirm native
     widgets still work.

4. Run the datalist test with trace enabled:

   ```bash
   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-784-exp3-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-784-exp3-wezboard.log
   ```

5. In `web`, open the native popup test page.

6. Test `input#browser`:
   - click into the datalist field;
   - select the existing `Roamium` text;
   - type `S`;
   - click the datalist affordance if visible, or press ArrowDown.

   `S` should match `Surfari`. If a suggestion popup appears, select `Surfari`
   and verify the field value changes to `Surfari`.

7. If a datalist suggestion popup appears, test Cmd-Tab dismissal for the new
   popup family:
   - open the datalist suggestions again;
   - Cmd-Tab to another app;
   - confirm the datalist suggestions dismiss and do not remain visible on
     screen;
   - Cmd-Tab back and confirm the page is still usable.

   Autofill suggestions are not PagePopups, so this must be verified separately
   from the existing date/time/color Cmd-Tab invariant.

8. Stop after the datalist succeeds or reaches the first new failure boundary.
   Do not continue with unrelated controls after the datalist attempt.

9. Extract the relevant trace:

   ```bash
   rg "\\[issue-779-trace\\].*datalist_autofill" \
     logs/issue-784-exp3-wezboard.log \
     logs/issue-784-exp3-state
   ```

10. Commit Chromium changes on the Issue 784 branch and regenerate
    `chromium/patches/issue-784/` after a successful or useful partial result.

#### Pass Criteria

The experiment passes if datalist suggestions visibly work:

- `ChromeClientImpl::OpenTextDataListChooser(...)` reports
  `autofill_client_present=1`;
- `AutofillAgent::OpenTextDataListChooser(...)` fires;
- the browser-side datalist client receives suggestions including `Surfari`;
- a visible popup appears for `input#browser`;
- selecting `Surfari` fills the field with `Surfari`;
- the datalist popup dismisses on Cmd-Tab;
- all known-good native-popup invariants still pass.

#### Partial Criteria

The experiment is Partial if it proves the next boundary but does not yet make a
visible popup work. Useful partial outcomes include:

- renderer Autofill installation works, but the browser query is rejected
  because no `ContentAutofillClient` or driver factory is attached;
- the browser client receives datalist suggestions, but Chromium's existing
  Autofill popup UI requires broad Chrome dependencies;
- the popup appears and can be navigated, but accepting a suggestion does not
  update the input;
- dependency additions become too broad and the trace identifies the smallest
  missing production interface.

#### Failure Criteria

The experiment fails if:

- it imports broad Chrome browser/profile UI infrastructure without a narrow
  datalist reason;
- it enables address/card/password Autofill features unintentionally;
- it changes the test page;
- any known-good native-popup invariant regresses;
- it makes the datalist state less diagnosable than Experiment 2.

#### Expected Interpretation

If this passes, datalist is fixed and the next experiment should be the promised
native-popup log cleanup pass.

If renderer Autofill installation works but browser-side Autofill is too large
to integrate safely, the next experiment should implement a narrow
TermSurf/content_shell datalist popup UI using the already-proven datalist
option extraction boundary.

If the browser receives suggestions but selection cannot be accepted through the
existing Autofill delegate path, the next experiment should focus only on
acceptance and value-setting, not on popup discovery.

**Result:** Fail

Manual testing showed no visible improvement. After selecting the existing
`Roamium` text in `input#browser`, typing `S`, and attempting to open datalist
suggestions, no datalist popup appeared. From the user's perspective, nothing
detectable changed: the field still accepted text, but no dropdown or suggestion
box appeared.

This failed the experiment's visible-behavior goal. The implemented slice may
still have moved the internal boundary from "Blink has no `WebAutofillClient`"
to a later Autofill boundary, but the tested behavior did not improve and the
experiment did not produce a working or visibly partial datalist UI.

#### Conclusion

Installing the stock Chromium `AutofillAgent` path in content_shell is not, by
itself, a useful fix for TermSurf's datalist bug. The next experiment should not
continue trying to pull in Chrome's full Autofill UI stack. It should pivot to a
narrow datalist-only implementation that directly extracts
`WebInputElement::FilteredDataListOptions()` and presents those options through
a TermSurf/content_shell-controlled popup path.

### Experiment 4: Implement a narrow content_shell datalist popup

#### Description

Experiment 3 showed that importing Chrome's normal Autofill UI path is the wrong
direction for TermSurf. The datalist bug needs a small embedding-level
implementation, not Chrome profile/UI infrastructure.

This experiment should implement a datalist-only popup for content_shell/Roamium
using Blink's public datalist APIs:

```text
ChromeClientImpl::OpenTextDataListChooser(...)
  -> WebAutofillClient::OpenTextDataListChooser(...)
  -> WebInputElement::FilteredDataListOptions()
  -> content_shell-owned popup UI
  -> WebFormControlElement::SetValue(..., send_events=true)
```

This deliberately bypasses Chrome's `ContentAutofillClient` and Chrome Autofill
Views UI. The goal is one working feature: native `<input list>` suggestions for
TermSurf's Chromium engine.

#### Non-Negotiable Invariants

Do not regress the existing native-popup fixes:

- do not modify `WebPagePopupImpl::SetWindowRect` or the PagePopup y-axis
  correction;
- do not modify Shell window movement or any `setIgnoresMouseEvents:YES`
  reassertion;
- do not modify `SetGuiActive`;
- do not modify `WebMenuRunner` direct `NSMenu` select placement;
- do not modify the native popup test page;
- do not import Chrome browser profile services, Chrome Autofill Views
  controllers, sync, identity, payments, address storage, card storage, or
  password storage.

If any invariant regresses, stop and fix the regression before continuing.

#### Changes

Continue on the Issue 784 Chromium branch, `148.0.7778.97-issue-784`.

1. Replace the failed stock `AutofillAgent` attempt with a datalist-only
   renderer client.

   Add a small class under `content/shell/renderer/`, for example
   `ShellDatalistAutofillClient`, that:
   - inherits `content::RenderFrameObserver`;
   - inherits `blink::WebAutofillClient`;
   - calls `render_frame->GetWebFrame()->SetAutofillClient(this)` in its
     constructor;
   - clears the frame's Autofill client on destruction if still appropriate;
   - implements only `OpenTextDataListChooser(...)` and
     `DataListOptionsChanged(...)`;
   - leaves all other `WebAutofillClient` hooks as default no-ops.

   `ShellContentRendererClient::RenderFrameCreated(...)` should create this
   class instead of constructing `autofill::AutofillAgent`,
   `PasswordAutofillAgent`, or `PasswordGenerationAgent`.

2. Extract datalist options directly.

   In `OpenTextDataListChooser(...)`, use:

   ```text
   blink::WebInputElement::FilteredDataListOptions()
   blink::WebOptionElement::Value()
   blink::WebOptionElement::GetText()
   blink::WebOptionElement::Label()
   blink::WebOptionElement::IsEnabled()
   ```

   Keep only enabled options with non-empty values. Preserve both display text
   and value. The value is what should be inserted into the input.

3. Show a minimal macOS popup.

   On macOS, present the extracted options with a small AppKit `NSMenu` anchored
   to the input element's bounds.

   The first implementation may live in a `.mm` helper under
   `content/shell/renderer/` or `content/shell/browser/renderer_host/`, but it
   must stay content_shell-scoped. Do not use Chrome Autofill popup UI.

   Anchor placement should reuse the coordinate conventions already proven in
   Issues 779 and 783:
   - get the input bounds in the renderer frame;
   - convert to screen coordinates using the same webview/screen origin logic
     used by the existing popup traces;
   - place the menu below the input unless AppKit constrains it.

   If it is much simpler and safer to present the `NSMenu` at the current mouse
   location for this experiment, that is acceptable as a Partial result only. A
   passing result requires the popup to appear at the datalist input.

4. Accept a suggestion by setting the input value.

   When the user selects a menu item:
   - set the input's value to the chosen option value;
   - pass `send_events=true` so normal input/change behavior fires;
   - keep focus usable after the popup closes.

   Prefer `blink::WebFormControlElement::SetValue(value, true)` if it produces
   correct DOM behavior. If it does not, record the exact failure and make the
   result Partial.

5. Dismissal and lifecycle.

   The datalist popup must dismiss when:
   - the user selects an item;
   - the user clicks away;
   - the Wezboard app deactivates through Cmd-Tab.

   If Cmd-Tab dismissal does not come for free from the menu's modal AppKit
   behavior, wire it to the existing `SetGuiActive(false)` path or record that
   as a Partial boundary.

6. Build wiring.

   Remove any Experiment 3 dependency additions that are no longer needed, such
   as `//components/autofill/content/renderer`, if the new datalist-only client
   does not use them.

   Keep dependencies narrow:
   - Blink public web APIs are allowed;
   - content_shell renderer/browser helpers are allowed;
   - AppKit is allowed on macOS;
   - Chrome browser/UI/profile dependencies are not allowed.

7. Trace only the new datalist path.

   Keep low-volume `datalist_autofill` logs for:
   - datalist client installed;
   - `OpenTextDataListChooser(...)` fired;
   - number of filtered options;
   - first few option labels/values;
   - popup shown with anchor rect;
   - selected value;
   - value set success/failure;
   - popup dismissed reason.

   Do not reintroduce broad mouse/input/AppKit logs.

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

3. Run the invariant checks first:
   - open the native popup test page;
   - open a date picker and confirm the y-position is still correct;
   - with the date picker still open, Cmd-Tab to another app and confirm the
     picker dismisses; Cmd-Tab back and confirm the page is still usable;
   - open a select dropdown and confirm the x-position is still correct;
   - dismiss the select, then open the date picker again and confirm native
     widgets still work.

4. Run the datalist test with trace enabled:

   ```bash
   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-784-exp4-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-784-exp4-wezboard.log
   ```

5. In `web`, open the native popup test page.

6. Test `input#browser`:
   - click into the datalist field;
   - select the existing `Roamium` text;
   - type `S`;
   - click the datalist affordance if visible, or press ArrowDown;
   - confirm a popup appears with `Surfari`;
   - select `Surfari`;
   - confirm the field value becomes `Surfari`.

7. Test full-list behavior:
   - clear the datalist input;
   - open the datalist suggestions;
   - confirm the available browser options appear, including `Roamium`,
     `Surfari`, `Waterwolf`, and `Girlbat`.

8. Test dismissal:
   - open the datalist popup and click away;
   - confirm it dismisses;
   - open it again and Cmd-Tab away;
   - confirm it dismisses and does not remain visible on screen;
   - Cmd-Tab back and confirm the page remains usable.

9. Stop after the datalist succeeds or reaches the first new failure boundary.

10. Extract the relevant trace:

    ```bash
    rg "\\[issue-779-trace\\].*datalist_autofill" \
      logs/issue-784-exp4-wezboard.log \
      logs/issue-784-exp4-state
    ```

11. Commit Chromium changes on the Issue 784 branch and regenerate
    `chromium/patches/issue-784/` after a successful or useful partial result.

#### Pass Criteria

The experiment passes if:

- the datalist popup appears for `input#browser`;
- typing `S` filters the options to include `Surfari`;
- selecting `Surfari` sets the input value to `Surfari`;
- clearing the input can show the full datalist option set;
- click-away dismissal works;
- Cmd-Tab dismissal works;
- all known-good native-popup invariants still pass;
- no Chrome browser/profile/Autofill UI dependencies are imported.

#### Partial Criteria

The experiment is Partial if it proves the narrow path but one edge remains, for
example:

- options are extracted correctly but the popup anchor is wrong;
- the popup appears at the mouse location but not yet at the input;
- the popup appears and selection works, but Cmd-Tab dismissal needs one more
  hook;
- selection changes the visible value but does not fire the expected DOM events;
- the macOS implementation works but non-macOS behavior is still unimplemented.

#### Failure Criteria

The experiment fails if:

- it keeps pursuing Chrome's full Autofill UI stack;
- it imports unrelated Autofill storage/services;
- it changes the test page;
- it regresses any prior native-popup fix;
- it cannot extract datalist options through Blink's public APIs.

#### Expected Interpretation

If this passes, datalist is fixed and the next experiment should be the
native-popup log cleanup pass.

If option extraction works but UI placement is wrong, the next experiment should
be a small positioning fix, using the coordinate lessons from Issues 779
and 783.

If option extraction and UI placement work but value acceptance is wrong, the
next experiment should focus only on setting the input value and dispatching the
right DOM events.

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
