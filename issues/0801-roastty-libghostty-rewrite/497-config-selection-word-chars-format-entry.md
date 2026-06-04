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

# Experiment 497: the SelectionWordChars config formatter (SelectionWordChars::format_entry)

## Description

Continuing the config **formatter** layer (Experiments 491–496), this experiment
ports `SelectionWordChars.formatEntry` (upstream `Config.SelectionWordChars`) —
the inverse of the Experiment 485 parser. It re-encodes the boundary codepoints
back to a UTF-8 string (**skipping** the leading null codepoint), skipping any
codepoint that cannot be encoded (a surrogate / out-of-range value), and writes
that string as one entry. Grounded by the `EntryFormatter` from Experiment 491.

## Upstream behavior

In `config/Config.zig`, `Config.SelectionWordChars.formatEntry`:

```zig
pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void {
    // Convert codepoints back to UTF-8 string for display
    var buf: [4096]u8 = undefined;
    var pos: usize = 0;

    // Skip the null character at index 0
    for (self.codepoints[1..]) |codepoint| {
        var utf8_buf: [4]u8 = undefined;
        const len = std.unicode.utf8Encode(codepoint, &utf8_buf) catch continue;
        if (pos + len > buf.len) break;
        @memcpy(buf[pos..][0..len], utf8_buf[0..len]);
        pos += len;
    }

    try formatter.formatEntry([]const u8, buf[0..pos]);
}
```

- The leading null (`codepoints[0]`, always present) is skipped; each remaining
  codepoint is re-encoded to UTF-8 and appended.
- A codepoint that `utf8Encode` rejects (a surrogate `0xD800..0xDFFF` or a value
  `> 0x10FFFF` — possible because the stored codepoints are `u21`) is
  **skipped** (`catch continue`).
- The accumulated UTF-8 string is written as a single string entry.

