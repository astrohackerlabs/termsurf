+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 566: the DFA UTF-8 decoder (Utf8Decoder)

## Description

This experiment ports upstream `terminal/UTF8Decoder.zig` — a DFA-based,
non-allocating, error-replacing UTF-8 decoder (Bjoern Hoehrmann's design with
error replacement). It lands at `terminal::utf8_decoder`. It is the canonical
standalone decoder upstream's terminal stream parser imports; roastty's
`terminal/stream.rs` currently has its **own, separate** length-based decoder
(`std::str::from_utf8` over a small byte buffer), so this port is independent
and non-duplicating — it faithfully mirrors upstream's source file without
touching `stream.rs`. (A future experiment could unify the two; this one does
not.)

## Upstream behavior

`terminal/UTF8Decoder.zig` is a byte-at-a-time state machine over two lookup
tables:

- `char_classes: [256]u4` — maps each byte to one of 12 character classes.
- `transitions: [108]u8` — the DFA transition table, indexed by
  `state + char_class`. States are multiples of 12 (`0, 12, 24, …, 96`);
  `ACCEPT_STATE = 0`, `REJECT_STATE = 12`.

State: an `accumulator: u21` (the codepoint under construction) and `state: u8`
(starting at `ACCEPT_STATE`). `next(byte)` returns a tuple `{ ?u21, bool }` —
the decoded codepoint (if any) and whether the byte was **consumed**:

```zig
pub inline fn next(self, byte) struct { ?u21, bool } {
    const char_class = char_classes[byte];
    const initial_state = self.state;
    if (self.state != ACCEPT_STATE) {
        self.accumulator <<= 6;
        self.accumulator |= (byte & 0x3F);            // continuation: 6 data bits
    } else {
        self.accumulator = (@as(u21, 0xFF) >> char_class) & byte; // first byte: class-masked
    }
    self.state = transitions[self.state + char_class];
    if (self.state == ACCEPT_STATE) {
        defer self.accumulator = 0;
        return .{ self.accumulator, true };           // full codepoint, consumed
    } else if (self.state == REJECT_STATE) {
        self.accumulator = 0;
        self.state = ACCEPT_STATE;
        // Replacement char. Consumed iff we rejected the FIRST byte of a sequence.
        return .{ 0xFFFD, initial_state == ACCEPT_STATE };
    } else {
        return .{ null, true };                        // mid-sequence, consumed, nothing emitted
    }
}
```

The only un-consumed case is an ill-formed continuation: a replacement character
is emitted and the caller must re-feed the same byte (it begins a new sequence).
Upstream's three tests:

- **ASCII** — `"Hello, World!"`: every byte consumed, each emits its own
  codepoint.
- **Well-formed** — `"😄✤ÁA"` (4-, 3-, 2-, 1-byte): every byte consumed first
  try, codepoints `0x1F604, 0x2724, 0xC1, 0x41`.
- **Partially invalid** — `"\xF0\x9F" ++ "😄" ++ "\xED\xA0\x80"` (truncated
  lead, valid sequence, surrogate): emits
  `0xFFFD, 0x1F604, 0xFFFD, 0xFFFD, 0xFFFD`.

## Rust mapping (`roastty/src/terminal/utf8_decoder.rs`)

A direct transcription. The two tables are copied **verbatim**; `u21` becomes
`u32` (Rust has no `u21`; every value fits in 21 bits) and the `{ ?u21, bool }`
tuple becomes `(Option<u32>, bool)`.

