+++
status = "open"
opened = "2026-05-22"
+++

# Issue 783: Remaining native popup bugs after Shell fix

## Goal

Fix the native popup bugs that remain after Issue 782, one focused bug at a
time, without reopening the completed Shell-window mouse transparency work.

## Background

Issue 779 fixed the primary y-axis placement bug for Blink PagePopup controls.
Date, time, date-time, and color controls now appear at the correct y position
inside the TermSurf webview overlay.

Issue 782 then fixed the session-wide native-widget shutdown that happened after
interacting with `<select>`. The root cause was an invisible Chromium Shell
window that overlapped Wezboard while still accepting AppKit mouse events. The
fix made TermSurf-managed Shell windows consistently mouse-transparent with
`ignoresMouseEvents=YES`.

That leaves several smaller but still user-visible popup bugs. They are
different enough that each should be isolated with its own experiment before any
fix is attempted.

## Remaining Bugs

### PagePopup remains visible after alt-tab

Date, time, date-time, and color popups can remain visible after the user
alt-tabs away from Wezboard. The owning TermSurf window is no longer active or
visible to the user, but the popup remains on screen.

This likely belongs to popup lifecycle, owner-window, app deactivation, or
window-ordering behavior. It should be investigated with logs around
`NSApplication` activation changes, Shell/Popup window visibility, PagePopup
close paths, and popup widget ownership.

### Select dropdown has the wrong x position

The `<select>` dropdown has the correct y position, but its x position is still
wrong. This path does not use Blink PagePopup. It goes through Chromium's AppKit
menu path:

```text
RenderFrameHostImpl::ShowPopupMenu
PopupMenuHelper::ShowPopupMenu
RenderWidgetHostNSViewBridge::DisplayPopupMenu
WebMenuRunner::runMenuInView
NSPopUpButtonCell
```

Chromium can log the select anchor before `NSPopUpButtonCell` takes over, but
AppKit owns the final menu placement. The next select experiment needs to
capture or infer the final x position and compare it against the anchor.

### Datalist does not work

Datalist could not be tested cleanly while the post-select shutdown was present.
Now that Issue 782 fixed the shutdown, datalist should get a fresh isolated run.
Its popup path may be different from both Blink PagePopup controls and AppKit
select menus.

### RenderWidgetPopupWindow cleanup is suspicious

Issue 782 traces repeatedly showed visible `RenderWidgetPopupWindow` entries at
level `101`, with `ignoresMouseEvents=false`, after popup interactions. These
windows did not cause the post-select shutdown once the main Shell window became
mouse-transparent, so they should not be treated as the next root cause by
default.

They should be revisited only if they explain one of the remaining symptoms,
especially PagePopup visibility after app deactivation.

## Approach

Handle one bug at a time. Do not bundle PagePopup deactivation, select x
placement, and datalist into a single fix.

The recommended order is:

1. PagePopup remains visible after alt-tab, because it affects every
   PagePopup-family control and may also explain the lingering
   `RenderWidgetPopupWindow` observations.
2. Select dropdown x position, because the y-axis and post-select shutdown are
   already fixed, leaving x placement as a clean AppKit-menu positioning bug.
3. Datalist behavior, because it needs a clean independent path trace now that
   native widgets no longer shut down after select.

If a trace proves that two remaining symptoms share one root cause, adjust the
order in the experiment result before designing the next experiment.

## Constraints

- Do not change the Issue 782 Shell-window mouse transparency fix unless a new
  trace proves it is wrong.
- Do not add runtime experiment flags.
- Keep using the existing trace gate for temporary diagnostic logs:
  `TERMSURF_ISSUE_779_TRACE=1`.
- If Chromium code changes are needed, create a new Issue 783 Chromium branch
  before editing Chromium, then register that branch in `chromium/README.md`.
- Design and implement one experiment at a time.

## Experiments

### Experiment 1: Clean trace surface for PagePopup alt-tab

#### Description

The next bug is PagePopup visibility after app/window deactivation. Date, time,
date-time, and color controls use Blink PagePopup widgets. After opening one of
these popups, alt-tabbing away from Wezboard can leave the native popup visible
on screen even though the owning TermSurf app is no longer active.

Before adding new logs, remove the broad diagnostics from Issue 782 that were
specific to the solved post-select shutdown. Those logs answered their question
and now make the next trace hard to read. This experiment should leave only a
small PagePopup lifecycle trace focused on activation, visibility, popup-window
ownership, and close paths.

This experiment is logs-only except for deleting obsolete trace lines. Do not
change popup behavior. Do not add any runtime experiment flag. Continue using
`TERMSURF_ISSUE_779_TRACE=1` as the single trace gate.

#### Changes

1. **Create the Issue 783 Chromium branch before editing Chromium.**

   Chromium changes for this issue must happen on a new branch:

   ```text
   148.0.7778.97-issue-783
   ```

   Branch it from the latest useful Issue 782 Chromium branch tip, then add the
   branch to `chromium/README.md`. Do not continue committing Issue 783 work to
   `148.0.7778.97-issue-782`.

2. **Remove obsolete Issue 782 shutdown logs.**

   Remove or disable trace lines that were only useful for the solved
   post-select shutdown:
   - Wezboard `NSApplication sendEvent:` and `NSWindow sendEvent:` swizzle logs;
   - Wezboard `appkit_view`, `window_event`, `mouse_event_impl`,
     `before_try_forward_mouse`, and `mouse_forward_boundary` click/move logs;
   - Roamium mouse forwarding boundary logs;
   - Chromium input-router, `RouteOrProcessMouseEvent`, `ProcessMouseEvent`, and
     `WebFrameWidgetImpl::HandleInputEvent` logs;
   - Blink `EventHandler`, `MouseEventManager`, and form-control default-handler
     logs added only to diagnose post-select click loss;
   - select-specific helper lifecycle logs that are unrelated to PagePopup
     deactivation.

   Keep the actual Issue 782 fix: TermSurf Shell windows must still set and
   reassert `ignoresMouseEvents=YES`.

3. **Keep only low-noise existing logs that help this bug.**

   Preserve or simplify logs for:
   - Shell window state when it helps correlate owner window visibility;
   - PagePopup open and close paths;
   - `RenderWidgetPopupWindow` creation, visibility, ordering, and destruction;
   - app/window activation notifications.

   The target trace should be readable without cursor-move or input-router
   floods.

