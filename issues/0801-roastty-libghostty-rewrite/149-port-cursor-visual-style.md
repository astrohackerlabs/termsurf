# Experiment 149: Port Cursor Visual Style

## Description

Port Ghostty's separate cursor visual style state into Roastty and use it to
complete DECSCUSR support.

Roastty already has text `style::Style` on the cursor, but Ghostty keeps cursor
shape as a distinct visual state in `terminal/cursor.zig` and
`Screen.Cursor.cursor_style`. Experiment 148 deliberately left DECRQSS DECSCUSR
(`DCS $ q <space> q ST`) unsupported because this separate cursor visual style
did not exist yet.

This experiment ports the cursor visual style layer:

- parse `CSI Ps SP q` / DECSCUSR;
- update cursor blink mode and visual shape at runtime;
- report DECSCUSR through the existing DECRQSS response path.

It should not add renderer behavior, public ABI, app integration, or custom
Ghostty-only cursor styles beyond the state needed for terminal parity.

## Changes

1. Add `roastty/src/terminal/cursor.rs`.
   - Port Ghostty's cursor visual style enum using Roastty naming:
     - `Bar`;
     - `Block`;
     - `Underline`;
     - `BlockHollow`.
   - `Block` is the default.
   - Add a helper that maps `(visual style, cursor blinking mode)` to the
     DECSCUSR report integer:
     - block or block-hollow: `1` if blinking, `2` if steady;
     - underline: `3` if blinking, `4` if steady;
     - bar: `5` if blinking, `6` if steady.
   - `BlockHollow` is included for parity with Ghostty's state model, but this
     experiment does not add any parser path that sets it.
   - Add `mod cursor;` in `roastty/src/terminal/mod.rs`.

2. Extend stream actions with DECSCUSR parsing.
   - Add a stream action such as `Action::CursorVisualStyle { style, blinking }`
     or equivalent.
   - Parse only `CSI Ps SP q`, where the intermediate byte is exactly space
     (`0x20`) and final byte is `q`.
   - Match Ghostty's accepted forms:
     - no params or param `0`: default cursor style;
     - `1`: blinking block;
     - `2`: steady block;
     - `3`: blinking underline;
     - `4`: steady underline;
     - `5`: blinking bar;
     - `6`: steady bar.

- Reject private forms such as `CSI ? 0 SP q`.
- Reject non-space or repeated-intermediate forms such as `CSI 1 ! q`,
  `CSI 1 $ q`, and `CSI 1 SP SP q`.
- Reject missing-space forms such as `CSI q` and `CSI 1 q`.
- Reject multiple params, colon/semicolon separators, and values outside
  `0..=6`.
- Keep `CSI ? Ps $ p` mode-report parsing intact. The current stream parser has
  a fast path for any intermediate byte; this experiment must allow the
  space-intermediate DECSCUSR path without regressing the dollar-intermediate
  mode-report path.

3. Store cursor visual style in `Screen`.
   - Add a cursor visual style field beside the existing cursor text style.
   - Default it to block.
   - Add internal accessors for terminal runtime and tests.
   - Do not conflate cursor visual style with `style::Style`.

4. Wire terminal runtime behavior.
   - On the DECSCUSR action:
     - set `Mode::CursorBlinking` according to the parsed style;
     - set the screen cursor visual style to block, underline, or bar.
   - Default / `0` should set steady block and clear cursor blinking, matching
     Ghostty's runtime mapping.
   - The action should not mutate display cells, dirty rows, PTY responses, text
     style, cursor position, or selection state.

