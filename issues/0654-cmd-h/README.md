+++
status = "closed"
opened = "2026-02-27"
closed = "2026-02-27"
+++

# Issue 654: Cmd+H keybinding not overriding macOS hide

## Goal

User-configured `keybind = cmd+h=new_split:left` should create a new pane to the
left, not hide the window. The same config works in upstream Ghostty.

## Background

The user's Ghostty config has:

```
keybind = cmd+j=new_split:down
keybind = cmd+k=new_split:up
keybind = cmd+l=new_split:right
keybind = cmd+h=new_split:left
```

All four work in upstream Ghostty. In TermSurf, `cmd+j`, `cmd+k`, and `cmd+l`
work, but `cmd+h` hides the window instead of creating a split. This is the
default macOS behavior — Cmd+H is the system "Hide Application" shortcut,
automatically added by AppKit to the application menu.

## Analysis

### The keyboard event chain

1. **`performKeyEquivalent()`** in `SurfaceView_AppKit.swift` is called first.
   It checks whether the keystroke is a Ghostty keybinding via
   `surface.keyIsBinding()`.

2. If it IS a binding with the `consumed` flag (the default), and NOT marked
   `.all` or `.performable`, the code **tries the menu first**:

   ```swift
   if bindingFlags.isDisjoint(with: [.all, .performable]),
      bindingFlags.contains(.consumed) {
       if let menu = NSApp.mainMenu, menu.performKeyEquivalent(with: event) {
           return true  // Let the menu handle it
       }
   }
   ```

3. macOS automatically adds "Hide TermSurf" (Cmd+H) to the application menu.
   `menu.performKeyEquivalent(with: event)` matches this menu item and returns
   `true` — the window hides, and the Ghostty keybinding never fires.

4. `cmd+j`, `cmd+k`, and `cmd+l` work because no macOS menu items claim those
   shortcuts, so `performKeyEquivalent` falls through to `keyDown()` which
   routes to the Zig keybinding system.

### Why it works in upstream Ghostty v1.2.3

Our fork is from Ghostty `tip` (commit `6692be4`, 2453 commits ahead of v1.2.3).
The `performKeyEquivalent()` logic changed between v1.2.3 and `tip`.

**v1.2.3** — when a binding matches, goes straight to `keyDown()`:

```swift
if match {
    self.keyDown(with: event)
    return true
}
```

**tip (our fork)** — when a binding matches, tries the menu first:

```swift
if let bindingFlags {
    if keySequence.isEmpty,
       keyTables.isEmpty,
       bindingFlags.isDisjoint(with: [.all, .performable]),
       bindingFlags.contains(.consumed) {
        if let menu = NSApp.mainMenu, menu.performKeyEquivalent(with: event) {
            return true  // menu catches Cmd+H here
        }
    }
    self.keyDown(with: event)
    return true
}
```

The menu-first behavior was added intentionally (for Cmd+C copy, Cmd+V paste,
etc.) but has the side effect of letting macOS system menu items like "Hide"
override user keybindings. This is a regression in upstream Ghostty's `tip`
branch.

### Where the Hide shortcut is defined

`MainMenu.xib:125` defines the "Hide Ghostty" menu item with `keyEquivalent="h"`
(Cmd+H by default):

```xml
<menuItem title="Hide Ghostty" keyEquivalent="h" id="Olw-nP-bQN">
    <connections>
        <action selector="hide:" target="-1" id="PnN-Uc-m68"/>
    </connections>
</menuItem>
```

This is a standard AppKit menu item — not managed by `syncMenuShortcuts`. The
`hide:` selector targets `-1` (the first responder chain), which resolves to
`NSApplication.hide()`.

### Approach

Remove the `keyEquivalent` from the "Hide TermSurf" menu item in `MainMenu.xib`.
The menu item stays — users can still click it — but it won't have a keyboard
shortcut that competes with user bindings. When `performKeyEquivalent()` tries
the menu, nothing will match Cmd+H, and the event will fall through to
`keyDown()` where the Ghostty binding fires.

## Experiments

### Experiment 1: Remove Cmd+H key equivalent from Hide menu item

**Goal:** Remove the keyboard shortcut from the "Hide TermSurf" menu item so
that user-configured `keybind = cmd+h=new_split:left` works.

#### Changes

**1. `gui/macos/Sources/App/macOS/MainMenu.xib:125`** — Remove the
`keyEquivalent="h"` attribute from the "Hide Ghostty" menu item and add an empty
`modifierMask` to suppress the default Cmd modifier:

```xml
<menuItem title="Hide Ghostty" id="Olw-nP-bQN">
    <modifierMask key="keyEquivalentModifierMask"/>
    <connections>
        <action selector="hide:" target="-1" id="PnN-Uc-m68"/>
    </connections>
</menuItem>
```

