+++
status = "open"
opened = "2026-05-21"
+++

# Issue 782: Remaining native popup bugs

## Goal

Fix the native popup bugs that remain after Issue 779 without reopening that
large investigation. Each remaining bug should be isolated, logged, fixed, and
verified one at a time.

## Background

Issue 779 fixed the primary PagePopup y-axis placement bug for date-family
controls. Date, time, date-time, and color controls now appear at the correct y
position in the TermSurf webview overlay.

That work also showed that native widgets are not one unified system in
Chromium:

- date, time, date-time, and color controls use Blink PagePopup widgets;
- `<select>` uses Chromium's AppKit menu path;
- datalist still needs to be isolated because testing was blocked by later popup
  failures.

The remaining failures are separate enough that they should be handled in this
new issue instead of extending Issue 779.

## Remaining Bugs

### PagePopup remains visible after alt-tab

Date, time, date-time, and color popups can remain visible after the user
alt-tabs away from Wezboard. The owning TermSurf window is no longer visible or
active, but the native popup stays on screen.

This is likely a popup lifecycle, owner-window, focus, or deactivation issue. It
should be investigated with logs around window deactivation, PagePopup
visibility, popup widget ownership, and dismissal.

### Select dropdown has the wrong x position

The `<select>` dropdown now has the correct y position, but its x position is
wrong. This path is different from PagePopup:

```text
RenderFrameHostImpl::ShowPopupMenu
PopupMenuHelper::ShowPopupMenu
RenderWidgetHostNSViewBridge::DisplayPopupMenu
WebMenuRunner::runMenuInView
NSPopUpButtonCell
```

Issue 779 confirmed that Chromium logs the select anchor, but AppKit owns the
final menu placement after `NSPopUpButtonCell` takes over. The next select
experiment needs to capture or infer the final menu x position and compare it
against the anchor.

### Native widgets stop opening after select/datalist interactions

After interacting with `<select>` once or twice, native widgets stop opening for
the rest of the session. Later mouse movement still produces cursor updates, so
the browser is not dead, but new popup-open paths stop firing.

This may be an activation, focus, event dispatch, AppKit menu-tracking, or
popup-state cleanup bug. It should be treated independently from positioning
until logs identify where popup requests stop.

### Datalist does not work

Datalist could not be tested reliably because the post-select failure prevents
further native widgets from opening. Once the session-stopping popup failure is
understood, datalist should get its own clean trace and fix path.

## Approach

Do not try to fix every remaining bug in one experiment. Start with one bug,
design the smallest experiment that can identify its cause, record the result,
and only then move to the next bug.

The likely order is:

1. Post-select native-widget shutdown, because it contaminates multi-control
   test runs and can block every later popup experiment.
2. PagePopup alt-tab visibility, because it affects every PagePopup-family
   control that now has correct y placement.
3. Select dropdown x placement.
4. Datalist behavior.

The order may change if new logs show that two symptoms share one root cause.

## Experiments

### Experiment 1: Trace post-select popup shutdown

#### Description

After interacting with a `<select>` dropdown once or twice, later native widgets
stop opening for the rest of the session. Cursor updates still arrive, so the
browser process and basic mouse routing are alive. The missing signal is where a
later click stops:

- before Chromium receives mouse down/up;
- after Chromium receives input but before Blink activates the control;
- after Blink activation but before popup-open IPC;
- inside Chromium because popup/menu state still says a popup is active;
- inside AppKit because menu tracking or window activation did not unwind.

This experiment is logs-only. It must not change popup behavior. The goal is to
capture one clean sequence:

```text
open select -> close select -> click date -> click select again
```

If native widgets stop opening, the logs must identify the first missing
boundary in the second popup-open attempt.

#### Changes

1. **Create the Issue 782 Chromium branch.**

   This experiment modifies Chromium trace code, so it must use a new Chromium
   issue branch:

   ```text
   148.0.7778.97-issue-782
   ```

   Branch it from the tip of `148.0.7778.97-issue-779`, then register the new
   branch in `chromium/README.md`. Do not continue adding Chromium commits to
   the Issue 779 branch.

2. **Use the existing trace gate.**

   All new logs must be gated by the existing trace variable:

   ```text
   TERMSURF_ISSUE_779_TRACE=1
   ```

   Do not add a new behavior flag or a new issue-specific trace flag for this
   experiment.

3. **Keep the Issue 779 popup trace hooks.**

   Preserve existing logs for:
   - `RenderFrameHostImpl::ShowPopupMenu`;
   - `PopupMenuHelper::ShowPopupMenu`;
   - `RenderWidgetHostNSViewBridge::DisplayPopupMenu`;
   - `WebMenuRunner::runMenuInView`;
   - `DateTimeChooserImpl`;
   - `WebPagePopupImpl::SetWindowRect`;
   - `WebContentsImpl::ShowCreatedWidget`;
   - `RenderWidgetHostViewMac::InitAsPopup`.

4. **Define the popup sequence key.**

   Use one field name consistently:

   ```text
   popup_sequence=...
   ```

   In the browser process, `popup_sequence` is a process-local atomic counter.
   Increment it once per popup-open intent at the top browser entry:
   - `RenderFrameHostImpl::ShowPopupMenu` for `<select>` menus;
   - the browser-side PagePopup entry for date/time/color controls.

   Downstream browser/AppKit logs reuse that same value. Renderer-side logs do
   not need to share the counter; they join by timestamp plus frame/WebContents
   identity fields.

5. **Log select menu lifecycle cleanup.**

   In Chromium's select/AppKit path, add trace lines for:
   - menu open entry;
   - menu selection callback;
   - menu cancel/dismiss callback;
   - `PopupMenuHelper` close/destructor cleanup;
   - renderer/browser notification that the popup menu closed.

   Each line should include enough join fields to follow one select menu:

   ```text
   path=select
   popup_sequence=...
   webcontents=...
   rfh=...
   helper=...
   view/window pointer
   selected_or_cancelled=...
   helper_alive_before/after=...
   ```

   If Chromium already has a process-local popup/menu counter or helper pointer,
   log it. Do not add protocol fields.

