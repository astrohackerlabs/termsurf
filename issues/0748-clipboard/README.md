+++
status = "closed"
opened = "2026-03-14"
closed = "2026-03-14"
+++

# Issue 748: Browser clipboard (copy/cut/paste)

## Goal

Copy, cut, and paste work inside browser overlays in Wezboard. Cmd+C copies
selected text to the system clipboard. Cmd+V pastes from the system clipboard
into the browser. Cmd+X cuts. This should work on any web page — text fields,
content-editable regions, and selecting text on a read-only page.

## Background

### The Chromium side is already solved

Issue 609 (Ghostboard) solved clipboard for Chromium. The key learning:
**Chromium's renderer does not re-interpret raw keyboard events.** On macOS,
Chromium's input path works like this:

1. NSEvent → `interpretKeyEvents:` → Cocoa calls `doCommandBySelector:`
2. The selector is converted to an editing command string (`"copy"`, `"paste"`)
3. `ForwardKeyboardEventWithCommands` sends the key event AND the editing
   commands to the renderer
4. The renderer applies the commands directly — it never looks at the raw key
   event

Issue 609 Experiment 4 fixed this by using `ForwardKeyboardEventWithCommands`
with explicit editing commands for Cmd+key combinations. This code was carried
forward into `libtermsurf_chromium` — the current
`TsBrowserMainParts::ForwardKeyEvent` in `ts_browser_main_parts.cc` already uses
`ForwardKeyboardEventWithCommands` with the correct command mapping:

- Cmd+A → `"selectAll"`
- Cmd+C → `"copy"`
- Cmd+V → `"paste"`
- Cmd+X → `"cut"`
- Cmd+Z → `"undo"`

The Chromium side needs no changes.

### Prior art: CEF (Issues 206, 318)

The ts2/ts3 CEF architecture used direct `frame.copy()` / `frame.paste()` /
`frame.cut()` FFI calls. Key learnings:

- **macOS clipboard asymmetry:** Background processes can WRITE to the clipboard
  but cannot READ it. Paste required proxying through the GUI.
- **Key event synthesis is unsafe:** Simulating key events through message loops
  caused re-entrancy panics. Direct API calls are safer.
- **Menu shortcuts intercept before key events:** `performKeyEquivalent` on
  macOS consumes Cmd+key before `keyDown` fires.

### The real problem: macOS menu system eats Cmd+C/V/X before Rust code sees them

The macOS event flow for Cmd+C:

1. macOS calls `performKeyEquivalent` on the view
2. Wezboard returns `Bool::NO` — "I didn't handle this"
3. macOS checks the menu bar, finds **Edit → Copy to clipboard** (Cmd+C)
4. macOS invokes `wezboardPerformKeyAssignment:` → `CopyTo(Clipboard)`
5. Terminal copies its selection to the system clipboard
6. **`keyDown` is never called** → `key_common` never runs → `try_forward_key`
   never runs → the browser never sees the event

The Rust key processing pipeline (`raw_key_event_impl` → `process_key` →
`try_forward_key`) never executes for Cmd+C/V because the macOS menu system
consumes the event at step 3. This is the same problem Ghostboard had (Issue 609
Experiment 2) — just manifesting through the menu bar instead of
`performKeyEquivalent` bindings.

### Two-part fix

The fix requires changes in two layers:

1. **`perform_key_equivalent`** (window crate, `window.rs`): When the Cmd
   modifier is held and the character is one of `c`, `v`, `x`, `a`, `z`, route
   the event through `key_common` and return `Bool::YES`. This prevents the
   macOS menu system from intercepting the event. We only intercept these
   specific keys — other Cmd+key combos (Cmd+T, Cmd+W, Cmd+N) must still reach
   the menu system.

2. **`try_forward_key`** (wezboard-gui, `input.rs`): Check browse mode before
   the `only_key_bindings` guard. When browsing, forward to the browser. When
   not browsing, return `None` so the normal keybinding lookup handles Cmd+C →
   `CopyTo` for terminal copy.

Together: `perform_key_equivalent` ensures the event reaches Rust code, and
`try_forward_key` routes it to either the browser (browse mode) or the terminal
keybinding system (control mode).

## Experiments

### Experiment 1: Bypass menu system for clipboard keys in browse mode

#### Description

Two changes that work together to route Cmd+C/V/X/A/Z to the browser when in
browse mode, while preserving terminal clipboard behavior in control mode.

#### Changes

**`wezboard/window/src/os/macos/window.rs`** — In `perform_key_equivalent`,
before the `Bool::NO` fallthrough, add a check for clipboard-related Cmd+key
events. Route them through `key_common` to bypass the menu system:

```rust
if modifiers == Modifiers::SUPER
    && matches!(chars, "a" | "c" | "v" | "x" | "z")
{
    Self::key_common(this, nsevent, true);
    return Bool::YES;
}
```

This goes after the existing Cmd+period / Ctrl+Esc / Ctrl+Tab / Shift+Tab block
and before the `Bool::NO` return. It uses the same `key_common` + `Bool::YES`
pattern that already exists for Ctrl+Esc.

This unconditionally intercepts these five keys — it doesn't check browse mode
because the `window` crate has no access to TermSurf state. The browse mode
routing happens in the next layer.

**`wezboard/wezboard-gui/src/termsurf/input.rs`** — In `try_forward_key()`,
check browse mode before the `only_key_bindings` guard:

```rust
pub fn try_forward_key(
    pane_id: usize,
    keycode: &KeyCode,
    modifiers: Modifiers,
    is_down: bool,
    key_event: Option<&::window::KeyEvent>,
    only_key_bindings: bool,
) -> Option<bool> {
    let pane_id_str = pane_id.to_string();
    let state = super::shared_state()?;
    let browsing = {
        let st = state.lock().unwrap();
        let pane = st.panes.get(&pane_id_str)?;
        pane.browsing
    };

    if !browsing {
        return None;
    }

    // Pane is browsing — forward to browser regardless of only_key_bindings.

    // Esc key press (no Ctrl) exits browse mode
    // ... rest unchanged
```

When browsing, the browser gets the key. When not browsing, `try_forward_key`
returns `None`, and `process_key` falls through to the keybinding lookup which
finds Cmd+C → `CopyTo(Clipboard)` — terminal copy still works.

#### Verification

```bash
scripts/build.sh wezboard
scripts/install.sh wezboard
```

Launch Wezboard, open a web page with `web`, click a text field to enter browse
mode.

| #  | Test                      | Steps                                             | Expected                      |
| -- | ------------------------- | ------------------------------------------------- | ----------------------------- |
| 1  | Cmd+C copies browser text | Select text on page, Cmd+C, paste in terminal     | Browser selection pasted      |
| 2  | Cmd+V pastes into browser | Copy text in terminal, click browser field, Cmd+V | Text appears in browser field |
| 3  | Cmd+X cuts browser text   | Select text in browser field, Cmd+X               | Text removed, on clipboard    |
| 4  | Cmd+A selects all         | Click browser field with text, Cmd+A, type "X"    | All text replaced with "X"    |
| 5  | Cmd+Z undoes              | Type "hello", Cmd+A, type "X", Cmd+Z              | "hello" restored              |
| 6  | Regular typing works      | Type "hello" in browser field                     | "hello" appears               |
| 7  | Esc exits browse mode     | Press Esc                                         | Returns to control mode       |
| 8  | Cmd+C in control mode     | Exit browse mode, Cmd+C                           | Copies terminal selection     |
| 9  | Cmd+V in control mode     | Exit browse mode, Cmd+V                           | Pastes into terminal          |
| 10 | Cmd+T still works         | Press Cmd+T                                       | Opens new tab                 |
| 11 | Cmd+W still works         | Press Cmd+W                                       | Closes current tab            |

Tests 8-9 verify terminal clipboard still works in control mode. Tests 10-11
verify that non-clipboard Cmd+key shortcuts still route through the menu system.

**Result:** Fail

No keybindings reached the browser at all — typing, Esc, everything was broken.

#### Root cause

The `input.rs` change removed the `only_key_bindings` early return, which caused
`try_forward_key` to consume keys during the `OnlyKeyBindings::Yes` passes. The
WezTerm key pipeline calls `process_key` **multiple times** per keystroke:

1. First with `OnlyKeyBindings::Yes` for `KeyCode::Physical(...)` — match
   physical key against keybinding table only
2. Then with `OnlyKeyBindings::Yes` for `KeyCode::RawCode(...)` — match raw
   scancode against keybinding table only
3. Then with `OnlyKeyBindings::Yes` for the resolved `key.key` — match resolved
   key against keybinding table only
4. Finally (in a separate code path, `key_event`) with `OnlyKeyBindings::No` —
   this is the only pass that should forward to the browser

The `only_key_bindings` guard exists to make `try_forward_key` invisible during
passes 1-3. By removing it, the function consumed the event on pass 1 (the
physical key pass). But physical key codes like `KeyCode::Physical(KeyC)` don't
carry UTF-8 text — the `key_event` parameter is `None` in these passes. So the
browser received key events with no text content, and the real key event
(pass 4) never ran because the event was already marked handled.

