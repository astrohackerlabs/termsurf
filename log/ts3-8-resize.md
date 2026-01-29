# TermSurf 3.0 Dynamic Browser Resize

## Background

### Progress So Far

ts3-7 completed the one-process-per-profile architecture:

- Multiple webviews can share a single CEF process
- Each webview renders within its pane bounds
- The launcher routes requests to existing profile processes
- IOSurface Mach ports are correctly sent to the right GUI panes

The webview rendering pipeline now works for the **initial render**. When a user
runs `web google.com`, the page renders at the correct pane size and displays
within the pane bounds.

### The Problem

When the user resizes the window or splits panes, the webview does not re-render
at the new size. Instead, the existing texture is stretched or compressed to fit
the new viewport dimensions. This produces a blurry, distorted image that does
not match what a user expects from a browser.

**Current behavior:**

1. User runs `web google.com` in a full-width pane
2. Page renders correctly at 1200x800
3. User splits the pane vertically (now 600x800)
4. The 1200x800 texture is squished into 600x800 viewport
5. Text becomes unreadable, images distorted

**Expected behavior:**

1. User runs `web google.com` in a full-width pane
2. Page renders correctly at 1200x800
3. User splits the pane vertically (now 600x800)
4. CEF re-renders the page at 600x800
5. Page reflows naturally, text remains crisp

This is how every browser works. When you resize a browser window, the page
reflows to fit the new width. TermSurf must do the same.

## Goal

When a pane containing a webview is resized, the browser must re-render at the
new dimensions so the page content reflows naturally.

## Product Requirements

### Core Requirement

**When a webview pane changes size, the browser must resize to match.**

This applies to all resize scenarios:

1. **Window resize** — User drags the window edge to make it larger or smaller
2. **Pane split** — User splits a pane, reducing the original pane's size
3. **Pane close** — User closes an adjacent pane, expanding the remaining pane
4. **Divider drag** — User drags the divider between panes to adjust sizes

In all cases, the webview must re-render at the new size, not stretch the old
texture.

### User Experience

**Resize should feel responsive.** The page should update quickly enough that
the user perceives it as "live" resizing, similar to resizing a Chrome or Safari
window.

**Content should reflow naturally.** Text should wrap to fit the new width.
Images should maintain aspect ratio. Responsive layouts should adapt.

**No visual artifacts.** During resize, it's acceptable to briefly show the
stretched old texture while waiting for the new render. However, the final state
must always be a correctly-sized render.

### Edge Cases

1. **Rapid resizing** — User drags window edge continuously. The system should
   debounce or throttle resize events to avoid overwhelming CEF with resize
   requests. A brief delay (e.g., 30-50ms settle time) before triggering
   re-render is acceptable.

2. **Multiple webviews** — When a window resize affects multiple webview panes,
   all of them must resize. Each webview is independent; they may complete their
   re-renders at different times.

3. **Minimum size** — There may be a minimum practical size for webviews. If a
   pane becomes too small (e.g., < 100px in either dimension), the webview
   behavior is undefined. It's acceptable to show a placeholder or simply not
   render.

4. **Profile process crash** — If the profile process crashes during resize, the
   GUI should handle this gracefully (e.g., show an error state in the pane).

### Non-Requirements (Deferred)

The following are explicitly **not** part of this task:

- **Zoom level** — Changing the page zoom (Cmd+/Cmd-) is separate from resize
- **DPI changes** — Moving window between Retina and non-Retina displays
- **Scroll position preservation** — Maintaining scroll position across resize
  (nice to have, but not required)

## Success Criteria

- [ ] Resize window → webview re-renders at new size
- [ ] Split pane → webview re-renders at new size
- [ ] Close adjacent pane → webview re-renders at new size
- [ ] Drag pane divider → webview re-renders at new size
- [ ] Text remains crisp and readable after resize
- [ ] Page content reflows naturally (responsive layouts work)
- [ ] Multiple webviews in same window all resize correctly
- [ ] Resize feels responsive (not sluggish)

## Tasks

- [ ] Detect pane resize events in the GUI
- [ ] Send new dimensions to the profile server
- [ ] Profile server calls CEF resize API
- [ ] CEF re-renders at new size
- [ ] New IOSurface sent to GUI
- [ ] GUI updates viewport to match new size
- [ ] Implement debounce/throttle for rapid resize events

## Research

_To be filled in during implementation planning._

## Experiments

_To be added as implementation progresses._