6. **Log `popup_menu_helper_` state directly.**

   Specifically log `WebContentsViewMac::popup_menu_helper_.get()` at:
   - the top of `WebContentsViewMac::ShowPopupMenu`, before replacing or
     rejecting any helper;
   - the callback that should clear the helper when the menu closes;
   - `PopupMenuHelper::CloseMenu` and the `PopupMenuHelper` destructor.

   These logs must answer whether an old helper remains alive after the select
   menu closes.

7. **Log the top of the popup-open decision points.**

   Add trace lines before any early return or suppression in:
   - `RenderFrameHostImpl::ShowPopupMenu`;
   - the PagePopup open path used by date/time/color;
   - any known "popup already active" or "suppress popup" guard near those
     paths.

   These logs must answer whether the failed post-select click reaches the
   popup-open functions and, if it does, why the open is rejected.

8. **Log renderer-side popup intent and active state.**

   Add renderer-side trace lines for:
   - PagePopup open intent and close in `WebPagePopupImpl` / `WebViewImpl`;
   - date/time/color open intent before crossing to the browser;
   - select popup intent in the Blink select path;
   - any renderer-side "popup already open" state checked before sending popup
     IPC.

   These logs are required because the failure may happen before the browser
   receives a popup-open request. If all browser-side popup logs are silent, the
   renderer trace must still say whether Blink activated the control and whether
   it suppressed the popup.

9. **Log PagePopup close cleanup.**

   Mirror the close-path logging for PagePopup controls:
   - `WebPagePopupImpl::ClosePopup`;
   - the `WebViewImpl` popup-open/closed state used by PagePopup;
   - browser-side PagePopup widget destruction or close notification if present.

   The trigger appears to be `<select>`, but the symptom affects date/time/color
   too, so both popup families need close-state visibility.

10. **Log mouse click delivery after select closes.**

    Add trace lines for the macOS `RenderWidgetHostViewMac` input path that sees
    mouse down/up events for the main webview after the select menu closes.

    Include:

    ```text
    event type
    location in window/view
    target RenderWidgetHostViewMac pointer
    webcontents pointer if available
    window isKey/isMain/isVisible
    firstResponder class if cheap to log
    ```

    The purpose is not to trace every cursor move. Log clicks only, or keep move
    logs out of the experiment trace, so the result is readable.

11. **Log window activation state around AppKit menu tracking.**

    In the AppKit select menu path, log before opening the menu, after it
    returns, and again at the next attempted popup/click boundary:

    ```text
    window isKey/isMain/isVisible
    app isActive
    firstResponder
    currentEvent type
    ```

    If AppKit leaves the hidden/transparent Chromium shell window in a different
    activation state after the select menu closes, this should make it visible.

12. **Add one concise summary line for each attempted popup.**

    Emit a summary line at each attempted popup boundary:

    ```text
    native_popup_attempt
      attempt=N
      control=select|date|unknown
      boundary=input|blink|browser-popup-open|appkit-open|cleanup
      outcome=entered|opened|closed|cancelled|suppressed|missing
      reason=...
    ```

    The `boundary` field is a fixed enum for this experiment:
    - `input`
    - `blink`
    - `browser-popup-open`
    - `appkit-open`
    - `cleanup`

    Do not invent new boundary values without updating this experiment text.

    The summary does not need to be perfect automation. It only needs to make
    the trace easy to scan and compare with the detailed lines.

#### Verification

0. Confirm the Chromium branch:

   ```bash
   cd /Users/ryan/dev/termsurf/chromium/src
   git branch --show-current
   ```

   The branch must be:

   ```text
   148.0.7778.97-issue-782
   ```

1. Build through the project scripts:

   ```bash
   cd /Users/ryan/dev/termsurf
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh webtui --release
   scripts/build.sh wezboard
   ```

2. Start the test page server:

   ```bash
   cd /Users/ryan/dev/termsurf
   bun test-html/server.ts
   ```

3. Run a trace-off baseline.

   Start Wezboard once without `TERMSURF_ISSUE_779_TRACE`, confirm the test page
   is usable, and confirm no `[issue-779-trace]` lines are emitted. Stop that
   run before starting the traced run.

