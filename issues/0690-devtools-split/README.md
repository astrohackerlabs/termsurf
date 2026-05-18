+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

# Issue 690: DevTools Split Command

Add a `:devtools` command to the TUI command bar that opens DevTools in a new
split pane. Typing `:devtools right` in a browser pane creates a split to the
right and runs `web devtools` in it, automatically inspecting the current tab.

## Background

Issue 688 attempted this three times and failed. The root blocker was orphaned
Chromium tabs — closing a DevTools pane never told the profile server to destroy
the DevTools tab, so reopening DevTools attached a second
`InspectorOverlayAgent` to the same renderer and crashed (Issue 686).

Issue 689 solved the orphan problem. `handleDisconnect` now sends a `close_tab`
XPC message to the profile server, and `CloseTabByPaneId` tears down the Shell
before the TabState in the correct order. Tabs are properly destroyed when panes
close.

This issue re-attempts the `:devtools` command with the tab lifecycle fix in
place. The design is the same as Issue 688 Experiment 1, minus the orphan
problem.

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

## What's different from Issue 688

Issue 688 failed for three reasons, all now resolved:

1. **Orphaned DevTools tabs** (Exp 1 failure) — Fixed by Issue 689. Closing a
   pane now sends `close_tab` to the profile server, which destroys the Shell
   and TabState in the correct order.
2. **Shared connection cancellation** (Exp 2 failure) — The `server_peer`
   approach was abandoned. Issue 689 uses an explicit `close_tab` message on the
   control connection instead.
3. **Unknown crash on first invocation** (Exp 3 failure) — Exp 3 was attempting
   both the `:devtools` command AND the `close_tab` fix simultaneously. With tab
   lifecycle now solved independently in Issue 689, this issue only needs to
   implement the `:devtools` command itself.

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
With Issue 689's tab lifecycle fix in place, closing and reopening DevTools
should work without crashing.

### Changes

Seven files across TUI and GUI. No Chromium changes needed.

#### 1. TUI: Add `CommandResult` variants and `devtools` command (`main.rs`)

Add two new variants to the `CommandResult` enum (line 60):

```rust
enum CommandResult {
    Quit,
    SetColorScheme(String),
    DevTools(String),   // direction: "right", "down", "left", "up"
    Error(String),      // error message to display in command bar
    None,
}
```

Add `devtools` command to the `COMMANDS` array (after `colorscheme`, line 89):

```rust
Command {
    name: "devtools",
    exec: |args| match args.first().copied() {
        Some("right" | "r") | None => CommandResult::DevTools("right".into()),
        Some("down" | "d") => CommandResult::DevTools("down".into()),
        Some("left" | "l") => CommandResult::DevTools("left".into()),
        Some("up" | "u") => CommandResult::DevTools("up".into()),
        Some(other) => CommandResult::Error(
            format!("Unknown direction: {}", other),
        ),
    },
},
```

#### 2. TUI: Capture executable path (`main.rs`)

Early in `main()`, before the event loop (near the other `let mut` declarations
around line 230):

```rust
let current_exe = std::env::current_exe()
    .ok()
    .and_then(|p| p.to_str().map(String::from))
    .unwrap_or_else(|| "web".to_string());
```

#### 3. TUI: Command error state and handling (`main.rs`)

Add state variable near other mode state (around line 230):

```rust
let mut command_error: Option<String> = None;
```

In the Mode::Command Enter handler (line 533), expand the match to handle
`DevTools` and `Error`:

```rust
match dispatch(&cmd_text) {
    CommandResult::Quit => break,
    CommandResult::SetColorScheme(scheme) => {
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            conn.send_set_color_scheme(pid, &scheme);
        }
    }
    CommandResult::DevTools(direction) => {
        if is_devtools {
            command_error = Some("Cannot open DevTools from a DevTools pane".into());
        } else if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            match conn.send_query_devtools(pid, 0, &profile) {
                Err(msg) => {
                    command_error = Some(msg);
                }
                Ok(_) => {
                    // Tab doesn't have DevTools yet — open the split.
                    let cmd = format!("{} devtools", current_exe);
                    conn.send_open_split(pid, &direction, &cmd);
                }
            }
        }
    }
    CommandResult::Error(msg) => {
        command_error = Some(msg);
    }
    CommandResult::None => {}
}
// Only return to Control mode if no error.
if command_error.is_none() {
    mode = Mode::Control;
}
```

