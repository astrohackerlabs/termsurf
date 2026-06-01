# Experiment 135: Port OSC ANSI Palette Operations

## Description

Experiments 133 and 134 established OSC parsing, OSC 0/2 title updates, OSC 7
working-directory reporting, and OSC 8 hyperlink storage. The next OSC slice
should use the terminal color state that already exists in Roastty:
`TerminalColors.palette`.

This experiment ports the ANSI palette portion of Ghostty's OSC color behavior:

- OSC 4 set/query indexed palette colors;
- OSC 104 reset indexed palette colors;
- OSC 104 with no parameters reset all palette colors that differ from the
  default palette.

This is deliberately narrower than Ghostty's full color operation surface.
Ghostty also handles OSC 5 special colors, OSC 10-19 dynamic colors, OSC 110-119
dynamic resets, OSC 21 Kitty color protocol, and surface color-change callbacks.
Roastty does not yet have dynamic/special color state or renderer/app callbacks,
so those are not part of this experiment. Unsupported color OSCs must remain
ignored without mutating terminal state.

## Changes

1. Re-read the current source of truth.
   - Use `vendor/ghostty/src/terminal/osc.zig` and
     `vendor/ghostty/src/terminal/osc/parsers/color.zig` to confirm OSC 4/104
     parse behavior.
   - Use `vendor/ghostty/src/terminal/stream_terminal.zig` to confirm
     terminal-state behavior for palette set/reset.
   - Use `vendor/ghostty/src/termio/stream_handler.zig` only for OSC query
     response formatting; Roastty has no surface message layer yet.
   - Do not modify `vendor/ghostty/`.

2. Add Roastty OSC color data types.
   - Extend `roastty/src/terminal/osc.rs` with private types for ANSI palette
     color operations:
     - operation: OSC 4 or OSC 104;
     - request: set palette index, query palette index, reset palette index, or
       reset all palette entries;
     - response terminator: BEL or ST, matching the OSC terminator used by the
       input sequence.
   - Represent multi-request OSC 4/104 sequences explicitly with a fixed-size
     private request list large enough to hold every valid request that can fit
     inside the existing OSC parser buffer. Do not choose a small arbitrary
     limit such as 32, because that would silently drop valid in-buffer OSC
     4/104 requests.
   - Use a capacity derived from `MAX_BUF`, for example:

     ```rust
     const OSC_COLOR_REQUEST_CAPACITY: usize = MAX_BUF / 2 + 1;

     struct ColorRequests {
         items: [Option<ColorRequest>; OSC_COLOR_REQUEST_CAPACITY],
         len: usize,
     }
     ```

   - Rationale: the densest valid OSC 104 reset body is a sequence of one-digit
     indexes separated by semicolons, so no valid request stream accepted by the
     existing `MAX_BUF` buffer can exceed `MAX_BUF / 2 + 1` requests. OSC 4 set
     and query pairs are less dense because each request needs an index and a
     color/query spec.
   - The list must be copyable or borrowable without heap allocation and must
     fit the existing `osc::Parser` ownership model, where command construction
     returns a command borrowing parser-owned state.
   - The parser must not silently drop valid requests that fit in `MAX_BUF`. If
     an implementation bug or future grammar change ever makes the derived
     request capacity insufficient, treat that as an invalid OSC color command
     and return no command rather than applying a truncated subset.
   - Keep the public stream action shape private to `roastty`.
   - Do not add ABI, app-callback, renderer, or config API surface.

3. Parse OSC 4 palette requests.
   - Parse `OSC 4 ; index ; spec` pairs.
   - Support multiple pairs in one sequence, applied in order.
   - Support palette indices `0..=255`.
   - Support `?` as a query request.
   - Support RGB specs needed for terminal compatibility and tests:
     - `rgb:r/g/b`;
     - `rgb:rr/gg/bb`;
     - `rgb:rrr/ggg/bbb`;
     - `rgb:rrrr/gggg/bbbb`;
     - `#rgb`;
     - `#rrggbb`;
     - `#rrrgggbbb`;
     - `#rrrrggggbbbb`.
   - Match Ghostty's `RGB.fromHex` scaling for every supported channel width:
     parse each channel as 1, 2, 3, or 4 hexadecimal digits and convert it to
     8-bit with `parsed_value * 255 / max_for_width`. This means 4-bit, 12-bit,
     and 16-bit inputs are scaled, not high-byte truncated.
   - Reject invalid pairs without panicking and preserve accumulated valid
     requests before the invalid data, matching Ghostty's "return results up to
     this point" behavior.
   - Do not implement X11 color names in this experiment. If a named color such
     as `red` is encountered, parsing should stop at that point and preserve
     prior valid requests. X11 named colors require a table and should be a
     separate experiment if needed.

4. Parse OSC 104 palette reset requests.
   - `OSC 104` with no parameters becomes reset-all-palette.
   - `OSC 104 ; index ; index ...` resets each valid index.
   - Empty index fields are ignored.
   - Invalid index fields are skipped, matching Ghostty's flexible reset
     behavior.
   - Palette indices remain restricted to `0..=255`; Ghostty's special color
     reset path for indices `256..` is out of scope.

