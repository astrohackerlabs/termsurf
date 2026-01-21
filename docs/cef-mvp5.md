# CEF MVP5: Copy+Paste

## Goal

Make copy/cut/paste work in a browser with keybindings cmd+c/x/v. Do not break
these keybindings for the terminal, which already work.

## Keybinding Interception Points (Highest to Lowest)

This section documents every place where keyboard events are intercepted in
TermSurf, from the macOS system level down to the terminal/CEF backend.

### Level 1: `performKeyEquivalent:` (NSView)

**File:** `window/src/os/macos/window.rs:2829`

The FIRST interception point. macOS calls this on the first responder before
checking menu key equivalents. If it returns `YES`, the key is consumed and
nothing else sees it.

WezTerm currently handles only 4 specific keys here:

- `Cmd+.` (Command-Period)
- `Ctrl+Esc`
- `Ctrl+Tab`
- `Shift+Tab`

For these, it calls `key_common()` and returns `YES` to prevent further macOS
handling. For all other keys (including Cmd+C/V), it returns `NO`, allowing them
to fall through to Level 2.

### Level 2: macOS Menu Key Equivalents

Only checked if `performKeyEquivalent:` returned `NO`. macOS walks the menu bar
looking for items with matching key equivalents. If found, the menu item's
action is triggered and `keyDown:` is never called.

- Menu items are created in `wezterm-gui/src/commands.rs` via
  `recreate_menubar()`
- Key equivalents are set via `MenuItem::new_with(..., &short_cut)` or
  `set_key_equivalent()`
- The action `weztermPerformKeyAssignment:` triggers
  `WindowEvent::PerformKeyAssignment`

**Currently**: Copy/Paste menu items have Cmd+C/V as key equivalents (defined in
`CommandDef` at lines 642-652 for Copy and lines 665-675 for Paste).

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

**CEF Browser Shortcuts** (lines 468-520, `#[cfg(feature = "cef")]`):

- Only active when `browser_mode == Some(BrowserMode::Browse)`
- Currently handles: `Cmd+[`, `Cmd+]`, `Cmd+R`, `Cmd+Shift+R`
- Does NOT yet handle `Cmd+C`, `Cmd+V`, `Cmd+X` (this is what MVP5 needs to add)
- If handled, calls `key.set_handled()` and returns

**Physical Key Binding Lookup** (lines 522-583):

- Tries physical key code, raw code, then main key
- Calls `process_key()` with `OnlyKeyBindings::Yes`
- If a keybinding matches, marks handled and returns

### Level 6: `key_event_impl()` (Main Key Processing)

**File:** `wezterm-gui/src/termwindow/keyevent.rs:655`

Called via `WindowEvent::KeyEvent` (after IME processing in `key_common`).

**CEF Browser Mode Handling** (lines 661-845, `#[cfg(feature = "cef")]`):

- `BrowserMode::Browse`: Forwards most keys to CEF via `send_key_event()`.
  Ctrl+C switches to Control mode.
- `BrowserMode::Control`: Enter switches to Browse mode, Ctrl+C closes browser.
  Other keys fall through to terminal keybindings.

**InputMap Keybinding Lookup** (lines 873-884):

- Calls `process_key()` with `OnlyKeyBindings::No`
- Checks leader key, active modal, then inputmap
- If no match, key falls through to terminal input

**Terminal Input** (lines 916+):

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
Browse mode. The most likely culprit is **Level 2 (Menu Key Equivalents)**, but
this contradicts the fact that Cmd+C/V work on the terminal.

**Unresolved question:** Why would the menu intercept Cmd+C in Browse mode but
not in terminal mode? The menu state should be identical in both cases.

## Technical Approaches to Implementation

Three approaches have been identified for making Cmd+C/V/X work with CEF while
keeping them in the menu.

### Approach 1: Browser-Aware Menu Action

**Concept**: The menu intercepts Cmd+C/V/X as it does now. But when the
`PerformKeyAssignment` action is executed, the handler checks if we're in Browse
mode and forwards the key to CEF instead of copying the terminal selection.

**Flow**:

1. User presses Cmd+C
2. Menu intercepts at Level 2
3. Menu triggers `WindowEvent::PerformKeyAssignment` with `CopyTo` action
4. Handler checks: is the active pane in Browse mode?
5. If Browse mode → Synthesize Cmd+C key event, send to CEF
6. If terminal mode → Copy terminal selection (current behavior)

**Pros**:

- Menu stays exactly as-is (Cmd+C/V visible, Cmd+X can be added)
- All browser-awareness logic lives in the WezTerm application layer
- No architectural changes needed to plumb state to window level
- Single point of change

**Cons**:

- Need to find where `PerformKeyAssignment` is handled
- Need to synthesize a key event to send to CEF
- Conceptually odd: menu "Copy" action sends a key event rather than copying