Note: `send_query_devtools` with `inspected_tab_id=0` (auto-target) returns
`Err(msg)` if the tab already has DevTools open (Issue 687). We reuse that
validation.

In the Mode::Command key handler, clear the error on any non-Enter keystroke:

```rust
// Clear command error on any keystroke (before passing to edtui).
command_error = None;
```

#### 4. TUI: Error display in command bar (`main.rs`)

Add `command_error: &Option<String>` parameter to `ui()` (line 663).

In the command bar rendering (around line 696), when `*mode == Mode::Command`:

- If `command_error.is_some()`, use `RED` border color instead of `YELLOW`
- Add `.title_bottom()` with the error text styled red

```rust
let border_color = if command_error.is_some() { RED } else { YELLOW };
let mut block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(border_color))
    .title(/* existing COMMAND title */);
if let Some(ref err) = command_error {
    block = block.title_bottom(
        Line::from(err.as_str()).style(Style::default().fg(RED))
    );
}
```

Update all `ui()` call sites to pass `&command_error`.

#### 5. TUI: `send_open_split` function (`xpc.rs`)

Add a fire-and-forget XPC send function following the same pattern as
`send_navigate` (line 598):

```rust
pub fn send_open_split(&self, pane_id: &str, direction: &str, command: &str) {
    unsafe {
        let msg = xpc_dictionary_create(std::ptr::null(), std::ptr::null_mut(), 0);
        let action = CString::new("open_split").unwrap();
        xpc_dictionary_set_string(msg, c"action".as_ptr(), action.as_ptr());
        let pid = CString::new(pane_id).unwrap();
        xpc_dictionary_set_string(msg, c"pane_id".as_ptr(), pid.as_ptr());
        let dir = CString::new(direction).unwrap();
        xpc_dictionary_set_string(msg, c"direction".as_ptr(), dir.as_ptr());
        let cmd = CString::new(command).unwrap();
        xpc_dictionary_set_string(msg, c"command".as_ptr(), cmd.as_ptr());
        xpc_connection_send_message(self.raw, msg);
        xpc_release(msg);
    }
}
```

#### 6. GUI: `open_split` action handler (`xpc.zig`)

Add to `handleMessage` (line 290, in the else-if chain):

```zig
} else if (std.mem.eql(u8, action_str, "open_split")) {
    handleOpenSplit(msg);
}
```

New handler function:

```zig
fn handleOpenSplit(msg: xpc_object_t) void {
    const pane_id = std.mem.span(
        xpc_dictionary_get_string(msg, "pane_id") orelse return);
    const direction_str = std.mem.span(
        xpc_dictionary_get_string(msg, "direction") orelse return);
    const command = std.mem.span(
        xpc_dictionary_get_string(msg, "command") orelse return);

    const p = panes.get(pane_id) orelse {
        log.warn("open_split: no pane for {s}", .{pane_id});
        return;
    };
    const surface = p.overlay_surface orelse {
        log.warn("open_split: no surface for {s}", .{pane_id});
        return;
    };

    const direction: apprt.action.SplitDirection = if (std.mem.eql(u8, direction_str, "right"))
        .right
    else if (std.mem.eql(u8, direction_str, "down"))
        .down
    else if (std.mem.eql(u8, direction_str, "left"))
        .left
    else if (std.mem.eql(u8, direction_str, "up"))
        .up
    else {
        log.warn("open_split: unknown direction {s}", .{direction_str});
        return;
    };

    log.info("open_split pane={s} dir={s} cmd={s}", .{
        pane_id, direction_str, command,
    });

    termsurf_surface_split_with_input(surface, direction, command.ptr);
}
```

