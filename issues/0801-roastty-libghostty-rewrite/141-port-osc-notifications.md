# Experiment 141: Port OSC Notifications

## Description

Experiments 138-140 advanced the OSC parser through Kitty colors, mouse shape,
and Kitty text sizing. The next self-contained OSC slice is desktop notification
parsing:

- iTerm2-style `OSC 9 ; body ST`
- rxvt-style `OSC 777 ; notify ; title ; body ST`

Ghostty parses these in:

- `vendor/ghostty/src/terminal/osc/parsers/osc9.zig`
- `vendor/ghostty/src/terminal/osc/parsers/rxvt_extension.zig`

Ghostty's termio layer forwards notifications to the app/surface message
boundary. Roastty does not yet have that boundary, so this experiment ports the
terminal parser/stream recognition and intentionally ignores notifications at
terminal runtime. This matches the current Roastty pattern for terminal-parsed
surface/app effects that have no Rust app boundary yet.

Do not include ConEmu progress reports, ConEmu sleep/message box/tab-title
commands, ConEmu xterm emulation, clipboard, or semantic prompt handling in this
experiment. OSC 9 has both notification fallback behavior and ConEmu subcommands
in Ghostty; this experiment owns only the notification fallback path.

## Changes

1. Add a notification OSC command.

   In `roastty/src/terminal/osc.rs`, add:

   ```rust
   Command::DesktopNotification { title: &'a [u8], body: &'a [u8] }
   ```

   Keep the command terminal-internal. Use raw byte slices, not `&str`: Ghostty
   forwards notification title/body as byte slices and does not validate UTF-8
   before copying them into the app/surface message.

2. Parse OSC 9 notification fallback.

   Ghostty's OSC 9 parser first attempts ConEmu subcommands. If the byte stream
   does not match a recognized ConEmu form, it falls back to an iTerm2-style
   desktop notification with an empty title and the entire OSC 9 body as the
   notification body.

   For this experiment, implement the fallback path only:
   - `OSC 9;Hello world` parses as title `""`, body `"Hello world"`;
   - `OSC 9;H` parses as title `""`, body `"H"`;
   - incomplete ConEmu-like strings such as `OSC 9;1`, `OSC 9;1a`, `OSC 9;2`,
     `OSC 9;2a`, `OSC 9;3`, `OSC 9;3a`, `OSC 9;4`, and `OSC 9;4;` must still
     fall back to notification bodies of `1`, `1a`, `2`, `2a`, `3`, `3a`, `4`,
     and `4;`.

   Do not parse recognized ConEmu commands yet. Recognized ConEmu forms should
   produce no command and no notification until a later experiment owns them
   explicitly. Representative suppressed forms include `OSC 9;1;420` and
   `OSC 9;4;1;100`.

3. Parse OSC 777 rxvt notifications.

   Add parsing for `OSC 777 ; notify ; title ; body ST`:
   - extension name must be exactly `notify`;
   - title is bytes between the second and third semicolon;
   - body is bytes after the third semicolon;
   - unknown extensions and missing title/body separators reject the command.

   Matching is exact and case-sensitive, matching Ghostty's
   `std.mem.eql(u8, ext, "notify")`.

   Title and body are raw bytes. Do not reject invalid UTF-8 in title or body.

4. Dispatch through the stream layer.

   Extend the stream test harness so valid OSC 9 and OSC 777 notifications reach
   the handler as OSC actions. Add tests showing invalid OSC 777 forms are
   consumed without dispatch or print leakage.

5. Ignore at terminal runtime.

   Add an explicit `TerminalStreamHandler` no-op arm for `DesktopNotification`.
   Tests should prove notifications do not mutate display contents, title, PWD,
   hyperlink state, colors, cursor position, dirty rows, or PTY response.

6. Keep scope limited.

   Do not add app/surface notification delivery, macOS notification APIs, public
   ABI, config, ConEmu command handling, progress reports, clipboard, or
   semantic prompt behavior in this experiment.

## Verification

Run formatting and tests:

```bash
cargo fmt
cargo test -p roastty notification
cargo test -p roastty osc
cargo test -p roastty terminal_stream_osc
cargo test -p roastty
```

Add tests for:

- `OSC 9;Hello world` -> empty title and body `Hello world`;
- `OSC 9;H` -> empty title and body `H`;
- incomplete ConEmu-like OSC 9 strings fall back to notification bodies:
  - `9;1` -> `1`
  - `9;1a` -> `1a`
  - `9;2` -> `2`
  - `9;2a` -> `2a`
  - `9;3` -> `3`
  - `9;3a` -> `3a`
  - `9;4` -> `4`
  - `9;4;` -> `4;`
- `OSC 777;notify;Title;Body` -> title `Title`, body `Body`;
- OSC 777 rejects unknown extension names;
- OSC 777 rejects missing title/body separators;
- OSC 9 and OSC 777 accept non-UTF-8 title/body payload bytes without print
  leakage;