The `perform_key_equivalent` change was correct in isolation — it successfully
routed Cmd+C/V/X/A/Z through `key_common`. But the `input.rs` change broke the
multi-pass pipeline, preventing ALL keys from working properly.

#### Conclusion

The `perform_key_equivalent` interception is the right approach for getting
Cmd+keys past the macOS menu system. The `input.rs` change was wrong — removing
the `only_key_bindings` guard breaks WezTerm's multi-pass key resolution.

The real problem: when `perform_key_equivalent` routes Cmd+C through
`key_common`, the event goes through `raw_key_event_impl` which calls
`process_key` with `OnlyKeyBindings::Yes` three times. `try_forward_key` returns
`None` on all three (because `only_key_bindings` is true). Then
`raw_key_event_impl` returns — it never reaches `key_event` (the
`OnlyKeyBindings::No` pass) because `raw_key_event_impl` handles the event
fully.

The fix for Experiment 2 needs to either:

1. Make `try_forward_key` handle `only_key_bindings == true` for specific
   Cmd+keys when browsing (check for Cmd+{a,c,v,x,z} specifically during the
   keybinding-only passes), or
2. Skip the multi-pass `raw_key_event_impl` entirely for browse mode keys and go
   straight to forwarding, or
3. Make `perform_key_equivalent` conditionally intercept based on browse mode
   state (requires exposing TermSurf state to the window crate).

### Experiment 2: Intercept clipboard Cmd+keys during keybinding-only passes

#### Description

Two changes that work together. The `perform_key_equivalent` change from
Experiment 1 is still needed — without it, macOS menus eat Cmd+C/V/X before Rust
code runs. The `input.rs` change is different: instead of removing the
`only_key_bindings` guard entirely (which broke the multi-pass pipeline), we add
a targeted carve-out for clipboard keys when browsing.

The key insight from the Experiment 1 failure: `key_common` dispatches both
`WindowEvent::RawKeyEvent` and `WindowEvent::KeyEvent`, but if
`raw_key_event_impl` handles the event (by matching a keybinding), it calls
`set_handled()` and `key_common` returns early — `WindowEvent::KeyEvent` is
never dispatched. Cmd+C matches `CopyTo(Clipboard)` in the keybinding table
during the `OnlyKeyBindings::Yes` passes, so `try_forward_key` with
`OnlyKeyBindings::No` never runs.

The fix: when `only_key_bindings` is true AND the pane is browsing AND the key
is Cmd+{a,c,v,x,z}, forward to the browser immediately instead of returning
`None`. This intercepts the event before keybinding lookup can consume it.

One additional subtlety: during `OnlyKeyBindings::Yes` passes, the `keycode`
parameter is `KeyCode::Physical(...)`, not `KeyCode::Char(...)`. The existing
`keycode_to_windows_vk` function doesn't handle physical keycodes (falls through
to `_ => 0`). We need to map `PhysKeyCode` to the correct Windows virtual key
code for the five clipboard keys.

#### Changes

**`wezboard/window/src/os/macos/window.rs`** — Same as Experiment 1. In
`perform_key_equivalent`, after the existing Cmd+period / Ctrl+Esc / Ctrl+Tab /
Shift+Tab block, add:

```rust
} else if modifiers == Modifiers::SUPER
    && matches!(chars, "a" | "c" | "v" | "x" | "z")
{
    Self::key_common(this, nsevent, true);
    Bool::YES
}
```

**`wezboard/wezboard-gui/src/termsurf/input.rs`** — Two changes:

1. At the top of `try_forward_key`, replace the blanket `only_key_bindings`
   early return with a targeted check. When `only_key_bindings` is true, still
   return `None` for most keys, but allow Cmd+{a,c,v,x,z} through when browsing:

```rust
pub fn try_forward_key(
    pane_id: usize,
    keycode: &KeyCode,
    modifiers: Modifiers,
    is_down: bool,
    key_event: Option<&::window::KeyEvent>,
    only_key_bindings: bool,
) -> Option<bool> {
    // Check browse mode first — needed to decide clipboard key routing.
    let pane_id_str = pane_id.to_string();
    let state = super::shared_state()?;
    let browsing = {
        let st = state.lock().unwrap();
        let pane = st.panes.get(&pane_id_str)?;
        pane.browsing
    };

    if !browsing {
        return None;
    }

    if only_key_bindings {
        // During OnlyKeyBindings::Yes passes, only intercept clipboard
        // Cmd+keys. Everything else returns None so the normal multi-pass
        // pipeline continues.
        if !(modifiers == Modifiers::SUPER && is_clipboard_key(keycode)) {
            return None;
        }
    }

    // ... rest unchanged (Esc handling, build KeyEvent proto, send)
```

