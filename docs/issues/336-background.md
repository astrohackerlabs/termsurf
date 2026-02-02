# Issue 336: Default White Background for Webviews

## Problem

Some web pages render with a transparent background, causing the WezTerm
terminal background color to show through. This looks broken because web pages
assume they're rendered on a white canvas (the browser standard).

### Example

A page with no explicit `background-color` CSS:

- **Expected**: White background (like Chrome/Safari)
- **Actual**: Terminal's dark background bleeds through

## Product Requirements

### User Story

As a user browsing the web in TermSurf, I expect pages to look the same as they
do in Chrome or Safari, with a white default background.

### Acceptance Criteria

1. Webviews render with a white background by default
2. Pages that explicitly set a background color (including dark mode sites)
   display correctly
3. Transparent elements blend over white, not the terminal background

### Non-Requirements (Out of Scope)

- Configurable default background color (future enhancement)
- Dark mode default option (future enhancement)
- Per-profile background settings (future enhancement)

## Technical Context

CEF likely has a setting for the default background color during browser
creation or in the render handler. The fix should set this to white (`#FFFFFF`
or `rgba(255, 255, 255, 1)`).

## Files Involved

- `ts3/termsurf-profile/src/main.rs` — Browser creation and render handler

---

## Experiments

(To be designed)
