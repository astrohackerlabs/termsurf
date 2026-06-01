# Experiment 142: Port OSC Clipboard Protocols

## Description

Experiment 141 finished the notification slice of Ghostty's OSC surface. The
next coherent OSC subsystem is clipboard parsing:

- OSC 52 clipboard operations;
- Kitty OSC 5522 clipboard protocol;
- the allocating OSC capture behavior these protocols need for realistic
  payloads.

Ghostty parses these in:

- `vendor/ghostty/src/terminal/osc/parsers/clipboard_operation.zig`
- `vendor/ghostty/src/terminal/osc/parsers/kitty_clipboard_protocol.zig`
- `vendor/ghostty/src/terminal/osc.zig` capture handling for allocating OSCs.

This experiment ports the terminal parser/stream recognition only. Roastty does
not yet have an app/surface clipboard boundary, so terminal runtime must
explicitly ignore clipboard actions. Do not read or write the macOS clipboard,
do not add public ABI, and do not synthesize PTY replies for clipboard reads in
this experiment.

This experiment must also fix the parser storage prerequisite. Ghostty uses an
allocating capture buffer for OSC 52, OSC 5522, and OSC 66 because those
payloads can exceed the normal 2048-byte OSC buffer. Roastty currently has only
a fixed 2048-byte OSC buffer, which would make any clipboard implementation
silently fail on realistic payloads. Add growable capture for the Ghostty
allocating OSC families while preserving fixed-buffer rejection for unrelated
oversized OSCs.

## Changes

1. Add a clipboard terminal module.

   Add `roastty/src/terminal/clipboard.rs` and register it from
   `roastty/src/terminal/mod.rs`.

   Include:
   - `ClipboardContents<'a> { kind: u8, data: &'a [u8] }` for OSC 52.
   - `KittyClipboard<'a> { metadata: &'a [u8], payload: Option<&'a [u8]>, terminator: osc::Terminator }`.
   - Kitty option enums for `loc`, `status`, and operation `type`, matching
     Ghostty's case-sensitive values:
     - `loc=primary`;
     - `status=DATA|DONE|EBUSY|EINVAL|EIO|ENOSYS|EPERM|OK`;
     - `type=read|walias|wdata|write`.
   - Option readers for `id`, `loc`, `mime`, `name`, `password`, `pw`, `status`,
     and `type`.

   The option parser should match Ghostty's metadata behavior:
   - options are colon-separated;
   - leading whitespace before an option is skipped;
   - whitespace between key and `=` is skipped;
   - value whitespace is trimmed on both ends;
   - matching is case-sensitive;
   - invalid/missing enum values return `None`;
   - `id` is valid only when non-empty and made of `A-Z a-z 0-9 - _ + .`.

2. Add growable OSC capture for allocating OSC families.

   In `roastty/src/terminal/osc.rs`, keep `MAX_BUF = 2048` as the normal OSC
   limit, but let these prefixes grow beyond it:
   - `52;`
   - `5522;`
   - `66;`

   A simple approach is to keep the existing fixed array for normal OSCs and add
   `overflow: Option<Vec<u8>>` to `Parser`. Once the fixed buffer fills, grow
   only if the captured prefix is one of the allocating families above. All
   subsequent bytes for that OSC should go into the overflow vector.

   The command parser must see the complete OSC byte stream. When a growable OSC
   crosses `MAX_BUF`, copy the already captured fixed-buffer bytes into the
   growable vector before appending the overflow byte, or use an equivalent
   representation that exposes one complete command slice. Do not store only the
   bytes after the overflow point.

   Reset must clear the overflow vector.

   Match Ghostty's allocating-capture policy rather than adding a Roastty-only
   protocol cap in this experiment: OSC 52 and OSC 5522 may grow beyond
   `MAX_BUF` as long as allocation succeeds, and OSC 66 may grow beyond
   `MAX_BUF` while still enforcing its existing 4096-byte protocol cap.
   Implement growth with fallible reservation (`try_reserve` or equivalent). If
   allocation fails, invalidate the OSC and consume bytes until the terminator
   without dispatch or print leakage.

   Do not make every OSC unbounded. Oversized unrelated OSCs must still be
   invalidated and consumed without print leakage.

3. Parse OSC 52 clipboard operations.

   Add:

   ```rust
   Command::ClipboardContents { value: clipboard::ClipboardContents<'a> }
   ```

   Parse from the data after `52;`:
   - `OSC 52;s;?` -> kind `b's'`, data `b"?"`;
   - `OSC 52;;?` -> kind `b'c'`, data `b"?"`;
   - `OSC 52;;` -> kind `b'c'`, empty data;
   - empty data after `52;` rejects;
   - one-byte data without a following semicolon rejects;
   - non-empty explicit kind requires byte 1 to be `;`.

   Carry `data` as raw bytes. Do not base64-decode it here; Ghostty's parser
   only preserves the encoded data.