4. **Add Wezboard app/window deactivation logs.**

   In Wezboard's macOS app/window layer, log only activation and visibility
   transitions relevant to alt-tab:
   - `NSApplicationDidResignActiveNotification`;
   - `NSApplicationDidBecomeActiveNotification`;
   - `NSWindowDidResignKeyNotification`;
   - `NSWindowDidBecomeKeyNotification`;
   - `NSWindowDidMiniaturizeNotification`, if applicable;
   - `NSWindowDidDeminiaturizeNotification`, if applicable;
   - any existing Wezboard hide/show or focus events that are already wired.

   Include:

   ```text
   app_is_active
   key_window / main_window pointer and class
   Wezboard window frame, is_key, is_main, is_visible
   first_responder class
   active pane id if cheap
   ```

   Use a concise summary line:

   ```text
   [issue-779-trace] pagepopup_alt_tab boundary=wezboard_activation event=...
   ```

5. **Add Chromium app/window deactivation logs.**

   In the Chromium/Roamium process, log app and Shell-window activation changes
   around alt-tab:
   - `NSApplicationDidResignActiveNotification`;
   - `NSApplicationDidBecomeActiveNotification`;
   - Shell `NSWindowDidResignKeyNotification`;
   - Shell `NSWindowDidBecomeKeyNotification`;
   - Shell hide/show/order changes if there is already a delegate or
     notification hook.

   Include:

   ```text
   app_is_active
   shell window pointer, frame, alpha, is_visible, is_key, is_main
   shell ignoresMouseEvents
   ordered Chromium windows top 5 with class, frame, level, visible, key/main
   ```

   Use:

   ```text
   [issue-779-trace] pagepopup_alt_tab boundary=chromium_activation event=...
   ```

6. **Add focused PagePopup lifecycle logs.**

   Keep or add logs at the PagePopup open/close boundaries:
   - `DateTimeChooserImpl::Open` / constructor before opening;
   - `DateTimeChooserImpl::CancelPopup`;
   - `DateTimeChooserImpl::EndChooser`;
   - `DateTimeChooserImpl::DidClosePopup`;
   - `WebViewImpl::OpenPagePopup`;
   - `WebViewImpl::CancelPagePopup`;
   - `WebViewImpl::ClosePagePopup`;
   - `WebViewImpl::CleanupPagePopup`;
   - `WebPagePopupImpl::ClosePopup`;
   - `WebContentsImpl::ShowCreatedWidget` for the popup widget.

   Include:

   ```text
   control type if known: date|time|datetime-local|color|unknown
   WebContents / frame / popup object pointer
   popup window or widget host view pointer
   popup rect and anchor rect
   popup visible/open state before and after close
   reason if close/cancel is triggered
   ```

   Use:

   ```text
   [issue-779-trace] pagepopup_alt_tab boundary=pagepopup_lifecycle event=...
   ```

7. **Add popup window state logs.**

   For `RenderWidgetPopupWindow` / popup `NSWindow` instances associated with
   PagePopup, log:
   - creation/init as popup;
   - `orderFront` / visibility changes if reachable;
   - `windowDidResignKey`, `windowDidBecomeKey`, and close notifications;
   - destruction/deallocation if a safe hook exists;
   - current frame, level, visible, key/main, parent/child relationship, and
     `ignoresMouseEvents`.

   Use:

   ```text
   [issue-779-trace] pagepopup_alt_tab boundary=popup_window event=...
   ```

8. **Do not add broad input tracing.**

   This experiment should not log every mouse event, input router step, Blink
   event handler, or select menu lifecycle. The only user interaction that
   matters is opening a PagePopup and then deactivating the app/window.

#### Verification

1. Confirm the Chromium branch:

   ```bash
   cd /Users/ryan/dev/termsurf/chromium/src
   git branch --show-current
   ```

   It must be:

   ```text
   148.0.7778.97-issue-783
   ```

2. Build through project scripts:

   ```bash
   cd /Users/ryan/dev/termsurf
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh wezboard
   scripts/build.sh webtui
   ```

3. Start the native popup test page:

   ```bash
   cd /Users/ryan/dev/termsurf
   bun test-html/server.ts
   ```

4. Start Wezboard with fresh Experiment 1 logs:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-783-exp1-state/termsurf

   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-783-exp1-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-783-exp1-wezboard.log
   ```

5. Launch the TUI:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

6. Run the minimum sequence:
   - click the date control and confirm the popup opens;
   - alt-tab to another app while the popup is still open;
   - observe whether the popup remains visible;
   - alt-tab back to Wezboard;
   - stop the run.

   If date does not reproduce the bug, repeat once with time, then once with
   color. Do not test select or datalist in this experiment.

7. Extract the focused trace:

   ```bash
   rg -a "\[issue-779-trace\]|pagepopup_alt_tab|DateTimeChooserImpl|WebViewImpl::.*PagePopup|WebPagePopupImpl|ShowCreatedWidget|RenderWidgetPopupWindow|NSApplicationDid|NSWindowDid|chromium_activation|wezboard_activation|popup_window|pagepopup_lifecycle" \
     logs/issue-783-exp1-wezboard.log \
     logs/issue-783-exp1-state/termsurf/webtui-trace.log \
     logs/issue-783-exp1-state/termsurf/roamium-trace.log \
     logs/issue-783-exp1-state/termsurf/chromium-server.log \
     > logs/issue-783-exp1-trace.log
   ```

8. Pass criteria:
   - the trace is quiet enough to read without input-router or cursor floods;
   - the popup open path names the PagePopup object and popup window/widget;
   - alt-tab produces Wezboard and Chromium activation/deactivation logs;
   - the trace shows whether PagePopup close/cancel cleanup fires on
     deactivation;
   - the trace shows whether the popup `NSWindow` remains visible after
     deactivation;
   - the result identifies the next fix boundary: Wezboard deactivation message,
     Chromium app activation hook, WebView/PagePopup cleanup, or popup window
     ownership.

9. Partial criteria:
   - obsolete logs are mostly removed, but one noisy subsystem remains;
   - the alt-tab bug reproduces and window state is visible, but one missing
     pointer prevents joining popup object to popup window;
   - the bug does not reproduce for date but does reproduce for time or color.

10. Fail criteria:
    - the trace is still dominated by Issue 782 shutdown logs;
    - the experiment changes popup behavior;
    - no app/window activation boundary is logged during alt-tab.

**Result:** Fail

The experiment changed popup behavior before reaching the intended alt-tab test.
After opening the date picker, the picker appeared at the wrong y position. This
regressed the y-axis fix that was completed in Issue 779.

Because the PagePopup position was wrong at the first step, the alt-tab
visibility trace is invalid. The experiment must not be used to diagnose the
remaining PagePopup deactivation bug until the regression is understood and
reverted or fixed.

#### Conclusion

Experiment 1 failed. The combined cleanup plus new trace change was too broad:
it disturbed the working date picker y-axis behavior. The next step is to audit
the Chromium diff against the known-good Issue 779/782 state and identify which
removed or changed log/positioning code was actually part of the fix path, then
restore the working y-axis behavior before running any further alt-tab
experiments.

### Experiment 2: Clean Obsolete Logs Without Breaking PagePopup Y

#### Description

Experiment 1 failed because it removed the actual PagePopup y-axis fix while
removing obsolete logs. This experiment repeats only the cleanup work, with an
explicit hard boundary around the working y-axis solution.

Do not add the new alt-tab activation or PagePopup deactivation logs in this
experiment. Save those for Experiment 3. The goal here is only to reduce stale
Issue 782 logging noise while preserving every working behavior.

The working y-axis fix lives in:

```text
chromium/src/third_party/blink/renderer/core/exported/web_page_popup_impl.cc
```

inside:

```text
WebPagePopupImpl::SetWindowRect
```

It must continue to:

```text
anchor_rect_in_screen = GetAnchorRectInScreen()
window_rect = rect_in_screen

