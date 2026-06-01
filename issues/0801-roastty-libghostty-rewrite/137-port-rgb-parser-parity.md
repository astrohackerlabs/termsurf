# Experiment 137: Port RGB Parser Parity

## Description

Experiments 135 and 136 routed OSC palette and dynamic color operations through
Roastty's local RGB parser. That parser currently accepts only:

- `rgb:r/g/b` with 1-4 hex digits per component; and
- `#rgb`, `#rrggbb`, `#rrrgggbbb`, `#rrrrggggbbbb`.

Ghostty's shared `terminal.color.RGB.parse` accepts more forms:

- X11 named colors, case-insensitive and with edge spaces trimmed;
- `rgbi:r/g/b`, where each component is a floating-point intensity from `0.0`
  through `1.0`;
- the existing `rgb:` hex forms; and
- the existing `#...` hex forms.

This experiment ports that shared RGB parser behavior to Roastty so OSC 4,
10/11/12, and future color protocols share one parser with Ghostty-compatible
syntax.

## Changes

1. Move RGB parsing into `roastty/src/terminal/color.rs`.

   Add a private `Rgb::parse(bytes: &[u8]) -> Option<Rgb>` helper that owns all
   accepted color syntaxes. Keep `osc.rs` using this helper instead of carrying
   its own parser logic.

2. Preserve existing hash and `rgb:` behavior.

   Keep the exact scaling semantics already ported in Experiment 135:
   - one hex digit: `value * 255 / 15`
   - two hex digits: `value * 255 / 255`
   - three hex digits: `value * 255 / 4095`
   - four hex digits: `value * 255 / 65535`

   Existing OSC 4 and OSC 10/11/12 tests must continue to pass unchanged. Match
   Ghostty's prefix behavior exactly: `rgb:` and `rgbi:` are lowercase-only.
   Uppercase forms such as `RGB:` and `RGBI:` must reject.

3. Add `rgbi:` intensity parsing.

   Parse `rgbi:r/g/b` where each component is a valid Rust `f64` parse result in
   the inclusive range `0.0..=1.0`. Convert to `u8` with Ghostty's truncating
   behavior:

   ```text
   component = intensity * 255
   ```

   Examples:
   - `rgbi:1.0/0/0` -> `rgb(255, 0, 0)`
   - `rgbi:0.5/0.25/0` -> `rgb(127, 63, 0)`

   Reject values below `0.0`, above `1.0`, non-finite values if Rust parsing
   accepts them, missing components, extra components, and non-numeric
   components.

4. Add X11 named color lookup.

   Add a Roastty-owned X11 color map based on Ghostty's
   `vendor/ghostty/src/terminal/res/rgb.txt` source data.

   Requirements:
   - Store the source data under `roastty/src/terminal/res/rgb.txt` or an
     equivalent Roastty source location, not as a runtime dependency on
     `vendor/ghostty`.
   - Add an adjacent attribution/license note explaining that the data comes
     from X11 `rgb.txt`, was copied via the Ghostty vendor source for parity
     research, and is MIT/X11 licensed. A source comment alone is not enough if
     the copied data or generated table needs its own license/provenance note.
   - Keep lookup case-insensitive.
   - Trim only edge spaces, matching Ghostty's `std.mem.trim(u8, value, " ")`.
   - Preserve internal spaces in names, so `medium spring green` works.
   - Preserve the compact-name aliases already present in the X11 data, so
     `mediumspringgreen`, `ForestGreen`, and `FoReStGReen` work.

   The implementation may use a compact static table, a generated match, or a
   lazy parser over the embedded `rgb.txt`, as long as tests prove behavior and
   no filesystem access is required at runtime.

5. Keep color execution behavior unchanged.

   This experiment changes which color specifications parse successfully. It
   must not add new OSC operations, renderer state, ABI functions, config
   behavior, or surface messages.

## Verification

Run formatting and tests:

```bash
cargo fmt
cargo test -p roastty color
cargo test -p roastty osc
cargo test -p roastty terminal_stream_osc
cargo test -p roastty
```

Add color parser tests for:

- existing `rgb:` hex widths: `rgb:f/ff/fff`, `rgb:7f/a0a0/0`;
- existing hash widths: `#fff`, `#ffffff`, `#fffffffff`, `#ffffffffffff`;
- lowercase-only prefixes: `RGB:` and `RGBI:` reject;
- `rgbi:1.0/0/0`;
- fractional intensity truncation, such as `rgbi:0.5/0.25/0`;
- invalid intensity values: negative, above one, non-numeric, missing, extra,
  and non-finite;
- X11 names: `red`, `white`, `black`, `blue`, `medium spring green`,
  `mediumspringgreen`, `ForestGreen`, and `FoReStGReen`;