4. Parse Kitty OSC 5522 clipboard protocol.

   Add:

   ```rust
   Command::KittyClipboard { value: clipboard::KittyClipboard<'a> }
   ```

   Parse from the data after `5522;`:
   - if no further semicolon exists, the entire data is metadata and payload is
     `None`;
   - if a semicolon exists, bytes before it are metadata and bytes after it are
     `Some(payload)`;
   - `OSC 5522;` -> empty metadata, `None`;
   - `OSC 5522;;` -> empty metadata, `Some(b"")`;
   - preserve the OSC terminator for possible future replies.

   Carry metadata and payload as raw bytes. Do not base64-decode payloads in
   this experiment.

5. Dispatch through the stream layer.

   Extend `OscAction` and the stream test harness so valid OSC 52 and OSC 5522
   actions reach the handler.

   Add stream tests for:
   - OSC 52 explicit and default clipboard kind;
   - OSC 5522 metadata-only and metadata-plus-payload;
   - ST and BEL terminators for Kitty OSC 5522;
   - oversized OSC 52/5522 payloads dispatch instead of being invalidated;
   - oversized unrelated OSCs are still consumed without dispatch or print
     leakage.

6. Ignore at terminal runtime.

   Add explicit no-op arms for `ClipboardContents` and `KittyClipboard` in
   `TerminalStreamHandler`.

   Tests must prove clipboard actions do not mutate display contents, title,
   PWD, hyperlink state, colors, cursor position, modes, dirty rows, or PTY
   response.

7. Keep scope limited.

   Do not implement:
   - macOS clipboard reads or writes;
   - terminal-to-app/surface clipboard messages;
   - public C ABI for clipboard events;
   - OSC 52 clipboard query replies;
   - Kitty OSC 5522 protocol replies;
   - base64 decoding;
   - security policy for whether clipboard access is allowed.

   Those require app/surface boundary and policy work and must be handled in a
   later experiment.

## Verification

Run formatting and tests:

```bash
cargo fmt
cargo test -p roastty clipboard
cargo test -p roastty osc
cargo test -p roastty terminal_stream_osc
cargo test -p roastty
```

Add tests for OSC 52:

- `52;s;?` -> kind `s`, data `?`;
- `52;;?` -> kind `c`, data `?`;
- `52;;` -> kind `c`, empty data;
- `52;` rejects;
- `52;s` rejects;
- `52;sx?` rejects;
- non-UTF-8 data is preserved as raw bytes.

Add tests for Kitty OSC 5522:

- `5522;` -> empty metadata, no payload;
- `5522;;` -> empty metadata, empty payload;
- `5522;type=read;dGV4dC9wbGFpbg==` -> metadata `type=read`, payload
  `dGV4dC9wbGFpbg==`;
- `id=` returns `None`;
- valid IDs accept `A-Z a-z 0-9 - _ + .`;
- invalid IDs return `None`;
- `loc=primary`, valid `status`, and valid `type` parse case-sensitively;
- invalid `loc`, `status`, or `type` return `None`;
- whitespace handling matches Ghostty for leading option whitespace, key/equals
  whitespace, and trimmed values;
- raw non-UTF-8 metadata and payload bytes are preserved.

Add growable-buffer tests:

- OSC 52 payload longer than `osc::MAX_BUF` dispatches successfully;
- the over-`MAX_BUF` payload preserves bytes from both sides of the fixed-buffer
  boundary, proving the growable command view did not drop pre-overflow data;
- OSC 5522 payload longer than `osc::MAX_BUF` dispatches successfully;
- existing OSC 66 payload handling can exceed `osc::MAX_BUF` up to the OSC 66
  protocol cap;
- an unrelated oversized OSC still invalidates and consumes without print
  leakage.

Allocation failure is not expected to be deterministically triggerable in a unit
test, but the implementation must use fallible growth and invalidate on
allocation error rather than panicking.

Add terminal runtime tests:

- OSC 52 and OSC 5522 are ignored without mutating display contents, title, PWD,
  hyperlink state, color state, cursor position, modes, dirty rows, or PTY
  response.

## Pass Criteria

- OSC 52 parsing matches Ghostty's parser behavior for explicit kind, default
  kind, empty data, and invalid forms.
- Kitty OSC 5522 parsing matches Ghostty's metadata/payload split and option
  reader behavior.
- Clipboard metadata, payload, and OSC 52 data are carried as raw bytes, not
  UTF-8 strings.
- Allocating OSC families can exceed the normal 2048-byte buffer.
- Growable OSC actions preserve the full payload, including bytes captured
  before the fixed-buffer boundary.
