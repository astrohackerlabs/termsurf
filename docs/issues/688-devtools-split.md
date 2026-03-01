# Issue 688: DevTools Split Command

Add a `:devtools` command to the TUI command bar that opens DevTools in a new
split pane. Typing `:devtools right` in a browser pane creates a split to the
right and runs `web devtools` in it, automatically inspecting the current tab.

## Background

Issue 687 enforced one DevTools per tab and locked DevTools panes from
navigating. DevTools now works reliably, but opening it requires manually
creating a split pane and typing `web devtools`. This issue automates that
workflow into a single command.

The end goal is a keyboard shortcut like `Opt+Cmd+I` that means "devtools right"
— but the shortcut is a separate issue. This issue builds the underlying command
infrastructure.

## How It Works

1. User is browsing in a `web` pane (e.g., `web google.com`)
2. User presses `:` to enter Command mode
3. User types `devtools right` and presses Enter
4. The TUI sends an XPC message to the GUI: "create a split to the right of my
   pane, and run this command in it"
5. The GUI creates the split using existing Ghostty split infrastructure
6. The new pane runs `web devtools`, which auto-targets the browser tab from
   step 1

### Why the full executable path matters

The command sent to the new pane must use the full path of the currently running
`web` binary (`std::env::current_exe()`), not just `web`. In development, the
user may have multiple builds — a release `web` in `$PATH` and a debug `web` in
the cargo target directory. Running the wrong one leads to confusing version
mismatches. Using the exact path of the current process guarantees the same
binary runs in the new pane.

## Design

### New TUI command: `:devtools <direction>`

Extend the command dispatcher in `main.rs` (the `dispatch()` function) to
recognize `devtools` commands:

- `:devtools right` — open DevTools in a split to the right
- `:devtools down` — open DevTools in a split below
- `:devtools left` — open DevTools in a split to the left
- `:devtools up` — open DevTools in a split above
- `:devtools` (no direction) — default to `right`

The command returns a new `CommandResult` variant that carries the direction.

### New XPC message: `open_split`

The TUI sends a new XPC message to the GUI:

```
{
  action: "open_split",
  pane_id: "...",
  direction: "right",          // "right", "down", "left", "up"
  command: "/full/path/to/web devtools"
}
```

The GUI receives this, finds the surface for the pane, and triggers a split
using the existing `termsurf_surface_split` API with a custom command in the
`SurfaceConfiguration`.

### GUI handler: `handleOpenSplit`

The GUI handler needs to:

1. Look up the surface for `pane_id` (via `surface_to_pane` / `panes` map)
2. Map the direction string to `SplitDirection` (right=0, down=1, left=2, up=3)
3. Create a split on that surface with the given command

The existing split flow is:

```
termsurf_surface_split(surface, direction)
  → TermSurf.App.newSplit() posts notification with SurfaceConfiguration
  → BaseTerminalController.newSplit() creates new SurfaceView
```

The `SurfaceConfiguration` has a `command` field. The challenge is threading a
custom command through this flow — `termsurf_surface_split()` doesn't currently
accept a command parameter. Options:

1. **Use `initialInput`** — create a normal split, then "type" the command into
   it via the `initialInput` field on `SurfaceConfiguration`. This sends the
   command text as keyboard input after the shell starts. Simpler but depends on
   shell prompt timing.
2. **Use `command`** — set `SurfaceConfiguration.command` to the full command
   string. The new pane runs `web devtools` directly without a shell. Cleaner
   but requires modifying the split flow to accept a custom command.
3. **Use a new XPC-to-surface path** — bypass `termsurf_surface_split` and
   create the split directly from the XPC handler, posting the notification with
   a custom `SurfaceConfiguration`.

Option 3 is the most direct — the XPC handler already has access to the surface
and can post the same notification that `termsurf_surface_split` would, but with
a custom `SurfaceConfiguration` that includes the command.

### Getting the executable path

In `main.rs`, capture the current executable path early:

```rust
let current_exe = std::env::current_exe()
    .ok()
    .and_then(|p| p.to_str().map(String::from))
    .unwrap_or_else(|| "web".to_string());
```

When building the `open_split` command, construct: `"{current_exe} devtools"`

### Error cases

Two cases must be caught before sending the `open_split` XPC message:

1. **`:devtools` typed in a DevTools pane.** You can't open DevTools for
   DevTools. Checked locally — `is_devtools` is already a flag in the TUI. No
   XPC needed.
2. **`:devtools` typed in a browser tab that already has DevTools open.** The
   `query_devtools` message (Issue 687) already checks for duplicates. The TUI
   calls it before sending `open_split`. If it returns an error, the command bar
   shows the error instead of splitting.

Both cases are validated before any split is attempted.

### Command bar error display

When a command fails, the command bar turns red and shows an error message on a
footer line below the input. This is a general-purpose error mechanism for
command mode — any command can use it.

