# Experiment 148: Port DCS Command Handling

## Description

Port the first command layer above Experiment 147's DCS framing.

Experiment 147 made Roastty recognize and contain DCS strings, but the terminal
runtime still ignores every `DcsHook`, `DcsPut`, and `DcsUnhook` action. Ghostty
has a small DCS command handler in `vendor/ghostty/src/terminal/dcs.zig` that
recognizes:

- DECRQSS (`DCS $ q ... ST`) status-string requests;
- XTGETTCAP (`DCS + q ... ST`) terminfo capability requests;
- tmux control mode (`DCS 1000 p ... ST`) when that feature is enabled.

Roastty can now port the DCS command parser and the DECRQSS terminal responses
whose state already exists. This should not grow into tmux viewer work, terminfo
resource generation, or cursor visual style work. Those are separate subsystem
slices.

## Changes

1. Add `roastty/src/terminal/dcs.rs`.
   - Port Ghostty's DCS handler shape:
     - `Handler`;
     - `Command`;
     - parser state for `inactive`, `ignore`, `xtgettcap`, and `decrqss`;
     - `hook`, `put`, and `unhook` methods.
   - Use Roastty naming and Rust ownership, but preserve Ghostty's behavior:
     - unknown hooks enter ignore until unhook;
     - XTGETTCAP captures payload bytes, uppercases payload on unhook, and
       splits keys on `;`;
     - DECRQSS captures at most two bytes;
     - malformed or over-capacity command payloads discard the command and
       ignore until unhook.
   - Keep `max_bytes` at Ghostty's 1 MiB default for commands with variable
     payloads. Add a test-only constructor or setter for smaller limits.
   - Do not implement tmux control mode in this experiment. Roastty has no tmux
     viewer yet, and Issue 801 is macOS-only but not tmux-ready. Unknown
     `DCS 1000 p` should remain contained and ignored for now.

2. Make `stream::DcsHook` usable by the DCS command handler.
   - Add non-test accessors for intermediates, params, and final byte, or make
     those fields `pub(super)`.
   - Do not expose DCS internals outside the terminal module.

3. Wire `Terminal` to own persistent DCS command state.
   - Add a `dcs::Handler` field to `Terminal`.
   - Pass `&mut dcs::Handler` into `TerminalStreamHandler`.
   - On `Action::DcsHook`, call `dcs.hook(...)`.
   - On `Action::DcsPut`, call `dcs.put(...)`.
   - On `Action::DcsUnhook`, call `dcs.unhook()`.
   - If any of those calls returns a command, dispatch it through a terminal
     helper such as `dcs_command`.
   - Preserve command state across multiple `Terminal::next_slice` calls.

4. Implement DECRQSS responses for terminal state Roastty already has.
   - Support `m` (SGR active attributes):
     - add a dedicated helper for the DECRQSS SGR payload, rather than reusing
       `Style::formatter_vt()` or any existing full VT formatter;
     - format the active cursor text style as Ghostty's `printAttributes`
       payload;
     - start with `0`;
     - append boolean attributes in Ghostty's order: bold, faint, italic,
       underline, blink, inverse, invisible, strikethrough;
     - append foreground color, then background color;
     - use Ghostty's palette/RGB payload forms, for example `;38:5:<idx>` for
       extended palette colors and `;38:2::<r>:<g>:<b>` for RGB.
     - omit unset/default foreground and background colors after the leading
       `0`, matching Ghostty.
   - Support `r` (DECSTBM vertical scrolling region):
     - respond with 1-based `top;bottom r`.
   - Support `s` (DECSLRM horizontal margins):
     - respond with 1-based `left;right s` only when left/right margin mode is
       enabled;
     - respond invalid when left/right margin mode is disabled, matching
       Ghostty.
   - Use Ghostty's response envelope:
     - valid response: `ESC P1$r<payload>ESC \`;
     - invalid or unsupported response: `ESC P0$rESC \`.

5. Defer command surfaces whose dependencies are not present yet.
   - `DECRQSS " q` / DECSCUSR:
     - Ghostty can answer this from `Screen.CursorStyle`.
     - Roastty has text style but not the separate cursor visual style state
       yet, so this experiment must respond invalid and document the deferral.
   - XTGETTCAP terminal responses:
     - the DCS handler should parse and return XTGETTCAP commands;
     - `TerminalStreamHandler` should ignore them for now because Roastty has
       not ported the terminfo source/map/resource layer yet.
   - tmux control mode:
     - no parser/runtime implementation beyond containment in this experiment;
     - `DCS 1000 p` must be treated as an unknown command: no command on hook,
       payload ignored, and no command on unhook.