5. Apply palette operations in `TerminalStreamHandler`.
   - Add explicit terminal color plumbing to `roastty/src/terminal/terminal.rs`:
     - destructure `colors` in `Terminal::next_slice`;
     - pass `&mut TerminalColors` into `TerminalStreamHandler`;
     - apply OSC color actions through that handler reference.
   - OSC 4 set mutates `terminal.colors.palette[index]`.
   - OSC 4 query appends a response to `pty_response`.
   - OSC 104 indexed reset restores `color::DEFAULT_PALETTE[index as usize]`.
   - OSC 104 reset-all restores the whole `TerminalColors.palette` to
     `color::DEFAULT_PALETTE`.
   - Multiple requests in one OSC are applied in order. Later sets for the same
     palette index override earlier sets.
   - Unsupported color OSC numbers remain ignored and must not clear or reset
     palette state.

6. Format OSC 4 query responses.
   - Add explicit terminator plumbing between the stream and OSC parser:
     - change `Stream::finish_osc` so it knows whether the OSC ended with BEL or
       ST;
     - pass that terminator into OSC command construction;
     - store the terminator on OSC color query actions;
     - preserve existing title/pwd/hyperlink behavior, which does not need the
       terminator.
   - Use Ghostty's default `osc-color-report-format = "16-bit"` behavior:

     ```text
     ESC ] 4 ; {index} ; rgb:{rrrr}/{gggg}/{bbbb} terminator
     ```

   - Expand 8-bit channels to 16-bit by multiplying each channel by `257`.
   - Preserve the input terminator:
     - BEL-terminated request replies with BEL;
     - ST-terminated request replies with `ESC \`.
   - Append responses to the existing `pty_response` buffer in request order.
   - Do not add a configuration option for 8-bit or disabled reports in this
     experiment.

7. Add tests.
   - Add `osc.rs` parser tests for:
     - OSC 4 set;
     - OSC 4 query;
     - multiple OSC 4 requests;
     - repeated index where the later request wins at terminal level;
     - invalid trailing data preserving prior requests;
     - each supported RGB syntax;
     - unsupported named colors stopping at the named color;
     - OSC 104 indexed reset;
     - OSC 104 reset all;
     - OSC 104 invalid and empty indexes.
   - Add terminal stream tests for:
     - OSC 4 mutates one palette entry;
     - OSC 4 mutates multiple palette entries in order;
     - OSC 4 query writes a 16-bit response with BEL terminator;
     - OSC 4 query writes a 16-bit response with ST terminator;
     - OSC 104 indexed reset restores only the requested palette entry;
     - OSC 104 reset all restores all changed palette entries;
     - unsupported OSC 5/10/11/12/110/111/112 color sequences do not mutate
       palette entries;
     - palette changes affect existing formatter palette output through the
       existing `TerminalFormatterExtra::palette` path.
   - Keep existing OSC title, pwd, hyperlink, SGR, formatter, page, page-list,
     screen, terminal, and ABI tests passing.

8. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty osc
     cargo test -p roastty terminal_stream_osc
     cargo test -p roastty terminal_formatter
     cargo test -p roastty terminal_stream_sgr
     cargo test -p roastty terminal::terminal
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

9. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Fix all real design findings before implementation.
   - Record the design-review outcome in this experiment file before
     implementation.
   - Commit the approved experiment design before implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real result findings before proceeding.
   - Commit the recorded experiment result separately from the design commit.

10. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - the exact OSC 4/104 parse surface implemented;
      - the response format used for queries;
      - unsupported color surfaces left for later;
      - verification command output summary;
      - Codex design-review outcome;
      - Codex result-review outcome.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- OSC 4 set requests mutate indexed palette entries;
- OSC 4 query requests append correct 16-bit palette responses to
  `pty_response`;
- OSC 4 multi-request sequences apply valid requests in order;
- OSC 104 indexed resets restore only the requested entries;
- OSC 104 reset-all restores the full palette to `color::DEFAULT_PALETTE`;
- BEL and ST query terminators are preserved in responses;
- invalid OSC 4 data preserves valid requests parsed before the invalid pair;
- invalid OSC 104 index data is skipped without aborting valid resets;
- unsupported color OSCs remain ignored without mutating palette state;
- existing OSC title, pwd, hyperlink, printed hyperlink, SGR, formatter, page,
  page-list, screen, terminal, and ABI behavior remains intact;
- no public API, public ABI, renderer callback, app callback, config option, PTY
  process behavior, or non-macOS behavior is added;
- `cargo fmt`, targeted tests, and full `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- OSC 4 set/reset work but query response formatting exposes a missing
  configurable report-format prerequisite;
- OSC 4 parsing works for `rgb:` forms but hex shorthand parsing exposes a
  parser ambiguity that should be handled in a smaller follow-up;
- palette mutation works internally, but formatter palette output reveals a
  pre-existing formatter gap unrelated to OSC handling.

The experiment fails if:

- OSC 4 set requests do not mutate `TerminalColors.palette`;
- OSC 104 reset requests reset the wrong palette entries;
- OSC 4 queries produce malformed responses or use the wrong terminator;
- unsupported dynamic/special color OSCs mutate palette state;
- existing OSC 8 hyperlink or SGR behavior regresses;
- the implementation adds public API/ABI or renderer/app callback surface.

## Design Review

Codex reviewed the initial design and found four real design gaps:
`logs/codex-review/20260601-083418-608545-last-message.md`.

The design was updated to:

- require an explicit multi-request color operation representation;
- require BEL/ST terminator plumbing from `Stream::finish_osc` into OSC color
  query actions;
- require explicit `TerminalColors` plumbing into `TerminalStreamHandler`;
- specify Ghostty's exact scaled hex conversion rule for 1-, 2-, 3-, and 4-digit
  color channels.

Codex re-reviewed that update and found one remaining blocker:
`logs/codex-review/20260601-083650-691821-last-message.md`. The fixed request
list could not use an arbitrary small capacity that would drop valid OSC
requests within the existing `MAX_BUF` input limit.

The design was updated again to require a fixed request-list capacity derived
from `MAX_BUF`, large enough to hold every valid OSC 4/104 request that can fit
in the existing OSC parser buffer. If a future grammar change makes that
capacity insufficient, the parser must reject the command rather than apply a
truncated subset.

Codex re-reviewed the final design and found no remaining blocking issues:
`logs/codex-review/20260601-083819-824093-last-message.md`.

The design is approved for implementation.

## Result

**Result:** Pass

Experiment 135 adds private OSC 4/104 ANSI palette color support to Roastty.
`osc.rs` now parses palette color operations into a fixed-size request list
derived from `MAX_BUF`, so every valid color request that can fit in the
existing OSC parser buffer can be represented without heap allocation or silent
truncation.

The implemented OSC 4 surface is:

- set palette entries with `OSC 4 ; index ; spec`;
- query palette entries with `OSC 4 ; index ; ?`;
- parse multiple set/query pairs in one OSC sequence, applying them in order;
- preserve valid requests before trailing invalid data;
- support palette indexes `0..=255`;
- support `rgb:r/g/b`, `rgb:rr/gg/bb`, `rgb:rrr/ggg/bbb`, `rgb:rrrr/gggg/bbbb`,
  `#rgb`, `#rrggbb`, `#rrrgggbbb`, and `#rrrrggggbbbb`;
- scale 1-, 2-, 3-, and 4-digit hex channels with Ghostty's
  `parsed_value * 255 / max_for_width` rule.

The implemented OSC 104 surface is:

- reset individual palette entries with `OSC 104 ; index`;
- reset multiple palette entries in one sequence;
- ignore empty reset fields;
- skip invalid reset fields;
- reset the entire palette to `color::DEFAULT_PALETTE` when OSC 104 has no
  non-empty parameters.

OSC 4 query responses use Ghostty's default 16-bit report format:

```text
ESC ] 4 ; {index} ; rgb:{rrrr}/{gggg}/{bbbb} terminator
```

Roastty expands each 8-bit channel by multiplying by `257`, appends the response
to `pty_response`, and preserves the request terminator: BEL requests receive
BEL responses, and ST requests receive `ESC \` responses.

The terminal stream handler now receives mutable access to `TerminalColors` and
applies palette operations there. Formatter palette output observes the changed
palette through the existing `TerminalFormatterExtra::palette` path.

Unsupported color surfaces intentionally remain ignored in this experiment:

- OSC 5 special colors;
- OSC 10-19 dynamic colors;
- OSC 110-119 dynamic resets;
- OSC 21 Kitty color protocol;
- X11 named color parsing;
- configurable 8-bit or disabled OSC color report formats;
- renderer/app color-change callbacks.

Verification passed:

```bash
cargo fmt
cargo test -p roastty osc
cargo test -p roastty terminal_stream_osc
cargo test -p roastty terminal_formatter
cargo test -p roastty terminal_stream_sgr
cargo test -p roastty terminal::terminal
cargo test -p roastty
```

The final full `cargo test -p roastty` run reported `1487 passed, 0 failed`; the
ABI harness also passed.

Codex reviewed the completed implementation and found one real coverage gap:
`logs/codex-review/20260601-084434-812849-last-message.md`. The implementation
supported `rgb:rrr/ggg/bbb` and `rgb:rrrr/gggg/bbbb`, but the parser tests did
not assert those forms directly.

The missing parser assertions were added, `cargo fmt`,
`cargo test -p roastty osc`, and full `cargo test -p roastty` were rerun
successfully, and Codex re-reviewed the result with no remaining blockers:
`logs/codex-review/20260601-084711-877995-last-message.md`.

## Conclusion

Roastty now supports the ANSI palette subset of Ghostty's OSC color operations:
OSC 4 set/query and OSC 104 indexed/all resets. This builds on the existing
palette storage and formatter path without adding dynamic/special color state,
renderer callbacks, app callbacks, config surface, public API, or ABI.

The remaining OSC color work should be handled in later experiments once the
needed state exists: dynamic colors, special colors, Kitty color protocol,
named-color parsing, report-format configuration, and frontend color-change
notifications.