```rust
//! A DFA-based, non-allocating, error-replacing UTF-8 decoder (port of upstream
//! `terminal/UTF8Decoder`).
//!
//! Based on Bjoern Hoehrmann's DFA decoder (http://bjoern.hoehrmann.de/utf-8/decoder/dfa, MIT),
//! with error replacement. The two lookup tables are copied verbatim from upstream.

#[rustfmt::skip]
const CHAR_CLASSES: [u8; 256] = [
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,  9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,
   7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,  7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
   8,8,2,2,2,2,2,2,2,2,2,2,2,2,2,2,  2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
  10,3,3,3,3,3,3,3,3,3,3,3,3,4,3,3, 11,6,6,6,5,8,8,8,8,8,8,8,8,8,8,8,
];

#[rustfmt::skip]
const TRANSITIONS: [u8; 108] = [
   0,12,24,36,60,96,84,12,12,12,48,72, 12,12,12,12,12,12,12,12,12,12,12,12,
  12, 0,12,12,12,12,12, 0,12, 0,12,12, 12,24,12,12,12,12,12,24,12,24,12,12,
  12,12,12,12,12,12,12,24,12,12,12,12, 12,24,12,12,12,12,12,12,12,24,12,12,
  12,12,12,12,12,12,12,36,12,36,12,12, 12,36,12,12,12,12,12,36,12,36,12,12,
  12,36,12,12,12,12,12,12,12,12,12,12,
];

const ACCEPT_STATE: u8 = 0;
const REJECT_STATE: u8 = 12;

/// A DFA-based error-replacing UTF-8 decoder (upstream `UTF8Decoder`).
#[derive(Debug, Default)]
pub(crate) struct Utf8Decoder {
    accumulator: u32, // the codepoint under construction (upstream `u21`)
    state: u8,        // the DFA state (starts at ACCEPT_STATE == 0, the `Default`)
}

impl Utf8Decoder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Feed the next byte. Returns the decoded codepoint (if a full one or a replacement was
    /// produced) and whether the byte was consumed. An un-consumed byte must be re-fed (it begins
    /// a new sequence after an ill-formed continuation).
    pub(crate) fn next(&mut self, byte: u8) -> (Option<u32>, bool) {
        let char_class = CHAR_CLASSES[byte as usize];
        let initial_state = self.state;

        if self.state != ACCEPT_STATE {
            self.accumulator <<= 6;
            self.accumulator |= (byte & 0x3F) as u32;
        } else {
            self.accumulator = (0xFF_u32 >> char_class) & (byte as u32);
        }

        self.state = TRANSITIONS[(self.state + char_class) as usize];

        if self.state == ACCEPT_STATE {
            let cp = self.accumulator;
            self.accumulator = 0;
            (Some(cp), true)
        } else if self.state == REJECT_STATE {
            self.accumulator = 0;
            self.state = ACCEPT_STATE;
            // Replacement char. Consumed iff we rejected the first byte of a sequence.
            (Some(0xFFFD), initial_state == ACCEPT_STATE)
        } else {
            (None, true)
        }
    }
}
```

## Scope / faithfulness notes

- **Ported (1:1)**: `terminal/UTF8Decoder.zig` →
  `terminal::utf8_decoder::Utf8Decoder`. The `char_classes` (256) and
  `transitions` (108) tables are copied verbatim; the state machine
  (`accumulator` shift-or for continuations, class-masked first byte, transition
  lookup, the three return cases) is identical.
- **Faithful adaptation**: `u21` → `u32` (Rust lacks `u21`; all values fit in 21
  bits); the `{ ?u21, bool }` tuple → `(Option<u32>, bool)`; the
  `defer accumulator = 0` → an explicit reset before the accept-state return.
  The unused `std.log` scope is dropped.
- **Independent of `stream.rs`**: roastty's `terminal/stream.rs` keeps its own
  separate length-based decoder; this experiment adds the faithful DFA decoder
  as a standalone module and does **not** rewire the stream (a later experiment
  could unify them).
- **Deferred**: nothing — the upstream file is fully covered.
- No C ABI/header/ABI-inventory change (internal Rust). Adds
  `terminal::utf8_decoder`.

## Changes

1. `roastty/src/terminal/utf8_decoder.rs` (new): the tables, state constants,
   `Utf8Decoder` with `new` / `next` as above.
2. `roastty/src/terminal/mod.rs`: add `#[allow(dead_code)] mod utf8_decoder;`
   (alphabetical).