if rect_in_screen.x == anchor_rect_in_screen.x
and rect_in_screen.y == anchor_rect_in_screen.bottom()
then window_rect.y = anchor_rect_in_screen.y
```

Then `window_rect`, not `rect_in_screen`, must continue flowing into:

```text
widget_base_->SetPendingWindowRect(window_rect)
popup_widget_host_->SetPopupBounds(window_rect, ...)
initial_rect_ = window_rect
```

**LOUD RULE: DO NOT REMOVE, REWRITE, BYPASS, DISABLE, OR "CLEAN UP" THE
`page_popup_y_fix` / `SetWindowRect` Y-CORRECTION CODE.**

**LOUD RULE: ANY DIFF THAT CHANGES THE SEMANTICS OF
`WebPagePopupImpl::SetWindowRect` IS AN AUTOMATIC FAILURE UNLESS IT IS ONLY
ADDING LOG FIELDS AROUND THE EXISTING CORRECTION.**

This experiment is logs-only except for deleting obsolete trace lines. The
cleanup must be surgical: remove high-volume diagnostics from the solved
post-select shutdown investigation and preserve the PagePopup y correction. No
new diagnostic surface is added here.

#### Changes

1. **Start from the restored, known-good code state.**

   Before editing, confirm that the current working tree code matches the
   pre-Experiment-1 code state and that the date picker y value is visually
   correct. The only existing differences from before Issue 783 code work should
   be documentation.

2. **Freeze the PagePopup y fix before cleanup.**

   Before removing any logs, inspect:

   ```text
   chromium/src/third_party/blink/renderer/core/exported/web_page_popup_impl.cc
   ```

   Confirm `WebPagePopupImpl::SetWindowRect` still has:
   - `gfx::Rect anchor_rect_in_screen = GetAnchorRectInScreen();`
   - `gfx::Rect window_rect = rect_in_screen;`
   - the `anchored_at_bottom` predicate comparing:
     - `rect_in_screen.x()` to `anchor_rect_in_screen.x()`;
     - `rect_in_screen.y()` to `anchor_rect_in_screen.bottom()`;
   - `window_rect.set_y(anchor_rect_in_screen.y())` when the predicate passes;
   - downstream use of `window_rect` for pending bounds, popup bounds, and
     deferred initial rect.

   This code is part of the product behavior now. It is not obsolete logging.

3. **Freeze the Shell mouse-inert fix before cleanup.**

   Before removing any logs, inspect:

   ```text
   chromium/src/content/libtermsurf_chromium/ts_shell_window_mac.mm
   chromium/src/content/app_shim_remote_cocoa/web_menu_runner_mac.mm
   chromium/src/content/browser/web_contents/web_contents_view_mac.mm
   ```

   Confirm the Issue 782 fix still reasserts:

   ```text
   [window setIgnoresMouseEvents:YES]
   ```

   at the Shell creation/move/menu-close boundaries where it currently exists.
   This code is also product behavior now. It is not obsolete logging.

4. **Remove obsolete high-volume Issue 782 shutdown logs only.**

   Remove logs that fire per input event or were only useful for the solved
   post-select click-loss bug:
   - Wezboard `NSApplication sendEvent:` / `NSWindow sendEvent:` swizzle logs;
   - Wezboard `appkit_view`, `window_event`, `mouse_event_impl`,
     `before_try_forward_mouse`, `after_try_forward_mouse`, and
     `mouse_forward_boundary` logs;
   - Roamium per-mouse forwarding boundary logs;
   - Chromium input-router, `RouteOrProcessMouseEvent`, `ProcessMouseEvent`,
     `WebFrameWidgetImpl::HandleInputEvent`, and Blink `EventHandler` /
     `MouseEventManager` logs added only for post-select click loss.

   Do not use a blanket revert of `Trace native popup event loss`. That commit
   contains the PagePopup y-axis fix. Cleanup must be by reviewed hunks, not by
   mechanical commit revert.

   Cleanup heuristic:
   - delete only `LOG`, `log::info!`, trace helper calls, and their
     immediately-bracketing trace guards;
   - do not delete field mutations, assignment statements, `set*` calls,
     geometry arithmetic, or non-trace function calls;
   - if a hunk mixes trace code and behavior, split it;
   - if a call is not obviously a trace/log call, leave it.

5. **Preserve the useful low-volume popup logs.**

   Keep logs that are useful for PagePopup-family placement or lifecycle:
   - `page_popup_y_fix`;
   - `DateTimeChooserImpl`;
   - `WebPagePopupImpl::SetWindowRect`;
   - `WebViewImpl::OpenPagePopup` / close lifecycle;
   - `WebContentsImpl::ShowCreatedWidget`;
   - `RenderWidgetHostViewMac::InitAsPopup`;
   - Shell window state logs that show `ignoresMouseEvents`.

   These are low-frequency popup-open / popup-close logs, not cursor floods.

6. **Do not add new alt-tab logs.**

   Do not add:
   - Wezboard activation logs;
   - Chromium activation/window notification logs;
   - new `pagepopup_alt_tab` lifecycle logs;
   - new popup window ownership logs.

   Those belong to Experiment 3 after this cleanup is verified.

7. **Add a hard pre-run diff audit.**

   Before building, inspect the diff and verify:
   - `WebPagePopupImpl::SetWindowRect` still contains the y correction;
   - any diff in `SetWindowRect` is only additive logging or harmless helper
     formatting;
   - no diff changes `window_rect` back to plain `rect_in_screen`;
   - no diff removes `page_popup_y_fix`;
   - no diff removes or weakens existing `setIgnoresMouseEvents:YES`
     reassertions;
   - no diff changes `MoveShellWindowToTermSurfScreenRect`;
   - no diff changes `<select>` / `PopupMenuHelper` behavior.

#### Verification

1. Confirm the Chromium branch:

   ```bash
   cd /Users/ryan/dev/termsurf/chromium/src
   git branch --show-current
   ```

   It must be:

   ```text
   148.0.7778.97-issue-783
   ```

2. Build through project scripts:

   ```bash
   cd /Users/ryan/dev/termsurf
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh wezboard
   scripts/build.sh webtui
   ```

3. Start the test page:

   ```bash
   cd /Users/ryan/dev/termsurf
   bun test-html/server.ts
   ```

4. Start Wezboard with fresh Experiment 2 logs:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-783-exp2-state/termsurf

   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-783-exp2-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-783-exp2-wezboard.log
   ```

