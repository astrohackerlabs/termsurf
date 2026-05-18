+++
status = "closed"
opened = "2026-02-26"
closed = "2026-02-26"
+++

# Issue 649: Start in Control Mode

## Goal

Open the browser in control mode instead of browse mode. The user should see the
TUI controls on startup, not immediately enter the browser.

## Current state

`tui/src/main.rs:93` initializes the mode:

```rust
let mut mode = Mode::Browse;
```

The first `set_overlay` message to the compositor includes
`mode == Mode::Browse` (`tui/src/main.rs:130`), which tells the GUI to start
forwarding input to Chromium immediately.

## The fix

Change `Mode::Browse` to `Mode::Control` at line 93. The first `set_overlay`
will then send `browsing: false`, so the GUI won't forward input to Chromium
until the user presses Enter to switch to browse mode.

## Experiments

### Experiment 1: Start in control mode

**Goal:** Change the initial TUI mode from Browse to Control.

#### Changes

One change in `tui/src/main.rs:93`. Change:

```rust
let mut mode = Mode::Browse;
```

To:

```rust
let mut mode = Mode::Control;
```

No other changes needed. The first `set_overlay` at line 130 already sends
`mode == Mode::Browse` dynamically, so it will correctly send `browsing: false`
on startup. The status bar, hint bar, and border colors all read from `mode`
directly and will render the Control mode UI.

#### Verification

Run `web <url>`. Confirm:

- The TUI starts in control mode (cyan URL bar border, dim viewport border).
- The status bar shows "CONTROL" with key hints (`<q> quit`, `<i> edit url`,
  `<enter> browse`).
- Keyboard input is NOT forwarded to Chromium.
- Pressing Enter switches to browse mode normally.

**Result: Pass.** TUI starts in control mode.

## Conclusion

The TUI now starts in control mode. One-line change.