4. Start Wezboard with deterministic logs:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-782-exp1-state/termsurf

   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-782-exp1-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-782-exp1-wezboard.log
   ```

5. Launch the TUI:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/release/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

6. Run one controlled interaction sequence:
   - click the date control and confirm it opens;
   - close it;
   - click the `<select>` dropdown and choose or dismiss one item;
   - click the date control again;
   - click the `<select>` dropdown again;
   - if widgets stop opening, stop the test immediately and preserve the logs.

7. Extract the trace:

   ```bash
   rg -a "\[issue-779-trace\]|native_popup_attempt|popup_sequence|popup_menu_helper_|ShowPopupMenu|PopupMenuHelper|DisplayPopupMenu|WebMenuRunner|DateTimeChooserImpl|WebPagePopupImpl|WebViewImpl|ShowCreatedWidget|InitAsPopup|mouse.*down|mouse.*up|firstResponder|isKey|isMain|app isActive|menu.*close|menu.*cancel|menu.*dismiss|popup.*active|popup.*suppress" \
     logs/issue-782-exp1-wezboard.log \
     logs/issue-782-exp1-state/termsurf/webtui-trace.log \
     logs/issue-782-exp1-state/termsurf/roamium-trace.log \
     logs/issue-782-exp1-state/termsurf/chromium-server.log \
     > logs/issue-782-exp1-trace.log
   ```

8. Pass criteria:
   - the first date click shows the normal PagePopup open chain;
   - the select click shows the full select menu open and cleanup chain;
   - after select closes, the next failed click shows exactly where the chain
     stops: no mouse click delivered, no Blink activation, popup-open
     suppressed, AppKit/menu state stuck, or another concrete boundary;
   - if browser-side popup-open logs are silent, renderer-side logs explain
     whether Blink received the activation and whether it suppressed the popup;
   - select cleanup logs show whether `popup_menu_helper_` was cleared;
   - logs are quiet enough to read without cursor-move floods.

9. Partial criteria:
   - the failure reproduces and the trace narrows the cause to a subsystem, but
     another experiment is needed to identify the exact function or state flag;
   - the failure does not reproduce, but the trace proves repeated date/select
     interactions can work in a clean run;
   - if mouse down/up stops reaching Chromium after `<select>` closes, run a
     follow-up experiment with Wezboard's mouse-forwarding path logged to
     distinguish AppKit input absorption from missing GUI forwarding.

10. Fail criteria:
    - the logs still only show cursor movement after the failure;
    - the trace cannot distinguish input delivery, Blink activation, Chromium
      popup suppression, and AppKit menu cleanup;
    - the experiment changes popup behavior instead of only adding logs.

**Result:** Partial

The failure reproduced. After opening the `<select>` menu once and clicking away
to dismiss it, subsequent native widgets stopped opening for the duration of the
session.

The trace was useful. Before the select interaction, the date/PagePopup path
opened and cleaned up normally:

- `DateTimeChooserImpl::ctor.before_open`
- `WebViewImpl::OpenPagePopup`
- `WebContentsImpl::ShowCreatedWidget`
- `RenderWidgetHostViewMac::InitAsPopup`
- `WebViewImpl::CleanupPagePopup.after page_popup_after=0`

The select path also opened and cleaned up normally:

- `MenuListSelectType::ShowPopup`
- `ExternalPopupMenu::ShowInternal`
- `RenderFrameHostImpl::ShowPopupMenu`
- `WebContentsViewMac::ShowPopupMenu`
- `PopupMenuHelper::PopupMenuClosed`
- `WebContentsViewMac::OnMenuClosed.after popup_menu_helper_after_reset=0`
- `SetNativePopupIsVisible native_popup_is_visible_after=0`

This rules out the simplest browser-side cleanup theory: `popup_menu_helper_`
does not remain alive after the select menu closes. Blink also clears the
select's native popup visible state.

After the select menu closes, mouse clicks still reach Chromium:

```text
RouteOrProcessMouseEvent event_type=MouseDown ... has_webcontents=1 should_route=1
RouteOrProcessMouseEvent event_type=MouseUp ... has_webcontents=1 should_route=1
```

But those post-select clicks no longer produce any native-control activation
logs:

- no `DateTimeChooserImpl::ctor.before_open`
- no `MenuListSelectType::ShowPopup`
- no `ExternalPopupMenu::ShowInternal`
- no `RenderFrameHostImpl::ShowPopupMenu`
- no `WebContentsImpl::ShowCreatedWidget`

The chain therefore stops after Chromium receives the mouse event, but before
Blink's form-control popup activation paths run. The bug is not missing input
delivery, not a stuck `popup_menu_helper_`, and not a browser-side popup-open
suppression. It is somewhere in the renderer-side hit-test/focus/activation path
after an AppKit select-menu dismissal.

One trace hygiene issue surfaced: the `display_popup_menu_sent` log in
`PopupMenuHelper::ShowPopupMenu` runs after `DisplayPopupMenu`, even though the
helper may already have been deleted reentrantly by the menu-close callback.
That log must be removed or moved before committing the trace patch. It does not
change the diagnostic conclusion.

#### Conclusion

Experiment 1 narrows the post-select shutdown bug to the renderer activation
boundary. The next experiment should trace the delivered mouse event through
Blink hit-testing, focus handling, and form-control activation to find why a
click that reaches Chromium no longer invokes `DateTimeChooserImpl`,
`MenuListSelectType::ShowPopup`, or related native popup entry points after the
select menu is dismissed.

### Experiment 2: Trace Blink activation after select dismissal

Experiment 1 proved that clicks still reach Chromium after the `<select>` menu
is dismissed, but Blink no longer reaches the native popup entry points. This
experiment traces the same click inside Blink: event entry, hit testing,
dispatch/default action, focus state, user activation, and form-control
activation gates.

The leading hypothesis is a modal-loop input-state imbalance: the AppKit
`NSMenu` used for `<select>` may consume the mouse-up or dismissal event while
Blink still believes the original select-opening press is active. If Blink's
`EventHandler` remains stuck with `mouse_pressed_ = true` and
`mouse_down_node_ = <select>`, later clicks can reach Chromium but fail to start
a fresh click sequence or synthesize a normal `click` event for form controls.

This is still a logs-only experiment. Do not change popup behavior. Use the
existing `TERMSURF_ISSUE_779_TRACE=1` trace gate and the existing
`148.0.7778.97-issue-782` Chromium branch.

#### Changes

1. **Clean up the Experiment 1 unsafe trace line first.**

   In `content/browser/renderer_host/popup_menu_helper_mac.mm`, remove or move
   the `display_popup_menu_sent` log that currently runs after
   `DisplayPopupMenu`. The helper can be destroyed reentrantly by the callback,
   so no log may read `this` after `DisplayPopupMenu` returns. If this line is
   still useful, log the intended values before the call instead.

2. **Trace renderer mouse event entry.**

   In Blink's widget input entry path, add click-only trace lines for
   `MouseDown`, `MouseUp`, and `MouseMove` only if movement is explicitly needed
   for hit-test state. Prefer no move logs. Candidate hooks:
   - `third_party/blink/renderer/core/frame/web_frame_widget_impl.cc`
   - `third_party/blink/renderer/platform/widget/widget_base.cc`

   Log:
   - event type;
   - event position in widget/root-frame coordinates;
   - `WebFrameWidgetImpl*`;
   - local root frame pointer;
   - focused frame pointer;
   - `Page` focus/active state if available;
   - currently focused element summary.

3. **Trace Blink mouse press/release state and hit testing.**

   In the renderer event handling path, make `EventHandler` the primary trace
   target. Candidate hook:
   - `third_party/blink/renderer/core/input/event_handler.cc`

   Add logs at `LocalFrame::EventHandler::HandleMousePressEvent` entry and after
   it updates mouse state. Log:
   - event type;
   - `mouse_pressed_` before and after;
   - `mouse_down_node_` before and after;
   - hit-test result for the new press;
   - whether this is a fresh press or a press while Blink already thinks a press
     is active.

   Add logs at `LocalFrame::EventHandler::HandleMouseReleaseEvent` entry and
   around the click-synthesis decision. Log:
   - `mouse_pressed_` at release entry;
   - `mouse_down_node_`;
   - release hit-test target;
   - whether the release target matches the press target;
   - whether Blink synthesizes or dispatches a `click`;
   - the exact reason if click synthesis is skipped.

   The smoking-gun condition is a post-select fresh user click entering
   `HandleMousePressEvent` with `mouse_pressed_ = true` and `mouse_down_node_`
   still pointing at the dismissed `<select>`.

4. **Trace Blink hit testing for the click target.**

   Around the same mouse down/up hit-test paths, add a summary line for the
   target. Log:
   - event type;
   - hit node pointer;
   - hit element pointer;
   - element tag name;
   - element id;
   - input type for `HTMLInputElement`;
   - whether the element is disabled/read-only;
   - layout object pointer/type;
   - document/frame pointer;
   - local/root coordinates;
   - whether the target is inside the expected native popup test page.

5. **Trace DOM event dispatch and default action.**

   Add logs at the boundary where Blink dispatches mouse/pointer/click events
   and decides whether to run a default action. Candidate hooks:
   - `third_party/blink/renderer/core/input/event_handler.cc`
   - `third_party/blink/renderer/core/events/event_dispatcher.cc`
   - `third_party/blink/renderer/core/dom/events/event_target.cc`

   Restrict these logs to events targeting `HTMLInputElement`,
   `HTMLSelectElement`, and `HTMLOptionElement`. Log for `pointerdown`,
   `mousedown`, `mouseup`, and `click`:
   - target element summary;
   - whether default was prevented;
   - whether the event is trusted;
   - whether the event has user gesture/user activation;
   - whether the event dispatch completed normally;
   - whether a default action is queued or skipped.

   If `HandleMousePressEvent` and `HandleMouseReleaseEvent` run but no `click`
   event reaches the form control after select dismissal, the bug is in mouse to
   click synthesis. If `click` reaches the form control but no popup opens, the
   bug is in the form-control default handler.

6. **Trace form-control activation gates and default handlers.**

   Add entry and suppression logs immediately before the native popup entry
   points for:
   - date/time/datetime/color inputs before `DateTimeChooserImpl` or equivalent
     picker creation;
   - `<select>` before `MenuListSelectType::ShowPopup`;
   - datalist suggestion opening if the hook is easy to locate.

   Also log entry into:
   - `HTMLInputElement::DefaultEventHandler`;
   - `HTMLSelectElement::DefaultEventHandler`;
   - any nearby helper that turns `click`/DOM activation into a picker open.

   Log:
   - event type;
   - element pointer/tag/id/type;
   - disabled/read-only state;
   - focused/active state;
   - user activation state;
   - popup/menu already-open state;
   - exact reason for any early return.

7. **Trace focus changes around the post-select click.**

   Add logs in the renderer focus path for focus changes caused by mouse
   activation. Candidate hooks:
   - `third_party/blink/renderer/core/page/focus_controller.cc`
   - `third_party/blink/renderer/core/dom/document.cc`
   - `third_party/blink/renderer/core/html/forms/html_form_control_element.cc`

   Log:
   - old focused element summary;
   - new focused element summary;
   - frame/document;
   - whether focus was refused or redirected;
   - whether the page/frame is active.

8. **Do not implement a synthetic mouse-up yet.**

   A likely future fix, if the stuck-press hypothesis is confirmed, is to
   balance Blink state when the AppKit select menu closes, potentially by
   sending or simulating the missing mouse-up at `PopupMenuHelper` /
   `WebMenuRunner` close time. Do not implement that in this experiment. This
   experiment should only log enough to prove whether that fix is needed.

   It is fine to grep Chromium for existing platform precedent such as
   `SyntheticMouseEvent`, `mouse up after menu`, `popup_was_hidden_`, or
   menu-close balancing logic, but do not change behavior yet.

9. **Use explicit comparison labels.**

   Every new summary line should include a comparison phase label when possible:

   ```text
   phase=pre_select_success
   phase=select_open_close
   phase=post_select_failure
   ```

   If the code cannot know the phase directly, include enough join fields to
   infer it from time order:
   - monotonic timestamp if available;
   - event type;
   - target element id/type;
   - widget/frame/document pointer.

10. **Keep logs readable.**

    Do not log cursor movement floods. Do not log every DOM event on the page.
    Restrict output to mouse/pointer/click/focus/default-action events and only
    when `TERMSURF_ISSUE_779_TRACE=1` is set.

#### Verification

1. Build through the project scripts:

   ```bash
   cd /Users/ryan/dev/termsurf
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh webtui
   scripts/build.sh wezboard
   ```

2. Start the test page server:

   ```bash
   cd /Users/ryan/dev/termsurf
   bun test-html/server.ts
   ```

3. Start Wezboard with fresh logs:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-782-exp2-state/termsurf

   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-782-exp2-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-782-exp2-wezboard.log
   ```