5. Launch the TUI:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

6. First verify the non-negotiable y-axis invariant:
   - click the date input;
   - confirm the date picker y position is correct;
   - if the y value is wrong, stop immediately and mark the experiment failed.

7. Verify the Issue 782 post-select invariant:
   - close the date picker;
   - click the select dropdown once;
   - dismiss it;
   - click the date input again;
   - confirm the date picker still opens and its y position remains correct.

   If native widgets stop opening after select, stop immediately and mark the
   experiment failed.

8. Extract the cleanup trace:

   ```bash
   rg -a "\[issue-779-trace\]|page_popup_y_fix|DateTimeChooserImpl|WebPagePopupImpl|ShowCreatedWidget|InitAsPopup|TermSurfMoveShellWindow|ignoresMouseEvents|PopupMenuHelper|WebMenuRunner" \
     logs/issue-783-exp2-wezboard.log \
     logs/issue-783-exp2-state/termsurf/webtui-trace.log \
     logs/issue-783-exp2-state/termsurf/roamium-trace.log \
     logs/issue-783-exp2-state/termsurf/chromium-server.log \
     > logs/issue-783-exp2-trace.log
   ```

9. Pass criteria:
   - date picker y position remains correct;
   - trace includes `page_popup_y_fix applied=true` for the date popup;
   - `corrected_rect.y == anchor_rect.y`;
   - obsolete input-router/mouse-dispatch flood logs are gone;
   - existing low-volume PagePopup placement/lifecycle logs remain;
   - existing Shell `ignoresMouseEvents` reassertions remain visible in code or
     trace;
   - the post-select date picker still opens.

10. Fail criteria:
    - **date y-axis regresses in any way;**
    - `page_popup_y_fix` is missing;
    - `SetWindowRect` no longer passes corrected `window_rect` downstream;
    - `setIgnoresMouseEvents:YES` reassertions are removed or weakened;
    - the trace is still dominated by old Issue 782 input logs;
    - select/dropdown behavior changes during this experiment;
    - the experiment adds new alt-tab diagnostics instead of only cleaning up
      obsolete logs;
    - the experiment changes popup behavior beyond deleting obsolete logs.

**Result:** Pass

The cleanup succeeded. Manual testing confirmed that the date picker y-axis
remained correct, and the date picker still opened after opening and dismissing
the select dropdown. This means Experiment 2 preserved both prior fixes: the
Issue 779 PagePopup y-axis correction and the Issue 782 Shell
`setIgnoresMouseEvents:YES` post-select click fix.

The trace confirms the same result. After the select dropdown closed, the date
picker path still reached `DateTimeChooserImpl`, `WebViewImpl::OpenPagePopup`,
`WebContentsImpl::ShowCreatedWidget`, and
`RenderWidgetHostViewMac::InitAsPopup`. The y-axis correction logged
`page_popup_y_fix applied=1`, with the raw popup rect at `1620,670`, the anchor
rect at `1620,627`, and the corrected popup rect at `1620,627`. The Shell window
state logs continued to show `ignoresMouseEvents=true` before and after the
select menu.

The obsolete high-volume input logs were removed: the Experiment 2 trace no
longer contains the old `mouse_forward_boundary`, `wezboard_mouse_dispatch`,
`wezboard_appkit_dispatch`, or `pagepopup_alt_tab` flood.

#### Conclusion

Experiment 2 restored a clean diagnostic baseline without changing popup
behavior. It did not newly fix the select dropdown; it verified that the
previous post-select fix survived the cleanup. The remaining bugs are still
deferred to follow-up experiments: PagePopup-family alt-tab persistence, select
dropdown x-position, datalist behavior, and suspicious lingering
`RenderWidgetPopupWindow` entries.

### Experiment 3: Trace PagePopup Alt-Tab Persistence

#### Description

This experiment targets exactly one remaining bug: PagePopup-family native
widgets (`date`, `time`, and `color`) remain visible after the user Cmd-Tabs
away from Wezboard. Do not investigate or change the select dropdown x-position,
the datalist input, or select menu behavior in this experiment.

The leading hypothesis is cross-process activation loss. The visible app is
Wezboard, but PagePopup windows are owned by the separate Roamium/Chromium
process. When the user Cmd-Tabs away from Wezboard, AppKit sends deactivation
notifications to the Wezboard process. Chromium may not receive a corresponding
`NSApplicationDidResignActiveNotification`, so it may not know it should dismiss
the PagePopup window.

The goal is to identify which boundary fails:

```text
Wezboard loses app/window focus
        ↓
TermSurf / Roamium learns focus changed
        ↓
Chromium app/window state changes or receives a protocol signal
        ↓
Blink/WebView/PagePopup closes active PagePopup
        ↓
RenderWidgetPopupWindow disappears
```

