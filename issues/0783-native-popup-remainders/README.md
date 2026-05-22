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