4. Launch the TUI:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

5. Run the minimum comparison sequence:
   - click the date control and confirm it opens;
   - close it;
   - click the `<select>` dropdown and dismiss it by clicking outside;
   - click the same date control again;
   - stop immediately after the failed post-select date click.

6. Extract the trace:

   ```bash
   rg -a "\[issue-779-trace\]|native_popup_attempt|blink_mouse|HandleMousePressEvent|HandleMouseReleaseEvent|mouse_pressed|mouse_down_node|hit_test|dispatch|default_action|DefaultEventHandler|focus|activation|DateTimeChooserImpl|MenuListSelectType|ExternalPopupMenu|ShowPopup|OpenPagePopup|ShowCreatedWidget|InitAsPopup|RouteOrProcessMouseEvent" \
     logs/issue-782-exp2-wezboard.log \
     logs/issue-782-exp2-state/termsurf/webtui-trace.log \
     logs/issue-782-exp2-state/termsurf/roamium-trace.log \
     logs/issue-782-exp2-state/termsurf/chromium-server.log \
     > logs/issue-782-exp2-trace.log
   ```

7. Pass criteria:
   - the trace includes one successful pre-select date click and one failed
     post-select date click;
   - both clicks have renderer mouse-event entry logs;
   - both clicks have hit-test result logs;
   - the trace identifies the first divergence between success and failure:
     wrong hit-test target, missing click dispatch, default action prevented,
     focus/active-state refusal, user activation missing, or form-control
     activation suppression;
   - specifically, the trace proves or disproves the stuck-press hypothesis by
     showing `mouse_pressed_` and `mouse_down_node_` before and after the
     select-dismissal boundary;
   - log volume remains small enough to compare the two clicks by hand.

8. Partial criteria:
   - the trace narrows the failure to one renderer subsystem but not one exact
     branch;
   - the failure does not reproduce, but the trace proves successful repeated
     date/select/date activation with all expected renderer boundaries.