6. Update module wiring.
   - Add `mod dcs;` in `roastty/src/terminal/mod.rs`.
   - Keep all new code under `roastty/src/terminal/`.
   - Do not add public ABI, app, PTY, renderer, or macOS frontend behavior.

## Verification

1. Run formatting:

   ```bash
   cargo fmt
   ```

2. Run focused DCS command tests:

   ```bash
   cargo test -p roastty terminal_dcs
   cargo test -p roastty dcs_command
   cargo test -p roastty decrqss
   ```

3. Run the full Roastty test suite:

   ```bash
   cargo test -p roastty
   ```

Required test coverage:

- DCS handler unit tests ported from Ghostty:
  - unknown DCS command enters ignore and produces no command;
  - XTGETTCAP single key;
  - XTGETTCAP mixed case uppercases on unhook;
  - XTGETTCAP multiple `;`-separated keys;
  - XTGETTCAP invalid-looking data is still returned uppercased, matching
    Ghostty;
  - DECRQSS `m` maps to SGR;
  - DECRQSS `r` maps to DECSTBM;
  - DECRQSS `s` maps to DECSLRM;
  - DECRQSS ` q` maps to DECSCUSR but remains terminal-runtime unsupported in
    this experiment;
  - invalid DECRQSS payload maps to `none` or ignored state as Ghostty does.
  - tmux `DCS 1000 p` is unknown/ignored in Roastty for this experiment.