This is a logs-only experiment. It must not add a dismissal fix yet.

#### Changes

1. **Preserve Experiment 2 invariants.**

   Before adding logs, inspect and preserve:
   - `WebPagePopupImpl::SetWindowRect` y correction;
   - `page_popup_y_fix`;
   - downstream `window_rect` usage;
   - all existing `setIgnoresMouseEvents:YES` reassertions;
   - the cleaned-up state where high-volume mouse-dispatch logs are absent.

   Use the deleted Experiment 1 observer code only as a reference for shape:

   ```bash
   cd /Users/ryan/dev/termsurf/chromium/src
   git show fb8a64ffe7386 -- content/libtermsurf_chromium/ts_shell_window_mac.mm
   ```

   ```bash
   cd /Users/ryan/dev/termsurf
   git show fb8a64ffe7386 -- \
     wezboard/window/src/os/macos/app.rs \
     wezboard/window/src/os/macos/window.rs
   ```

   Do not cherry-pick, revert, or resurrect the whole commit. Manually re-add
   only the activation observer helpers. Do not touch
   `WebPagePopupImpl::SetWindowRect`, the y-correction predicate, downstream
   `window_rect` usage, or any `setIgnoresMouseEvents:YES` callsite.

2. **Add Wezboard activation logs.**

   In `wezboard/window/src/os/macos/app.rs`, add low-frequency app activation
   logs behind `TERMSURF_ISSUE_779_TRACE=1`:
   - `applicationDidResignActive:`
   - `applicationDidBecomeActive:`

   In `wezboard/window/src/os/macos/window.rs`, add matching low-frequency
   window activation logs:
   - `windowDidResignKey:`
   - `windowDidBecomeKey:`

   Log:
   - event name;
   - `NSApp.isActive`;
   - key window pointer;
   - main window pointer;
   - active window id if cheaply available;
   - window id for window delegate notifications;
   - timestamp.

   Use a new marker:

   ```text
   [issue-779-trace] pagepopup_alt_tab boundary=wezboard_activation ...
   ```

   These logs should fire only on app activation changes, not per mouse event.

3. **Add Chromium activation logs.**

   In `chromium/src/content/libtermsurf_chromium/ts_shell_window_mac.mm`, add a
   notification observer installed from `TsBrowserMainParts` startup, behind the
   existing trace gate, for:
   - `NSApplicationWillResignActiveNotification`;
   - `NSApplicationDidResignActiveNotification`;
   - `NSApplicationDidBecomeActiveNotification`;
   - `NSWindowDidResignKeyNotification`;
   - `NSWindowDidBecomeKeyNotification`;
   - `NSWindowDidChangeOcclusionStateNotification`.

   Log:
   - notification name;
   - `NSApp.isActive`;
   - key window summary;
   - main window summary;
   - ordered windows top entries;
   - each listed window's class, frame, level, `isVisible`,
     `ignoresMouseEvents`, `isKeyWindow`, and `isMainWindow`.

   Use:

   ```text
   [issue-779-trace] pagepopup_alt_tab boundary=chromium_activation ...
   ```

   Anticipated result: Chromium may be silent during Wezboard Cmd-Tab. If so,
   that silence is a real diagnostic result, not a logging failure.

4. **Add PagePopup lifecycle state logs around deactivation.**

   Keep the existing low-volume PagePopup logs and add only the missing
   deactivation-oriented fields in:
   - `third_party/blink/renderer/core/exported/web_view_impl.cc`
   - `third_party/blink/renderer/core/exported/web_page_popup_impl.cc`
   - `third_party/blink/renderer/core/html/forms/date_time_chooser_impl.cc`

   Log when:
   - a PagePopup is open;
   - `CancelPagePopup`, `ClosePagePopup`, `CleanupPagePopup`, `Cancel`, and
     `ClosePopup` run;
   - the PagePopup still has `page_`, `popup_client_`, and `closing_` state.

   Use the existing `native_popup_attempt` / `WebPagePopupImpl::*` lines where
   possible. Do not create per-frame or per-input logs.

5. **Add RenderWidgetPopupWindow state snapshots.**

   In Chromium's macOS popup/window code, add low-frequency snapshots when:
   - the PagePopup window is created or initialized as popup;
   - the PagePopup is closed/cancelled;
   - Chromium receives any activation/window notification from step 3.

   Snapshot only PagePopup-family transient popup windows, especially
   `RenderWidgetPopupWindow` or level-101 windows associated with an active
   PagePopup. Include the top ordered windows only as context. Do not dump every
   unrelated window in the process.

   Log:
   - window pointer;
   - class name;
   - frame;
   - level;
   - visibility;
   - `ignoresMouseEvents`;
   - key/main state;
   - parent/child window summary.

   This should tell us whether the visible leftover widget is a
   `RenderWidgetPopupWindow` still owned by Chromium and whether it survives
   after Wezboard deactivation.

6. **Do not add a protocol message or fix.**

   This experiment must not add:
   - a TermSurf deactivation protobuf;
   - a Wezboard-to-Roamium deactivation IPC;
   - forced PagePopup dismissal;
   - synthetic clicks, key events, or mouse events.

   Those are candidate fixes for the next experiment after this trace names the
   failing boundary.

#### Verification

1. Confirm the Chromium branch:

   ```bash
   cd /Users/ryan/dev/termsurf/chromium/src
   git branch --show-current
   ```

   It must be:

   ```text
   148.0.7778.97-issue-783
   ```

2. Build through project scripts:

   ```bash
   cd /Users/ryan/dev/termsurf
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh wezboard
   scripts/build.sh webtui
   ```

3. Run a trace-off baseline.

   Start the test page:

   ```bash
   cd /Users/ryan/dev/termsurf
   bun test-html/server.ts
   ```

   Start Wezboard without `TERMSURF_ISSUE_779_TRACE`:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-783-exp3-baseline-state/termsurf

   XDG_STATE_HOME="$PWD/logs/issue-783-exp3-baseline-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-783-exp3-baseline-wezboard.log
   ```

   Launch the TUI with the same command from step 5, then confirm:
   - date y-axis is correct;
   - date still opens after a select dismissal;
   - no `[issue-779-trace]` lines appear in the baseline logs.

   Quit Wezboard and the TUI before continuing.

4. Start Wezboard with fresh Experiment 3 trace logs:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-783-exp3-state/termsurf

   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-783-exp3-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-783-exp3-wezboard.log
   ```