9. Fail criteria:
   - the trace again only proves that Chromium receives mouse events;
   - the trace cannot compare hit test, dispatch, focus, default action, and
     form-control activation between the successful and failed clicks;
   - the experiment changes popup behavior instead of only adding logs.

**Result:** Partial.

The run reproduced the post-select shutdown: after opening and dismissing the
native `<select>` menu, clicking the date control did not open the date popup.
The test stopped immediately after the failed post-select date click, as
requested.

The trace disproved the main stuck-Blink-state hypothesis. After the successful
select interaction, Blink and browser cleanup both looked healthy:

- `WebContentsViewMac::OnMenuClosed.after` reported
  `popup_menu_helper_after_reset=0`, so the browser-side `popup_menu_helper_`
  did not leak.
- `MenuListSelectType::SetNativePopupIsVisible` reported
  `native_popup_is_visible_after=0`, so the select-specific native-popup flag
  was cleared.
- `EventHandler::HandleMouseReleaseEvent.exit` reported `mouse_pressed=0`, so
  Blink was not left in a stuck pressed state.

The successful first date click showed the expected full renderer path:
`HandleMousePressEvent`, `HandleMouseReleaseEvent`,
`HTMLInputElement::DefaultEventHandler`, `DateTimeChooserImpl::ctor`, and
`WebViewImpl::OpenPagePopup`. The successful select click likewise showed the
full path through `MenuListSelectType::DefaultEventHandler.before_show_popup`,
`RenderFrameHostImpl::ShowPopupMenu`, AppKit menu display, and cleanup.

The failed post-select date click produced only
`WebViewImpl::CancelPagePopup path=date-page-popup web_view=... page_popup=0`.
It did not produce `HandleMousePressEvent`, `HandleMouseReleaseEvent`,
`HTMLInputElement::DefaultEventHandler`, `DateTimeChooserImpl::ctor`, or
`OpenPagePopup` for the date control. One `CursorChanged` message arrived after
select cleanup, which shows the Chromium process and cursor/move path were still
alive.

#### Conclusion

The failure is earlier than Blink form-control activation. It is not caused by
`mouse_pressed_`, `mouse_down_node_`, select native-popup visibility, or
`popup_menu_helper_` lifetime. After the AppKit select menu closes, subsequent
clicks no longer reach Blink's normal `LocalFrame::EventHandler` mouse
press/release path, even though movement/cursor activity can still reach the
browser process.

The next experiment should move upward in the input pipeline: log Wezboard mouse
forwarding and Chromium's top-level WebView/RenderWidgetHost receipt of
`MouseDown`/`MouseUp` after the select menu closes. The goal is to distinguish
between a Wezboard forwarding failure, AppKit/RemoteCocoa swallowing click
events, and Chromium receiving the native click but not routing it to the
renderer.

### Experiment 3: Trace Mouse Forwarding Above Blink

Experiment 2 proved that the failed post-select date click does not reach
Blink's `LocalFrame::EventHandler`. It also proved the select cleanup state is
healthy: `popup_menu_helper_` resets, `native_popup_is_visible_` clears, and
Blink's mouse pressed state is not stuck.

The next question is whether the failed click is lost before the TermSurf
protocol, inside Roamium/Chromium FFI forwarding, or inside Chromium's
RenderWidgetHost input pipeline. The leading Chromium-side hypothesis is stale
browser-process mouse routing state after AppKit's modal `<select>` menu:
`RenderWidgetHostInputEventRouter` may retain a stale `mouse_capture_target_`,
choose a stale popup/page-popup target, or return no target from hit testing.
This experiment adds logs only. It must not change mouse routing, focus, menu
cleanup, popup behavior, or event synthesis.

Keep all new logs behind the existing `TERMSURF_ISSUE_779_TRACE=1` trace gate.
Continue using Chromium branch `148.0.7778.97-issue-782`.

#### Changes

1. Add click-only Wezboard forwarding logs in
   `wezboard/wezboard-gui/src/termsurf/input.rs`.

   Log from `try_forward_mouse` at each decision point for `Down`, `Up`, and
   click-like drag termination only:
   - event kind, button, modifiers, raw terminal-cell position, pane id;
   - whether `hit_test_overlay`/`clamp_to_overlay` found a browser overlay;
   - overlay-local logical coordinates and pixel coordinates sent to the
     browser;
   - whether the event was sent through the browser channel;
   - reason when the event is not forwarded, such as no overlay hit, no active
     tab, no browser sender, outside overlay, drag capture mismatch, or browser
     mode disabled.

   Use a single summary format:

   ```text
   [issue-779-trace] mouse_forward_boundary boundary=wezboard outcome=...
   ```

2. Add Roamium protocol receive logs in `roamium/src/dispatch.rs`.

   For `Msg::MouseEvent`, log before calling `ts_forward_mouse_event` and after
   returning from it:
   - tab id, event type, button, x/y, click count, modifiers;
   - whether the tab id maps to a valid Chromium web contents handle;
   - whether the event was a move or click;
   - outcome `forwarded` or `dropped` with a concrete reason.

   This tells whether the failed post-select click makes it across the Unix
   socket boundary.

3. Add C FFI entry logs in
   `chromium/src/content/libtermsurf_chromium/libtermsurf_chromium.cc`.

   In `ts_forward_mouse_event` and `ts_forward_mouse_move`, log:
   - incoming `ts_web_contents_t`;
   - type, button, x/y, click count, and modifiers;
   - whether `g_main_parts` exists;
   - whether the call is forwarded into `TsBrowserMainParts`.

   This separates Roamium dispatch from Chromium library entry.

4. Add Chromium TermSurf input construction logs in
   `chromium/src/content/libtermsurf_chromium/ts_browser_main_parts.cc`.

   In `TsBrowserMainParts::ForwardMouseEvent`, log before and after building the
   `blink::WebMouseEvent`:
   - web contents handle and resolved `WebContents*`;
   - resolved `RenderWidgetHostView*` and `RenderWidgetHost*`;
   - input type, button, coordinates, click count, modifiers;
   - constructed `WebMouseEvent::GetType()`, position in widget, position in
     screen, and modifier flags;
   - whether the call reaches `view->GetRenderWidgetHost()->ForwardMouseEvent`.

   Also log `ForwardMouseMove` separately so the trace can compare the known
   working cursor/move path against the failing click path.

