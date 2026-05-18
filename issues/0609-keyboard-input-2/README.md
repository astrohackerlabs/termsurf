+++
status = "closed"
opened = "2026-02-21"
closed = "2026-02-21"
+++

# Issue 609: Keyboard Input (continued)

## Goal

Complete keyboard input on Chromium overlays. Backspace deletes, Tab moves
between form fields, Enter submits, arrow keys navigate within text, Cmd+A
selects all, Cmd+C copies, Cmd+V pastes. All keys that a user expects to work in
a browser text field work.

## Background

Issue 607 built the keyboard forwarding pipeline end to end:

- **Ghost (Zig):** `keyToWindowsVK` maps Ghostty's key enum to Windows VK codes.
  `sendKeyEvent` constructs XPC messages. The `keyCallback` forwarding block
  routes keys to Chromium when in browse mode.
- **Chromium (C++):** `HandleKeyEvent` receives the XPC message, constructs
  `NativeWebKeyboardEvent` (`kRawKeyDown` + `kChar` for characters, `kKeyUp` for
  release), and calls `ForwardKeyboardEvent`.
- **Ctrl+Esc:** Always exits browse mode, regardless of browser state.

Issue 607 Experiment 2 proved character typing works — letters appear in text
fields. But testing was blocked by a navigation freeze (pressing Enter or
clicking Search froze the overlay). Issue 608 fixed that: `PrimaryPageChanged`
now recreates the capturer when the `RenderWidgetHost` changes.

With the navigation freeze resolved, we can now test the full keyboard feature
set. The pipeline exists but has only been validated for character input. The
following remain untested:

| Key          | Expected behavior                       |
| ------------ | --------------------------------------- |
| Enter        | Submit form (was blocked by 608 freeze) |
| Backspace    | Delete character before cursor          |
| Tab          | Move to next focusable element          |
| Arrow keys   | Move cursor within text field           |
| Home / End   | Move to start / end of line             |
| Cmd+A        | Select all text                         |
| Cmd+C        | Copy selected text to clipboard         |
| Cmd+V        | Paste from clipboard                    |
| Cmd+X        | Cut selected text                       |
| Cmd+Z        | Undo                                    |
| Shift+arrows | Extend text selection                   |

### Potential issues

The current `HandleKeyEvent` constructs `NativeWebKeyboardEvent` with only
`windows_key_code` and `text` fields set. Other fields that Chromium may need:

- **`native_key_code`** — macOS keycode. Not currently set. Chromium may use
  this for some key handling paths.
- **`dom_code`** — USB HID usage code. Not currently set. Some Chromium features
  (e.g., keyboard shortcuts) may check this.
- **`dom_key`** — DOM key enum. Not currently set.
- **`is_system_key`** — Whether this is a system key event (Alt+key on Windows,
  Cmd+key on macOS). Not set. Chromium may need this for Cmd+C/V/A to trigger
  clipboard operations.

If basic keys work but Cmd shortcuts don't, these missing fields are the likely
cause.

### Clipboard

Cmd+C and Cmd+V require clipboard access. Two possibilities:

1. **Chromium handles it internally.** If `ForwardKeyboardEvent` with Cmd+C
   triggers Chromium's built-in copy command, the text is copied to the system
   clipboard (which Ghost can read). This is the ideal case — no extra work.
2. **Chromium doesn't handle it.** If Chromium's headless/content_shell mode
   doesn't wire up clipboard shortcuts, we may need to invoke clipboard commands
   explicitly via the `WebContents` editing API (`Copy()`, `Paste()`, etc.).

### Key files

- `ghost/src/apprt/xpc.zig` — `keyToWindowsVK`, `sendKeyEvent`
- `ghost/src/Surface.zig` — Key forwarding block in `keyCallback`
- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`
  — `HandleKeyEvent`, XPC dispatch
- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.h`
  — `HandleKeyEvent` declaration

### Chromium branch

Create `146.0.7650.0-issue-609` from `146.0.7650.0-issue-608`. The 609 branch
builds on 608's capturer re-attach fix and 607's keyboard forwarding code.

## Experiment 1: Test matrix

### Goal

Determine which keys work and which don't, now that the navigation freeze is
resolved. No code changes — just test and record.

### Design

No code changes. The keyboard pipeline from Issue 607 is already in place. Issue
608 fixed the navigation freeze that blocked testing. This experiment
systematically tests every key behavior from the issue goal.

### Verification

```bash
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://lite.duckduckgo.com
```

Click the search box to enter browse mode and focus the text field. Run through
each test and record pass/fail:

| #  | Test                       | Steps                                              | Expected                             | Result | Description                                                         |
| -- | -------------------------- | -------------------------------------------------- | ------------------------------------ | ------ | ------------------------------------------------------------------- |
| 1  | Character typing           | Type "hello"                                       | "hello" appears in text field        | y      |                                                                     |
| 2  | Enter submits              | Type "test", press Enter                           | Search results page loads            | y      |                                                                     |
| 3  | Backspace deletes          | Type "helloo", press Backspace                     | Last "o" deleted, "hello" remains    | y      |                                                                     |
| 4  | Tab moves focus            | Press Tab from search box                          | Focus moves to next element          | n      |                                                                     |
| 5  | Arrow left/right           | Type "hello", press Left 3x, type "X"              | "heXllo" — cursor moved, then insert | y      |                                                                     |
| 6  | Arrow up/down              | In a multi-line textarea, press Up/Down            | Cursor moves between lines           | y      |                                                                     |
| 7  | Home / End                 | Type "hello", press Home, type "X"                 | "Xhello" — cursor at start           | y      |                                                                     |
| 8  | Shift+arrow selects        | Type "hello", Shift+Left 3x, type "X"              | "heX" — selection replaced           | y      |                                                                     |
| 9  | Cmd+A selects all          | Type "hello", Cmd+A, type "X"                      | "X" — all text replaced              | n      | Cmd+A selects the terminal contents instead of the webpage contents |
| 10 | Cmd+C / Cmd+V              | Type "hello", Cmd+A, Cmd+C, click new field, Cmd+V | "hello" pasted into new field        | n      |                                                                     |
| 11 | Cmd+X cuts                 | Type "hello", Cmd+A, Cmd+X                         | Text field empty, clipboard has text | n      |                                                                     |
| 12 | Cmd+Z undoes               | Type "hello", Cmd+A, type "X", Cmd+Z               | "hello" restored                     | n      |                                                                     |
| 13 | Ctrl+Esc exits browse mode | Press Ctrl+Esc                                     | Exits browse mode (regression check) | y      |                                                                     |

