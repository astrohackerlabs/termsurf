+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

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

## Experiment 1: Switch `initialInput` to `command`

### Hypothesis

If we set `config.command` instead of `config.initialInput` in `newSplit`, the
`web devtools` process becomes the pane's direct child. When DevTools exits, the
PTY closes, and Ghostty's existing child-exit logic closes the pane
automatically. `TERMSURF_PANE_ID` is already in the environment for all
surfaces, so the TUI can still connect via XPC.

### Changes

One line in one file.

#### 1. Swift: `config.command` instead of `config.initialInput` (`TermSurf.App.swift`)

In `newSplit` (line 844), replace:

```swift
config.initialInput = String(cString: pendingInput) + "\n"
```

with:

```swift
config.command = String(cString: pendingInput)
```

No `\n` — `command` is executed directly, not typed into a shell.

No other files change. The TUI already sends `"{exe_path} devtools"` as the
command string, which works with both `initialInput` and `command`.

### Test

1. Open a browser: `web google.com`
2. `:devtools right` → split opens with DevTools
3. Close DevTools (`:q` in the DevTools pane)
4. The DevTools pane should close automatically — no leftover shell
5. `:devtools left` → reopen without crash
6. Close and reopen 3 times → stable
7. All error cases still work (DevTools-in-DevTools, invalid direction,
   duplicate detection)

### Result: SUCCESS

The pane closes automatically when DevTools exits. Two changes were needed
instead of one:

1. **`TermSurf.App.swift`**: `config.command` instead of `config.initialInput`,
   plus `config.waitAfterCommand = false` to override any user config.
2. **`embedded.zig`**: Fixed a bug where `wait_after_command` was only applied
   when `true` — `if (opts.wait_after_command) { config = true; }` never cleared
   an inherited `true` value. Changed to unconditional assignment:
   `config.@"wait-after-command" = opts.wait_after_command`.

## Conclusion

DevTools splits now launch `web devtools` as the pane's direct command instead
of typing it into a shell. When the user quits DevTools (`:q`), the pane closes
immediately — no leftover shell, no "press any key" prompt.

Two files changed from Issue 690's implementation:

- **`TermSurf.App.swift`**: `config.command` replaces `config.initialInput`,
  `waitAfterCommand` forced to `false`.
- **`embedded.zig`**: `wait-after-command` is now unconditionally set from the
  surface options, fixing a pre-existing bug where `false` could never override
  an inherited `true`.