#### 7. GUI: `termsurf_surface_split_with_input` C API (`embedded.zig`)

Add a module-level variable and two new exports:

```zig
var pending_initial_input: ?[*:0]const u8 = null;

export fn termsurf_surface_split_with_input(
    ptr: *Surface,
    direction: apprt.action.SplitDirection,
    input: [*:0]const u8,
) void {
    // Duplicate the input string so it survives until Swift reads it.
    const len = std.mem.len(input);
    const buf = alloc.alloc(u8, len + 1) catch return;
    @memcpy(buf[0..len], input[0..len]);
    buf[len] = 0;
    pending_initial_input = @ptrCast(buf.ptr);

    termsurf_surface_split(ptr, direction);
}

export fn termsurf_surface_get_pending_input() ?[*:0]const u8 {
    const result = pending_initial_input;
    pending_initial_input = null;
    return result;
}

export fn termsurf_surface_free_pending_input(ptr: [*:0]const u8) void {
    const len = std.mem.len(ptr);
    alloc.free(@constCast(ptr[0..len + 1]));
}
```

Three exports: `split_with_input` stores the input and calls normal split,
`get_pending_input` returns and clears it (one-shot), `free_pending_input` frees
the allocated string.

#### 8. Swift: Read pending initial input (`TermSurf.App.swift`)

In the `newSplit` function (line 828), after creating the `SurfaceConfiguration`
from inherited config (line 847):

```swift
var config = SurfaceConfiguration(
    from: termsurf_surface_inherited_config(surface, TERMSURF_SURFACE_CONTEXT_SPLIT))

// Check for pending initial input from open_split (Issue 690).
if let pendingInput = termsurf_surface_get_pending_input() {
    config.initialInput = String(cString: pendingInput) + "\n"
    termsurf_surface_free_pending_input(pendingInput)
}
```

The `\n` triggers execution — the shell receives the command text followed by a
newline, just as if the user typed it and pressed Enter.

### Why `initialInput` over `command`

Using `initialInput` (typing into the shell) rather than `command` (replacing
the shell):

- The new pane has a real shell. If `web devtools` exits (user quits DevTools),
  the pane stays open with a shell prompt.
- With `command`, the pane would close when `web devtools` exits.
- `initialInput` is typed after the shell starts, so shell configuration
  (.zshrc, aliases, etc.) is fully loaded.
- Ghostty's existing `initialInput` infrastructure buffers input until the PTY
  is ready, so timing is not a concern.

### Test

1. Open a browser: `web google.com`
2. Press `:`, type `devtools right`, press Enter
3. A split should open to the right, running `web devtools`
4. The DevTools pane should auto-target the google.com tab
5. Close DevTools pane (`:q`)
6. `:devtools left` → DevTools reopens without crash (Issue 689 fix)
7. Close and reopen 3 times → stable
8. In the DevTools pane, `:devtools right` → red command bar:
   `"Cannot open DevTools from a DevTools pane"`
9. `:devtools` (no direction) → defaults to right
10. `:devtools banana` → red command bar: `"Unknown direction: banana"`
11. Type any character after seeing error → error clears, bar returns to yellow
12. `:devtools down` → split below
13. `:devtools right` when DevTools already open → red command bar with
    duplicate error from `query_devtools`

### Result: FAILURE (partial success)

The `:devtools` command works on first invocation — the split opens, DevTools
auto-targets the browser tab, error validation works (red bar for
DevTools-in-DevTools, invalid direction, duplicate detection), and errors clear
on keystroke.

The failure is on the second invocation. After closing the DevTools pane and
typing `:devtools` again, the entire app crashes with runaway audio (GPU process
dies mid-frame, audio buffers loop). This is not a Chromium orphan problem —
`web devtools` can be opened and closed repeatedly from the command line without
issues. The crash only happens when the split is created via the `:devtools` TUI
command a second time.