So a default `SelectionWordChars` formats its boundary set back to e.g.
`" \t'\"│`|:;,()[]{}<>$"`; the round-trip of `" \t;,"`formats to`" \t;,"`.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
impl SelectionWordChars {
    /// Format as a config entry (upstream `SelectionWordChars.formatEntry`):
    /// re-encode the codepoints (skipping the leading null) to UTF-8, skipping any
    /// that cannot be encoded.
    pub(crate) fn format_entry(&self, formatter: &mut EntryFormatter) {
        let mut out = String::new();
        for &cp in self.codepoints.iter().skip(1) {
            if let Some(c) = char::from_u32(cp) {
                // Upstream caps the output at a [4096]u8 buffer: stop before a
                // codepoint that would exceed it (writing only the buffered prefix).
                if out.len() + c.len_utf8() > 4096 {
                    break;
                }
                out.push(c);
            }
        }
        formatter.entry_str(&out);
    }
}
```

`format_entry` mirrors upstream: it skips the leading null (`skip(1)`),
re-encodes each remaining codepoint to UTF-8 (`char::from_u32(cp)` then `push`),
skips a codepoint that cannot be encoded (`char::from_u32` returns `None` for a
surrogate or a value `> 0x10FFFF` — exactly the cases `utf8Encode` rejects via
`catch continue`), and **breaks** before a codepoint that would push the UTF-8
length past 4096 bytes (upstream's `pos + len > buf.len → break`, writing only
the buffered prefix). The order matches upstream: an un-encodable codepoint is
skipped (and does not count toward the cap or trigger the break); an encodable
one that would overflow ends the loop. The result is written as a string entry.
`format_entry` takes `&self` (it holds a non-`Copy` `Vec`).

## Scope / faithfulness notes

- **Ported (bridged)**: `SelectionWordChars::format_entry` (upstream
  `SelectionWordChars.formatEntry`).
- **Faithful**: the leading-null skip; the per-codepoint UTF-8 re-encoding; the
  skip of an un-encodable codepoint (surrogate / out-of-range); the 4096-byte
  output cap (break before a codepoint that would exceed it) — exactly
  upstream's `formatEntry`. `char::from_u32` rejects exactly the codepoints
  `utf8Encode` rejects.
- **Faithful adaptation**: the fixed `[4096]u8` buffer + `utf8Encode` loop → a
  growing `String` with `char::from_u32(cp)` + a
  `out.len() + c.len_utf8() > 4096` break; `formatEntry([]const u8, …)` →
  `entry_str`. (The 4096-byte cap is modeled on `out.len()` — the UTF-8 byte
  length — folded in from the design review.)
- **Deferred**: the remaining types' `formatEntry` (ported in later slices), the
  generic field-dispatch `formatEntry`, and the broader config parser/formatter.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `SelectionWordChars::format_entry` (in the
   existing `impl SelectionWordChars`).
2. Tests (in `config/mod.rs`):
   - a round-trip: `parse_cli(Some(" \t;,"))` then `format_entry` under `a` →
     `"a =  \t;,\n"` (the boundary chars after the null).
   - a constructed value with a multi-byte char:
     `codepoints = [0, ';', ',', 0x2502]` → `"a = ;,│\n"`.
   - the un-encodable skip: `codepoints = [0, 'A', 0xD800, 'B']` (a surrogate) →
     `"a = AB\n"` (the surrogate is skipped).
   - the null-only case: `codepoints = [0]` → `"a = \n"`.
   - the 4096-byte cap (design-review Low): null + 4096 ASCII `'a'` → the value
     is 4096 chars (full); null + 4097 ASCII `'a'` → the value is 4096 chars
     (the 4097th is dropped at the cap).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty selection_word_chars_format_entry
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `SelectionWordChars::format_entry` re-encodes the codepoints (skipping the
  leading null and any un-encodable codepoint) to a UTF-8 string entry —
  faithful to upstream's `formatEntry`;
- the tests pass (the round-trip; the multi-byte char; the surrogate skip; the
  null-only case), and the existing tests still pass;
- the other types' `formatEntry` and the generic field-dispatch stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a formatted entry differs from upstream (null not
skipped, an un-encodable codepoint not skipped, wrong encoding), an unrelated
item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation across two rounds.

**Round 1 — one Required finding (fixed) + one Low (folded in).** Codex
confirmed the leading-null skip (`skip(1)`) and the surrogate / out-of-range
skip (`char::from_u32` rejects exactly what `utf8Encode` rejects) faithful, but
flagged that the upstream `[4096]u8` cap is **observable** (it stops formatting
when the next encoded codepoint would exceed `buf.len` and writes only the
buffered prefix — `Config.zig:6188`/`:6195`), so the port must model it. Fixed
by adding a `out.len() + c.len_utf8() > 4096` break (modeled on the UTF-8 byte
length). The Low (a cap boundary test) was folded into the test plan.

**Round 2 — approved, no findings.** Codex confirmed the revised cap logic
matches upstream exactly (encodable codepoints are checked against the 4096-byte
buffer before appending, over-cap breaks the loop, and un-encodable codepoints
are skipped without affecting the buffer or cap —
`Config.zig:6192`/`:6194`/`:6195`); `out.len()`

- `c.len_utf8()` is the right byte-count equivalent of Zig's `pos + len`; and
  the added 4096/4097 ASCII tests cover the observable cap behavior. "Approved
  with no findings."

Review artifacts:

- Round 1 prompt: `logs/codex-review/20260604-153710-d497-prompt.md`
- Round 1 result: `logs/codex-review/20260604-153710-d497-last-message.md`
- Round 2 prompt: `logs/codex-review/20260604-153853-d497b-prompt.md`
- Round 2 result: `logs/codex-review/20260604-153853-d497b-last-message.md`

## Result

**Result:** Pass

`SelectionWordChars::format_entry` was added to the existing impl exactly as the
(Round-2-approved) design — it skips the leading null (`skip(1)`), re-encodes
each remaining codepoint to UTF-8 (`char::from_u32`, skipping surrogates /
out-of-range), breaks before exceeding the 4096-byte buffer, and writes the
prefix as a string entry. The new test
`selection_word_chars_format_entry_reencodes_codepoints` covers the round-trip,
a multi-byte char, the surrogate skip, the null-only case, and the
4096/4097-byte cap.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 2982 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches upstream `SelectionWordChars.formatEntry`
(skips the leading null, UTF-8-encodes valid codepoints, skips un-encodable
values, stops before exceeding the 4096-byte buffer, and writes the buffered
prefix as a string entry — `Config.zig:6185`/`:6192`/`:6195`); the tests cover
the round-trip, multi-byte UTF-8, invalid-codepoint skipping, null-only output,
and the 4096-byte cap; gates are clean. "Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-154124-r497-prompt.md` (result)
- Result: `logs/codex-review/20260604-154124-r497-last-message.md` (result)

## Conclusion

`SelectionWordChars` now round-trips: the Experiment 485 parser reads the
escape-laden boundary string, and `format_entry` re-encodes the codepoints back
to a UTF-8 string (with the leading-null skip, the un-encodable skip, and the
upstream 4096-byte cap). The config formatter side now covers eleven types. The
next slices can port the remaining types' `formatEntry` (`QuickTerminalSize`,
the `RepeatableClipboardCodepointMap`), then the generic field-dispatch
`formatEntry`, continuing toward the full config formatter and loader.