For tests 6 and 10, if lite.duckduckgo.com doesn't have a suitable multi-line
textarea or second field, use a different site (e.g., a form test page or
Wikipedia's search).

Record each result as Pass, Fail, or N/A (if the site doesn't support that
test). For any Fail, note the observed behavior.

**Result:** Partial

8 of 13 tests pass. 5 fail. The failures fall into two groups:

**Group 1: Tab (test 4).** Tab doesn't move focus between form fields. The VK
code for Tab (0x09) is mapped and sent, but it may not be reaching Chromium, or
Chromium may need additional event fields (e.g., `dom_code`) to process Tab as a
focus-move event. Alternatively, Ghost or macOS may be intercepting Tab before
it reaches `keyCallback` (Ghostty uses Tab for terminal focus cycling in some
configurations).

**Group 2: All Cmd shortcuts (tests 9-12).** Cmd+A selects terminal contents
instead of webpage contents. Cmd+C, Cmd+V, Cmd+X, and Cmd+Z all fail similarly.
These events never reach the Zig `keyCallback` forwarding block because macOS
intercepts them at the AppKit level before `keyDown` is called.

The Cmd+key interception path is:

1. macOS calls `performKeyEquivalent` on the view for any Cmd+key press.
2. `performKeyEquivalent` checks if the key is a Ghostty binding (e.g., Cmd+A →
   `select_all`).
3. If it is a binding, it calls `keyDown` which routes to Ghostty's binding
   system — never reaching our forwarding block.
4. If it's not a binding, the macOS menu system checks `MainMenu.xib` for
   matching menu items (e.g., Cmd+A → `selectAll:` IBAction on the responder
   chain).

Either way, Cmd+key events are consumed before our Zig code sees them. The
forwarding block in `keyCallback` only runs for keys that survive both the
`performKeyEquivalent` check and the AppKit responder chain.

#### Conclusion

The basic keyboard pipeline works well — characters, Enter, Backspace, arrows,
Home/End, and Shift+arrow selection all function correctly. The two remaining
problems are:

1. **Cmd shortcuts are intercepted by macOS/Ghostty before reaching Zig.** This
   requires changes in the Swift layer (`performKeyEquivalent` or the responder
   chain) to detect browse mode and forward Cmd+key events to `keyCallback`
   instead of handling them as Ghostty bindings or menu actions.

2. **Tab doesn't work.** This may be a simpler issue — possibly a missing VK
   code mapping, a Ghostty binding consuming Tab, or Chromium needing additional
   fields on the keyboard event. Investigate separately from the Cmd shortcut
   problem.

### Experiment 2: Forward Cmd+key in browse mode

#### Goal

Cmd+A, Cmd+C, Cmd+V, Cmd+X, and Cmd+Z work in Chromium overlays when in browse
mode.

#### Description

Cmd+key events never reach the Zig `keyCallback` because macOS intercepts them
at the AppKit level. The interception path:

1. macOS calls `performKeyEquivalent` on the view for any Cmd+key press.
2. `performKeyEquivalent` checks if the key is a Ghostty binding (e.g., Cmd+A →
   `select_all`). If yes, it calls `keyDown` which routes to Ghostty's binding
   system.
3. If not a binding, the macOS menu system checks `MainMenu.xib` for matching
   menu items (e.g., Cmd+A → `selectAll:` IBAction on the responder chain).

Either way, the event is consumed before our Zig forwarding block sees it.

The fix: add a browse mode check at the top of `performKeyEquivalent`. When the
surface is in browse mode, route the event directly to `keyDown` — bypassing
both the Ghostty binding check and the menu system. `keyDown` calls
`ghostty_surface_key`, which calls `keyCallback`, where our forwarding block
sends the event to Chromium via XPC.

This requires exposing `isOverlayForwarding` to Swift via a new C API function.

#### Chromium branch

No Chromium changes. Continue on `146.0.7650.0-issue-608` (read-only — no new
commits to this branch). The VK codes and meta modifier bit are already sent
correctly by the existing `HandleKeyEvent`.

#### Changes

**`ghost/src/apprt/embedded.zig`** — Add a new C API export:

```zig
export fn ghostty_surface_is_overlay_forwarding(
    surface: *Surface,
) bool {
    const xpc = @import("xpc.zig");
    return xpc.isOverlayForwarding(&surface.core_surface);
}
```

**`ghost/include/ghostty.h`** — Add the declaration:

```c
bool ghostty_surface_is_overlay_forwarding(ghostty_surface_t);
```

**`ghost/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`** — At the
top of `performKeyEquivalent`, after the `focused` check, add a browse mode
bypass:

```swift
// In browse mode, forward all Cmd+key events to keyDown so they
// reach the Zig forwarding block instead of being consumed by
// Ghostty bindings or the macOS menu system (Issue 609 Experiment 2).
if let surface = self.surface,
   ghostty_surface_is_overlay_forwarding(surface) {
    self.keyDown(with: event)
    return true
}
```

This goes after the `if (!focused) { return false }` guard (line 1206-1208) and
before the `keyIsBinding` check (line 1211).

#### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://lite.duckduckgo.com
```

Click the search box to enter browse mode and focus the text field. Test:

| # | Test                      | Steps                                              | Expected                       |
| - | ------------------------- | -------------------------------------------------- | ------------------------------ |
| 1 | Cmd+A selects all         | Type "hello", Cmd+A, type "X"                      | "X" — all text replaced        |
| 2 | Cmd+C / Cmd+V             | Type "hello", Cmd+A, Cmd+C, click new field, Cmd+V | "hello" pasted into new field  |
| 3 | Cmd+X cuts                | Type "hello", Cmd+A, Cmd+X                         | Text field empty               |
| 4 | Cmd+Z undoes              | Type "hello", Cmd+A, type "X", Cmd+Z               | "hello" restored               |
| 5 | Ctrl+Esc still works      | Press Ctrl+Esc                                     | Exits browse mode              |
| 6 | Cmd+A outside browse mode | Exit browse mode, press Cmd+A                      | Selects terminal text (normal) |

Test 6 is a regression check — Cmd+A must still work normally when not browsing.

**Result:** Fail

The `performKeyEquivalent` bypass works — Cmd+key events now reach Chromium via
the forwarding pipeline. The logs confirm key down/up events arriving with the
correct VK codes (0x41 for A, 0x43 for C, 0x58 for X). However, none of the Cmd
shortcuts produce their expected behavior in the webpage. Cmd+A does not select
text, Cmd+C does not copy, etc. The events arrive but Chromium does not
interpret them as shortcuts.

The Ghost-side fix (routing Cmd+key events past `performKeyEquivalent` into
`keyDown` → `keyCallback` → XPC) is correct and should be kept. The problem is
on the Chromium side: `HandleKeyEvent` constructs a `NativeWebKeyboardEvent`
with `windows_key_code` and `modifiers` (meta bit), but this is not sufficient
for Chromium to recognize Cmd shortcuts. The issue document's "Potential issues"
section predicted this — several fields are unset:

- **`is_system_key`** — On macOS, Cmd+key events should have this set so
  Chromium routes them through the editing command system.
- **`dom_code`** — USB HID usage code. Chromium's shortcut handling may check
  this.
- **`dom_key`** — DOM key enum. Some command dispatch paths may rely on this.
- **`native_key_code`** — macOS keycode. Some platform-specific paths may need
  this.

#### Ideas for next steps

1. **Add diagnostic logging on the Chromium side.** Log the modifier bits
   arriving in `HandleKeyEvent` to confirm they're actually set (the current log
   only prints VK code, not modifiers). This rules out a modifier encoding bug.

2. **Set `is_system_key = true` for Cmd+key events.** This is the most likely
   fix. On macOS, Chromium uses `is_system_key` to distinguish shortcuts from
   regular typing. When `modifiers & 8` (meta) is set, set
   `key_event.is_system_key = true`.

3. **Populate `dom_code` and `dom_key`.** If `is_system_key` alone doesn't fix
   it, these fields may be needed. The VK code can be mapped to `dom_code`
   (e.g., `ui::DomCode::US_A` for VK 0x41) and `dom_key` (e.g.,
   `ui::DomKey::FromCharacter('a')`).

4. **Study how Chromium's own Mac input path constructs
   `NativeWebKeyboardEvent`.** Look at `RenderWidgetHostViewMac::HandleKeyEvent`
   or the `NativeWebKeyboardEvent` Mac-specific constructor that takes an
   `NSEvent*`. This would reveal exactly which fields Chromium sets for Cmd+key
   events on macOS.

5. **Skip `kChar` event for Cmd+key.** When the meta modifier is set, don't send
   the follow-up `kChar` event — Cmd shortcuts should only produce
   `kRawKeyDown` + `kKeyUp`, never `kChar`. This may not be the primary cause
   but is still incorrect behavior that should be fixed.

### Experiment 3: Populate missing NativeWebKeyboardEvent fields

#### Goal

Cmd+A, Cmd+C, Cmd+V, Cmd+X, and Cmd+Z work in Chromium overlays. Experiment 2
proved the Ghost-side bypass works (events reach Chromium with correct VK codes
and modifiers), but Chromium doesn't interpret them as shortcuts because the
`NativeWebKeyboardEvent` is missing fields that Chromium's command dispatch
requires.

#### Description

Studying Chromium's own Mac input path (`WebKeyboardEventBuilder::Build()` in
`components/input/web_input_event_builders_mac.mm`) revealed that the normal
constructor sets several fields our `HandleKeyEvent` omits:

- **`is_system_key`** — `true` for all Cmd+key except Cmd+B and Cmd+I. Tells
  Blink to route the event through the system command path.
- **`dom_code`** — USB HID usage code (e.g., `DomCode::US_A`). Some shortcut
  dispatch paths check this.
- **`dom_key`** — DOM key value (e.g., `DomKey::FromCharacter('a')`). Some
  command matching uses this.

Testing confirmed that the modifier bits ARE sent correctly — pressing Cmd+A
does not type 'a' (Chromium sees the Meta modifier and suppresses text input),
but it also doesn't trigger "select all" because the event is missing the fields
above.

Chromium provides `ui::UsLayoutKeyboardCodeToDomCode()` which maps Windows VK
codes to `DomCode` assuming US layout. Since we already send the VK code, we can
derive `dom_code` without adding the native macOS keycode to the XPC message.
From `dom_code`, `ui::DomCodeToUsLayoutDomKey()` gives us the `dom_key`.

This experiment also skips the `kChar` event for Cmd+key combinations. In
Chromium's normal Mac input path, Cmd shortcuts produce only `kRawKeyDown` +
`kKeyUp`, never `kChar`.

#### Chromium branch

Create `146.0.7650.0-issue-609` from `146.0.7650.0-issue-608`.

#### Changes

**`chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`**
— Update `HandleKeyEvent`:

Add includes:

```cpp
#include "ui/events/keycodes/dom/dom_code.h"
#include "ui/events/keycodes/dom/dom_key.h"
#include "ui/events/keycodes/keyboard_code_conversion.h"
#include "ui/events/keycodes/keyboard_codes.h"
```

Replace the `HandleKeyEvent` body with:

```cpp
void ShellBrowserMainParts::HandleKeyEvent(
    const std::string& pane_id, const std::string& type,
    int windows_key_code, const std::string& utf8_text,
    uint64_t modifiers) {
  DCHECK_CURRENTLY_ON(BrowserThread::UI);

  TabState* tab = nullptr;
  for (auto& t : tabs_) {
    if (t->pane_id == pane_id) { tab = t.get(); break; }
  }
  if (!tab) return;

  int web_modifiers = static_cast<int>(modifiers & 0xF);
  bool has_meta = (web_modifiers & blink::WebInputEvent::kMetaKey) != 0;

  auto event_type = blink::WebInputEvent::Type::kRawKeyDown;
  if (type == "up")
    event_type = blink::WebInputEvent::Type::kKeyUp;

  input::NativeWebKeyboardEvent key_event(
      event_type, web_modifiers, base::TimeTicks::Now());
  key_event.windows_key_code = windows_key_code;

  // Populate dom_code from VK code (assumes US layout).
  key_event.dom_code = static_cast<int>(
      ui::UsLayoutKeyboardCodeToDomCode(
          static_cast<ui::KeyboardCode>(windows_key_code)));

  // Populate dom_key from dom_code.
  ui::DomKey dom_key;
  ui::KeyboardCode dummy_vkey;
  if (ui::DomCodeToUsLayoutDomKey(
          static_cast<ui::DomCode>(key_event.dom_code),
          web_modifiers, &dom_key, &dummy_vkey)) {
    key_event.dom_key = static_cast<int>(dom_key.ToBase());
  }

  // Mark Cmd+key as system key (except Cmd+B and Cmd+I, following Chromium's
  // IsSystemKeyEvent logic).
  if (has_meta &&
      windows_key_code != ui::VKEY_B &&
      windows_key_code != ui::VKEY_I) {
    key_event.is_system_key = true;
  }

  auto* view = tab->shell->web_contents()->GetRenderWidgetHostView();
  if (!view) {
    LOG(WARNING) << "[ProfileServer] Key view is null for pane=" << pane_id;
    return;
  }

  view->GetRenderWidgetHost()->ForwardKeyboardEvent(key_event);

  // For key down with text, send a Char event — but NOT for Cmd+key shortcuts,
  // which should only produce kRawKeyDown + kKeyUp.
  if (type != "up" && !utf8_text.empty() && !has_meta) {
    input::NativeWebKeyboardEvent char_event(
        blink::WebInputEvent::Type::kChar, web_modifiers,
        base::TimeTicks::Now());
    char_event.windows_key_code = windows_key_code;

    std::u16string text16 = base::UTF8ToUTF16(utf8_text);
    if (!text16.empty()) {
      char_event.text[0] = text16[0];
      char_event.unmodified_text[0] = text16[0];
    }

    view->GetRenderWidgetHost()->ForwardKeyboardEvent(char_event);
  }

  LOG(INFO) << "[ProfileServer] Key " << type << " vk=0x" << std::hex
            << windows_key_code << std::dec
            << " mods=" << web_modifiers
            << " system=" << key_event.is_system_key
            << " pane=" << pane_id;
}
```

Three changes from the current code:

1. **`is_system_key = true`** when Meta modifier is set (except Cmd+B, Cmd+I).
2. **`dom_code` and `dom_key` populated** from the Windows VK code using
   `UsLayoutKeyboardCodeToDomCode` and `DomCodeToUsLayoutDomKey`.
3. **`kChar` event skipped** when Meta is held.
4. **Log now includes modifiers and system key flag** for diagnostics.

No Ghost-side changes — Experiment 2's `performKeyEquivalent` bypass is already
in place.

#### Verification

```bash
# Build Chromium (branch 146.0.7650.0-issue-609)
cd chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default chromium_profile_server

# Launch
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://lite.duckduckgo.com
```

Click the search box to enter browse mode and focus the text field. Test:

| # | Test                       | Steps                                              | Expected                       |
| - | -------------------------- | -------------------------------------------------- | ------------------------------ |
| 1 | Cmd+A selects all          | Type "hello", Cmd+A, type "X"                      | "X" — all text replaced        |
| 2 | Cmd+C / Cmd+V              | Type "hello", Cmd+A, Cmd+C, click new field, Cmd+V | "hello" pasted into new field  |
| 3 | Cmd+X cuts                 | Type "hello", Cmd+A, Cmd+X                         | Text field empty               |
| 4 | Cmd+Z undoes               | Type "hello", Cmd+A, type "X", Cmd+Z               | "hello" restored               |
| 5 | Regular typing still works | Type "hello"                                       | "hello" appears                |
| 6 | Ctrl+Esc still works       | Press Ctrl+Esc                                     | Exits browse mode              |
| 7 | Cmd+A outside browse mode  | Exit browse mode, press Cmd+A                      | Selects terminal text (normal) |

**Result:** Fail

The logs confirm all three fields are set correctly: `mods=8` (kMetaKey),
`system=1` (is_system_key), and dom_code/dom_key populated. Cmd+key events
arrive at Chromium with the right data. Yet no Cmd shortcuts produce their
expected behavior.

#### Conclusion

The experiment was based on a wrong mental model. We assumed Chromium's renderer
interprets raw keyboard events and matches them to editing commands internally.
It doesn't — at least not on macOS.

Chromium's actual Mac input path works differently. When a Cmd+key event arrives
at `RenderWidgetHostViewCocoa`, it calls `interpretKeyEvents:` on the NSEvent.
Cocoa's input system recognizes the shortcut and calls `doCommandBySelector:`
with the appropriate selector (e.g., `selectAll:` for Cmd+A). The
`doCommandBySelector:` implementation converts the selector to an editing
command string (`"selectAll"`) and pushes it into an `_editCommands` vector.
Then `ForwardKeyboardEventWithCommands` sends both the raw key event AND the
editing commands to the renderer.

The renderer applies the editing commands directly. It never re-interprets the
raw key event to figure out which editing command to run. The `is_system_key`
flag actually makes this worse — it tells Blink to defer to the browser's
editing command system, but we never provide editing commands because we use
`ForwardKeyboardEvent` (the plain version without commands).

The fix is to use `ForwardKeyboardEventWithCommands` and attach the correct
editing commands for each Cmd+key combination. This matches how Chromium's own
Mac input path works (`render_widget_host_view_cocoa.mm` lines 1430-1431).

### Experiment 4: Forward editing commands with Cmd+key events

#### Goal

Cmd+A, Cmd+C, Cmd+V, Cmd+X, and Cmd+Z work in Chromium overlays.

#### Description

Experiments 2 and 3 proved that: (a) Cmd+key events reach Chromium with the
correct VK codes and modifier bits, and (b) populating `is_system_key`,
`dom_code`, and `dom_key` on the raw keyboard event is not sufficient. The
renderer doesn't re-interpret raw keyboard events to determine editing commands.

Chromium's Mac input path
(`render_widget_host_view_cocoa.mm:doCommandBySelector:`) explicitly maps Cocoa
selectors to editing command strings and attaches them to the keyboard event via
`ForwardKeyboardEventWithCommands`. The renderer applies these commands directly
without interpreting the key combination.

Since we don't have an NSEvent to pass through `interpretKeyEvents`, we manually
map Cmd+key VK codes to the same editing command strings that Chromium's
`RenderWidgetHostViewMacEditCommandHelper` produces. We also keep Experiment 3's
`dom_code`/`dom_key` fields (they're correct to set) but remove `is_system_key`
— it causes Blink to defer to the browser's command system, which is unnecessary
when we're providing commands explicitly.

The public `RenderWidgetHost` API only exposes `ForwardKeyboardEvent`.
`ForwardKeyboardEventWithCommands` is on `RenderWidgetHostImpl`, which is
already included in our file
(`content/browser/renderer_host/
render_widget_host_impl.h`). We static_cast to
access it.

#### Chromium branch

Continue on `146.0.7650.0-issue-609`.

#### Changes

**`chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`**

Add includes (in the `#if BUILDFLAG(IS_MAC)` block):

```cpp
#include "third_party/blink/public/mojom/input/input_handler.mojom.h"
#include "ui/latency/latency_info.h"
```

Replace the `HandleKeyEvent` body. The key changes from Experiment 3:

1. Remove `is_system_key` — we're providing commands explicitly.
2. For Cmd+key down events, build an editing command and use
   `ForwardKeyboardEventWithCommands` instead of `ForwardKeyboardEvent`.
3. For all other events (key up, non-Cmd keys), use
   `ForwardKeyboardEventWithCommands` with an empty command vector (matching
   Chromium's normal behavior of always using this method for key events).
4. Keep `dom_code` and `dom_key` population from Experiment 3.
5. Keep skipping `kChar` for Cmd+key from Experiment 3.

```cpp
void ShellBrowserMainParts::HandleKeyEvent(
    const std::string& pane_id, const std::string& type,
    int windows_key_code, const std::string& utf8_text,
    uint64_t modifiers) {
  DCHECK_CURRENTLY_ON(BrowserThread::UI);

  TabState* tab = nullptr;
  for (auto& t : tabs_) {
    if (t->pane_id == pane_id) { tab = t.get(); break; }
  }
  if (!tab) return;

  int web_modifiers = static_cast<int>(modifiers & 0xF);
  bool has_meta = (web_modifiers & blink::WebInputEvent::kMetaKey) != 0;

  auto event_type = blink::WebInputEvent::Type::kRawKeyDown;
  if (type == "up")
    event_type = blink::WebInputEvent::Type::kKeyUp;

  input::NativeWebKeyboardEvent key_event(
      event_type, web_modifiers, base::TimeTicks::Now());
  key_event.windows_key_code = windows_key_code;

  // Populate dom_code from VK code (assumes US layout).
  key_event.dom_code = static_cast<int>(
      ui::UsLayoutKeyboardCodeToDomCode(
          static_cast<ui::KeyboardCode>(windows_key_code)));

  // Populate dom_key from dom_code.
  ui::DomKey dom_key;
  ui::KeyboardCode dummy_vkey;
  if (ui::DomCodeToUsLayoutDomKey(
          static_cast<ui::DomCode>(key_event.dom_code),
          web_modifiers, &dom_key, &dummy_vkey)) {
    key_event.dom_key = static_cast<int>(
        static_cast<ui::DomKey::Base>(dom_key));
  }

  auto* view = tab->shell->web_contents()->GetRenderWidgetHostView();
  if (!view) {
    LOG(WARNING) << "[ProfileServer] Key view is null for pane=" << pane_id;
    return;
  }

  auto* rwhi = static_cast<RenderWidgetHostImpl*>(
      view->GetRenderWidgetHost());

  // Build editing commands for Cmd+key shortcuts (key down only).
  std::vector<blink::mojom::EditCommandPtr> commands;
  if (has_meta && type != "up") {
    std::string cmd;
    switch (windows_key_code) {
      case ui::VKEY_A: cmd = "selectAll"; break;
      case ui::VKEY_C: cmd = "copy"; break;
      case ui::VKEY_V: cmd = "paste"; break;
      case ui::VKEY_X: cmd = "cut"; break;
      case ui::VKEY_Z: cmd = "undo"; break;
      default: break;
    }
    if (!cmd.empty()) {
      commands.push_back(
          blink::mojom::EditCommand::New(cmd, ""));
    }
  }

  ui::LatencyInfo latency;
  rwhi->ForwardKeyboardEventWithCommands(
      key_event, latency, std::move(commands));

  // For key down with text, send a Char event — but NOT for Cmd+key shortcuts.
  if (type != "up" && !utf8_text.empty() && !has_meta) {
    input::NativeWebKeyboardEvent char_event(
        blink::WebInputEvent::Type::kChar, web_modifiers,
        base::TimeTicks::Now());
    char_event.windows_key_code = windows_key_code;

    std::u16string text16 = base::UTF8ToUTF16(utf8_text);
    if (!text16.empty()) {
      char_event.text[0] = text16[0];
      char_event.unmodified_text[0] = text16[0];
    }

    std::vector<blink::mojom::EditCommandPtr> no_commands;
    rwhi->ForwardKeyboardEventWithCommands(
        char_event, latency, std::move(no_commands));
  }

  LOG(INFO) << "[ProfileServer] Key " << type << " vk=0x" << std::hex
            << windows_key_code << std::dec
            << " mods=" << web_modifiers
            << " cmds=" << (commands.empty() ? 0 : 1)
            << " pane=" << pane_id;
}
```

No Ghost-side changes — Experiment 2's `performKeyEquivalent` bypass remains.

#### Verification

```bash
cd chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default chromium_profile_server

open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://lite.duckduckgo.com
```

Click the search box to enter browse mode and focus the text field. Test:

| # | Test                       | Steps                                              | Expected                       | Result |
| - | -------------------------- | -------------------------------------------------- | ------------------------------ | ------ |
| 1 | Cmd+A selects all          | Type "hello", Cmd+A, type "X"                      | "X" — all text replaced        | y      |
| 2 | Cmd+C / Cmd+V              | Type "hello", Cmd+A, Cmd+C, click new field, Cmd+V | "hello" pasted into new field  | y      |
| 3 | Cmd+X cuts                 | Type "hello", Cmd+A, Cmd+X                         | Text field empty               | y      |
| 4 | Cmd+Z undoes               | Type "hello", Cmd+A, type "X", Cmd+Z               | "hello" restored               | y      |
| 5 | Regular typing still works | Type "hello"                                       | "hello" appears                | y      |
| 6 | Ctrl+Esc still works       | Press Ctrl+Esc                                     | Exits browse mode              | y      |
| 7 | Cmd+A outside browse mode  | Exit browse mode, press Cmd+A                      | Selects terminal text (normal) | y      |

**Result:** Pass

All 7 tests pass. Cmd+A selects all text, Cmd+C copies, Cmd+V pastes, Cmd+X
cuts, Cmd+Z undoes, regular typing still works, Ctrl+Esc still exits browse
mode, and Cmd+A outside browse mode still selects terminal text.

#### Conclusion

The fix required two pieces working together:

1. **Ghost side (Experiment 2):** Bypass `performKeyEquivalent` in browse mode
   so Cmd+key events reach the Zig forwarding block instead of being consumed by
   Ghostty bindings or the macOS menu system.

2. **Chromium side (Experiment 4):** Use `ForwardKeyboardEventWithCommands`
   instead of `ForwardKeyboardEvent`, attaching explicit editing commands
   (`"selectAll"`, `"copy"`, `"paste"`, `"cut"`, `"undo"`) for Cmd+key
   combinations. This matches how Chromium's own Mac input path works — the
   renderer applies editing commands directly rather than re-interpreting raw
   keyboard events.

Experiments 2 and 3 were necessary steps: Experiment 2 proved the Ghost-side
bypass works, and Experiment 3 proved that populating event fields alone is
insufficient — the renderer needs explicit editing commands.

The remaining keyboard issue from Experiment 1 is Tab not moving focus between
form fields (test 4). This is unrelated to the Cmd shortcut problem and can be
addressed in a separate experiment.

## Conclusion

All 13 tests from the Experiment 1 matrix pass. Characters, Enter, Backspace,
Tab, arrow keys, Home/End, Shift+arrow selection, Cmd+A/C/V/X/Z, and Ctrl+Esc
all work correctly in Chromium overlays. The issue goal is fully satisfied.

### What was built

Two changes, one on each side of the pipeline:

1. **Ghost side:** A browse mode bypass in `performKeyEquivalent`
   (`SurfaceView_AppKit.swift`). When the surface is in browse mode, Cmd+key
   events route to `keyDown` instead of being consumed by Ghostty bindings or
   the macOS menu system. A new C API function
   (`ghostty_surface_is_overlay_forwarding`) exposes the browse mode state to
   Swift.

2. **Chromium side:** `HandleKeyEvent` uses `ForwardKeyboardEventWithCommands`
   instead of `ForwardKeyboardEvent`, attaching explicit editing commands for
   Cmd+key shortcuts (`"selectAll"`, `"copy"`, `"paste"`, `"cut"`, `"undo"`). It
   also populates `dom_code` and `dom_key` from the VK code using
   `UsLayoutKeyboardCodeToDomCode` and `DomCodeToUsLayoutDomKey`, and skips the
   `kChar` event for Cmd+key combinations.

### Key learnings

**Chromium's renderer does not re-interpret raw keyboard events.** On macOS,
Chromium's input path works like this: NSEvent → `interpretKeyEvents:` → Cocoa
calls `doCommandBySelector:` with the appropriate selector → the selector is
converted to an editing command string → `ForwardKeyboardEventWithCommands`
sends the key event AND the editing commands to the renderer. The renderer
applies the commands directly. It never looks at the raw key event to figure out
which command to run. This is the single most important learning from this
issue.

**`is_system_key` without editing commands creates a dead zone.** Setting
`is_system_key = true` tells Blink to defer to the browser's editing command
system. If no editing commands are attached (because we used
`ForwardKeyboardEvent` instead of `ForwardKeyboardEventWithCommands`), the event
falls into a void — modified enough to suppress text input, but missing the
commands to trigger any shortcut. This was Experiment 3's failure mode.

**macOS `performKeyEquivalent` intercepts Cmd+key before `keyDown`.** AppKit
calls `performKeyEquivalent` for any Cmd+key press. Ghostty's implementation
checks bindings first, then falls through to the menu system. Either way, the
event never reaches `keyDown` or `keyCallback`. The browse mode bypass must go
at the top of `performKeyEquivalent`, before both checks.

**Ctrl+key events follow a different path.** Unlike Cmd+key, Ctrl+key events go
through `keyDown` (not `performKeyEquivalent`), so they already reach the Zig
forwarding block without any bypass. Only Cmd+key needed special handling.

**Tab worked without any changes.** Despite failing in Experiment 1's initial
test, Tab now works for moving focus between form fields. The `dom_code` and
`dom_key` fields added in Experiment 3 (and kept in Experiment 4) likely fixed
this — Chromium may need `dom_code` to distinguish Tab-as-focus-move from
Tab-as-character.

### Files changed

- `ghost/src/apprt/embedded.zig` — `ghostty_surface_is_overlay_forwarding`
  export
- `ghost/include/ghostty.h` — C declaration
- `ghost/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift` — browse
  mode bypass in `performKeyEquivalent`
- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`
  — `HandleKeyEvent` with `ForwardKeyboardEventWithCommands` and editing
  commands