5. Launch the TUI:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

6. Re-check the non-negotiable invariants:
   - click the date input;
   - confirm the date picker y position is correct;
   - close it;
   - click the select dropdown once;
   - dismiss it;
   - click the date input again;
   - confirm it still opens and y remains correct.

   If either invariant regresses, stop and mark the experiment failed.

7. Run the focused alt-tab sequence:
   - open the date picker;
   - while it is still open, Cmd-Tab to Finder or another standard windowed app
     on the same Space;
   - observe whether the date picker remains visible;
   - Cmd-Tab back to Wezboard;
   - close the date picker if it remains.

   Mouse-based deactivation by clicking another app's window is a future
   follow-up if Cmd-Tab does not explain the bug.

8. Repeat step 7 for `time` and `color` only if the date trace is readable. Stop
   after the first unreadable or ambiguous trace.

9. Extract the focused trace:

   ```bash
   rg -a "\[issue-779-trace\]|pagepopup_alt_tab|page_popup_y_fix|DateTimeChooserImpl|WebPagePopupImpl|WebViewImpl::.*PagePopup|ShowCreatedWidget|InitAsPopup|RenderWidgetPopupWindow|chromium_shell_window_state|ignoresMouseEvents" \
     logs/issue-783-exp3-wezboard.log \
     logs/issue-783-exp3-state/termsurf/webtui-trace.log \
     logs/issue-783-exp3-state/termsurf/roamium-trace.log \
     logs/issue-783-exp3-state/termsurf/chromium-server.log \
     > logs/issue-783-exp3-trace.log
   ```

10. Pass criteria:
    - date y-axis remains correct;
    - date still opens after select;
    - Wezboard activation logs fire on Cmd-Tab away;
    - the trace shows whether Chromium activation logs fire or stay silent;
    - the trace shows whether PagePopup close/cancel cleanup fires during
      Cmd-Tab;
    - the trace shows whether a `RenderWidgetPopupWindow` remains visible after
      Wezboard deactivation;
    - no high-volume per-mouse or per-frame logs are reintroduced.

    After a successful implementation and result, export Chromium patches to
    `chromium/patches/issue-783/` before closing the experiment.

11. Fail criteria:
    - **date y-axis regresses;**
    - post-select date opening regresses;
    - the trace reintroduces the old Issue 782 input flood;
    - the experiment changes popup behavior instead of logging only;
    - the trace cannot distinguish:
      - Wezboard deactivated but Chromium was silent;
      - Chromium deactivated but PagePopup cleanup did not run;
      - PagePopup cleanup ran but the popup NSWindow remained visible.

#### Expected Interpretations

- If Wezboard logs `applicationDidResignActive` but Chromium logs nothing, the
  bug is likely structural cross-process activation loss. The next experiment
  should design a TermSurf protocol signal from Wezboard to Roamium/Chromium to
  dismiss active native popups on GUI deactivation.
- If Chromium logs deactivation but PagePopup cleanup does not run, the fix is
  likely a Chromium-side activation observer that calls the existing PagePopup
  cancellation path.
- If PagePopup cleanup runs but `RenderWidgetPopupWindow` remains visible, the
  fix is likely in popup window ownership/destruction.
- If all cleanup runs and the window disappears, the visible persistence may be
  from a different popup window family and needs a narrower follow-up.
- If all cleanup runs and the Chromium popup NSWindow disappears, but the popup
  remains visually present, the persistence is likely at the CALayerHost or
  Wezboard overlay compositor layer. The next experiment should trace Wezboard's
  overlay rendering path.

**Result:** Pass

The user ran the trace and confirmed the bug still reproduces: PagePopup-family
widgets remain visible when Cmd-Tabbing away from Wezboard. The trace also
confirmed that the protected invariants survived this experiment:

- `page_popup_y_fix applied=1` fired on PagePopup opens, with corrected y
  matching the anchor y. The date/time/color y-axis fix remains intact.
- Post-select date opening remained usable before the alt-tab test.
- The new logs were low-frequency activation and popup lifecycle logs, not a
  return of the Issue 782 per-mouse input flood.

The decisive signal is at the activation boundary. On each focused Cmd-Tab
sequence, Wezboard logged:

```text
pagepopup_alt_tab boundary=wezboard_activation event=windowDidResignKey app_is_active=false
pagepopup_alt_tab boundary=wezboard_activation event=applicationDidResignActive app_is_active=false
```

At the same boundary, Chromium did not log
`NSApplicationWillResignActiveNotification` or
`NSApplicationDidResignActiveNotification`. Chromium activation logs did fire
for its own window/occlusion events, so the observer was installed and working,
but Chromium never learned that the visible GUI app had deactivated. In
Chromium's process, `app_is_active` stayed true.

The popup-window snapshots showed the visible leftovers are real Chromium popup
windows, not just stale Wezboard compositor images. The trace repeatedly showed
visible `RenderWidgetPopupWindow` instances at level 101 with
`ignoresMouseEvents=false`, for example:

```text
pagepopup_window_count=4
class=RenderWidgetPopupWindow level=101 visible=true ignoresMouseEvents=false
```

Cleanup did not run at Cmd-Tab time. It ran only after returning to Wezboard and
closing/dismissing the picker:

```text
WebViewImpl::CleanupPagePopup page_popup_before=...
WebViewImpl::CleanupPagePopup.after page_popup_after=0
WebPagePopupImpl::ClosePopup.after has_page=0 closing=1
```

#### Conclusion

The bug is structural cross-process activation loss. Wezboard is the foreground
AppKit app, but PagePopup windows live in the separate Roamium/Chromium process.
When Wezboard deactivates, AppKit notifies Wezboard, but Chromium does not
receive a corresponding app deactivation notification, so Chromium has no native
signal telling it to dismiss the active PagePopup.

The next experiment should implement the narrow fix boundary named by this
trace: send a TermSurf protocol signal from Wezboard to Roamium/Chromium when
the GUI app deactivates, and have Chromium dismiss any active PagePopup-family
native widget on receipt. Do not chase popup window ownership or compositor
staleness first; the trace shows the cleanup path is correct when it is actually
invoked.

### Experiment 4: Sync GUI Active State to Chromium

#### Description