No other files need to change. The menu item itself remains functional (click
still works), it just loses the Cmd+H shortcut.

#### Verification

1. Build the debug app: `cd gui && zig build`
2. Launch `TermSurf-Debug.app`
3. Confirm the "Hide TermSurf" menu item no longer shows "Cmd+H" next to it
4. With `keybind = cmd+h=new_split:left` in the config, press Cmd+H in the
   terminal — a new pane should open to the left
5. Confirm Cmd+J, Cmd+K, Cmd+L still work for the other split directions
6. Confirm "Hide TermSurf" still works when clicked from the menu

**Result: Pass.**

Removing `keyEquivalent="h"` from the "Hide Ghostty" menu item in `MainMenu.xib`
fixed the issue. Cmd+H now creates a split pane to the left as configured. The
"Hide TermSurf" menu item remains functional via click, and Cmd+J/K/L continue
to work for the other split directions.

However, this is a workaround — it removes a standard macOS shortcut from the
menu rather than fixing the root cause. The real problem is that
`performKeyEquivalent()` tries the menu before the user binding. Any other macOS
system menu item with a shortcut that conflicts with a user keybinding would
have the same problem.

### Experiment 2: Revert to v1.2.3 binding-first behavior

**Goal:** Make user keybindings always take priority over menu shortcuts, like
Ghostty v1.2.3 did. Revert the Experiment 1 workaround since it's no longer
needed.

#### Background

Between v1.2.3 and `tip`, Ghostty added a menu-first check in
`performKeyEquivalent()` so that bindings like Cmd+C and Cmd+V trigger the
corresponding menu item (which makes the menu flash). This is cosmetic — the
binding still works without it, because `keyDown()` routes to Zig's
`maybeHandleBinding()` which executes the action regardless.

The side effect is that system menu items (like "Hide", Cmd+H) can steal user
keybindings. The v1.2.3 approach — go straight to `keyDown()` — avoids this
entirely.

#### Changes

**1. `gui/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`** —
Remove the menu-first block (lines 1230–1243) from `performKeyEquivalent()`.
When a binding matches, go straight to `keyDown()`:

```swift
// If this is a binding then we want to perform it.
if let bindingFlags {
    self.keyDown(with: event)
    return true
}
```

The `bindingFlags` variable and the `surfaceModel.flatMap` check above it stay —
they still determine whether the event is a binding. Only the menu-first block
inside the `if let bindingFlags` is removed.

**2. `gui/macos/Sources/App/macOS/MainMenu.xib`** — Revert the Experiment 1
change. Restore `keyEquivalent="h"` on the "Hide Ghostty" menu item and remove
the empty `modifierMask`:

```xml
<menuItem title="Hide Ghostty" keyEquivalent="h" id="Olw-nP-bQN">
    <connections>
        <action selector="hide:" target="-1" id="PnN-Uc-m68"/>
    </connections>
</menuItem>
```

The menu item keeps its standard Cmd+H shortcut. It just won't fire when the
user has a keybinding configured for Cmd+H, because the binding check now
happens before the menu check.

#### Verification

1. Build the debug app: `cd gui && zig build`
2. Launch `TermSurf-Debug.app`
3. Confirm the "Hide TermSurf" menu item shows "Cmd+H" (restored)
4. With `keybind = cmd+h=new_split:left` in the config, press Cmd+H — a new pane
   should open to the left (binding wins over menu)
5. Confirm Cmd+J, Cmd+K, Cmd+L still work
6. Remove the `cmd+h` keybinding from the config, relaunch — Cmd+H should hide
   the window again (default menu behavior restored)
7. Confirm Cmd+C (copy) and Cmd+V (paste) still work

**Result: Pass.**

Removing the menu-first block from `performKeyEquivalent()` restores v1.2.3
behavior. User keybindings now take priority over menu shortcuts. Cmd+H creates
a split when configured, and the "Hide TermSurf" menu item retains its Cmd+H
shortcut for users who don't override it.

## Conclusion

User keybindings now take priority over macOS menu shortcuts. The fix was a
single change to `performKeyEquivalent()` in `SurfaceView_AppKit.swift`: remove
the menu-first block that was added between Ghostty v1.2.3 and `tip`.

The menu-first block tried `NSApp.mainMenu.performKeyEquivalent(with: event)`
before dispatching to `keyDown()`. This let system menu items like "Hide"
(Cmd+H) intercept user-configured keybindings. Removing it restores v1.2.3
behavior — bindings go straight to `keyDown()`, which routes to Zig's
`maybeHandleBinding()`.

All menu shortcuts still work for users who don't override them, because unbound
Cmd+key events fall through `performKeyEquivalent()` to the standard AppKit
responder chain, which delivers them to the menu system as before. The only
behavioral change is that the menu item no longer "flashes" when a user binding
overrides its shortcut — a cosmetic loss.
