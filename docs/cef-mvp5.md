# CEF MVP5: Copy+Paste

## Goal

Make copy/cut/paste work in a browser with keybindings cmd+c/x/v. Do not break
these keybindings for the terminal, which already work.

## Keybinding Interception Points (Highest to Lowest)

This section documents every place where keyboard events are intercepted in
TermSurf, from the macOS system level down to the terminal/CEF backend.

### Level 1: macOS Menu Key Equivalents

**Before any WezTerm code runs**, macOS checks if a pressed key combination
matches a menu item's key equivalent. If a match is found, the menu item's
action is triggered and `keyDown:` is never called.

- Menu items are created in `wezterm-gui/src/commands.rs` via
  `recreate_menubar()`
- Key equivalents are set via `MenuItem::new_with(..., &short_cut)` or
  `set_key_equivalent()`
- The action `weztermPerformKeyAssignment:` triggers
  `WindowEvent::PerformKeyAssignment`

**Currently**: Copy/Paste menu items have Cmd+C/V as key equivalents (defined in
`CommandDef` at lines 660 and 683). There is `#[cfg(feature = "cef")]` code at
lines 520-532 that attempts to clear these, but its effectiveness is unverified.

### Level 2: `performKeyEquivalent:` (NSView)

**File:** `window/src/os/macos/window.rs:2829`

Called by macOS before `keyDown:`. WezTerm handles only 4 specific keys here:

- `Cmd+.` (Command-Period)
- `Ctrl+Esc`
- `Ctrl+Tab`
- `Shift+Tab`

For these, it calls `key_common()` and returns `YES` to prevent further macOS
handling. For all other keys (including Cmd+C/V), it returns `NO`.

### Level 3: `keyDown:` / `keyUp:` (NSView)

**File:** `window/src/os/macos/window.rs:2876-2881`

Standard macOS key event entry point. Only called if:

1. `performKeyEquivalent:` returned `NO`
2. No menu key equivalent matched

Both call `key_common(this, nsevent, key_is_down)`.

### Level 4: `key_common()` (Key Preprocessing)

**File:** `window/src/os/macos/window.rs:2472`

This function does extensive preprocessing:

1. **Extract raw key data** from NSEvent (chars, modifiers, keyCode)
2. **Dispatch `RawKeyEvent`** (line 2546) → triggers `raw_key_event_impl`
3. **Check if handled** (line 2549) - if marked handled, returns early
4. **Dead key detection** (line 2557) - can return early
5. **IME handling** (line 2643) - if `use_ime && forward_to_ime`, calls
   `interpretKeyEvents:` which can consume the key
6. **Key encoding and normalization** (lines 2747-2807)
7. **Dispatch `KeyEvent`** (line ~2800) → triggers `key_event_impl`

### Level 5: `raw_key_event_impl()` (Early Interception)

**File:** `wezterm-gui/src/termwindow/keyevent.rs:430`

Called via `WindowEvent::RawKeyEvent`. This is the FIRST place WezTerm
application code can intercept keys.

**CEF Browser Shortcuts** (lines 468-544, `#[cfg(feature = "cef")]`):

- Only active when `browser_mode == Some(BrowserMode::Browse)`
- Currently handles: `Cmd+[`, `Cmd+]`, `Cmd+R`, `Cmd+Shift+R`, `Cmd+C`, `Cmd+V`,
  `Cmd+X`
- If handled, calls `key.set_handled()` and returns

**Physical Key Binding Lookup** (lines 546-607):

- Tries physical key code, raw code, then main key
- Calls `process_key()` with `OnlyKeyBindings::Yes`
- If a keybinding matches, marks handled and returns

### Level 6: `key_event_impl()` (Main Key Processing)

**File:** `wezterm-gui/src/termwindow/keyevent.rs:679`

Called via `WindowEvent::KeyEvent` (after IME processing in `key_common`).

**CEF Browser Mode Handling** (lines 685-869, `#[cfg(feature = "cef")]`):

- `BrowserMode::Browse`: Forwards most keys to CEF via `send_key_event()`.
  Ctrl+C switches to Control mode.
- `BrowserMode::Control`: Enter switches to Browse mode, Ctrl+C closes browser.
  Other keys fall through to terminal keybindings.

**InputMap Keybinding Lookup** (lines 897-909):

- Calls `process_key()` with `OnlyKeyBindings::No`
- Checks leader key, active modal, then inputmap
- If no match, key falls through to terminal input

**Terminal Input** (lines 918+):

- Encodes key and sends to pane via `pane.key_down()` or `pane.key_up()`

### Level 7: CEF Key Forwarding

**File:** `wezterm-gui/src/cef_browser/mod.rs:249`

`send_key_event()` forwards key events to CEF's browser host. CEF then processes
them as browser keyboard input.

## Current Problem

Cmd+C/V work on the terminal (no browser) but do NOT work in Browse mode.
Debugging shows that Cmd+C/V key events do not appear in the `[CEF RAW]` debug
log when in Browse mode, even though Ctrl+C does appear.

This suggests Cmd+C/V are being intercepted BEFORE `raw_key_event_impl` when in
Browse mode. The most likely culprit is **Level 1 (Menu Key Equivalents)**, but
this contradicts the fact that Cmd+C/V work on the terminal.

**Unresolved question:** Why would the menu intercept Cmd+C in Browse mode but
not in terminal mode? The menu state should be identical in both cases.