5. Add Chromium browser-process input-router logs in
   `chromium/src/content/browser/renderer_host/render_widget_host_input_event_router.cc`.

   This is the primary diagnostic hook. In
   `RenderWidgetHostInputEventRouter::RouteMouseEvent`, log only `MouseDown` and
   `MouseUp`:
   - source `RenderWidgetHostViewBase*` and source `RenderWidgetHostImpl*`;
   - event type, position in widget, position in root/screen if available,
     button, modifiers, and click count;
   - current `mouse_capture_target_` pointer before routing;
   - current hover/last mouse target fields if present in this Chromium version;
   - whether routing uses capture, a hit-test target, or a root fallback;
   - chosen target `RenderWidgetHostViewBase*` and target
     `RenderWidgetHostImpl*`;
   - whether the chosen target is the original main-page view, a popup/page
     popup view, null, hidden, destroyed, or otherwise unable to receive input;
   - final outcome `routed`, `dropped`, `fallback-root`, `captured`, or
     `suppressed`, with a concrete reason.

   Also log inside the target-selection helpers used by `RouteMouseEvent`,
   depending on the names in this Chromium version:
   - `FindMouseEventTarget`;
   - `FindTargetSynchronously`;
   - any helper that queries Viz hit-test data for the mouse event.

   For those helpers, log:
   - input root/screen position;
   - hit-test result target view/host;
   - local surface id or frame sink id if readily available;
   - whether a cached target or fallback root was used;
   - whether no target was found.

   These logs distinguish stale capture, stale hit-test target, null hit-test,
   and correct target selection.

6. Add target-side Chromium receipt logs below the router.

   Add click-only logs at the first method that processes the routed event on
   the chosen target view:
   - `RenderWidgetHostViewBase::ProcessMouseEvent`, if that is where the target
     receives routed mouse events; otherwise
   - the closest target-side `RenderWidgetHostView*` or
     `RenderWidgetHostImpl::ForwardMouseEvent` method that receives the event
     after router target selection.

   Log:
   - target view pointer and class/path name if cheaply available;
   - target host pointer, routing id, process id if readily available;
   - event type, position, button, modifiers, click count;
   - whether the target has a live host/delegate/frame widget;
   - whether the event is forwarded onward to the renderer or dropped locally.

7. Add renderer widget entry logs in
   `chromium/src/third_party/blink/renderer/core/frame/web_frame_widget_impl.cc`
   or the equivalent renderer widget input entry for this Chromium version.

   In `WebFrameWidgetImpl::HandleInputEvent`, log only `MouseDown` and
   `MouseUp`:
   - widget pointer and local root frame pointer;
   - event type, position, button, modifiers, click count;
   - whether the widget is a main-frame widget or popup/page-popup widget if
     readily available;
   - whether the method dispatches to `LocalFrame::EventHandler` or returns
     early, with reason if there is a visible branch.

   This final hook distinguishes browser-process routing bugs from renderer
   widget dispatch bugs.

8. Keep the existing Experiment 2 Blink logs in place for the run.

   The expected trace should show either:
   - a complete Wezboard -> Roamium -> FFI -> TsBrowserMainParts ->
     RenderWidgetHostInputEventRouter -> target ProcessMouseEvent ->
     WebFrameWidgetImpl -> Blink chain for the first date click; and
   - a shorter chain for the failed post-select date click, with the first
     missing boundary naming the bug.

9. Interpret the router logs mechanically:

   | Trace pattern                                                         | Diagnosis                                                                |
   | --------------------------------------------------------------------- | ------------------------------------------------------------------------ |
   | `RouteMouseEvent` chooses a popup/page-popup target after it closed   | Stale `mouse_capture_target_` or stale hit-test target                   |
   | `RouteMouseEvent` finds no target                                     | Hit-test query failed or Viz surface data is stale                       |
   | Router chooses the main-page target, target `ProcessMouseEvent` runs  | Browser routing is healthy; inspect renderer widget dispatch             |
   | Target processing runs, but no `WebFrameWidgetImpl::HandleInputEvent` | Mojo/InputRouter forwarding issue below the target view                  |
   | `WebFrameWidgetImpl::HandleInputEvent` runs but bails before Blink    | Renderer widget dispatch issue                                           |
   | All hooks run and Blink still does not receive it                     | Experiment 2 Blink logs missed a narrower renderer event-dispatch branch |

#### Verification

1. Build through the project scripts:

   ```bash
   cd /Users/ryan/dev/termsurf
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh webtui
   scripts/build.sh wezboard
   ```

2. Start the native popup test page:

   ```bash
   cd /Users/ryan/dev/termsurf
   bun test-html/server.ts
   ```