- Growable OSC allocation failure invalidates and consumes the OSC without
  panic, dispatch, or print leakage.
- Unrelated oversized OSCs remain invalidated and leak no printable bytes.
- Terminal runtime explicitly ignores clipboard actions without mutating
  unrelated state or emitting PTY replies.
- No app/surface/macOS clipboard/public-ABI behavior is added.
- Existing OSC behavior keeps passing.

## Failure Criteria

- OSC 52 requires UTF-8, base64-decodes data, strips bytes, or normalizes data.
- OSC 52 accepts malformed explicit-kind forms that Ghostty rejects.
- OSC 5522 requires UTF-8, base64-decodes payload, or loses the terminator.
- Kitty option parsing is case-insensitive or accepts invalid enum/id values.
- The parser lets every OSC grow without limit instead of limiting growable
  capture to Ghostty's allocating OSC families.
- The parser drops bytes captured before the growable-buffer transition.
- The parser uses infallible `Vec` growth that can panic instead of invalidating
  on allocation failure.
- Oversized unrelated OSCs dispatch actions or leak bytes into display output.
- Terminal runtime reads/writes the system clipboard, emits replies, or mutates
  unrelated terminal state.
- The experiment adds app/surface/public-ABI clipboard behavior.
- Existing OSC behavior regresses.

## Design Review

Codex reviewed the initial design and found two real blocking issues in the
growable-buffer plan:

- The draft said overflow bytes should go into a `Vec`, but did not require the
  already captured fixed-buffer bytes to be copied into the growable
  representation. That could have made the parser see only bytes after the
  overflow point.
- The draft did not define an allocation/OOM policy for growable OSC capture.

The design now requires a complete command byte view across the
fixed-to-growable transition, boundary-content tests, Ghostty-style allocating
capture for OSC 52 and OSC 5522, continued OSC 66 protocol-cap enforcement, and
fallible growth that invalidates on allocation failure instead of panicking.
Codex re-reviewed the revised design and approved it for implementation with no
blocking findings.

## Result

**Result:** Pass

Implemented terminal-internal clipboard parsing for the scoped OSC forms:

- `OSC 52` clipboard operations now parse explicit clipboard kind, default
  clipboard kind `c`, empty data, and raw byte data.
- Kitty `OSC 5522` now parses raw metadata, optional raw payload, and preserves
  the OSC terminator.
- Kitty clipboard metadata option readers now cover `id`, `loc`, `mime`, `name`,
  `password`, `pw`, `status`, and operation `type`, with Ghostty-compatible
  case-sensitive enum handling, identifier validation, and whitespace trimming.
- OSC capture now grows beyond the normal 2048-byte buffer only for Ghostty's
  allocating OSC families: `52;`, `5522;`, and `66;`.
- Growable capture preserves bytes from both sides of the fixed-buffer boundary,
  uses fallible `try_reserve` growth, and keeps unrelated oversized OSCs
  invalidated without print leakage.
- Terminal runtime explicitly ignores OSC 52 and OSC 5522 actions. It does not
  touch the system clipboard, emit PTY replies, add public ABI, or mutate
  unrelated terminal state.

Verification passed:

```bash
cargo fmt -- roastty/src/terminal/clipboard.rs roastty/src/terminal/mod.rs roastty/src/terminal/osc.rs roastty/src/terminal/stream.rs roastty/src/terminal/terminal.rs
cargo test -p roastty clipboard
cargo test -p roastty osc
cargo test -p roastty terminal_stream_osc
cargo test -p roastty
```

The full `roastty` suite passed with 1546 unit tests plus the ABI harness.

## Result Review

Codex reviewed the completed implementation and found three real issues:

- malformed `OSC 5522` without the required semicolon could dispatch as an empty
  Kitty clipboard command;
- OSC 66 growable capture was capped by total captured bytes instead of the OSC
  66 text payload length;
- terminal no-op coverage did not prove color state stayed unchanged.

The implementation was updated so `OSC 5522` dispatch requires the initial
separator, OSC 66 relies on its parser-level 4096-byte payload cap, exact-cap
OSC 66 cases are tested, malformed `5522` no-leak stream behavior is tested, and
terminal no-op coverage snapshots foreground color around clipboard OSCs.

Codex re-reviewed the revised implementation and found no remaining blockers.

## Conclusion

Roastty now has Ghostty-compatible parser and stream recognition for OSC 52 and
Kitty OSC 5522, plus the growable OSC capture needed for realistic clipboard
payloads. Clipboard side effects remain intentionally deferred until Roastty has
an app/surface boundary and explicit clipboard policy. The OSC parser foundation
is now better aligned with Ghostty for subsequent app-boundary OSC work.
