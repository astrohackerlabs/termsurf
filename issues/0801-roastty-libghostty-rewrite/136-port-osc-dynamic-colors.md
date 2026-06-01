# Experiment 136: Port OSC Dynamic Colors

## Description

Experiment 135 ported OSC 4/104 ANSI palette set/query/reset behavior. Ghostty's
same color operation pipeline also supports dynamic terminal colors:

- OSC 10 sets or queries the default foreground color.
- OSC 11 sets or queries the default background color.
- OSC 12 sets or queries the cursor color.
- OSC 110, 111, and 112 reset those three colors to their defaults.

This experiment ports that first dynamic-color slice into Roastty. The target is
the same behavior Ghostty implements for `.foreground`, `.background`, and
`.cursor` in `vendor/ghostty/src/terminal/osc/parsers/color.zig` and
`vendor/ghostty/src/termio/stream_handler.zig`.

Ghostty also has dynamic color targets for pointer, Tektronix, and highlight
colors (OSC 13-19 and OSC 113-119), special colors (OSC 5/105), Kitty's OSC 21
color protocol, X11 named colors, configurable color report format, and surface
color-change messages. Those are intentionally not implemented here. This
experiment should keep them ignored unless adding explicit parser rejection is
required to preserve current behavior.

## Changes

1. Extend Roastty color state in `roastty/src/terminal/color.rs`.

   Add a small Rust equivalent of Ghostty's `DynamicRGB`:
   - `override: Option<Rgb>`
   - `default: Option<Rgb>`
   - `unset()`
   - `init(default: Rgb)`
   - `get() -> Option<Rgb>`
   - `set(rgb)`
   - `reset()`, where reset restores `override` to `default`

   Keep the type private to `roastty`'s terminal internals unless an existing
   public ABI surface already requires exposing it.

2. Extend `TerminalColors` in `roastty/src/terminal/terminal.rs`.

   Add:
   - `foreground: color::DynamicRgb`
   - `background: color::DynamicRgb`
   - `cursor: color::DynamicRgb`

   Initialize them with current Roastty defaults derived from the existing
   palette until the full config color system exists:
   - foreground: `DEFAULT_PALETTE[7]`
   - background: `DEFAULT_PALETTE[0]`
   - cursor: unset

   This matches Ghostty's state shape: cursor may be unset, and query/report
   logic falls back to foreground when cursor has no value.

3. Extend OSC parsing in `roastty/src/terminal/osc.rs`.

   Add dynamic color requests to the existing `ColorRequest` stream:
   - `SetDynamic { target, rgb }`
   - `QueryDynamic { target, terminator }`
   - `ResetDynamic { target }`

   Add a compact `DynamicColor` enum for the three implemented targets:
   - foreground = 10
   - background = 11
   - cursor = 12

   Parse OSC 10/11/12 using Ghostty's sequential behavior:
   - OSC 10 starts at foreground.
   - OSC 11 starts at background.
   - OSC 12 starts at cursor.
   - Empty fields are skipped before processing and do not advance the target,
     matching Ghostty's `std.mem.tokenizeScalar` behavior.
   - Each non-empty semicolon-separated value applies to the current target and
     then advances to the next target if one exists.
   - `?` creates a query for the current target.
   - a valid RGB spec creates a set for the current target.
   - any invalid color spec stops parsing and preserves prior valid requests.
   - extra values after cursor are ignored by ending the request list, matching
     Ghostty's `color.next() orelse return result`.

   Parse OSC 110/111/112 as reset commands when they have no non-empty
   parameters. Trailing empty fields such as `OSC 110;` still reset because
   Ghostty's tokenizer skips them. If any non-empty parameter is present,
   produce no command, matching Ghostty's `parseResetDynamicColor`.

4. Extend color operation execution in `roastty/src/terminal/terminal.rs`.

   Execute the new dynamic requests:
   - set mutates the corresponding `DynamicRgb`
   - reset restores it to its default
   - query writes a 16-bit OSC color response to `pty_response`

   Query format should match Ghostty's default
   `osc-color-report-format = 16-bit` behavior:
   - foreground: `ESC ] 10 ; rgb:rrrr/gggg/bbbb terminator`
   - background: `ESC ] 11 ; rgb:rrrr/gggg/bbbb terminator`
   - cursor: `ESC ] 12 ; rgb:rrrr/gggg/bbbb terminator`

   Preserve the request terminator from the incoming OSC, as Experiment 135 did
   for OSC 4 palette queries.

5. Keep this experiment out of renderer and app integration.

   Do not add surface messages, renderer state APIs, config parsing, ABI
   getters/setters, or color-scheme reload behavior in this experiment. Those
   require broader config/rendering slices and should be designed separately.

## Verification

Run formatting and tests:

```bash
cargo fmt
cargo test -p roastty osc
cargo test -p roastty terminal_stream_osc
cargo test -p roastty terminal_formatter
cargo test -p roastty
```

Add parser tests for:

- OSC 10 set foreground.
- OSC 11 set background and then cursor from multiple values.
- OSC 12 query cursor preserving BEL and ST terminators.
- OSC 10 query foreground then set background then query cursor from one
  sequence.
- OSC 10/11/12 skip empty fields without advancing targets, for example OSC 10
  with an empty first field still applies the next non-empty value to
  foreground.
- OSC 10 with an invalid trailing color preserves prior valid requests.
- OSC 110/111/112 reset with no parameters.
- OSC 110/111/112 with only trailing empty parameters still reset.
- OSC 110/111/112 with any non-empty parameter produces no command.
- OSC 13-19 and OSC 113-119 remain unsupported/ignored.

Add terminal execution tests for:

- OSC 10/11/12 set the correct dynamic color without changing the ANSI palette.
- OSC 110/111 reset foreground/background to their defaults.
- OSC 112 resets cursor to unset.
- OSC 12 query falls back to the current foreground when cursor is unset.
- OSC 12 query reports the cursor override after OSC 12 sets it.
- mixed dynamic set/query sequences execute in order.
- OSC 4/104 palette behavior from Experiment 135 still passes unchanged.

## Pass Criteria

- Dynamic foreground/background/cursor state exists in the terminal color state.
- OSC 10/11/12 set/query behavior matches Ghostty for the implemented targets.
- OSC 110/111/112 reset behavior matches Ghostty for the implemented targets.
- Query responses use 16-bit color components and preserve BEL/ST terminators.
- Cursor query fallback to foreground works when cursor is unset.
- ANSI palette behavior from Experiment 135 is not regressed.
- Unsupported color families remain out of scope and do not silently mutate
  state.

## Failure Criteria

- The implementation changes renderer, ABI, config, or surface-message behavior.
- The implementation treats cursor as permanently defaulted instead of allowing
  it to be unset.
- Dynamic color queries ignore the incoming OSC terminator.
- OSC 10/11/12 multiple-value parsing does not advance through foreground,
  background, and cursor in order.
- OSC 10/11/12 empty fields advance targets or stop parsing instead of being
  skipped.
- OSC 110/111/112 reset commands reject trailing empty fields or accept
  non-empty parameters.
- OSC 4/104 palette tests regress.

## Design Review

Codex reviewed the initial design and found two real issues:

- Ghostty skips empty OSC fields before dynamic color parsing, so OSC 10/11/12
  must skip empty fields without advancing targets.
- Ghostty treats OSC 110/111/112 with only trailing empty fields as reset
  commands, while rejecting resets with non-empty parameters.

The design now pins both behaviors and requires parser coverage for them. Codex
re-reviewed the revised design and approved it for implementation with no
remaining blocking findings.

## Result

**Result:** Pass

Implemented the approved OSC dynamic color slice:

- added `DynamicRgb` state with Ghostty-style `override`/`default` behavior;
- added terminal foreground, background, and cursor dynamic color state;
- parsed OSC 10/11/12 set/query requests with Ghostty's sequential target
  advancement;
- skipped empty dynamic color fields without advancing targets;
- parsed OSC 110/111/112 reset requests when they have no non-empty parameters;
- rejected parameterized dynamic resets with non-empty fields;
- executed dynamic set/reset/query requests in the terminal stream handler;
- wrote 16-bit OSC query responses with the incoming BEL/ST terminator;
- preserved cursor-query fallback to foreground when cursor color is unset; and
- kept renderer, ABI, config, surface-message, OSC 13-19, OSC 113-119, OSC
  5/105, and Kitty OSC 21 behavior out of scope.

Verification:

```bash
cargo fmt -- roastty/src/terminal/color.rs roastty/src/terminal/osc.rs roastty/src/terminal/terminal.rs
cargo test -p roastty osc
cargo test -p roastty terminal_stream_osc
cargo test -p roastty terminal_formatter
cargo test -p roastty
```

Observed results:

- `cargo test -p roastty osc`: 47 passed.
- `cargo test -p roastty terminal_stream_osc`: 22 passed.
- `cargo test -p roastty terminal_formatter`: 67 passed.
- `cargo test -p roastty`: 1499 unit tests passed; ABI harness passed.

## Result Review

Codex reviewed the completed diff and recorded result. It found no blocking
issues and approved the experiment output as a Pass. The review specifically
confirmed the dynamic RGB state shape, OSC 10/11/12 empty-field behavior, OSC
110/111/112 trailing-empty reset behavior, 16-bit query responses, BEL/ST
terminator preservation, cursor-query fallback to foreground, and clean scope.

## Conclusion

Roastty now has Ghostty-style dynamic foreground/background/cursor color state
and OSC 10/11/12 plus OSC 110/111/112 behavior for the implemented targets. The
next OSC color parity work can build on this state to add the remaining color
families deliberately: special colors, extended dynamic colors, Kitty OSC 21,
named color parsing, config-driven defaults, report-format configuration, and
event/renderer integration.