Experiment 3 found the actual boundary: Wezboard deactivates, but Chromium does
not. PagePopup windows are owned by the separate Roamium/Chromium process, so
AppKit does not naturally tell Chromium to dismiss the active PagePopup when the
visible Wezboard app loses focus.

This experiment implements the narrow fix for that boundary. On Wezboard
application deactivation, send a TermSurf protocol message to Roamium/Chromium
for each connected browser process. Roamium forwards the request through
`libtermsurf_chromium`, and Chromium marks the affected page inactive. Blink's
existing page-focus-loss path dismisses active PagePopup-family widgets.

The protocol is symmetric. When Wezboard becomes active again, it sends the
active state back to Chromium for the focused browser pane so the page is not
left permanently defocused after one Cmd-Tab cycle.

This experiment targets only PagePopup-family controls: `date`, `time`,
`datetime-local`, and `color`. It must not change select dropdown positioning,
datalist behavior, Shell window placement, Shell mouse transparency, or the
PagePopup y-axis correction.

#### Non-Negotiable Invariants

Before and after this experiment:

- `WebPagePopupImpl::SetWindowRect` must keep the Issue 779 y correction.
- `page_popup_y_fix` must still log when the correction runs.
- Corrected `window_rect`, not raw `rect_in_screen`, must still flow into
  `SetPendingWindowRect`, `SetPopupBounds`, and `initial_rect_`.
- All Issue 782 `[window setIgnoresMouseEvents:YES]` Shell-window reassertions
  must remain intact.
- Opening and dismissing a `<select>` must not stop later PagePopup controls
  from opening.

If any of these regress, the experiment fails immediately, even if the alt-tab
bug improves.

#### Changes

1. **Add a focused protocol message.**

   In `proto/termsurf.proto`, add a GUI-to-Chromium state message:

   ```proto
   message SetGuiActive {
     int64 tab_id = 1;
     bool active = 2;
     string reason = 3;
   }
   ```

   Add it to `TermSurfMessage` as the next available field:

   ```proto
   SetGuiActive set_gui_active = 33;
   ```

   Semantics:
   - `tab_id > 0`: set GUI active state for that tab.
   - `tab_id == 0`: set GUI active state for every tab in the receiving browser
     process.
   - `active=false`: the visible GUI app deactivated; Chromium should mark the
     page inactive and dismiss active PagePopups through existing cleanup.
   - `active=true`: the visible GUI app reactivated; Chromium should restore
     page focus for the focused browser pane/tab.
   - `reason` should be `"gui_deactivated"` or `"gui_activated"` for this
     experiment.

   Use a new message instead of overloading `FocusChanged`; this keeps app
   activation explicit and separate from pane/mode focus.

2. **Send the message from Wezboard when the app activates/deactivates.**

   In `wezboard/window/src/connection.rs`, add application events such as:

   ```rust
   ApplicationDeactivated
   ApplicationActivated
   ```

   In `wezboard/window/src/os/macos/app.rs`, update
   `application_did_resign_active` so it still logs the Experiment 3 activation
   trace, then dispatches `ApplicationEvent::ApplicationDeactivated`. Update
   `application_did_become_active` to dispatch
   `ApplicationEvent::ApplicationActivated`.

   In `wezboard/wezboard-gui/src/frontend.rs`, handle both new application
   events by asking the TermSurf connection/state layer to sync GUI active
   state.

   Adding `ApplicationEvent` variants may expose additional exhaustive Rust
   matches. Update each match site explicitly: handle these variants where the
   frontend can reach TermSurf state, and use a deliberate no-op only where app
   activation is genuinely irrelevant.

   In `wezboard/wezboard-gui/src/termsurf/conn.rs`, add a small helper that
   sends:

   ```rust
   TermSurfMessage {
       msg: Some(Msg::SetGuiActive(proto::SetGuiActive {
           tab_id,
           active,
           reason,
       })),
   }
   ```

   Deactivation should broadcast `active=false`, `tab_id=0`,
   `reason="gui_deactivated"` to every connected Chromium server with a live
   `server.tx`. This dismisses any PagePopup that may be visible from any
   browser process.

   Reactivation should send `active=true`, `reason="gui_activated"` only for the
   currently focused browser pane/tab. Do not broadcast `active=true` to every
   tab; inactive panes should not all gain page focus when the app comes back.

   Log one low-frequency summary:

   ```text
   [issue-779-trace] pagepopup_alt_tab boundary=wezboard_protocol event=set_gui_active active=... reason=... servers=N messages_sent=M tab_id=...
   ```

   Do not send per-pane mouse or focus logs.

3. **Forward the message in Roamium.**

   In `roamium/src/dispatch.rs`, handle `Msg::SetGuiActive`.
   - If `tab_id == 0`, iterate all known tabs and call a new FFI function for
     each tab handle.
   - If `tab_id > 0`, find that tab and call the same FFI function once.
   - If the tab is missing, log the miss and do nothing else.

   In `roamium/src/ffi.rs`, add the new C binding:

   ```rust
   pub fn ts_set_gui_active(wc: TsWebContents, active: bool);
   ```

   Add a trace summary:

   ```text
   [issue-779-trace] pagepopup_alt_tab boundary=roamium_protocol event=set_gui_active active=... reason=... tab_id=... target_count=N
   ```

   `target_count` means the number of tabs in this Roamium process that the
   message was forwarded to. It is typically one for a specific `tab_id`, and
   all open tabs for `tab_id=0`.

