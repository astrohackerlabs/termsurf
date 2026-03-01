# Issue 691: DevTools Direct Command

Change the `:devtools` split to launch `web devtools` as the pane's command
instead of typing it into a shell. When DevTools exits, the pane closes
automatically.

## Background

Issue 690 implemented `:devtools [direction]` using `initialInput` — the split
opens a shell, then types `web devtools\n` into it. This works, but the pane
stays open with a shell prompt after DevTools exits. The user has to manually
`:q` the leftover shell.

The better behavior: DevTools IS the pane's process. Close DevTools, close the
pane. No leftover shell.

## Why This Works

The original reason for `initialInput` over `command` was to ensure the shell
environment (`.zshrc`, aliases, `$PATH`) was loaded before `web devtools` ran.
But this concern is moot — `web devtools` is a standalone binary invoked by
absolute path. It doesn't need shell configuration.

The critical dependency is `TERMSURF_PANE_ID`. The `web` TUI reads this
environment variable to connect back to TermSurf via XPC. Without it, the TUI
can't identify which pane it belongs to.

`TERMSURF_PANE_ID` is set automatically for every surface in `Surface.zig`
(line 672) before any process launches — including direct commands. So
`config.command` gets the same environment as `config.initialInput`. No shell
needed.

## Design

Two files change. No new infrastructure needed.

### 1. Swift: Use `command` instead of `initialInput` (`TermSurf.App.swift`)

In `newSplit` (line 844), change:

```swift
// Before (Issue 690 — shell survives after DevTools exits):
config.initialInput = String(cString: pendingInput) + "\n"

// After (DevTools IS the process — pane closes on exit):
config.command = String(cString: pendingInput)
```

The `\n` is no longer needed — `command` executes directly, not as simulated
typing.

### 2. TUI: Remove shell wrapping assumption (`main.rs`)

Currently the TUI sends `"{exe_path} devtools"` as the command string. This
already works with `config.command` because Ghostty passes non-`direct:` command
strings through `/bin/sh -c`, which handles path resolution and argument
splitting.

No change needed in `main.rs` — the command string format is already correct.

## Test

1. Open a browser: `web google.com`
2. `:devtools right` → split opens with DevTools
3. Close DevTools (`:q` in the DevTools pane)
4. The DevTools pane should close automatically — no leftover shell
5. `:devtools left` → reopen without crash
6. Close and reopen 3 times → stable
7. All error cases still work (DevTools-in-DevTools, invalid direction,
   duplicate detection)