2. Add a helper function `is_clipboard_key` that matches both `Char` and
   `Physical` keycode variants:

```rust
/// Returns true for the five clipboard-related keys (a, c, v, x, z).
/// Handles both Char and Physical keycode representations, since
/// WezTerm's multi-pass pipeline uses Physical keycodes in the
/// OnlyKeyBindings::Yes passes and Char keycodes in the No pass.
fn is_clipboard_key(keycode: &KeyCode) -> bool {
    match keycode {
        KeyCode::Char(c) => matches!(c, 'a' | 'c' | 'v' | 'x' | 'z'),
        KeyCode::Physical(phys) => {
            use ::window::PhysKeyCode;
            matches!(
                phys,
                PhysKeyCode::A
                    | PhysKeyCode::C
                    | PhysKeyCode::V
                    | PhysKeyCode::X
                    | PhysKeyCode::Z
            )
        }
        _ => false,
    }
}
```

3. Add a physical keycode branch to `keycode_to_windows_vk` for the five
   clipboard keys, so the correct VK code is sent to Chromium during the
   `OnlyKeyBindings::Yes` pass:

```rust
fn keycode_to_windows_vk(key: &KeyCode) -> i64 {
    match key {
        KeyCode::Char(c) => match c {
            // ... existing matches ...
        },
        KeyCode::Physical(phys) => {
            use ::window::PhysKeyCode;
            match phys {
                PhysKeyCode::A => 0x41,
                PhysKeyCode::C => 0x43,
                PhysKeyCode::V => 0x56,
                PhysKeyCode::X => 0x58,
                PhysKeyCode::Z => 0x5A,
                _ => 0,
            }
        }
        // ... existing matches ...
    }
}
```

#### Verification

```bash
scripts/build.sh wezboard
```

Same 11-test table as Experiment 1. The critical difference: tests 6 (regular
typing) and 7 (Esc exits browse mode) must still work — these broke in
Experiment 1 because the `only_key_bindings` guard was removed entirely. Now
only the five clipboard keys bypass it.

**Result:** Pass

All 11 tests passed. Cmd+C/V/X/A/Z work in browse mode, regular typing and Esc
still work, terminal clipboard works in control mode, and Cmd+T/W still route
through the menu system.

#### Conclusion

The targeted carve-out approach works. The key lessons:

1. `perform_key_equivalent` must intercept Cmd+{a,c,v,x,z} to prevent the macOS
   menu system from consuming them — this was correct in Experiment 1.
2. The `only_key_bindings` guard cannot be removed entirely — it protects the
   multi-pass key pipeline. Instead, a targeted exception for clipboard keys
   when browsing lets them through during `OnlyKeyBindings::Yes` passes while
   leaving all other keys on the normal path.
3. Physical keycodes need explicit VK mapping since `keycode_to_windows_vk` only
   handled `Char` variants, and the `OnlyKeyBindings::Yes` passes use
   `KeyCode::Physical`.

## Conclusion

Copy, cut, paste, select all, and undo (Cmd+C/X/V/A/Z) now work in browser
overlays. The fix has two parts: `perform_key_equivalent` intercepts these five
Cmd+keys before the macOS menu system can consume them, and `try_forward_key`
routes them to the browser during browse mode's keybinding-only passes.

The Chromium side needed no changes — `ForwardKeyboardEventWithCommands` from
Issue 609 already handles editing commands correctly.

Key architectural finding: WezTerm's key pipeline calls `process_key` multiple
times per keystroke with `OnlyKeyBindings::Yes` before the final
`OnlyKeyBindings::No` pass. Any interception of keys that have default
keybindings (like Cmd+C → `CopyTo`) must happen during the `Yes` passes, or the
keybinding consumes the event first and the `No` pass never runs. The
`perform_key_equivalent` interception alone is insufficient — it gets the event
into the Rust pipeline, but the keybinding lookup still wins. The targeted
carve-out in `try_forward_key` is what actually routes the event to the browser.

Note: these five keys now bypass the macOS menu system unconditionally (both
browse and control mode). In control mode they still reach the correct
keybinding via `raw_key_event_impl` instead of the menu path. If the menu flash
animation or menu item validation is ever needed, browse mode state would need
to be exposed to the window crate.
