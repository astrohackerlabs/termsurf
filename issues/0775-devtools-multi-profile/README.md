+++
status = "open"
opened = "2026-04-11"
+++

# Issue 775: DevTools gets confused with multiple profiles open

## Goal

Fix DevTools targeting so opening DevTools always identifies the exact browser
engine, profile, and tab number to inspect. The DevTools protocol must not infer
the target from "the last tab" or any other global fallback; every DevTools open
request must be unambiguous even when multiple profiles or browser engines are
active.

Also update the `web` TUI UI so browser panes and DevTools panes visibly show
the full target identity: browser engine, profile, and tab number. The tab
number is currently missing from the displayed context and should be shown
alongside browser and profile so users can confirm which tab DevTools is
inspecting.

## Background

DevTools currently gets confused when multiple browser profiles are open. The
root cause is that DevTools requests do not explicitly specify which browser
engine process, profile, and tab they refer to. Current code tries to be helpful
by resolving some DevTools requests against "the last tab," but that fallback is
ambiguous and does not properly account for profile/process boundaries. When
only one profile is active this works by accident, but with multiple profiles
the targeting becomes ambiguous.

Since each profile runs in its own browser engine process (one process per
profile is a hard architectural constraint), DevTools must route to the correct
process. Opening DevTools from a tab should always open DevTools for that
specific tab — this must be guaranteed regardless of how many profiles or
engines are active.

## Analysis

The DevTools protocol messages need to be redesigned to explicitly include:

1. **Browser engine process** — Which engine process (identified by profile or
   process ID) to target.
2. **Profile** — Which profile the tab belongs to.
3. **Tab** — Which specific tab within that profile to inspect.

When a user triggers "open DevTools" from a pane, the GUI already knows which
pane is focused and which browser/profile/tab that pane maps to. This context
must be threaded through the DevTools open request so there is zero ambiguity at
every level of the message path.

This may require changes to:

- The TermSurf protocol (`termsurf.proto`) — DevTools messages may need
  additional fields for profile/engine targeting.
- The GUI's DevTools request handling — Must resolve the focused pane to a
  specific (engine, profile, tab) tuple before sending the request.
- The browser engine's DevTools handler — Must validate that the request targets
  a tab it actually owns.
- The `web` TUI display — Must show browser engine, profile, and tab number for
  browser and DevTools panes so the active target is visible to the user.