**Key observation:** The crash is NOT in Chromium. Running `web devtools`
manually in a split (created via the normal Cmd+D keybinding) works
indefinitely. The crash only occurs when the split is created programmatically
via the `open_split` → `termsurf_surface_split_with_input` path.

**Hypotheses:**

1. **`@fieldParentPtr` on a stale CoreSurface pointer.** After the first
   DevTools pane closes, the original browser pane's Surface may have been
   reallocated (the split tree reorganizes when a split is removed). The
   `overlay_surface` pointer in the Pane struct still points to the old
   CoreSurface address, but the memory may have been reused or the Surface
   struct moved. `@fieldParentPtr("core_surface", surface)` then computes a
   garbage Surface pointer, and calling `termsurf_surface_split_with_input` on
   it corrupts memory.

2. **Pending input leak.** If the first `termsurf_surface_split_with_input`
   stores a pending input pointer but the Swift side fails to consume it (e.g.,
   a race condition between the XPC dispatch queue and the main thread where
   `newSplit` runs), the second call overwrites `pending_initial_input` without
   freeing the first allocation. This is a memory leak, not a crash — but if the
   stale pointer is read after the memory is freed, it's use-after-free.

3. **Thread safety of `pending_initial_input`.** The variable is set on the XPC
   serial queue (via `handleOpenSplit`) and read on the main thread (via Swift's
   `newSplit` notification handler). There's no synchronization. If the second
   `open_split` arrives while Swift is still processing the first notification,
   the pending input could be overwritten mid-read.

4. **Split tree corruption.** The split tree manipulation in
   `BaseTerminalController.newSplit()` may not handle programmatic splits
   correctly when the source surface has changed since the last split operation.
   The `surfaceTree.inserting()` call uses the `oldView` reference, which may be
   invalid if the view hierarchy was restructured after the first DevTools pane
   was removed.

Hypothesis 1 is the most likely — the `overlay_surface` pointer going stale
after a split is removed is a known risk in the pane tracking architecture. The
pointer is set once in `handleSetOverlay` and never updated when the split tree
changes.

## Experiment 2: Dispatch split creation to main thread

### Hypothesis

The crash is caused by an AppKit threading violation, not a stale pointer.

When `handleOpenSplit` runs on the XPC serial background queue
(`com.termsurf.ghost.xpc`), the entire call chain executes on that queue:

```
handleOpenSplit                          (xpc_queue)
  → termsurf_surface_split_with_input    (xpc_queue)
    → termsurf_surface_split             (xpc_queue)
      → performAction(.new_split)        (xpc_queue)
        → App.action callback            (xpc_queue)
          → App.newSplit()               (xpc_queue)
            → NotificationCenter.post    (xpc_queue)
              → termsurfDidNewSplit       (xpc_queue)  ← observer runs on posting thread
                → replaceSurfaceTree     (xpc_queue)
                  → surfaceTree = new    (xpc_queue)  ← @Published didSet
                    → surfaceTreeDidChange  (xpc_queue)
                      → window.surfaceIsZoomed = ...   ← NSView mutation off main thread
                      → window?.close()                ← NSWindow call off main thread
```

`NotificationCenter.post()` delivers to observers synchronously on the calling
thread. Since the post originates on the XPC queue, `termsurfDidNewSplit` runs
on the XPC queue, and `replaceSurfaceTree` manipulates the NSView hierarchy off
the main thread.

The first invocation may work because AppKit sometimes tolerates single
off-main-thread mutations. The second invocation crashes because the view
hierarchy is more complex (the split tree was already modified once) and
AppKit's internal state is inconsistent from the first violation.

Other XPC handlers (like `handleSetOverlay`) don't hit this because they only
modify data-model fields on CoreSurface — no NSView manipulation. The split is
the only handler that triggers view hierarchy changes.

### Design

One change in xpc.zig: dispatch the split call to the main queue using
`dispatch_async_f`.

#### 1. GUI: Dispatch to main queue in `handleOpenSplit` (`xpc.zig`)

Add extern declarations for GCD main queue dispatch:

```zig
extern "c" fn dispatch_async_f(
    queue: *anyopaque,
    context: ?*anyopaque,
    work: *const fn (?*anyopaque) callconv(.C) void,
) void;
extern "c" fn dispatch_get_main_queue() *anyopaque;
```

Modify `handleOpenSplit` to package arguments into a heap-allocated struct and
dispatch to the main queue:

```zig
const SplitRequest = struct {
    surface: *anyopaque,
    direction: c_int,
    command_buf: [512]u8,
    command_len: usize,
};

fn handleOpenSplit(msg: xpc_object_t) void {
    // ... existing pane/surface/direction lookup (unchanged) ...

    const Embedded = @import("embedded.zig");
    const surface_ptr: *Embedded.Surface = @fieldParentPtr("core_surface", surface);

    // Package args for main-queue dispatch.
    const req = alloc.create(SplitRequest) catch return;
    req.surface = @ptrCast(surface_ptr);
    req.direction = direction;
    const cmd = std.mem.span(command);
    const copy_len = @min(cmd.len, req.command_buf.len - 1);
    @memcpy(req.command_buf[0..copy_len], cmd[0..copy_len]);
    req.command_buf[copy_len] = 0;
    req.command_len = copy_len;

    dispatch_async_f(
        dispatch_get_main_queue(),
        @ptrCast(req),
        &splitOnMainThread,
    );
}

fn splitOnMainThread(ctx: ?*anyopaque) callconv(.C) void {
    const req: *SplitRequest = @alignCast(@ptrCast(ctx orelse return));
    defer alloc.destroy(req);
    termsurf_surface_split_with_input(
        req.surface,
        req.direction,
        @ptrCast(&req.command_buf),
    );
}
```

This ensures the entire chain — `performAction` → `NotificationCenter.post` →
`termsurfDidNewSplit` → `replaceSurfaceTree` → NSView manipulation — runs on the
main thread.

### What's different from Experiment 1

| Aspect        | Experiment 1                | Experiment 2                |
| ------------- | --------------------------- | --------------------------- |
| Split call    | Direct from XPC queue       | Dispatched to main queue    |
| NSView safety | Off-main-thread (violation) | On main thread (correct)    |
| Other code    | —                           | Unchanged from Experiment 1 |

### Test

Same test plan as Experiment 1, with emphasis on the close → reopen cycle:

1. Open a browser: `web google.com`
2. `:devtools right` → split opens with DevTools
3. Close DevTools pane (`:q`)
4. `:devtools left` → should open without crash
5. Close and reopen 5 times → stable
6. All error cases still work (DevTools-in-DevTools, invalid direction,
   duplicate detection)

### Result: SUCCESS

Dispatching the split creation to the main thread via `dispatch_async_f` fixes
the crash. The `:devtools` command now works reliably across multiple close →
reopen cycles. The root cause was confirmed: `handleOpenSplit` ran the entire
`performAction` → `NotificationCenter.post` → `replaceSurfaceTree` → NSView
mutation chain on the XPC background queue, violating AppKit's main thread
requirement.

## Conclusion

The `:devtools [direction]` TUI command works end-to-end. Seven files across TUI
and GUI implement the feature:

1. **TUI** (`main.rs`, `xpc.rs`): Parses `:devtools [right|down|left|up]`,
   validates (no DevTools-from-DevTools, no duplicates), sends `open_split` XPC
   message with the `web devtools` command string.
2. **GUI** (`xpc.zig`): Receives `open_split`, dispatches to main thread via
   `dispatch_async_f`, calls `termsurf_surface_split_with_input`.
3. **GUI** (`embedded.zig`, `termsurf.h`): Stores pending initial input, exposes
   C API for Swift to read it.
4. **Swift** (`TermSurf.App.swift`): Reads pending input in `newSplit()`, sets
   `config.initialInput` so the new shell receives the command.

The critical fix was Experiment 2: dispatching the split creation from the XPC
background queue to the main thread. Without this, the second invocation crashed
because AppKit's NSView hierarchy was being mutated off the main thread.