5. Update DECRQSS DECSCUSR response.
   - Change `dcs::Decrqss::Decscusr` terminal handling from invalid to valid.
   - Return the Ghostty DECRPSS envelope:
     - `ESC P1$r<value> q ESC \`
   - Use the current cursor visual style plus `Mode::CursorBlinking` to compute
     `<value>`.

6. Keep scope narrow.
   - Do not add renderer/C ABI accessors for cursor visual style.
   - Do not add app/frontend cursor rendering behavior.
   - Do not add custom parser support for `BlockHollow`; it remains a state
     value that future renderer/app slices may observe or set through
     configuration.

## Verification

1. Run formatting:

   ```bash
   cargo fmt
   ```

2. Run focused tests:

   ```bash
   cargo test -p roastty cursor_visual
   cargo test -p roastty decscusr
   cargo test -p roastty decrqss
   ```

3. Run the full Roastty test suite:

   ```bash
   cargo test -p roastty
   ```

Required test coverage:

- Cursor visual style unit tests:
  - default is block;
  - report mapping returns `1/2` for block and block-hollow, `3/4` for
    underline, and `5/6` for bar based on blink state.
- Stream parser tests:
  - `CSI SP q` dispatches default / steady block;
  - `CSI 0 SP q` dispatches default / steady block;
  - `CSI 1 SP q` dispatches blinking block;
  - `CSI 2 SP q` dispatches steady block;
  - `CSI 3 SP q` dispatches blinking underline;
  - `CSI 4 SP q` dispatches steady underline;
  - `CSI 5 SP q` dispatches blinking bar;
  - `CSI 6 SP q` dispatches steady bar;
  - `CSI ? 0 SP q`, `CSI q`, `CSI 1 q`, `CSI 1;2 SP q`, `CSI 1:2 SP q`, and
    `CSI 7 SP q` dispatch nothing and do not leak final bytes.
  - `CSI > 1 SP q` and `CSI = 1 SP q` dispatch nothing and do not leak final
    bytes.
  - `CSI 1 ! q`, `CSI 1 $ q`, and `CSI 1 SP SP q` dispatch nothing and do not
    leak final bytes.
  - Split-feed DECSCUSR inside the CSI sequence, such as `ESC [ 5` followed by
    `SP q`, preserves parser state and dispatches blinking bar.
  - `CSI ? 7 $ p` mode report still works after adding the space-intermediate
    path.
- Terminal runtime tests:
  - DECSCUSR sets visual style and cursor blinking mode for block, underline,
    and bar.
  - default / `0` sets steady block and clears cursor blinking.
  - DECSCUSR does not mutate visible content, dirty rows, PTY responses, cursor
    text style, or cursor position.
- DECRQSS tests:
  - default cursor returns `ESC P1$r2 q ESC \`.
  - blinking block returns `ESC P1$r1 q ESC \`.
  - steady underline returns `ESC P1$r4 q ESC \`.
  - blinking bar returns `ESC P1$r5 q ESC \`.
  - split-feed DECSCUSR followed by DECRQSS reports the updated visual style.

## Non-Negotiable Invariants

- Cursor visual style is separate from text `style::Style`.
- Do not use the SGR/text-style formatter for cursor visual style.
- Do not regress `CSI ? Ps $ p` mode-report parsing.
- Do not expose new public ABI or renderer/app behavior.
- Do not add `ghostty_*` names. Use Roastty names except when citing upstream
  Ghostty source paths or behavior.
- Run `cargo fmt` and accept its output.

## Failure Criteria

This experiment fails if:

- DECSCUSR without the space intermediate is accepted.
- private, multi-param, colon/semicolon, or out-of-range DECSCUSR forms are
  accepted.
- non-space or repeated-intermediate DECSCUSR-like forms are accepted.
- DECSCUSR changes text style, display cells, cursor position, dirty rows, or
  PTY responses.
- DECRQSS DECSCUSR still returns the invalid response after cursor visual style
  is ported.
- Cursor blink state and visual style disagree with Ghostty's mapping.
- Existing mode-report parsing for `CSI ? Ps $ p` regresses.
- The patch adds renderer, app/frontend, public ABI, PTY, terminfo, tmux, or
  browser overlay behavior.

## Design Review

Codex reviewed the initial design and agreed this is the right next slice after
Experiment 148, but requested tighter parser-edge coverage before approval:

- explicit negative tests for non-space and repeated intermediates;
- broader private-form rejection for `?`, `>`, and `=`;
- a split-feed DECSCUSR test inside the CSI sequence;
- an explicit module-wiring checklist item for `mod cursor;`.

The design was updated with those requirements. Pending follow-up Codex review.

Codex reviewed the revised design and approved it with no findings. It confirmed
that the verification now covers exact `CSI Ps SP q` handling,
non-space/repeated-intermediate rejection, broader private-form rejection,
split-feed DECSCUSR, module wiring, runtime state mapping, DECRQSS payloads, no
unwanted terminal mutations, and preservation of `CSI ? Ps $ p`.

## Result

**Result:** Pass

Experiment 149 ported cursor visual style state into Roastty and completed
DECSCUSR handling.

Implemented changes:

- Added `roastty/src/terminal/cursor.rs` with `VisualStyle` values for bar,
  block, underline, and block-hollow, plus DECSCUSR report mapping.
- Registered the new cursor module in `roastty/src/terminal/mod.rs`.
- Added separate cursor visual style storage to `ScreenCursor`, leaving cursor
  text `style::Style` as a separate field.
- Added stream parsing for exact `CSI Ps SP q` DECSCUSR forms.
- Rejected missing-space, private, multi-param, colon/semicolon, out-of-range,
  non-space-intermediate, and repeated-intermediate forms without leaking final
  bytes.
- Preserved `CSI ? Ps $ p` mode-request parsing after broadening CSI
  intermediate-byte handling.
- Wired runtime DECSCUSR behavior to set `Mode::CursorBlinking` and the screen
  cursor visual style.
- Changed DECRQSS DECSCUSR from invalid to valid, reporting the current cursor
  visual style and blink state as `ESC P1$r<value> q ESC \`.

Verification commands:

```bash
cargo fmt
cargo test -p roastty cursor_visual
cargo test -p roastty decscusr
cargo test -p roastty decrqss
cargo test -p roastty
```

Verification results:

- `cargo test -p roastty cursor_visual`: 5 passed, 0 failed.
- `cargo test -p roastty decscusr`: 9 passed, 0 failed.
- `cargo test -p roastty decrqss`: 12 passed, 0 failed.
- `cargo test -p roastty`: 1639 unit tests passed, 1 ABI harness test passed, 0
  doc tests.

## Conclusion

Roastty now has Ghostty-parity cursor visual style state for DECSCUSR and
DECRQSS reporting. Cursor shape is separate from text styling, DECSCUSR does not
mutate terminal content or PTY responses, and the existing dollar-intermediate
mode-report parser still works.

The next experiment can move to another terminal-control slice that depends on
cursor state, or continue through the remaining DCS/query surface based on the
current upstream parity gap.

## Result Review

Codex reviewed the completed implementation, issue result record, diff, new
cursor module, and verification summary. It reported no findings and confirmed
that the implementation matches the approved design, preserves mode-request
parsing, keeps cursor visual style separate from text style, updates only cursor
visual state plus blink mode at runtime, and records accurate verification
results.
