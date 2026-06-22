+++
status = "open"
opened = "2026-06-22"
+++

# Issue 836: WebTUI Top Browser Controls

## Goal

Move the WebTUI browser controls from the bottom of the terminal pane to the
top, above the browser viewport, so the `web` interface reads more like a normal
browser.

## Background

The current WebTUI layout places the URL bar and keybinding indicators at the
bottom of the pane. TermSurf has two interaction modes:

- **Web mode**: keyboard and mouse input primarily target the web page.
- **Control mode**: keyboard input primarily targets the WebTUI chrome and
  browser controls.

The requested visual model is browser-like chrome at the top: URL bar and
mode/keybinding indicators should appear above the web viewport rather than
below it.

## Scope

Primary target:

- `webtui/`

The implementation should audit the current layout code and move all visible
control chrome above the viewport, including:

- URL bar
- mode indicator
- keybinding indicators / help strip
- loading or navigation status that is currently grouped with those controls

This issue should preserve the existing browser viewport semantics: the viewport
should remain the area below the controls, and protocol geometry sent to
Ghostboard should continue to match the actual browser overlay area.

## Acceptance Criteria

- In Web mode, the URL bar and keybinding indicators render above the browser
  viewport.
- In Control mode, the URL bar and keybinding indicators render above the
  browser viewport.
- The browser viewport begins below the controls and does not overlap them.
- Webview overlay geometry still matches the visible viewport after the move.
- Existing keybindings keep their current behavior unless an experiment
  explicitly documents a necessary change.
- The layout works across small and large terminal panes without text overlap or
  viewport collapse.
- The issue records screenshots or terminal captures before and after the layout
  change.

## Experiments

- [Experiment 1: Move browser controls above viewport](01-move-browser-controls-above-viewport.md)
  — **Designed**
