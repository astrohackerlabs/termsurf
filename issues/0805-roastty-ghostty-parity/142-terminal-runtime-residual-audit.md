# Experiment 142: Terminal Runtime Residual Audit

## Description

`RUNTIME-009B2B2B3B2B2B2B3` is now the only remaining terminal-family CFG-223
gap, but it is vague: "other remaining terminal behavior effects." Experiments
117, 122, 124, 126-131, and 135-140 have already split out the concrete
config-driven terminal behaviors found in pinned Ghostty's `Termio.zig` and
`stream_handler.zig` paths.

This experiment will audit the residual terminal-runtime bucket against the
pinned Ghostty source and either:

- close the residual row if every config-driven terminal behavior in the pinned
  Ghostty termio/stream-handler path is already represented by an
  oracle-complete inventory row; or
- replace the vague residual row with one or more concrete follow-up rows for
  any terminal config behavior still lacking runtime proof.

The scope is terminal-runtime only. Font renderer output, compositor/window
pixels, macOS app UI, native notifications, bell UI/audio, link previews, and
context-menu/link GUI flows stay in their existing non-terminal CFG-223 gaps.

## Changes

- `issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py`
  - Add a static guard that reads pinned Ghostty `Termio.zig` and
    `stream_handler.zig`, identifies the config-derived terminal fields used by
    that path, and asserts each is covered by a named oracle-complete runtime
    inventory row.
  - Enumerate every pinned Ghostty `DerivedConfig` field plus direct
    `opts.full_config` or `opts.config` terminal-runtime use, and map each one
    to an oracle-complete terminal row or a documented non-terminal row.
  - Assert the covered fields include the known Ghostty stream-handler config
    inputs: `osc_color_report_format`, `clipboard_write`, `enquiry_response`,
    `cursor_style`, and `cursor_blink`.
  - Assert Ghostty terminal initialization/config-derived paths for scrollback,
    terminal identity/shell integration, title/PWD behavior, Kitty image
    storage, and grapheme width are represented by the existing completed rows.
  - Assert that the script does not count renderer, font, macOS app,
    notification/link GUI, or bell presentation gaps as terminal-runtime
    closure.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - If the audit finds no uncovered terminal config behavior, mark
    `RUNTIME-009B2B2B3B2B2B2B3` as `Oracle complete` with evidence from the new
    guard and explain that remaining CFG-223 gaps are non-terminal.
  - If the audit finds a real uncovered terminal behavior, split the residual
    row into concrete rows instead of closing it.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 counts from the inventory script.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning recording whether the broad terminal residual row was closed
    or split and why.

No Roastty source code should change in this experiment. If the audit finds a
concrete terminal-runtime parity bug, record it as a concrete remaining row and
leave the implementation for the next experiment.

## Verification

Pass criteria:

- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
- `prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/142-terminal-runtime-residual-audit.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
- `git diff --check`

The experiment passes only if the residual terminal row is no longer vague:
either the guard proves all pinned Ghostty config-driven terminal-runtime fields
are covered by completed rows, or the inventory records the exact uncovered
terminal behavior that remains. CFG-223 may still remain a gap because the font,
renderer, macOS app, and notification/link GUI rows are outside this experiment.

## Design Review

Fresh-context adversarial design review initially returned **Changes required**:

- the first design allowed small Roastty source fixes but did not require Rust
  formatting or focused Rust tests if that happened;
- the reviewer also suggested making the audit manifest explicitly exhaustive
  over every pinned Ghostty `DerivedConfig` field and direct `opts.full_config`
  or `opts.config` terminal-runtime use.

The design was updated to forbid Roastty source changes in this experiment and
to require an exhaustive mapping of pinned Ghostty derived/config terminal uses
to completed terminal rows or documented non-terminal rows.

Re-review returned **Approved** with no required findings.

## Result

**Result:** Pass

The vague terminal residual row is now closed by audit. The new
`terminal_runtime_residual_audit.py` guard enumerates pinned Ghostty
`DerivedConfig`, direct `opts.full_config` and `opts.config` terminal-runtime
uses, and stream-handler config update paths, then maps them to completed
runtime rows. The audit covers color/palette state, scrollback, shell
integration, title/PWD behavior, OSC 7 edge behavior, ENQ replies, OSC color
query formatting, primary device-attributes clipboard capability,
cursor-style/cursor-blink defaults, Kitty image storage limits, and
grapheme-width default mode behavior.

No Roastty source code changed. CFG-223 remains a gap overall, but the remaining
gaps are no longer terminal-runtime gaps; they are font renderer output/live
grid effects, renderer-visible GUI/pixel effects, macOS
app/window/tab/split/menu UI, and native notification/link/bell presentation
flows.

Verification completed:

- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py`
  — pass.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  — pass: `runtime_rows=50`, `oracle_complete=44`, `closed=46`, `incomplete=4`,
  `gap=4`, `cfg223=Gap`.
- `prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/142-terminal-runtime-residual-audit.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
  — pass.
- `git diff --check` — pass.

## Conclusion

The terminal-family residual was not a hidden runtime toggle; it was an
under-specified audit bucket. The pinned Ghostty termio config paths now have
explicit row-level coverage, and the next Issue 805 experiment should move to
one of the four remaining non-terminal CFG-223 gaps.

## Completion Review

Fresh-context adversarial completion review initially returned **Changes
required**:

- the audit checked that pinned Ghostty `DerivedConfig` contained
  `conditional_state`, but did not prove the field's terminal-runtime use in
  `colorSchemeReportLocked` or require the mapped `RUNTIME-006` row to mention
  theme/color-scheme evidence.

The guard was strengthened to assert `colorSchemeReportLocked`,
`self.config.conditional_state.theme`, the light/dark DSR outputs, and
`RUNTIME-006` coverage for `color, palette`, `theme`, and `color-scheme`.

Re-review returned **Approved**. The reviewer confirmed the prior finding was
resolved, found no new required issues, and independently reran the terminal
audit guard and `git diff --check`.
