+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
+++

# Experiment 60: Phase F — cursor default config

## Description

Experiment 59 deliberately left cursor config out of the mouse behavior slice.
The next narrow Phase-F slice is the cursor default group that Ghostty routes
through Termio and the renderer:

- `cursor-style`
- `cursor-style-blink`
- `cursor-opacity`

Roastty already exposes terminal cursor visual style, cursor blinking, and
renderer cursor overlay drawing, but the defaults are hardcoded: terminal
construction uses a block cursor with mode defaults, `DECSCUSR 0`/blank resets
to a hardcoded steady block, and live/frame rendering currently uses opaque
cursor overlay alpha. This experiment represents the three fields on aggregate
config and routes them into the existing terminal and renderer paths.

This experiment intentionally excludes:

- `cursor-color` / `cursor-text`, which already exist as config fields and are
  mostly renderer-color work rather than cursor default style work;
- `cursor-click-to-move`, which requires prompt-click behavior and shell
  integration semantics;
- `mouse-hide-while-typing`, which requires app/runtime mouse visibility
  ownership;
- renderer focus-state policy beyond the existing focused render path. Upstream
  keeps unfocused hollow cursors fully opaque; Roastty's live frame path does
  not yet thread focus through cursor overlay alpha.

## Changes

- `roastty/src/config/mod.rs`
  - Add upstream defaults:
    - `cursor-opacity = 1.0`
    - `cursor-style = block`
    - `cursor-style-blink = null` / unset
  - Add/route a cursor style enum matching upstream keywords:
    - `block`
    - `bar`
    - `underline`
    - `block_hollow`
  - Route all three fields through `Config::set`, `format_config`, CLI/file
    loading, clone/equality, and diagnostics.
  - Preserve formatter order around upstream's cursor group: `cursor-color`,
    `cursor-opacity`, `cursor-style`, `cursor-style-blink`, `cursor-text`.
  - Keep `cursor-opacity` parse permissive like upstream and clamp at the use
    site rather than failing parse for values outside `[0, 1]`.
- `roastty/src/terminal/terminal.rs`
  - Add terminal initialization options for default cursor visual style and
    default cursor blink.
  - Initialize `Mode::CursorBlinking` from `cursor-style-blink.unwrap_or(true)`.
  - Initialize the active screen cursor visual style from `cursor-style`.
  - Preserve upstream's DEC mode 12 rule: if `cursor-style-blink` is explicitly
    configured (`Some(true)` or `Some(false)`), `DECSET 12` / `DECRST 12` must
    not mutate `Mode::CursorBlinking`; if it is unset, DEC mode 12 remains
    respected.
  - Make `DECSCUSR` default (`CSI q` / `CSI 0 q`) reset to the configured
    default cursor style and `cursor-style-blink.unwrap_or(true)` instead of the
    current hardcoded steady block.
  - Preserve explicit `DECSCUSR 1..6` behavior.
- `roastty/src/termio.rs`
  - Extend `TermioSpawnOptions` with the cursor defaults needed by terminal
    initialization.
  - Keep existing tests/callers on faithful defaults when they do not provide
    cursor options.
- `roastty/src/lib.rs`
  - Pass the finalized app parsed config cursor defaults into
    `Termio::spawn_with_options` when a surface starts.
  - Keep existing surface/app config update behavior bounded: changing config
    should affect newly-started terminals and future renderer frames, but this
    experiment does not retroactively mutate an already-running terminal's
    current cursor style/blink state.
  - Ensure the live present path renders with the app's parsed config instead of
    `Config::default()` so cursor opacity and other config-derived render knobs
    are visible in the live app.
- `roastty/src/renderer/frame_renderer.rs`
  - Add a cursor overlay alpha knob sourced from
    `ceil(clamp(cursor-opacity, 0, 1) * 255)`.
  - Use that knob only for cursor overlay drawing. Non-cursor text alpha remains
    the existing opaque `255`, and faint/background opacity behavior remains
    unchanged.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add this experiment to the index as `Designed`.
  - After implementation, add any durable operating note for cursor default
    config.

## Verification

