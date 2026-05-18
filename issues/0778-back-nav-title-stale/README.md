+++
status = "open"
opened = "2026-04-12"
+++

# Issue 778: Back navigation leaves stale title in webtui

## Goal

When the user navigates back in the webtui, the displayed page title must update to match the page being restored, not remain stuck on the title of the page they navigated away from.

## Background

In the `web` TUI, navigating from page A to page B via a link works correctly — both the URL and the title update to reflect page B. However, when the user then presses "back" to return to page A:

- The URL correctly updates back to page A.
- The title does **not** update — it remains showing page B's title.

This means the title shown in the TUI is out of sync with the actual page displayed in the browser pane after a back navigation.

## Analysis

The webtui listens for protocol messages from the browser engine to keep its chrome (URL bar, title) in sync with the tab state. The URL update path works for back navigation, but the title update path does not.

Possible causes:

1. **Missing title update on history navigation** — The browser engine may emit a `TitleChanged` message on initial page load but not when a history entry is restored from the back/forward cache.
2. **Cached title not re-sent** — When restoring from bfcache, Chromium may not fire the title-changed notification because the title hasn't technically "changed" from the engine's perspective, leaving the TUI with the previously cached value.
3. **webtui ignores title updates tied to history events** — The TUI may only update its title on explicit navigation-complete events, missing the separate title notification.

The fix likely involves ensuring the browser engine emits a title update whenever a history navigation commits, or having the webtui proactively request the current title after a back/forward navigation completes.