3. Tests (in `utf8_decoder.rs`), the three upstream tests plus a re-feed
   assertion:
   - **ASCII**: `"Hello, World!"` — every byte consumed, codepoints reconstruct
     the string.
   - **well-formed**: `"😄✤ÁA"` — every byte consumed first try; codepoints
     `[0x1F604, 0x2724, 0xC1, 0x41]`.
   - **partially invalid**: `b"\xF0\x9F" ++ "😄" ++ b"\xED\xA0\x80"` —
     codepoints `[0xFFFD, 0x1F604, 0xFFFD, 0xFFFD, 0xFFFD]`.
   - **un-consumed byte is re-fed**: assert that in the partially-invalid case
     at least one `next` returns `consumed == false` (the rejecting byte that
     begins a new sequence), validating the re-feed contract.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::utf8_decoder
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/utf8_decoder.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Utf8Decoder` reproduces upstream's DFA decoding exactly — the verbatim
  tables, the accumulator construction, the transition lookup, and the accept /
  reject (replacement, consumed-iff-first-byte) / mid-sequence return cases —
  faithful to `terminal/UTF8Decoder.zig`;
- the three upstream test vectors pass (ASCII / well-formed / partially-invalid)
  plus the re-feed assertion, and the existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the table contents, the state-machine arithmetic, or
the return-tuple semantics (codepoint / consumed) diverge from upstream, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed the design and **approved it with no findings**. It mechanically
compared the design's `CHAR_CLASSES` (256) and `TRANSITIONS` (108) tables
against `terminal/UTF8Decoder.zig` and confirmed both lengths and contents match
exactly; the state arithmetic and return semantics are faithful (including the
`consumed == false` reject case and the reset behavior). The `u21 → u32`,
`(Option<u32>, bool)`, `Default` state initialization, and standalone
unused-module scope are all acceptable adaptations, the test vectors match
upstream, and the re-feed assertion covers the key caller contract.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d566-prompt.md`
- Result: `logs/codex-review/20260604-d566-last-message.md`

## Result

**Result:** Pass

`terminal::utf8_decoder::Utf8Decoder` was added: the two verbatim lookup tables
(`CHAR_CLASSES` 256, `TRANSITIONS` 108), the `ACCEPT_STATE` / `REJECT_STATE`
constants, and a `Utf8Decoder` (`accumulator: u32`, `state: u8`, `Default`) with
`new` and `next(byte) -> (Option<u32>, bool)` reproducing the DFA exactly —
continuation shift-or, class-masked first byte, transition lookup, and the
accept (codepoint, consumed) / reject (`0xFFFD`, consumed iff the first byte) /
mid-sequence (`None`, consumed) return cases. Registered via
`#[allow(dead_code)] mod utf8_decoder;` in `terminal/mod.rs`. `stream.rs` was
not touched.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3135 passed, 0 failed (three new tests; no
  regressions, up from 3132).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/utf8_decoder.rs +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The three new tests are the exact upstream vectors: ASCII (`"Hello, World!"`
reconstructed), well-formed (`"😄✤ÁA"` → `[0x1F604, 0x2724, 0xC1, 0x41]`, every
byte consumed first try), and partially-invalid
(`b"\xF0\x9F\xF0\x9F\x98\x84\xED\xA0\x80"` →
`[0xFFFD, 0x1F604, 0xFFFD, 0xFFFD, 0xFFFD]`, with a re-feed loop and an
assertion that at least one `next` returned `consumed == false`).

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
or Optional findings** (one Nit: the `## Result` / `## Conclusion` sections were
not yet in the saved file — added here as part of result recording). Codex
mechanically re-checked the _implemented_ Rust tables against upstream —
`CHAR_CLASSES` 256/256 identical, `TRANSITIONS` 108/108 identical — and
confirmed the DFA arithmetic, accept/reject reset behavior, `consumed`
semantics, default state, and tests all match the approved design and upstream
vectors, with `stream.rs` untouched.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r566-prompt.md` (result)
- Result: `logs/codex-review/20260604-r566-last-message.md` (result)

## Conclusion

`terminal::utf8_decoder::Utf8Decoder` is a 1:1 port of
`terminal/UTF8Decoder.zig` — Bjoern Hoehrmann's DFA-based, error-replacing UTF-8
decoder, with both lookup tables transcribed verbatim and verified byte-for-byte
by the reviewer. It is the canonical standalone decoder upstream's stream parser
imports; roastty's `stream.rs` keeps its own separate length-based decoder, so
this port is independent (a future experiment could unify them — replacing
`stream.rs`'s decoder with this one and rewiring the pending/re-feed logic — but
that is a larger, test-sensitive change held for its own slice). This is the
first port mined from `terminal/`'s small leaf files (after the `os/` and
`datastruct/` layers); other unported leaves include `csi`, `file_type`,
`ScreenSet`, and `UTF8Decoder`'s siblings. The remaining big-ticket items are
`datastruct/split_tree` (2517 lines) and the terminal **search subsystem**
(coupled to `PageList` / `Pin` / `Screen` / `Selection` / `PageFormatter`). The
objc/bundle-id helpers, the `home()` resolver, and config `loadDefaultFiles`
remain deferred pending roastty's naming decision; `background-image-opacity`
stays float-blocked.