### Approach 2: Intercept in `performKeyEquivalent:` Before the Menu

**Concept**: In `performKeyEquivalent:` (Level 1), check if we're in Browse
mode. If yes, handle Cmd+C/V/X there (call `key_common()`, return YES). If no,
return NO and let the menu handle them.

**Flow**:

1. User presses Cmd+C
2. `performKeyEquivalent:` is called
3. Check: is the active pane in Browse mode?
4. If Browse mode → Call `key_common()`, return YES (reaches CEF via normal key
   path)
5. If terminal mode → Return NO, menu intercepts, normal copy behavior

**Pros**:

- Key events flow through the normal path to CEF
- Clean separation: menu handles terminal, key events handle browser
- No event synthesis needed

**Cons**:

- `performKeyEquivalent:` is at the Cocoa/window layer, doesn't have access to
  browser mode
- Need to plumb browser mode state down to the window level (architectural
  change)
- State synchronization concerns (what if mode changes mid-event?)

### Approach 3: Dynamic Menu Key Equivalents

**Concept**: When entering Browse mode, remove the Cmd+C/V/X key equivalents
from the menu items. When leaving Browse mode, restore them.

**Flow**:

1. User opens browser, enters Browse mode
2. System removes Cmd+C/V key equivalents from Edit menu items
3. User presses Cmd+C
4. Menu has no matching key equivalent, so `keyDown:` is called
5. Key flows through `key_common` → `raw_key_event_impl` → `key_event_impl` →
   CEF
6. User exits Browse mode
7. System restores Cmd+C/V key equivalents

**Pros**:

- Keys flow naturally to CEF without synthesis
- No architectural changes to window layer
- Menu items remain visible (just without shortcuts temporarily)

**Cons**:

- UI shows menu items without keyboard shortcuts in Browse mode (potentially
  confusing)
- Need to track mode changes and update menus in sync
- Menu management complexity
- Race conditions if mode changes rapidly

### Approach Comparison

| Aspect               | Approach 1  | Approach 2             | Approach 3                    |
| -------------------- | ----------- | ---------------------- | ----------------------------- |
| Menu appearance      | Unchanged   | Unchanged              | Shortcuts disappear in Browse |
| Architectural change | Minimal     | Significant (plumbing) | Moderate (menu sync)          |
| Key event path       | Synthesized | Natural                | Natural                       |
| Complexity           | Low-Medium  | Medium-High            | Medium                        |
| Risk of bugs         | Low         | Medium (state sync)    | Medium (menu sync)            |

**Recommended**: Approach 1 appears most pragmatic due to minimal architectural
disruption and consistent menu appearance.

## Experiment Log

_This section tracks implementation attempts and their outcomes._

### Experiment 1: Approach 1 (Browser-Aware Menu Action)

**Status:** In progress

**Rationale:** Minimal architectural disruption, consistent menu UX, single
point of change. The existing `send_key_event()` infrastructure can be reused.

#### Research: Tracing the PerformKeyAssignment Flow

**Event flow:**

```
Menu Cmd+C pressed
    ↓
WindowEvent::PerformKeyAssignment(CopyTo(Clipboard)) dispatched
    (window/src/os/macos/window.rs:2294)
    ↓
Event received in termwindow/mod.rs:965-970
    ↓
self.perform_key_assignment(&pane, &action) called
    (termwindow/mod.rs:967)
    ↓
Match on CopyTo at termwindow/mod.rs:2756-2759:
    CopyTo(dest) => {
        let text = self.selection_text(pane);
        self.copy_to_clipboard(*dest, text);
    }
```

**State access confirmed** at the `CopyTo`/`PasteFrom` match arms:

| Need           | Available? | How                                      |
| -------------- | ---------- | ---------------------------------------- |
| Active pane ID | ✅ Yes     | `pane.pane_id()` (pane is a parameter)   |
| browser_states | ✅ Yes     | `self.browser_states` (TermWindow field) |
| Browser mode   | ✅ Yes     | `browser.get_mode()`                     |
| send_key_event | ✅ Yes     | `browser.send_key_event(&event)`         |

**Execution model:** Synchronous — the match arm executes directly.

**Key codes for CEF:**

| Key | macOS native | Windows VK |
| --- | ------------ | ---------- |
| C   | 0x08         | 0x43       |
| V   | 0x09         | 0x56       |
| X   | 0x07         | 0x58       |

**Modifier flag:** `EVENTFLAG_COMMAND_DOWN` (0x80) defined in
`cef_browser/mod.rs:393`

#### Implementation Plan

Modify `termwindow/mod.rs` at the `CopyTo` and `PasteFrom` match arms (~lines
2756-2765):

1. Check if pane has a browser in Browse mode
2. If yes: synthesize Cmd+C/V key event, send to CEF, return early
3. If no: execute existing terminal copy/paste logic

All infrastructure exists. This is a surgical change to ~20 lines in one
location.