- edge-space trimming around named colors; and
- tabs or newlines around named colors reject, proving the implementation does
  not use Rust's broader `trim()`;
- invalid names still reject.

Add OSC integration tests for:

- OSC 4 accepts a named color such as `red`;
- OSC 10/11/12 accepts `rgbi:` colors;
- invalid named colors still stop parsing and preserve prior valid color
  requests; and
- unsupported OSC families remain ignored.

## Pass Criteria

- Roastty has one shared RGB parser used by OSC color operations.
- Existing `rgb:` and `#...` behavior from Experiments 135 and 136 is unchanged.
- `rgbi:` intensity parsing matches Ghostty's accepted range and truncating
  conversion.
- X11 named colors work case-insensitively with Ghostty-compatible edge-space
  trimming.
- The X11 data is embedded in Roastty source and does not depend on the vendor
  checkout at runtime.
- The copied X11 data or generated table carries clear MIT/X11
  license/provenance attribution.
- No renderer, ABI, config, surface-message, or new OSC operation behavior is
  added.

## Failure Criteria

- The implementation leaves duplicate RGB parsers in `osc.rs` and `color.rs`.
- Existing OSC 4 or OSC 10/11/12 color parsing regresses.
- `rgbi:` rounds instead of truncating.
- `rgb:` or `rgbi:` accepts uppercase prefixes.
- Named color lookup becomes case-sensitive.
- Named color parsing trims or normalizes internal spaces beyond what Ghostty
  does.
- Named color parsing accepts tabs or newlines as edge whitespace.
- Copied/generated X11 color data lacks MIT/X11 license provenance.
- The implementation reads `vendor/ghostty` or another external file at runtime.
- The experiment drifts into special colors, Kitty OSC 21, config, renderer, or
  ABI work.

## Design Review

Codex reviewed the initial design and found three real issues:

- copied/generated X11 data needs explicit MIT/X11 license provenance, not just
  a source comment;
- `rgb:` and `rgbi:` prefixes must remain lowercase-only, even though X11 names
  are case-insensitive; and
- named-color edge trimming must trim only literal spaces, so tabs/newlines
  around names should reject.

The design now pins those behaviors and requires verification coverage for them.
Codex re-reviewed the revised design and approved it for implementation with no
remaining blocking findings.

## Result

**Result:** Pass

Implemented the approved shared RGB parser parity slice:

- moved RGB parsing into `roastty/src/terminal/color.rs` as `Rgb::parse`;
- routed OSC palette and dynamic color parsing through that shared parser;
- preserved existing `rgb:` and `#...` hex scaling behavior;
- added `rgbi:` intensity parsing with truncating conversion;
- kept `rgb:` and `rgbi:` prefixes lowercase-only;
- embedded X11 `rgb.txt` data under Roastty source;
- added a private X11 named-color lookup with Ghostty-compatible
  case-insensitive matching;
- trimmed only literal edge spaces for named colors;
- added MIT/X11 provenance comments adjacent to the embedded lookup/data; and
- kept special colors, Kitty OSC 21, renderer, config, ABI, and surface-message
  work out of scope.

Verification:

```bash
cargo fmt -- roastty/src/terminal/mod.rs roastty/src/terminal/x11_color.rs roastty/src/terminal/color.rs roastty/src/terminal/osc.rs roastty/src/terminal/terminal.rs
cargo test -p roastty color
cargo test -p roastty osc
cargo test -p roastty terminal_stream_osc
cargo test -p roastty
```

Observed results:

- `cargo test -p roastty color`: 32 passed.
- `cargo test -p roastty osc`: 49 passed.
- `cargo test -p roastty terminal_stream_osc`: 22 passed.
- `cargo test -p roastty`: 1507 unit tests passed; ABI harness passed.

## Result Review

Codex reviewed the completed implementation and recorded result. It found no
blocking issues and approved the experiment output as a Pass. The review
specifically confirmed the centralized parser, OSC routing through it,
lowercase-only `rgb:`/`rgbi:`, truncating `rgbi:` conversion, literal-space-only
X11 edge trimming, embedded X11 data provenance, and test coverage. Codex noted
that the new `x11_color.rs` and `res/rgb.txt` files must be staged with the rest
of the result commit.

## Conclusion

Roastty now has one shared Ghostty-compatible RGB parser for the color syntaxes
used by OSC color operations so far. This unlocks named colors and `rgbi:`
colors for both palette and dynamic color operations without adding new terminal
state. The remaining OSC color work is now more cleanly isolated to missing
operation families, such as OSC 5/105 special colors, extended dynamic colors,
Kitty OSC 21, report-format configuration, and renderer/config integration.
