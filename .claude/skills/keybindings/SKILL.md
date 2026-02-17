---
name: keybindings
description: "Track and document all TermSurf keybindings. Use when adding, changing, or removing keybindings in either the GUI (CompositorXPC.swift, AppDelegate.swift) or the TUI (web/src/main.rs). Ensures docs/keybindings.md stays accurate."
---

# Keybindings

Keep `docs/keybindings.md` as the single source of truth for all TermSurf
keybindings.

## Philosophy

TermSurf controls only the keybindings it needs to function. Everything else
falls back to Ghostty defaults or user-configured Ghostty keybindings. We do not
override Ghostty keybindings unless there is a specific TermSurf feature that
requires it.

This means the keybinding surface is small and intentional. Every binding in
`docs/keybindings.md` exists for a reason.

## When This Skill Applies

After any code change that:

- **Adds** a new keybinding (new `KeyCode` match arm, new NSEvent check)
- **Changes** an existing keybinding (different key, different mode, different
  action)
- **Removes** a keybinding
- **Changes mode behavior** (adds a new mode, changes mode transitions)

## Where Keybindings Live

Keybindings are handled in two places:

| Layer   | File                                            | Mechanism             | When                                                       |
| ------- | ----------------------------------------------- | --------------------- | ---------------------------------------------------------- |
| **GUI** | `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | NSEvent local monitor | Before the PTY — intercepts keys the terminal can't encode |
| **TUI** | `web/src/main.rs`                               | crossterm key events  | Inside the terminal — standard key handling via PTY        |

GUI keybindings fire first (NSEvent monitor runs before the responder chain). If
the GUI consumes an event (returns `nil`), the TUI never sees it.

## What to Document

Each keybinding entry in `docs/keybindings.md` must include:

| Field      | Description                                                        |
| ---------- | ------------------------------------------------------------------ |
| **Key**    | The key combination (e.g., `Ctrl+Esc`, `Enter`)                    |
| **Mode**   | Which mode the binding is active in (`Browse`, `Control`, `Any`)   |
| **Action** | What happens when pressed                                          |
| **Notes**  | Implementation details: XPC messages sent, focus checks, key codes |

## Process

When adding or changing a keybinding:

1. **Implement** the keybinding in the appropriate file (GUI or TUI)
2. **Read** `docs/keybindings.md` to see the current state
3. **Update** the correct table (`web TUI keybindings` or `GUI keybindings`)
4. **Update the Modes table** if mode behavior changed
5. **Update Mode synchronization** if the sync protocol changed
6. **Verify** the doc matches the code — every keybinding in code should be in
   the doc, and vice versa

## Conflict Avoidance

Before adding a new keybinding, check:

- Does Ghostty already use this key? If so, our binding will shadow it. Only do
  this if the TermSurf feature is more important in that context.
- Does the key work through the PTY? If not (like Ctrl+Esc in Ghostty), it must
  be handled at the GUI level via NSEvent.
- Is the key mode-specific? Most TermSurf bindings should only be active in a
  specific mode to avoid interfering with normal terminal use.

## Ghostty Fallback Rule

If a key is not explicitly listed in `docs/keybindings.md`, Ghostty handles it.
We never document Ghostty's own keybindings — only TermSurf additions. The doc
links to Ghostty's documentation for everything else.