- Run formatting:
  - `cargo fmt -- roastty/src/config/mod.rs roastty/src/terminal/terminal.rs roastty/src/termio.rs roastty/src/lib.rs roastty/src/renderer/frame_renderer.rs`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/60-cursor-default-config.md`
- Run targeted tests:
  - `cargo test -p roastty cursor_default_config`
  - `cargo test -p roastty cursor_style_config`
  - `cargo test -p roastty cursor_opacity`
  - `cargo test -p roastty cursor_style_blink`
  - `cargo test -p roastty config_format_config`
  - `cargo test -p roastty terminal_stream_decscusr`
  - `cargo test -p roastty surface_start`
  - `cargo test -p roastty render_frame`
- Add concrete test cases proving:
  - config defaults match upstream (`cursor-opacity = 1.0`,
    `cursor-style = block`, `cursor-style-blink =` blank/unset);
  - all cursor-style keywords parse and format, and invalid values diagnose;
  - `cursor-opacity` round-trips raw values and clamps only in renderer knobs;
  - `cursor-style-blink` accepts unset, `true`, and `false`, and invalid values
    diagnose;
  - terminal initialization from config sets visual style and blinking defaults;
  - explicit `cursor-style-blink = true/false` disables DEC mode 12 cursor blink
    mutation, while unset `cursor-style-blink` leaves DEC mode 12 honored;
  - `DECSCUSR` default resets to the configured default style/blink, while
    explicit `DECSCUSR 1..6` still override both;
  - `Termio::spawn_with_options` carries cursor defaults into the started
    terminal;
  - `FrameRenderKnobs::from_config` turns cursor opacity into cursor overlay
    alpha without changing normal text alpha or faint opacity;
  - the live present path uses app parsed config rather than `Config::default()`
    for frame rendering.
- Run full Roastty tests:
  - `cargo test -p roastty`
- Run `git diff --check`.
- Run `git status --short` and verify only intended source/docs are present.

**Pass** = the three cursor default fields are represented on `Config`,
round-trip through config loading/formatting, initialize new terminal sessions
with configured cursor style/blink defaults, reset `DECSCUSR` defaults to the
configured values, and feed cursor opacity into renderer cursor overlay alpha;
targeted and full tests pass.

**Partial** = config representation and either terminal defaults or renderer
opacity land, but one side exposes a bounded prerequisite in terminal
initialization or live renderer config ownership.

**Fail** = current terminal/render ownership cannot safely route the fields
without a broader Termio or renderer-state refactor.

## Design Review

Reviewed by Codex adversarial reviewer (`Socrates`,
`019eb365-d10e-7372-94e3-5cb5d7779e48`) with fresh context.

**Initial verdict:** Changes required.

- **Required:** The original design omitted upstream's `cursor-style-blink`
  interaction with DEC mode 12. In Ghostty, when `cursor-style-blink` is
  explicitly configured, `DECSET 12` / `DECRST 12` must not mutate cursor
  blinking; when the option is unset, DEC mode 12 remains honored.

Fix:

- Added the DEC mode 12 rule to the terminal changes.
- Added explicit verification that configured `cursor-style-blink = true/false`
  disables DEC mode 12 blinking mutation, while unset `cursor-style-blink`
  leaves DEC mode 12 honored.

**Final verdict:** Approved.

No findings.

## Result

**Result:** Pass

Implemented the cursor default config slice:

- Added `cursor-opacity`, `cursor-style`, and `cursor-style-blink` to aggregate
  config, formatting, parsing, defaults, and diagnostics. `cursor-style` maps
  the upstream keywords `block`, `bar`, `underline`, and `block_hollow` to the
  terminal cursor visual styles; `cursor-style-blink` remains unset by default.
- Routed cursor style/blink defaults into terminal initialization through
  `TermioSpawnOptions` and `TerminalInitOptions`.
- Preserved upstream's DEC mode 12 behavior: when `cursor-style-blink` is
  explicitly configured, in-band `DECSET 12` / `DECRST 12` do not mutate cursor
  blinking; when unset, DEC mode 12 remains honored.
- Changed `DECSCUSR` default reset (`CSI q` / `CSI 0 q`) to restore the
  configured default style and `cursor-style-blink.unwrap_or(true)`, while
  keeping explicit `DECSCUSR 1..6` behavior intact.
- Threaded app parsed config into live renderer frame rendering, and used
  `cursor-opacity` for cursor overlay alpha only. Non-cursor overlay/text alpha
  remains opaque.
- Updated the terminal mode default for DEC mode 12 to default-on, matching the
  configured default blinking semantics and avoiding spurious `?12h` mode
  formatter output for fresh terminals.

Verification run:

- `cargo fmt -- roastty/src/config/mod.rs roastty/src/terminal/terminal.rs roastty/src/terminal/stream.rs roastty/src/terminal/modes.rs roastty/src/termio.rs roastty/src/lib.rs roastty/src/renderer/frame_renderer.rs roastty/src/renderer/frame_rebuild.rs`
- `cargo test -p roastty cursor_default_config`
- `cargo test -p roastty cursor_style_config`
- `cargo test -p roastty cursor_opacity`
- `cargo test -p roastty cursor_style_blink`
- `cargo test -p roastty config_format_config`
- `cargo test -p roastty terminal_stream_decscusr`
- `cargo test -p roastty surface_start`
- `cargo test -p roastty render_frame`
- `cargo test -p roastty --test abi_harness`
- `cargo test -p roastty`
- `git diff --check`

Final full result:

- `cargo test -p roastty`: 4492 Rust tests passed, ABI harness passed, doc-tests
  passed.
- The ABI harness emitted its existing enum-conversion warnings in
  `abi_harness.c`; they did not fail the harness.

## Conclusion

The cursor default config group is now represented on `Config`, routed into new
terminal sessions, respected by `DECSCUSR` default resets and DEC mode 12, and
visible in renderer cursor opacity. The next Phase-F slice can continue with the
remaining config groups rather than revisiting cursor style/blink plumbing.

## Completion Review

Reviewed by Codex adversarial reviewer (`Linnaeus`,
`019eb37c-3708-76c3-ae73-5682bb29519f`) with fresh context.

**Final verdict:** Approved.

No findings.