**Visual behavior:**

- The command bar border turns red (replacing the normal yellow)
- A single-line error message appears below the command input, inside the bar's
  bottom border area (e.g., `"Tab 4 already has DevTools open"`)
- The error persists until the user types another character or exits command
  mode (Esc)

**Implementation:**

Add a `CommandResult::Error(String)` variant to the `CommandResult` enum. When
`dispatch()` returns an error, the event loop stores the error message in a
`command_error: Option<String>` variable. The `ui()` function checks this
variable — if set, it renders the command bar with a red border and the error
text as a bottom title. Any subsequent keystroke in command mode clears the
error.

This pattern generalizes beyond DevTools — unrecognized commands, invalid
arguments, or any future command that can fail will use the same red-bar
mechanism.

## Relevant Code

- `tui/src/main.rs` — `dispatch()` function (command mode), `CommandResult` enum
- `tui/src/xpc.rs` — XPC message sending
- `gui/src/apprt/xpc.zig` — XPC message handling, `panes` map, `surface_to_pane`
  map
- `gui/src/apprt/embedded.zig` — `termsurf_surface_split`, C API exports
- `gui/src/apprt/action.zig` — `SplitDirection` enum
- `gui/macos/Sources/TermSurf/TermSurf.App.swift` — `newSplit()`, notification
  posting
- `gui/macos/Sources/Features/Terminal/BaseTerminalController.swift` —
  `newSplit()`, `SurfaceConfiguration`
- `gui/macos/Sources/TermSurf/Surface View/SurfaceView.swift` —
  `SurfaceConfiguration` struct with `command` and `initialInput` fields

## Experiment 1: End-to-end `:devtools` command

### Hypothesis

If the TUI parses `:devtools [direction]`, validates the request, sends an
`open_split` XPC message, and the GUI creates a split with `initialInput` set to
the `web devtools` command, then DevTools opens in a new split with one command.

### Changes

#### 1. TUI: `CommandResult::Error` and `CommandResult::DevTools` (`main.rs`)

Add two new variants to the `CommandResult` enum:

```rust
enum CommandResult {
    Quit,
    SetColorScheme(String),
    DevTools(String),   // direction: "right", "down", "left", "up"
    Error(String),      // error message to display
    None,
}
```

Add a `devtools` command to the `COMMANDS` array:

```rust
Command {
    name: "devtools",
    exec: |args| match args.first() {
        Some(&"right" | &"r") | None => CommandResult::DevTools("right".into()),
        Some(&"down" | &"d") => CommandResult::DevTools("down".into()),
        Some(&"left" | &"l") => CommandResult::DevTools("left".into()),
        Some(&"up" | &"u") => CommandResult::DevTools("up".into()),
        Some(other) => CommandResult::Error(
            format!("Unknown direction: {}", other),
        ),
    },
},
```

#### 2. TUI: Command error display (`main.rs`)

Add state variable and pass to `ui()`:

```rust
let mut command_error: Option<String> = None;
```

In the `Mode::Command` Enter handler, after calling `dispatch()`:

- `CommandResult::Error(msg)` → store in `command_error`, stay in Command mode
  (don't switch to Control)
- All other results → clear `command_error`, proceed as normal

In the `Mode::Command` key handler, clear the error on any keystroke that isn't
Enter (so the user sees the error until they start typing again).

Add `command_error: &Option<String>` parameter to `ui()`. In the command bar
rendering:

- If `command_error.is_some()`, use red border color instead of yellow
- Add `.title_bottom()` with the error text styled red

#### 3. TUI: DevTools validation and `open_split` (`main.rs`)

In the `Mode::Command` Enter handler, when `dispatch()` returns
`CommandResult::DevTools(direction)`:

1. **Check `is_devtools`.** If true, set `command_error` to
   `"Cannot open DevTools from a DevTools pane"` and stay in Command mode.
2. **Call `query_devtools`.** Send `query_devtools(pane_id, 0, &profile)` to
   check if the current tab already has DevTools. If it returns `Err(msg)`, set
   `command_error` to the error message and stay in Command mode.
3. **Send `open_split`.** If both checks pass, call
   `send_open_split(pane_id, &direction, &command_string)` where
   `command_string` is `"{current_exe} devtools"`.
4. Switch to Control mode.

Capture the executable path early in `main()`:

```rust
let current_exe = std::env::current_exe()
    .ok()
    .and_then(|p| p.to_str().map(String::from))
    .unwrap_or_else(|| "web".to_string());
```

#### 4. TUI: `send_open_split` function (`xpc.rs`)

Add a fire-and-forget XPC send function:

```rust
pub fn send_open_split(
    &self,
    pane_id: &str,
    direction: &str,
    command: &str,
)
```

Sends:

```
{
  action: "open_split",
  pane_id: "...",
  direction: "right",
  command: "/full/path/to/web devtools"
}
```

#### 5. GUI: `handleOpenSplit` handler (`xpc.zig`)

Register `"open_split"` in `handleMessage`. The handler:

1. Extract `pane_id`, `direction`, `command` from the XPC message.
2. Look up the pane in `panes`, get its `overlay_surface`.
3. Find the `Surface` (apprt surface) from the core surface. The existing
   pattern is `app.findSurfaceByPaneId(pane_id)` — use the same lookup.
4. Map direction string to `SplitDirection` enum (right=0, down=1, left=2,
   up=3).
5. Call `termsurf_surface_split_with_input(surface, direction, command)` — a new
   C API export.

#### 6. GUI: `termsurf_surface_split_with_input` C API (`embedded.zig`)

Add a new export that behaves like `termsurf_surface_split` but also stores a
pending `initialInput` string for the new surface:

```zig
export fn termsurf_surface_split_with_input(
    ptr: *Surface,
    direction: apprt.action.SplitDirection,
    input: [*:0]const u8,
) void
```

This function:

1. Stores the input string in a module-level `pending_initial_input` variable.
2. Calls `termsurf_surface_split(ptr, direction)` — the normal split path.
3. The Swift notification handler picks up the pending input.

#### 7. Swift: Read pending initial input (`TermSurf.App.swift`)

In the `newSplit` case of the action dispatcher (line 838–850), after creating
the `SurfaceConfiguration` from the inherited config:

1. Check if `termsurf_surface_get_pending_input()` returns a non-null string.
2. If so, set `config.initialInput` to that string + `"\n"` (the newline
   triggers execution).
3. Clear the pending input.

Add a new C export in `embedded.zig`:

```zig
export fn termsurf_surface_get_pending_input() ?[*:0]const u8
```

This returns and clears the pending input string. It's a one-shot: the first
call returns the string, subsequent calls return null until a new
`split_with_input` is called.

### Why `initialInput` over `command`

Using `initialInput` (typing into the shell) rather than `command` (replacing
the shell):

- The new pane has a real shell. If `web devtools` exits (user quits DevTools),
  the pane stays open with a shell prompt — the user can run another command.
- With `command`, the pane would close when `web devtools` exits (or show
  "Process exited" if `wait_after_command` is set). Less useful.
- `initialInput` is typed after the shell starts, so shell configuration
  (.zshrc, aliases, etc.) is fully loaded.

The timing concern (shell not ready when input arrives) is mitigated by
Ghostty's existing `initialInput` infrastructure — it buffers the input and
sends it after the PTY is ready.

### Test

1. Open a browser: `web google.com`
2. Press `:`, type `devtools right`, press Enter
3. A split should open to the right, running `web devtools`
4. The DevTools pane should auto-target the google.com tab
5. Press `:`, type `devtools right` again → red command bar:
   `"Tab N already has DevTools open"`
6. Close DevTools, try again → should work
7. In the DevTools pane, press `:`, type `devtools right` → red command bar:
   `"Cannot open DevTools from a DevTools pane"`
8. `:devtools down` → split below
9. `:devtools` (no direction) → defaults to right
10. `:devtools banana` → red command bar: `"Unknown direction: banana"`
11. Type any character after seeing error → error clears, bar returns to yellow

### Result: FAILURE

`:devtools right` works on the first invocation — the split opens,
`web devtools` runs, and DevTools auto-targets the browser tab correctly. The
command bar error display also works: `:devtools` in a DevTools pane shows the
red bar, `:devtools banana` shows the direction error, and errors clear on the
next keystroke.

The failure is in the close → reopen cycle. After closing the DevTools pane and
typing `:devtools left`, Chromium crashes with runaway audio (GPU process dies
mid-frame, audio buffers loop), requiring a force kill of the profile server.

**Root cause:** Closing a DevTools pane removes it from the GUI's `panes` map
(`cleanupPane` clears the overlay and deletes the pane entry), but does not tell
Chromium's profile server to close the DevTools tab. The orphaned DevTools
session stays alive inside Chromium, still attached to the browser tab's
renderer via its `InspectorOverlayAgent`.

When a new `:devtools` command runs, `query_devtools` checks the `panes` map for
duplicates — but the old pane was already removed, so no duplicate is detected.
The new `web devtools` creates a second DevTools tab for the same inspected tab.
Two `InspectorOverlayAgent` instances attach to one renderer, triggering the
same `PaintController` DCHECK crash from Issue 686.

This is the Issue 686 crash resurfacing through a code path that Issue 687's
validation cannot catch: the duplicate isn't visible in the `panes` map because
the tracking was cleaned up while the Chromium session persisted.

**What needs to happen:** `cleanupPane` must send a "close DevTools tab" message
to Chromium's profile server when a pane with `inspected_tab_id != 0` is
removed. The Chromium-side DevTools session must be fully torn down before a new
one can be created. This is a prerequisite for the `:devtools` command to work
reliably.
