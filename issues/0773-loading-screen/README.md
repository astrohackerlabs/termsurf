+++
status = "closed"
opened = "2026-04-05"
closed = "2026-04-05"
+++

# Issue 773: Loading screen for browser startup

## Goal

Show a loading indicator in the Web TUI viewport while the browser engine
starts, and display errors if something goes wrong.

## Background

Chromium's first launch after a fresh install takes ~60 seconds. During this
time, the user sees a blank terminal pane with no feedback. It looks broken.
Even subsequent launches take several seconds while the GPU process initializes.

The Web TUI already occupies the full terminal pane viewport. There is plenty of
space to display loading status, progress, and errors.

## Requirements

1. **Loading indicator** — show something immediately when `web` starts. The
   user should see feedback within the first frame.

2. **Status messages** — update as the browser progresses through startup:
   - Connecting to GUI
   - Spawning browser engine
   - Waiting for browser to initialize
   - Browser ready / page loading

3. **Error display** — if something goes wrong, show the error in the viewport
   instead of silently failing:
   - Roamium not found (not installed)
   - Roamium crashed (code signature, sandbox failure)
   - Connection timeout (browser never connected)
   - Socket error

4. **Disappear on success** — once the browser overlay appears, the loading
   screen should be replaced by the normal TUI chrome (URL bar, mode indicator).

## Analysis

The Web TUI (`webtui/src/main.rs`) already renders a TUI interface with ratatui.
The loading screen would be rendered in the same viewport before the browser
overlay appears.

The TUI currently waits for `BrowserReady` from the GUI before it knows the
browser is connected. The sequence of events that could be surfaced:

1. TUI sends `HelloRequest` → show "Connecting..."
2. TUI sends `SetOverlay` → show "Starting browser..."
3. TUI waits for `BrowserReady` → show "Waiting for Chromium..." with elapsed
   time
4. TUI receives `BrowserReady` → show "Loading page..."
5. Browser renders first frame → loading screen disappears

If step 3 takes more than ~30 seconds, show a warning that first launch is slow.
If it takes more than ~120 seconds, suggest checking if Roamium is installed.

The TUI already has a main event loop that redraws on every event. Adding a
loading state that renders a centered message in the viewport should be
straightforward.

## Experiments

### Experiment 1: Loading log in the viewport

Render a vertical log of startup stages inside the viewport area. Each stage
gets a line with a status icon (✓ done, ⠋ in progress, ✗ error). The log
replaces the current viewport debug text (`origin: ... size: ...`) until the
browser overlay appears.

#### Changes

**`webtui/src/main.rs`**

1. Add a `LoadingStage` enum and a
   `loading_log: Vec<(LoadingStage, StageStatus)>` state variable:

   ```rust
   enum LoadingStage {
       ConnectingToGui,
       StartingBrowser,
       WaitingForChromium,
       LoadingPage,
       Ready,
   }

   enum StageStatus {
       InProgress,
       Done,
       Error(String),
   }
   ```

2. Add `browser_ready: bool` flag (initially `false`, set to `true` on
   `BrowserReady` message).

3. Populate stages at the appropriate points in the existing flow:
   - After `CompositorConnection::connect` succeeds → push
     `ConnectingToGui / Done`
   - If connect fails → push `ConnectingToGui / Error`
   - After `send_set_overlay` → push `StartingBrowser / Done`
   - Enter event loop with `WaitingForChromium / InProgress`
   - On `BrowserReady` message → mark `WaitingForChromium / Done`, push
     `LoadingPage / InProgress`, set `browser_ready = true`
   - On first `LoadingState { state: "done" }` → mark `LoadingPage / Done`, push
     `Ready / Done`, clear loading log after a brief delay

4. Update the `ui()` function:
   - Add `loading_log: &[(LoadingStage, StageStatus)]` and `browser_ready: bool`
     parameters
   - When `!browser_ready`, render the loading log inside the viewport area
     instead of the debug text. Each line:
     - `InProgress`: `⠋ {stage description}` in accent color, with elapsed time
       if `WaitingForChromium`
     - `Done`: `✓ {stage description}` in success color
     - `Error`: `✗ {error message}` in danger color
   - When `browser_ready`, render the viewport as before (the browser overlay
     covers it anyway)

5. Add a 30-second warning: if `WaitingForChromium` exceeds 30s, append a muted
   line: `First launch is slow — Chromium is initializing`

6. Add a 120-second timeout: if `WaitingForChromium` exceeds 120s, change status
   to `Error("Timeout — is Roamium installed?")`.

7. Thread an `Instant` through the `WaitingForChromium` stage to compute elapsed
   time for display.

#### No IPC changes needed

All the stage transitions are already observable from existing events:

- GUI connection → happens before the event loop
- SetOverlay sent → happens on first viewport render
- BrowserReady → already an IPC message
- LoadingState → already an IPC message

#### Verification

1. **Fresh install (cold start):**
   - Uninstall and reinstall via Homebrew.
   - Open Wezboard, type `web ryanxcharles.com`.
   - **Pass:** See staged log: connecting, starting, waiting (with elapsed
     time), loading, then browser appears.

2. **Warm start:**
   - Close and reopen `web ryanxcharles.com`.
   - **Pass:** Stages flash quickly, browser appears in a few seconds.

3. **Roamium not installed:**
   - Temporarily rename `/opt/homebrew/opt/termsurf-roamium/roamium`.
   - Run `web ryanxcharles.com`.
   - **Pass:** Log shows error after timeout.

4. **Normal operation after load:**
   - Navigate to other pages after initial load.
   - **Pass:** Loading log does not reappear. Normal TUI behavior.

**Result:** Pass

All stages display with animated spinner, elapsed time, and smooth transitions.
Browser overlay appears automatically after loading completes.

#### Additional fixes during implementation

- **Animated spinner:** 10-frame braille spinner (⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏) cycling at
  100ms via `recv_timeout` during loading, blocking `recv` after.
- **Overlay not appearing until keypress:** The GUI's paint_pass only runs when
  terminal content changes. Fixed by keeping the 100ms polling active until
  `page_loaded` (LoadingState "done"), not just `browser_ready`.
- **2-second grace period:** After page_loaded, keep polling for 2 more seconds
  to ensure the GUI creates and displays the CALayerHost overlay before switching
  to blocking recv.

## Conclusion

The loading screen shows a vertical log of startup stages in the viewport with
status icons (✓ done, ⠋ spinning, ✗ error), elapsed time for the Chromium wait,
a 30-second warning for first launch, and a 120-second timeout. The animated
spinner and auto-redraw use `recv_timeout(100ms)` during loading, switching to
blocking `recv()` after a 2-second grace period post-load for zero idle CPU.
