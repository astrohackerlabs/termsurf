+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 669: Active Pane Indicator (Take 2)

Revisit pane borders and desaturation now that the resize regression is fixed.

## Background

Issue 667 attempted to add configurable pane borders and unfocused-pane
desaturation. Three experiments all failed with the same symptom: panes didn't
resize when the window was resized or a new split was opened. Typing a key
triggered the resize.

Issue 668 discovered the real cause: Issue 666 had broken TUI resize by only
forwarding `Event::Key` from the crossterm reader thread, silently dropping
`Event::Resize`. The fix was trivial — forward all relevant event types.

This means **all three Issue 667 experiments were tested against a broken
baseline**. The conclusions blaming SwiftUI modifiers and non-empty
`updateOSView` may be wrong. We retry both borders and desaturation with the
resize fix in place.

## Strategy

Follow Ghostty's existing pattern: layout-dependent visual effects live in the
platform layer, not the renderer. The renderer doesn't know if it's in a split —
that state lives in `SurfaceWrapper.isSplit` (Swift) and GTK's `is-split`
property. The existing unfocused pane dimming uses this same pattern (SwiftUI
overlay on macOS, GTK Revealer on Linux).

Config fields go in Zig (Config.zig) — shared across all platforms. The visual
effect goes in Swift (SurfaceView.swift) — same ZStack overlay pattern as the
existing unfocused dimming.

Test features incrementally. Each experiment adds one thing.

## Experiment 1: Border via SwiftUI overlay

### Hypothesis

A `Rectangle().strokeBorder()` in the ZStack — the same pattern as the existing
unfocused opacity overlay — will render pane borders without breaking resize.

This was never tested in isolation in Issue 667. Experiment 2 combined it with
`.saturation()` on the representable and `.shadow()` on the ZStack. Experiment 3
used `updateOSView` instead. Neither tested a plain SwiftUI overlay alone.

The existing unfocused overlay
(`Rectangle().fill().allowsHitTesting(false) .opacity()`) proves that SwiftUI
overlays in the ZStack work. A stroke border overlay follows the same pattern.

### Config

Three new config options (same as Issue 667):

```
focused-split-border-color = 7dcfff
unfocused-split-border-color = 565f89
split-border-width = 2
```

All default to off (no border). Backward compatible.

### Changes

#### 1. Config.zig — 3 new fields after `split-divider-color`

```zig
@"focused-split-border-color": ?Color = null,
@"unfocused-split-border-color": ?Color = null,
@"split-border-width": f64 = 0,
```

Clamp in `finalize()`:

```zig
self.@"split-border-width" = @min(10.0, @max(0, self.@"split-border-width"));
```

#### 2. TermSurf.Config.swift — 3 new properties after `splitDividerColor`

```swift
var focusedSplitBorderColor: Color? { ... }
var unfocusedSplitBorderColor: Color? { ... }
var splitBorderWidth: Double { ... }
```

Same `termsurf_config_get` pattern as existing properties.

#### 3. SurfaceView.swift — border overlay in the ZStack

Add after the unfocused opacity overlay (line 231), before the grab handle (line
233). Same pattern as the existing overlay — a separate view in the ZStack, no
modifiers on the representable or the ZStack itself:

```swift
// Pane border (Issue 669).
if isSplit {
    let borderWidth = termsurf.config.splitBorderWidth
    if borderWidth > 0 {
        let borderColor = surfaceFocus
            ? termsurf.config.focusedSplitBorderColor
            : termsurf.config.unfocusedSplitBorderColor
        if let color = borderColor {
            Rectangle()
                .strokeBorder(color, lineWidth: borderWidth)
                .allowsHitTesting(false)
        }
    }
}
```

**No changes to SurfaceRepresentable.** `updateOSView` stays empty. No SwiftUI
visual modifiers on the representable or the ZStack.

### Result: PASS

Borders render correctly and resize works. All test criteria met:

1. Build compiles without errors.
2. Focused pane shows cyan border, unfocused shows dim border.
3. Switching focus swaps border colors immediately.
4. Window resize works correctly — no regression.
5. Opening a new split resizes existing panes correctly.
6. `split-border-width = 0` disables borders (backward compatible).

