# Experiment 138: Cursor Default Runtime

## Description

`RUNTIME-009B2B2B3B2B2B2` still groups other remaining terminal behavior
effects. One concrete unproven config-driven terminal effect in that gap is the
live runtime behavior of `cursor-style` and `cursor-style-blink`.

Pinned Ghostty stores both fields in `termio.DerivedConfig`, passes them into
`StreamHandler.default_cursor_style` and `StreamHandler.default_cursor_blink`,
and updates the active stream handler in `StreamHandler.changeConfig`:

- `self.default_cursor_style = config.cursor_style`;
- `self.default_cursor_blink = config.cursor_blink`;
- if the stream is still in the default cursor state, `changeConfig` immediately
  calls `setCursorStyle(.default)`;
- `setCursorStyle(.default)` applies the configured default visual style and
  `cursor-style-blink` value, falling back to blinking when the blink config is
  unset;
- if a program has set a non-default DECSCUSR cursor style, live config updates
  change the stored default only; the visible cursor remains program-controlled
  until the program sends `CSI 0 q` / default DECSCUSR.

Roastty already threads `cursor-style` and `cursor-style-blink` into initial
`TermioSpawnOptions` and has terminal tests for startup defaults, DECSCUSR
reset, and DEC mode 12 gating. However, active surfaces do not currently update
the terminal's stored default cursor style/blink in `Surface::apply_config`, so
live config reload parity is not yet proven.

This experiment will split the remaining terminal row:

- `RUNTIME-009B2B2B3B2B2B2A`: **Oracle complete** for live `cursor-style` and
  `cursor-style-blink` default cursor runtime effects.
- `RUNTIME-009B2B2B3B2B2B2B`: **Gap** for other remaining terminal behavior
  effects.

This experiment will not claim renderer pixel parity for cursor shapes,
password/preedit cursor priority, or broader renderer-visible cursor output.
Those remain owned by renderer runtime rows such as `RUNTIME-008B2B`.

## Changes

- `roastty/src/terminal/terminal.rs`
  - Add terminal-owned runtime update support for default cursor visual style
    and default cursor blink.
  - Track whether the current cursor remains in the default DECSCUSR state, so
    config updates apply immediately only while the cursor is default.
  - Preserve existing DECSCUSR behavior: explicit program cursor styles remain
    active until a default reset request.
  - Preserve existing DEC mode 12 gating when `cursor-style-blink` is explicitly
    configured.
  - Add focused terminal tests proving startup defaults, live default updates,
    non-default DECSCUSR preservation, later `CSI 0 q` reset to the updated
    default, unset blink fallback, and DEC mode 12 gating after update.
- `roastty/src/termio.rs`
  - Add a focused PTY-backed test proving initial `cursor-style` and
    `cursor-style-blink` spawn options still reach the child-backed terminal
    runtime after any terminal changes.
- `roastty/src/lib.rs`
  - Update active surfaces in `Surface::apply_config` so parsed `cursor-style`
    and `cursor-style-blink` changes reach existing terminal runtimes.
  - Add a focused surface/app config test proving startup and live config
    updates change the active terminal default cursor behavior.
- `issues/0805-roastty-ghostty-parity/cursor_default_runtime_parity.py`
  - Add a static guard checking pinned Ghostty markers for derived config,
    `changeConfig`, default cursor state, `setCursorStyle(.default)`, and DEC
    mode 12 gating.
  - Check Roastty markers for parsed config, terminal default cursor update
    state, Termio spawn wiring, surface live config update wiring, focused
    runtime tests, and the inventory split.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-009B2B2B3B2B2B2` into a cursor-default complete row and a
    reduced remaining-terminal gap row.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 summary. It must remain `Gap`.
- Existing CFG-223 static guards that hard-code current runtime row counts or
  the remaining terminal gap row
  - Update expected counts after the split.
  - Update references from `RUNTIME-009B2B2B3B2B2B2` to the reduced remaining
    terminal gap row.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after the result.

## Verification

Pass criteria:

- Pinned Ghostty evidence shows `cursor-style` and `cursor-style-blink` are
  stored on the active stream handler, updated through `changeConfig`, and
  immediately applied only when the cursor is still in the default state.
- Roastty terminal core applies live default cursor updates immediately when the
  cursor is default.
- Roastty terminal core preserves explicit program DECSCUSR cursor style through
  live config update until a default DECSCUSR reset is received.
- `cursor-style-blink = true` and `false` continue to gate DEC mode 12 after a
  live update, while unset blink falls back to blinking.
- Initial app/surface config and live config updates both propagate
  `cursor-style` and `cursor-style-blink` to the active terminal runtime.
- `RUNTIME-009B2B2B3B2B2B2A` is Oracle complete and cites terminal, Termio,
  app/surface, and static guard evidence.
- `RUNTIME-009B2B2B3B2B2B2B` remains `Gap` for other remaining terminal behavior
  effects.
- `CFG-223` remains `Gap`.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml terminal_cursor_default_runtime
cargo test --manifest-path roastty/Cargo.toml termio_cursor_default_runtime
cargo test --manifest-path roastty/Cargo.toml surface_cursor_default_runtime
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/cursor_default_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
cargo fmt --manifest-path roastty/Cargo.toml
cargo fmt --manifest-path roastty/Cargo.toml --check
prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/138-cursor-default-runtime.md
git diff --check
```

Fail criteria:

- The implementation updates stored config but not the active terminal cursor
  state while the cursor is default.
- The implementation overwrites a program-selected explicit DECSCUSR cursor
  during live config update.
- `cursor-style-blink` live updates do not affect later DEC mode 12 gating.
- The surface test only proves startup config and not live config update.
- The experiment promotes renderer cursor pixels, password/preedit priority, or
  unrelated terminal behavior from the remaining gap.
- CFG-223 is marked complete.

## Design Review

**Reviewer:** Codex adversarial subagent with fresh context.

**Verdict:** Approved.

The reviewer found no findings.