- Terminal stream tests:
  - `DCS $ q m ST` returns exactly `ESC P1$r0m ESC \` for default style.
  - SGR response includes active bold/faint/italic/underline/blink/inverse/
    invisible/strikethrough flags in Ghostty's order with exact payload bytes.
  - SGR response omits unset/default foreground and background colors after the
    leading `0`.
  - SGR response includes exact palette foreground/background forms:
    - palette `0..=7`: `;3<idx>` / `;4<idx>`;
    - palette `8..=15`: `;9<idx - 8>` / `;10<idx - 8>`;
    - palette `16..=255`: `;38:5:<idx>` / `;48:5:<idx>`.
  - SGR response includes exact RGB foreground/background forms:
    `;38:2::<r>:<g>:<b>` and `;48:2::<r>:<g>:<b>`.
  - `DCS $ q r ST` returns the current vertical scrolling region with 1-based
    coordinates.
  - `DCS $ q s ST` returns invalid when left/right margin mode is disabled.
  - `DCS $ q s ST` returns the current horizontal margins when left/right margin
    mode is enabled. If Roastty still lacks a public stream path for setting
    DECSLRM, set the mode and region through existing test helpers and document
    that the query response is being tested against existing internal region
    state.
  - `DCS $ q <unsupported> ST` returns exactly `ESC P0$rESC \` and does not
    mutate display content.
  - `DCS $ q <space> q ST` (DECSCUSR) returns exactly `ESC P0$rESC \` and does
    not mutate display content.
  - XTGETTCAP parses at the DCS handler layer but produces no terminal PTY
    response yet; terminal tests must also assert no display mutation, no dirty
    rows, and split-feed XTGETTCAP command state through unhook.
  - DCS command state survives split `Terminal::next_slice` calls.
  - Unknown and over-capacity DCS command payloads remain contained and do not
    leak payload bytes into visible content.

## Non-Negotiable Invariants

- Do not treat `stream::DcsHook` itself as the DCS command parser. Experiment
  147 owns byte framing; this experiment owns Ghostty's command layer above that
  framing.
- Do not implement tmux viewer behavior in this experiment.
- Do not implement a partial terminfo source/resource system just to answer
  XTGETTCAP. That belongs in a later terminfo experiment.
- Do not invent a cursor visual style model inside the DECRQSS implementation.
  DECSCUSR response support must wait until cursor visual style is ported.
- Do not expose new public ABI.
- Do not add `ghostty_*` names. Use Roastty names except when citing upstream
  Ghostty source paths or behavior.
- Run `cargo fmt` and accept its output.

## Failure Criteria

This experiment fails if:

- DCS command state does not survive split `Terminal::next_slice` calls.
- Unknown or malformed DCS command payload leaks into visible terminal content.
- DECRQSS responses use the wrong `DCS {0|1} $ r ... ST` envelope.
- SGR DECRQSS uses the existing VT formatter output directly instead of the
  Ghostty `printAttributes` payload shape.
- SGR DECRQSS includes unset/default foreground or background colors after the
  leading `0`.
- DECSLRM reports a valid horizontal margin response while left/right margin
  mode is disabled.
- Unsupported DECRQSS variants, including DECSCUSR while cursor visual style is
  unported, do anything other than return the invalid DECRPSS response
  `ESC P0$rESC \`.
- XTGETTCAP or tmux-deferred DCS commands write PTY responses or mutate display
  state in this experiment.
- The patch adds tmux viewer behavior, terminfo resource generation, public ABI,
  PTY, renderer, or app/frontend changes.

## Design Review

Codex reviewed the initial design and agreed this is the right next slice after
Experiment 147, but did not approve until these points were pinned down:

- DECRQSS SGR must use a dedicated Ghostty `printAttributes`-style payload
  helper, not the existing VT formatter.
- DECSCUSR must have an explicit terminal-runtime invalid-response test.
- XTGETTCAP runtime deferral must prove no PTY response, no visible mutation, no
  dirty rows, and split-feed state preservation.
- tmux `DCS 1000 p` must be pinned as unknown/ignored in this experiment.
- DECSLRM tests must either use a real CSI setup path or explicitly document the
  current helper-based setup if no stream path exists.
- Unsupported DECRQSS variants must return the invalid DECRPSS envelope, while
  XTGETTCAP/tmux deferrals must produce no PTY response.

Codex reviewed the revised design and approved it with no blocking findings. It
confirmed that the scope is coherent, the deferrals are accurate and testable,
and the verification/failure criteria cover the main risks: split DCS command
state, containment, exact DECRPSS envelopes, SGR payload shape, DECSLRM mode
gating, no-op deferrals, and unrelated subsystem creep.

## Result

**Result:** Pass

Roastty now has a DCS command layer above the DCS byte framing from Experiment
147:

- `terminal::dcs::Handler` recognizes Ghostty-shaped DCS commands for DECRQSS
  and XTGETTCAP.
- Unknown DCS commands and tmux `DCS 1000 p` remain contained and ignored.
- XTGETTCAP payloads are captured, uppercased on unhook, and split by `;` in the
  parser layer, but terminal runtime responses remain deferred until the
  terminfo source/map/resource layer is ported.
- DECRQSS now answers supported terminal state:
  - `m` returns active SGR attributes using a dedicated Ghostty
    `printAttributes`-style payload helper;
  - `r` returns the vertical scrolling region;
  - `s` returns horizontal margins only when left/right margin mode is enabled.
- Unsupported DECRQSS requests, including DECSCUSR while cursor visual style is
  unported, return the invalid DECRPSS response.
- DCS command state is owned by `Terminal` and survives split
  `Terminal::next_slice` calls.

The implementation intentionally does not add tmux viewer behavior, terminfo
XTGETTCAP responses, cursor visual style state, public ABI, PTY, renderer, or
app/frontend behavior.

Verification run:

```bash
cargo fmt
cargo test -p roastty terminal_dcs
cargo test -p roastty dcs_command
cargo test -p roastty decrqss
cargo test -p roastty
```

All tests passed. The full Roastty suite reported 1629 unit tests, 1 ABI harness
test, and 0 doc tests passing.

## Result Review

Codex reviewed the completed implementation and approved it with no blocking
code findings. It confirmed that the DCS parser matches the approved non-tmux
scope, DECRQSS terminal handling is correct for this slice, XTGETTCAP and tmux
remain runtime no-ops, and the tests cover exact SGR payload forms, DECSCUSR
invalid response, XTGETTCAP split-feed no-op behavior, tmux ignored behavior,
DECSLRM mode gating, and split DCS command state.

## Conclusion

Experiment 148 completed the non-tmux DCS command parser and the DECRQSS
responses that can be answered from Roastty's current terminal state. The
remaining DCS-related work is now clearly split into separate future slices:
cursor visual style for DECSCUSR, terminfo resources for XTGETTCAP responses,
and tmux control mode/viewer support.