3. Start Wezboard with fresh Experiment 3 logs:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-782-exp3-state/termsurf

   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-782-exp3-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-782-exp3-wezboard.log
   ```

4. Launch the TUI:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

5. Run the same minimum sequence:
   - click the date control and confirm it opens;
   - close it;
   - click the `<select>` dropdown and dismiss it by clicking outside;
   - click the same date control again;
   - stop immediately after the failed post-select date click.

6. Extract the trace:

   ```bash
   rg -a "\[issue-779-trace\]|mouse_forward_boundary|ts_forward_mouse|ForwardMouseEvent|ForwardMouseMove|RouteMouseEvent|FindMouseEventTarget|FindTargetSynchronously|mouse_capture_target|ProcessMouseEvent|RenderWidgetHostInputEventRouter|RenderWidgetHostImpl|RenderInputRouter|WebFrameWidgetImpl|HandleInputEvent|HandleMousePressEvent|HandleMouseReleaseEvent|DefaultEventHandler|DateTimeChooserImpl|MenuListSelectType|PopupMenuClosed|OnMenuClosed|CursorChanged" \
     logs/issue-782-exp3-wezboard.log \
     logs/issue-782-exp3-state/termsurf/webtui-trace.log \
     logs/issue-782-exp3-state/termsurf/roamium-trace.log \
     logs/issue-782-exp3-state/termsurf/chromium-server.log \
     > logs/issue-782-exp3-trace.log
   ```

7. Pass criteria:
   - the trace includes one successful pre-select date click and one failed
     post-select date click;
   - the successful date click has all boundaries:
     `wezboard -> roamium -> libtermsurf_chromium -> TsBrowserMainParts -> RenderWidgetHostInputEventRouter -> target ProcessMouseEvent -> WebFrameWidgetImpl -> Blink EventHandler -> DateTimeChooser`;
   - the failed date click has a clearly shorter boundary chain;
   - the first missing or suppressed boundary is identified by a concrete
     outcome/reason field;
   - if `RenderWidgetHostInputEventRouter::RouteMouseEvent` sees the failed
     click, the trace identifies the selected target and the
     `mouse_capture_target_` state;
   - mouse move/cursor activity after select close is captured as a comparison
     path if it continues to work.

8. Partial criteria:
   - the trace narrows the failure to either Wezboard forwarding,
     Roamium/Chromium FFI, Chromium input-router target selection, target view
     processing, or renderer widget dispatch but not one exact branch;
   - the failure does not reproduce, but the trace proves repeated
     date/select/date clicks cross every forwarding boundary.

9. Fail criteria:
   - the failed post-select date click has no logs above Blink either;
   - logs are too broad or noisy to compare the successful and failed clicks;
   - the experiment changes input routing or popup behavior instead of only
     adding logs.

**Result:** Partial.

The experiment narrowed the failure boundary, but not in the expected Chromium
input-router region. The successful pre-select date click crossed the full
instrumented path:

```text
Wezboard -> Roamium -> libtermsurf_chromium -> TsBrowserMainParts -> WebFrameWidgetImpl -> Blink EventHandler -> DateTimeChooser
```

The date popup opened with the expected `DateTimeChooserImpl` and
`show_created_widget` logs. The subsequent click used to close that date popup
also crossed the same forwarding path and reached Blink's event handler.

The select dropdown interaction also crossed the expected path. Blink entered
`MenuListSelectType::ShowPopup`, the browser entered
`RenderFrameHostImpl::ShowPopupMenu`, `WebContentsViewMac::ShowPopupMenu`
created a popup helper, `WebMenuRunner` opened the AppKit menu, and cleanup
completed. The cleanup logs again showed that `popup_menu_helper_` reset to null
and Blink's `native_popup_is_visible_` changed from `1` back to `0`.

After the select menu closed, mouse movement continued to flow through Roamium
and Chromium. The trace contains many `ts_forward_mouse_move` and
`ForwardMouseMove` lines. However, the failed post-select date click did not
appear as a click in the instrumented path at all. After select cleanup there
were no later Wezboard `event_type=down` or `event_type=up` forwarding logs, no
Roamium mouse down/up logs, no `ts_forward_mouse_event` calls, no
`TsBrowserMainParts::ForwardMouseEvent` calls, no Chromium input-router click
logs, and no Blink `HandleMousePressEvent` logs.

This means the failed click did not reach Chromium's input router in this run.
The stale `RenderWidgetHostInputEventRouter` hypothesis is therefore not
supported by the Experiment 3 trace. The failure boundary is higher than the
current hooks: either the click was swallowed before Wezboard called
`try_forward_mouse`, or Wezboard's top-level mouse dispatch chose a different
non-browser path for button down/up after the AppKit select menu closed.

#### Conclusion

Experiment 3 showed that the post-select shutdown is not currently explained by
Chromium input-router target selection. Mouse moves still forward after the
select closes, but button down/up forwarding stops before Roamium and Chromium
see the click.

The next experiment should move the first hook above `try_forward_mouse` in
Wezboard's top-level mouse input pipeline. It should log every raw mouse down/up
received by the GUI, the pane and overlay hit-test result, browsing and focus
state, whether `try_forward_mouse` is called, and the concrete reason when the
click is routed to the terminal/TUI instead of the browser overlay.

### Experiment 4: Trace Wezboard Mouse Dispatch Before Forwarding

Experiment 3 showed that the failed post-select date click never reached Roamium
or Chromium as a mouse down/up event. Mouse moves still forwarded after the
select menu closed, so the browser process was not fully disconnected. The
remaining failure boundary is inside Wezboard's top-level mouse handling, before
or around the call to `termsurf::input::try_forward_mouse`.

This experiment adds logs only. It must not change mouse capture, focus
handling, terminal mouse reporting, browser forwarding, AppKit menu behavior, or
any native popup behavior.

Keep all new logs behind the existing `TERMSURF_ISSUE_779_TRACE=1` trace gate.
Continue using Chromium branch `148.0.7778.97-issue-782`; this experiment is
expected to modify Wezboard only unless implementation discovers that the raw
window event source lives elsewhere.

#### Changes

1. Add a raw window mouse-event entry log in
   `wezboard/wezboard-gui/src/termwindow/mod.rs`.

   In the `WindowEvent::MouseEvent(event)` arm, log only mouse down/up and
   click-like drag release events before calling `mouse_event_impl`:
   - event kind, button, modifiers;
   - window pixel coordinates and screen coordinates;
   - whether the window is focused if that state is cheaply available;
   - active pane id from `get_active_pane_or_overlay()` if available;
   - current `is_click_to_focus_window` state if accessible at this layer.

   Use a summary line:

   ```text
   [issue-779-trace] wezboard_mouse_dispatch boundary=window_event outcome=entered ...
   ```

   This answers whether the failed click reaches Wezboard's window event handler
   at all.

2. Add entry and pre-routing logs in
   `wezboard/wezboard-gui/src/termwindow/mouseevent.rs`.

   At the top of `TermWindow::mouse_event_impl`, log only mouse down/up and
   click-like drag release events:
   - event kind, button, modifiers;
   - window pixel coordinates and screen coordinates;
   - active pane id;
   - `current_mouse_capture`, `current_mouse_buttons`, `dragging`,
     `window_drag_position`, `is_click_to_focus_window`, and whether
     `focused.is_some()`;
   - `last_ui_item` and resolved `ui_item` after `resolve_ui_item` runs;
   - the computed `ClickPosition` for the active pane.

   Add outcome logs for every early return before `mouse_event_terminal`,
   including:
   - no active pane or overlay;
   - completed window drag;
   - completed UI drag;
   - routed to a UI item instead of a terminal pane;
   - skipped terminal routing because capture state was neither `None` nor
     `TerminalPane`.

   Use a summary line:

   ```text
   [issue-779-trace] wezboard_mouse_dispatch boundary=mouse_event_impl outcome=...
   ```

3. Add terminal-routing decision logs in
   `wezboard/wezboard-gui/src/termwindow/mouseevent.rs`.

   In `TermWindow::mouse_event_terminal`, log immediately before
   `try_forward_mouse`:
   - pane id and computed terminal `ClickPosition`;
   - global window-cell position;
   - `current_mouse_capture`, `current_mouse_buttons`,
     `is_click_to_focus_window`;
   - `focused.is_some()`, computed `is_focused`, and config flags
     `swallow_mouse_click_on_window_focus`, `swallow_mouse_click_on_pane_focus`,
     and `pane_focus_follows_mouse`;
   - whether the event is a click-to-focus pane candidate.

   Log immediately after `try_forward_mouse`:
   - `try_forward_mouse_return=true|false`;
   - if `true`, `outcome=forwarded_to_browser`;
   - if `false`, continue logging subsequent routing decisions.

   Use:

   ```text
   [issue-779-trace] wezboard_mouse_dispatch boundary=before_try_forward_mouse outcome=entered ...
   [issue-779-trace] wezboard_mouse_dispatch boundary=after_try_forward_mouse outcome=...
   ```

4. Add focus-swallow and terminal-fallback logs in `mouse_event_terminal`.

   Log every branch that can consume or redirect a click after
   `try_forward_mouse` returns false:
   - `outcome=swallowed_window_focus` when the click enters
     click-to-focus-window state;
   - `outcome=swallowed_window_focus_release` when the matching release exits
     click-to-focus-window state;
   - `outcome=allow_action_false` when the event is not allowed to run bindings
     or terminal forwarding;
   - `outcome=mouse_binding_action` when a mouse binding handles the event;
   - `outcome=sent_to_terminal_pane` when `pane.mouse_event(mouse_event)` runs;
   - `outcome=dropped_terminal_pane_focus` when
     `swallow_mouse_click_on_pane_focus` suppresses terminal delivery.

   These logs should include the same pane id, event kind, button, focus state,
   click-to-focus state, and capture state fields so the successful and failed
   clicks can be compared mechanically.

5. Keep the existing Experiment 3 logs in place for the run.

   The expected trace for a successful date click still includes:

   ```text
   window_event -> mouse_event_impl -> before_try_forward_mouse -> after_try_forward_mouse(forwarded) -> Roamium -> Chromium -> Blink
   ```

   The failed post-select date click should now show one of:
   - no `window_event` log, meaning AppKit/window delivery swallowed the click
     before Wezboard;
   - `window_event` but no `mouse_event_impl`, meaning TermWindow dispatch did
     not run;
   - `mouse_event_impl` but no `before_try_forward_mouse`, meaning UI/capture
     routing consumed the click;
   - `before_try_forward_mouse` followed by `after_try_forward_mouse=false`,
     meaning overlay hit testing or browser sender state rejected the click;
   - `after_try_forward_mouse=false` followed by focus, binding, or terminal
     fallback logs naming the consumer.

6. Interpret the logs mechanically:

   | Trace pattern                                               | Diagnosis                                                        |
   | ----------------------------------------------------------- | ---------------------------------------------------------------- |
   | No `window_event` for failed click, but moves still forward | Native/AppKit event delivery is losing button events             |
   | `window_event` only                                         | Wezboard event dispatch stops before `mouse_event_impl`          |
   | `mouse_event_impl` routes to UI item                        | Hit testing/capture thinks the click is terminal chrome, not web |
   | `mouse_event_impl` skips terminal route due capture         | Wezboard mouse capture is stuck after select menu dismissal      |
   | `try_forward_mouse=false` with overlay miss                 | Overlay hit testing disagrees with rendered browser position     |
   | `try_forward_mouse=false` with no sender/tab                | Browser overlay state was torn down or no active tab was mapped  |
   | Focus-swallow outcome                                       | Window/pane focus state changed after AppKit menu dismissal      |
   | Sent to terminal pane                                       | Wezboard is treating the click as terminal input, not web input  |

#### Verification

1. Build through the project scripts:

   ```bash
   cd /Users/ryan/dev/termsurf
   scripts/build.sh wezboard
   scripts/build.sh roamium
   scripts/build.sh chromium
   scripts/build.sh webtui
   ```

2. Start the native popup test page:

   ```bash
   cd /Users/ryan/dev/termsurf
   bun test-html/server.ts
   ```

3. Start Wezboard with fresh Experiment 4 logs:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-782-exp4-state/termsurf

   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-782-exp4-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-782-exp4-wezboard.log
   ```