This confirms that the Issue 667 failures were caused by the Issue 666 resize
regression, not by SwiftUI overlays. The `Rectangle().strokeBorder()` overlay in
the ZStack — following the exact same pattern as the existing unfocused opacity
overlay — works without any side effects.

Key lesson: SwiftUI overlays in the ZStack are safe. The Issue 667 experiments
that used `.saturation()` on the representable or non-empty `updateOSView` were
red herrings — the resize was already broken before those changes.

## Experiment 2: Unfocused pane desaturation

### Hypothesis

A `.saturation()` modifier on `SurfaceRepresentable` will desaturate unfocused
panes without breaking resize.

Issue 667 Experiment 2 blamed `.saturation()` for breaking resize. But
Experiment 1 above proved that the resize regression was caused by Issue 666,
not by SwiftUI modifiers. The `.saturation()` modifier was never tested in
isolation against a working baseline.

SwiftUI's `.saturation()` applies a Core Image filter to the view's backing
layer. For a Metal-backed NSView inside an NSViewRepresentable, this should work
— SwiftUI wraps the view in a hosting layer and applies the CI filter on that
layer. The question is whether it breaks resize. We test that directly.

### Config

One new config option:

```
unfocused-split-saturation = 0.5
```

Defaults to 1.0 (full color, no desaturation). Setting to 0.0 gives full
grayscale. Backward compatible — existing behavior unchanged unless configured.

### Changes

#### 1. Config.zig — 1 new field after `split-border-width`

```zig
@"unfocused-split-saturation": f64 = 1.0,
```

Clamp in `finalize()`:

```zig
self.@"unfocused-split-saturation" = @min(1.0, @max(0, self.@"unfocused-split-saturation"));
```

#### 2. TermSurf.Config.swift — 1 new property after `splitBorderWidth`

```swift
var unfocusedSplitSaturation: Double {
    guard let config = self.config else { return 1.0 }
    var value: Double = 0
    let key = "unfocused-split-saturation"
    _ = termsurf_config_get(config, &value, key, UInt(key.lengthOfBytes(using: .utf8)))
    return value
}
```

#### 3. SurfaceView.swift — `.saturation()` on `SurfaceRepresentable`

Add after `.focused($surfaceFocus)` (line 74):

```swift
SurfaceRepresentable(view: surfaceView, size: geo.size)
    .focused($surfaceFocus)
    .saturation(isSplit && !surfaceFocus
        ? termsurf.config.unfocusedSplitSaturation : 1.0)
    // ... remaining existing modifiers
```

No ZStack overlay needed — this is a direct modifier on the representable.

**No changes to `updateOSView`.** It stays empty.

### Result: PASS

Desaturation works correctly. Unfocused panes appear washed out, focused pane
stays full color. Switching focus swaps saturation immediately. Resize works —
no regression. Borders from Experiment 1 work alongside desaturation.

This definitively proves that Issue 667's conclusion — that `.saturation()` on
`SurfaceRepresentable` breaks resize — was wrong. The resize regression was
caused by Issue 666 (dropped `Event::Resize` in the TUI reader thread), not by
SwiftUI modifiers.

## Conclusion

Two new visual indicators for active pane identification, both working
correctly:

1. **Configurable pane borders** (Experiment 1) — `Rectangle().strokeBorder()`
   overlay in the ZStack, same pattern as Ghostty's existing unfocused dimming.
   Three config options: `focused-split-border-color`,
   `unfocused-split-border-color`, `split-border-width`.

2. **Unfocused pane desaturation** (Experiment 2) — `.saturation()` modifier on
   `SurfaceRepresentable`. One config option: `unfocused-split-saturation` (1.0
   = full color, 0.0 = grayscale).

Both features default to off (no border, full saturation) and are backward
compatible. Neither breaks resize.

The key insight across Issues 667–669: all three Issue 667 experiments were
tested against a broken baseline (Issue 666 had silently dropped
`Event::Resize`). Once Issue 668 fixed the resize regression, both SwiftUI
overlays and `.saturation()` modifiers worked on the first try.

Changes across 3 files:

- `gui/src/config/Config.zig` — 4 new config fields + clamping in `finalize()`
- `gui/macos/Sources/TermSurf/TermSurf.Config.swift` — 4 new Swift property
  accessors
- `gui/macos/Sources/TermSurf/Surface View/SurfaceView.swift` — border overlay
  in ZStack + `.saturation()` modifier on representable
