+++
status = "closed"
opened = "2026-06-18"
closed = "2026-06-18"
+++

# Issue 822: Restore Ghostboard Cmd-Key Browser Forwarding

## Goal

Make browser-owned Command keybindings work in Ghostboard browse mode again,
starting with `Cmd+[` for browser Back.

## Background

In browse mode, `Cmd+[` should be forwarded to Roamium/Chromium as browser Back.
The documented browser navigation keybindings are:

| Key     | Mode   | Action  |
| ------- | ------ | ------- |
| `Cmd+[` | Browse | Back    |
| `Cmd+]` | Browse | Forward |
| `Cmd+R` | Browse | Reload  |

The current Ghostboard browser-side path exists:

- Swift maps macOS `[` to Windows keycode `0xDB` and Command to modifier bit
  `8`.
- Zig forwards key events only when the pane is in browse mode with an attached
  browser tab.
- Roamium forwards the protobuf `KeyEvent` to Chromium.
- Chromium special-cases `Meta + VKEY_OEM_4` as Back, `Meta + VKEY_OEM_6` as
  Forward, and `Meta + VKEY_R` as Reload.

The likely regression is earlier in the AppKit path. On macOS, Command-key
shortcuts can be routed through `performKeyEquivalent(with:)` before
`keyDown(with:)`. Current Ghostboard does not restore the legacy browse-mode
bypass there, so `Cmd+[` can be consumed by AppKit/Ghostty key-equivalent
handling before the TermSurf browser forwarding path runs.

## Legacy Behavior

Ghostboard-legacy solved this exact problem in Issue 609. Its
`performKeyEquivalent(with:)` implementation checked whether the surface was in
overlay forwarding state before Ghostty binding lookup or macOS menu fallback:

```swift
if let surface = self.surface,
   termsurf_surface_is_overlay_forwarding(surface) {
    self.keyDown(with: event)
    return true
}
```

The legacy `termsurf_surface_is_overlay_forwarding` C API returned true only
when:

- the Swift surface was registered to a TermSurf pane;
- the pane was in browse mode;
- that pane was the focused pane.

Once the event was forced into `keyDown`, legacy `Surface.keyCallback` forwarded
the key event to Chromium and consumed it while overlay forwarding was active.

## Analysis

Current Ghostboard should restore the legacy behavior in the current
architecture without reintroducing XPC. The equivalent browse-mode check now
lives in the current TermSurf socket/protobuf forwarding path, but
`performKeyEquivalent` needs an early browser-forwarding branch so AppKit cannot
swallow Command shortcuts before `keyDown`.

The fix should be scoped to browser-owned shortcuts in browse mode. It must not
break normal Ghostty/Ghostboard shortcuts outside browse mode, and it must not
steal terminal-owned shortcuts when no browser tab is attached or the pane is
not in browse mode.

## Proposed Approach

1. Restore an early `performKeyEquivalent(with:)` forwarding path for
   browse-mode browser shortcuts.
2. Use the current `termsurf_forward_key_event`/pane state path, not legacy XPC.
3. Confirm `Cmd+[` reaches Chromium and triggers Back when history exists.
4. Confirm `Cmd+]` and `Cmd+R` still work as browser-owned shortcuts in browse
   mode.
5. Confirm the same keys do not get consumed outside browse mode.
6. Add a focused regression guard that is fast enough to keep in the normal
   Ghostboard smoke suite.

## Constraints

- Do not reintroduce legacy XPC.
- Do not forward all Command-key events blindly unless the implementation proves
  the pane is in browse mode and the browser forwarding path accepts the event.
- Preserve current Control-mode behavior, including `Cmd+C` copying the current
  URL.
- Keep `docs/keybindings.md` accurate if behavior or implementation details
  change.
- Use a narrow regression test for browser navigation shortcuts; avoid a slow
  exhaustive keybinding matrix.

## Experiments

- [Experiment 1: Restore Browse-Mode Command Navigation Forwarding](01-restore-browse-command-navigation.md)
  — **Pass**

## Conclusion

Ghostboard now restores browser-owned Command navigation shortcuts in browse
mode without reintroducing legacy XPC. AppKit key equivalents for `Cmd+[`,
`Cmd+]`, and `Cmd+R` are recognized before Ghostty binding/menu fallback, but
they are consumed only when the existing TermSurf browser forwarding path
accepts the event.

The new `browser-command-navigation` smoke proves:

- pre-browse `Cmd+[` does not reach Roamium;
- browse-mode `Cmd+[` reaches Roamium as `windows_key_code=219` with Command
  modifier `8` and navigates Back;
- browse-mode `Cmd+]` reaches Roamium as `windows_key_code=221` with Command
  modifier `8` and navigates Forward.

The existing `copy-current-url-smoke` still passes, proving Control-mode `Cmd+C`
continues to copy the current URL and Browse-mode `Cmd+C` does not run the
Ghostboard URL-copy action.