4. **Dismiss PagePopups inside Chromium.**

   In `chromium/src/content/libtermsurf_chromium/libtermsurf_chromium.h` and
   `.cc`, add:

   ```c++
   TS_EXPORT void ts_set_gui_active(ts_web_contents_t wc, bool active);
   ```

   In `chromium/src/content/libtermsurf_chromium/ts_browser_main_parts.h` and
   `.cc`, add a matching method.

   The implementation must run on Chromium's UI thread and find the tab for the
   supplied `WebContents`.

   For `active=false`, send renderer page focus loss through the main frame
   widget. Blink's existing `WebViewImpl::SetPageFocus(false)` calls
   `CancelPagePopup()`, which is the cleanup path Experiment 3 proved works.

   For `active=true`, restore renderer page focus for the focused browser tab by
   sending the matching page-focus-gained path. This prevents a Cmd-Tab cycle
   from leaving the page permanently defocused.

   This is broader than a hypothetical direct `CancelPagePopup()` call because
   it also fires normal focus/blur behavior. That is intentional: Wezboard is
   the user-visible app, so Chromium should track Wezboard's app-active state.
   The symmetry above is required because page focus is intentionally changed.

   Concretely, use the tab's main `RenderWidgetHostImpl` and call
   `SetPageFocus(active)` or the closest existing Chromium API that produces the
   same renderer-side `WebViewImpl::SetPageFocus(active)` path. Do not destroy
   `NSWindow` objects directly, and do not synthesize mouse, keyboard, or click
   events.

   Keep the existing Experiment 3 PagePopup cleanup logs. The successful path
   for `active=false` should show:

   ```text
   WebViewImpl::CancelPagePopup
   WebPagePopupImpl::Cancel
   WebPagePopupImpl::ClosePopup
   WebViewImpl::CleanupPagePopup
   ```

   Add one low-frequency entry before the request is sent:

   ```text
   [issue-779-trace] pagepopup_alt_tab boundary=chromium_protocol event=set_gui_active active=... webcontents=... reason=...
   ```

5. **Regenerate protocol bindings through the normal build.**

   Use the project scripts. Do not run one-off protobuf commands unless the
   build scripts fail and the failure proves a manual regeneration step is
   required.

6. **Do not broaden scope.**

   This experiment must not:
   - change PagePopup placement math;
   - change `<select>` menu positioning;
   - change datalist behavior;
   - change Shell window frame movement;
   - remove or weaken `ignoresMouseEvents`;
   - reintroduce high-volume mouse/input-router logs;
   - add runtime experiment flags.

#### Verification

1. Confirm the Chromium branch:

   ```bash
   cd /Users/ryan/dev/termsurf/chromium/src
   git branch --show-current
   ```

   It must be:

   ```text
   148.0.7778.97-issue-783
   ```

2. Build through project scripts:

   ```bash
   cd /Users/ryan/dev/termsurf
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh wezboard
   scripts/build.sh webtui
   ```

3. Start the test page:

   ```bash
   cd /Users/ryan/dev/termsurf
   bun test-html/server.ts
   ```

4. Start Wezboard with fresh Experiment 4 logs:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-783-exp4-state/termsurf

   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-783-exp4-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-783-exp4-wezboard.log
   ```

5. Launch the TUI:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

6. Re-check protected invariants first:
   - click the date input;
   - confirm the date picker y position is correct;
   - close it;
   - click the select dropdown once;
   - dismiss it;
   - click the date input again;
   - confirm it still opens and y remains correct.

   Stop immediately if either invariant fails.

7. Test the PagePopup deactivation fix:
   - open the date picker;
   - while it is open, Cmd-Tab to Finder or another standard windowed app on the
     same Space;
   - confirm the date picker disappears when Wezboard deactivates;
   - Cmd-Tab back to Wezboard;
   - confirm the page is still usable and the date picker can open again;
   - type into a normal text input on the page, such as the datalist text input
     if it accepts typing, and confirm keyboard input works after reactivation.

8. Repeat step 7 for:
   - `time`;
   - `datetime-local`, if present on the test page;
   - `color`.

   Do not test select x-position or datalist in this experiment.

9. Extract the focused trace:

   ```bash
   rg -a "\[issue-779-trace\]|pagepopup_alt_tab|SetGuiActive|set_gui_active|ts_set_gui_active|page_popup_y_fix|DateTimeChooserImpl|WebPagePopupImpl|WebViewImpl::.*PagePopup|RenderWidgetPopupWindow|chromium_protocol|roamium_protocol|wezboard_protocol" \
     logs/issue-783-exp4-wezboard.log \
     logs/issue-783-exp4-state/termsurf/webtui-trace.log \
     logs/issue-783-exp4-state/termsurf/roamium-trace.log \
     logs/issue-783-exp4-state/termsurf/chromium-server.log \
     > logs/issue-783-exp4-trace.log
   ```

10. Pass criteria:
    - date y-axis remains correct;
    - date still opens after select;
    - Wezboard logs `ApplicationDeactivated` and sends
      `SetGuiActive(active=false)`;
    - Roamium receives the deactivation message and calls
      `ts_set_gui_active(false)`;
    - Chromium receives the deactivation request;
    - existing PagePopup cleanup logs run on Cmd-Tab away;
    - the visible PagePopup disappears for date, time, and color when Wezboard
      deactivates;
    - Wezboard logs `ApplicationActivated` and sends `SetGuiActive(active=true)`
      for the focused browser tab;
    - Roamium and Chromium receive the reactivation request;
    - returning to Wezboard leaves the page usable, including keyboard input to
      a normal text input;
    - no high-volume input logs are reintroduced.

11. Fail criteria:
    - **date y-axis regresses;**
    - post-select date opening regresses;
    - any `setIgnoresMouseEvents:YES` reassertion is removed or weakened;
    - Wezboard deactivation does not send `SetGuiActive(active=false)`;
    - Roamium receives a GUI active-state message but Chromium does not;
    - Chromium receives `active=false` but no PagePopup cleanup runs;
    - cleanup runs but the popup window remains visible;
    - Wezboard reactivation does not send `SetGuiActive(active=true)` for the
      focused browser tab;
    - keyboard input or focused-page behavior is broken after Cmd-Tab back;
    - the fix closes or navigates the tab;
    - select dropdown x-position, datalist behavior, or unrelated popup paths
      change.

#### Expected Interpretations

- If all pass criteria hold, the PagePopup alt-tab bug is fixed. The next issue
  783 experiment should move to the select dropdown x-position bug.
- If the protocol reaches Chromium but no cleanup runs, the C API is not
  reaching the renderer page-focus-loss path; inspect the `RenderWidgetHostImpl`
  focus call and the renderer `WebViewImpl::SetPageFocus(false)` delivery.
- If deactivation works but page input is broken after returning to Wezboard,
  the `active=true` restoration path is incomplete or is targeting the wrong
  tab.
- If cleanup runs but the popup remains visible, the next experiment should
  target popup `NSWindow` ownership/destruction, not the protocol boundary.
- If date is fixed but time or color remains visible, split the PagePopup-family
  paths and add control-type-specific cleanup tracing.

After a passing Chromium implementation, export the Chromium changes to
`chromium/patches/issue-783/` before considering the experiment complete.