4. Launch the TUI:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

5. Run the minimum sequence:
   - click the date control and confirm it opens;
   - close it;
   - click the `<select>` dropdown and dismiss it by clicking outside;
   - click the same date control again;
   - stop immediately after the failed post-select date click.

6. Extract the trace:

   ```bash
   rg -a "\[issue-779-trace\]|wezboard_mouse_dispatch|mouse_forward_boundary|ts_forward_mouse|ForwardMouseEvent|ForwardMouseMove|HandleMousePressEvent|HandleMouseReleaseEvent|DateTimeChooserImpl|MenuListSelectType|PopupMenuClosed|OnMenuClosed" \
     logs/issue-782-exp4-wezboard.log \
     logs/issue-782-exp4-state/termsurf/webtui-trace.log \
     logs/issue-782-exp4-state/termsurf/roamium-trace.log \
     logs/issue-782-exp4-state/termsurf/chromium-server.log \
     > logs/issue-782-exp4-trace.log
   ```

7. Pass criteria:
   - the trace includes the successful pre-select date click, select open/close,
     and failed post-select date click;
   - the failed click has at least one `wezboard_mouse_dispatch` line;
   - the first missing or consuming boundary is identified by an explicit
     `outcome` and `reason`;
   - the trace distinguishes AppKit/window delivery loss, Wezboard capture/UI
     routing, focus swallowing, overlay hit-test failure, browser sender state,
     and terminal fallback.

8. Partial criteria:
   - the failed click produces no `wezboard_mouse_dispatch` down/up logs, but
     the run confirms mouse moves still forward after select close;
   - the trace narrows the failure to a small Wezboard region but needs one more
     hook to name the exact branch.

9. Fail criteria:
   - the failure does not reproduce;
   - the logs are too noisy to pair the successful and failed clicks;
   - the experiment changes routing behavior instead of only adding logs.
