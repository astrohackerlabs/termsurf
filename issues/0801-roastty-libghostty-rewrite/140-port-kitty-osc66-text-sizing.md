# Experiment 140: Port Kitty OSC 66 Text Sizing

## Description

Experiment 139 added terminal-owned OSC 22 mouse-shape state. The next
self-contained OSC parser slice is Kitty's text sizing protocol, OSC 66:

```text
OSC 66 ; key=value:key=value ; text ST
```

Ghostty parses this command in
`vendor/ghostty/src/terminal/osc/parsers/kitty_text_sizing.zig`. The parser
validates a small parameter set and validates the payload as "escape-code safe
UTF-8." Ghostty's stream layer currently treats the parsed command as an
unimplemented OSC callback rather than applying visible terminal behavior.

This experiment ports that current Ghostty boundary exactly: Roastty should
parse OSC 66 into a typed command, dispatch it through the stream layer, and
ignore it in the terminal runtime without mutating display state, PTY responses,
or app/surface state.

## Changes

1. Add Kitty text sizing types.

   Add terminal-internal types, either in a new module or inside `osc.rs`:
   - `KittyTextSizing`
     - `scale: u8`, default `1`, valid `1..=7`
     - `width: u8`, default `0`, valid `0..=7`
     - `numerator: u8`, default `0`, valid `0..=15`
     - `denominator: u8`, default `0`, valid `0..=15`
     - `valign: KittyTextVerticalAlign`, default `Top`
     - `halign: KittyTextHorizontalAlign`, default `Left`
     - borrowed `text: &'a str`
   - `KittyTextVerticalAlign`
     - `Top`
     - `Bottom`
     - `Center`
   - `KittyTextHorizontalAlign`
     - `Left`
     - `Right`
     - `Center`

   Alignment values follow Ghostty's enum order:
   - vertical: `0 = top`, `1 = bottom`, `2 = center`
   - horizontal: `0 = left`, `1 = right`, `2 = center`

2. Add safe UTF-8 validation.

   Port Ghostty's `vendor/ghostty/src/terminal/osc/encoding.zig::isSafeUtf8`
   behavior for OSC 66 payloads:
   - payload must be valid UTF-8;
   - reject C0 controls `0x00..=0x1f`;
   - reject DEL `0x7f`;
   - reject C1 controls `0x80..=0x9f`.

   Keep this helper terminal-internal unless a future OSC parser also needs it.

3. Parse OSC 66 in `roastty/src/terminal/osc.rs`.

   Add `Command::KittyTextSizing { value }`.

   Parser behavior must match Ghostty:
   - find the first semicolon inside the OSC 66 body; bytes before it are
     parameters and bytes after it are payload;
   - missing semicolon invalidates the command;
   - payload length must be at most `4096` bytes;
   - payload must pass safe UTF-8 validation;
   - empty parameters are allowed;
   - parameter groups split on `:`;
   - each parameter splits on `=`, reads only the first key and first value, and
     ignores any later `=` segments, so `s=2=ignored` behaves like `s=2`;
   - keys must be exactly one byte;
   - unknown keys are ignored;
   - invalid values are ignored;
   - recognized keys are `s`, `w`, `n`, `d`, `v`, and `h`;
   - all values parse with Ghostty's `std.fmt.parseInt(u4, value, 10)` behavior,
     so the range is `0..=15`, a leading `+` is accepted, and negative values
     are rejected by overflow;
   - `s=0` is invalid and leaves default scale;
   - `s` and `w` values must fit `0..=7` after the `u4` parse;
   - `n` and `d` accept `0..=15`;
   - `v` and `h` accept only alignment values `0..=2`;
   - parameters after the first invalid parameter continue parsing.

4. Dispatch through the stream layer.

   Extend the stream test harness so a valid OSC 66 command reaches the handler
   as an OSC action. Add a test proving invalid/unsafe OSC 66 payloads are
   consumed without dispatching an action or leaking text into the print stream.

5. Ignore at terminal runtime.

   Add a `TerminalStreamHandler` match arm that intentionally ignores
   `KittyTextSizing`, matching Ghostty's current unimplemented callback
   behavior. Tests should prove it does not mutate title, PWD, hyperlinks, color
   state, display contents, dirty rows, cursor position, or PTY response.

6. Keep scope limited.

   Do not implement visible text scaling, renderer behavior, font behavior,
   layout behavior, public ABI, app/surface messages, or configuration knobs in
   this experiment.

## Verification

Run formatting and tests:

```bash
cargo fmt
cargo test -p roastty kitty_text_sizing
cargo test -p roastty osc
cargo test -p roastty terminal_stream_osc
cargo test -p roastty
```

Add tests for:

- empty parameters: `66;;bobr` parses with defaults and text `bobr`;
- one parameter: `66;s=2;kurwa`;
- multiple parameters: `66;s=2:w=7:n=13:d=15:v=1:h=2;long`;
- `s=0` leaves default scale;
- invalid parameters such as `w=8:v=3:n=16` leave defaults for those fields but
  still dispatch a command when the payload is valid;
- UTF-8 payloads such as `👻魑魅魍魉ゴースッティ` parse;
- unsafe UTF-8 payloads containing newline, BEL, ESC, DEL, or C1 controls are
  rejected;
- payloads over `4096` bytes are rejected;
- missing payload separator is rejected;
- signed numeric parity:
  - `s=+2` is accepted and sets scale `2`;
  - `s=-2` is rejected and leaves default scale;
- extra `=` parameter segments are ignored, so `s=2=ignored` sets scale `2`;
- stream dispatches valid OSC 66 and suppresses invalid OSC 66;
- terminal runtime ignores valid OSC 66 without mutating unrelated state;
- existing OSC title, PWD, hyperlink, color, Kitty color, and mouse-shape tests
  still pass.

## Pass Criteria

- OSC 66 parses into a typed Kitty text sizing command with Ghostty-compatible
  defaults and validation.
- Safe UTF-8 validation matches Ghostty's C0/DEL/C1 rejection behavior.
- Invalid OSC 66 commands do not dispatch actions or leak bytes into display
  output.
- Terminal runtime ignores valid OSC 66 without mutating unrelated state.
- No visible text sizing, renderer, font, app/surface, public ABI, or config
  behavior is added.
- Existing OSC behavior keeps passing.

## Failure Criteria

- Safe UTF-8 accepts C0, DEL, or C1 controls.
- Leading `+` numeric values are rejected, diverging from Ghostty's `parseInt`
  behavior.
- Negative numeric values are accepted.
- Extra `=` parameter segments reject a parameter instead of being ignored after
  the first value.
- Invalid parameter values invalidate the whole command instead of being ignored
  field-locally.
- Invalid or unsafe OSC 66 payloads dispatch actions.
- Terminal runtime mutates display, title, PWD, hyperlinks, color state, cursor
  position, dirty rows, or PTY response for OSC 66.
- The experiment adds renderer/font/app/surface/public-ABI behavior.
- Existing OSC behavior regresses.

## Design Review

Codex reviewed the initial design and found two real Ghostty parity issues:

- OSC 66 numeric values use Ghostty's `parseInt(u4, ..., 10)` behavior, so
  leading `+` values such as `s=+2` are accepted while negative values reject.
- Extra `=` segments after the first key/value pair are ignored because Ghostty
  reads only the first key and first value from `splitScalar`.

The design now pins both details and adds verification coverage for them. Codex
re-reviewed the revised design and approved it for implementation with no
remaining blocking findings.

## Result

**Result:** Pass

Roastty now parses Kitty OSC 66 text sizing commands into typed OSC actions and
intentionally ignores them at terminal runtime, matching Ghostty's current
unimplemented-callback boundary for this protocol.

The implementation added `KittyTextSizing` with Ghostty-compatible defaults for
scale, width, numerator, denominator, vertical alignment, horizontal alignment,
and text payload. The parser validates safe UTF-8 payloads by rejecting C0
controls, DEL, and C1 controls. It preserves Ghostty's parameter behavior:
invalid parameter values are ignored field-locally, leading `+` numeric values
are accepted, negative values reject, and extra `=` segments after the first
value are ignored.

OSC 66 is wired through the stream dispatcher so valid commands can reach an OSC
handler. The terminal runtime has an explicit no-op arm for the command and does
not mutate display contents, title, PWD, hyperlink state, colors, cursor
position, dirty rows, or PTY response.

Verification passed:

```text
cargo fmt
completed successfully

cargo test -p roastty kitty_text_sizing
5 passed; 0 failed

cargo test -p roastty osc
68 passed; 0 failed

cargo test -p roastty terminal_stream_osc
25 passed; 0 failed

cargo test -p roastty
1531 unit tests passed; 0 failed
1 ABI harness test passed; 0 failed
```

## Result Review

Codex reviewed the completed implementation and result record and found no real
issues. The review confirmed Ghostty-compatible OSC 66 defaults and validation,
safe UTF-8 rejection for C0/DEL/C1 controls, accepted leading `+` numeric
values, rejected negative values, ignored extra `=` segments, stream dispatch
for valid commands, and intentional terminal-runtime no-op behavior.

## Conclusion

Experiment 140 completes the current Ghostty-compatible parser/dispatch slice
for Kitty OSC 66 text sizing. Roastty can now recognize and carry the command
without pretending to implement visible text scaling, font behavior, renderer
behavior, app/surface messages, public ABI, or config.

The next experiment can continue with another remaining OSC/parser subsystem or
move to the next terminal feature slice informed by the current Ghostty parity
gap.