- representative recognized ConEmu forms such as `9;1;420` and `9;4;1;100`
  produce no command and no notification in this experiment;
- stream dispatches valid OSC 9 and OSC 777 notifications;
- invalid OSC 777 forms do not leak printable bytes;
- terminal runtime ignores notifications without mutating unrelated state;
- existing OSC title, PWD, hyperlink, color, Kitty color, mouse-shape, and Kitty
  text-sizing behavior remains unchanged.

## Pass Criteria

- OSC 9 notification fallback parses Ghostty-compatible notification bodies for
  the covered fallback forms.
- OSC 777 `notify` parses Ghostty-compatible title/body pairs.
- Notification title/body payloads are carried as raw bytes, not UTF-8 strings.
- Recognized ConEmu forms remain explicitly suppressed until a later experiment.
- Invalid OSC 777 forms do not dispatch actions or leak bytes into display
  output.
- Terminal runtime ignores notifications without mutating unrelated state.
- ConEmu command handling remains explicitly out of scope.
- No app/surface/macOS notification/public-ABI behavior is added.
- Existing OSC behavior keeps passing.

## Failure Criteria

- OSC 9 fallback strips, trims, or otherwise normalizes the body.
- OSC 9 incomplete ConEmu-like strings stop falling back to notifications.
- OSC 777 accepts unknown extension names or case-insensitive `notify`.
- Notification title/body bytes are rejected for invalid UTF-8.
- Representative recognized ConEmu forms dispatch notifications.
- Invalid OSC 777 forms dispatch actions.
- Invalid notification payload bytes leak into display output.
- Terminal runtime mutates display, title, PWD, hyperlinks, color state, cursor
  position, dirty rows, or PTY response for notifications.
- The experiment adds app/surface/macOS notification/public-ABI behavior.
- The experiment attempts to implement ConEmu progress reports or other ConEmu
  subcommands.
- Existing OSC behavior regresses.

## Design Review

Codex reviewed the initial design and found two real issues:

- Ghostty forwards notification title/body as raw byte slices, not UTF-8
  strings, so the design must not reject invalid UTF-8 payload bytes.
- Recognized ConEmu OSC 9 forms were out of scope but needed explicit temporary
  behavior so a fallback-only implementation would not accidentally turn them
  into desktop notifications.

The design now carries notification title/body as raw bytes and explicitly
suppresses representative recognized ConEmu forms until a later experiment owns
them. Codex re-reviewed the revised design and approved it for implementation
with no remaining blocking findings.

## Result

**Result:** Pass

Implemented terminal-internal desktop notification parsing for the scoped OSC
forms:

- `OSC 9;body` now dispatches a `DesktopNotification` action with an empty raw
  byte title and raw byte body.
- `OSC 777;notify;title;body` now dispatches a `DesktopNotification` action with
  raw byte title/body payloads.
- Representative recognized ConEmu OSC 9 forms are explicitly suppressed until a
  later experiment owns ConEmu command handling.
- Terminal runtime explicitly ignores notification actions, so notifications do
  not mutate display text, title, PWD, hyperlink state, cursor position, modes,
  dirty rows, or PTY response.

The implementation intentionally does not add app/surface notification delivery,
macOS notification APIs, public ABI, ConEmu progress, clipboard, or semantic
prompt behavior.

Verification passed:

```bash
cargo fmt -- roastty/src/terminal/osc.rs roastty/src/terminal/stream.rs roastty/src/terminal/terminal.rs
cargo test -p roastty notification
cargo test -p roastty osc
cargo test -p roastty terminal_stream_osc
cargo test -p roastty
```

The full `roastty` suite passed with 1536 unit tests plus the ABI harness.

## Result Review

Codex reviewed the completed implementation against the experiment design and
the relevant Ghostty OSC parsers. It found no blocking issues and approved the
result for commit.

Codex specifically confirmed:

- OSC 9 fallback carries the full body as raw bytes and preserves the required
  incomplete ConEmu-like fallback cases.
- Recognized ConEmu forms are explicitly suppressed for the deferred scope.
- OSC 777 accepts exact lowercase `notify`, carries raw byte title/body
  payloads, and rejects unknown or malformed forms.
- Stream dispatch covers valid OSC 9/777 notification actions and invalid OSC
  777 no-leak behavior.
- Terminal runtime has the required no-op arm and mutation coverage.

## Conclusion

Roastty now has Ghostty-compatible terminal-layer parsing for the notification
subset of OSC 9 and OSC 777. Notification delivery to an app/surface boundary
remains intentionally deferred until Roastty has that boundary. The next
experiment can continue through the remaining OSC surface, with ConEmu OSC 9
subcommands and semantic prompt handling still explicitly unimplemented.
