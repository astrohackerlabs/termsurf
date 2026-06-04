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

# Experiment 475: the config Color formatter (Color::format_buf)

## Description

Experiments 473–474 ported the parse side of the config `Color`
(`Color::from_hex`, `Color::parse_cli`). This experiment ports the formatter
core — `Color.formatBuf` — which renders a `Color` back to its `#rrggbb` string
form (lowercase, two hex digits per channel). It is the inverse of `from_hex`
and the value the config formatter writes for a color entry. The `formatEntry`
wrapper (which depends on the not-yet-ported config `EntryFormatter`) stays
deferred.

## Upstream behavior

In `config/Config.zig`, `Color.formatBuf`:

```zig
/// Format the color as a string.
pub fn formatBuf(self: Color, buf: []u8) Allocator.Error![]const u8 {
    return std.fmt.bufPrint(
        buf,
        "#{x:0>2}{x:0>2}{x:0>2}",
        .{ self.r, self.g, self.b },
    ) catch error.OutOfMemory;
}
```

The color is formatted as `#` followed by the three channels, each as lowercase
hex zero-padded to two digits (`{x:0>2}`). Upstream's `formatEntry` test
exercises this: a `Color{ .r = 10, .g = 11, .b = 12 }` formats to `#0a0b0c` (the
config entry line is `a = #0a0b0c\n`).

The `formatEntry` wrapper builds a 128-byte stack buffer, calls `formatBuf`, and
hands the result to the `EntryFormatter`:

```zig
pub fn formatEntry(self: Color, formatter: formatterpkg.EntryFormatter) !void {
    var buf: [128]u8 = undefined;
    try formatter.formatEntry([]const u8, try self.formatBuf(&buf));
}
```

The config `EntryFormatter` is not ported yet, so `formatEntry` stays deferred;
this experiment lands `formatBuf` — the actual string production.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
impl Color {
    /// Format the color as a `#rrggbb` string (upstream `Color.formatBuf`): a
    /// `#` followed by each channel as lowercase hex, zero-padded to two digits.
    /// The inverse of [`Color::from_hex`].
    pub(crate) fn format_buf(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}
```

`format_buf` mirrors upstream's `#{x:0>2}{x:0>2}{x:0>2}` exactly: `#` plus each
of `r`, `g`, `b` as lowercase two-digit hex (`{:02x}`). Zig's `bufPrint` into a
caller buffer with an `Allocator.Error!`/OOM result is a fixed-buffer artifact;
the faithful Rust analog returns an owned `String` (a `format!` into a growable
buffer cannot meaningfully fail), so there is no error type. The string content
is identical.

## Scope / faithfulness notes

- **Ported (bridged)**: the config `Color` formatter core (`Color::format_buf`,
  upstream `Color.formatBuf`).
- **Faithful**: the `#` prefix; each channel as lowercase hex, zero-padded to
  two digits; the `r`, `g`, `b` order — exactly upstream's
  `#{x:0>2}{x:0>2}{x:0>2}`.
- **Faithful adaptation**: Zig's `bufPrint(buf, ...) Allocator.Error![]const u8`
  (write into a caller buffer, OOM on overflow) maps to a returned `String`; the
  `format!` cannot fail, so the `Allocator.Error`/OOM path has no Rust analog.
  The rendered text is identical.
- **Round-trip**: `from_hex(format_buf(c)) == c` for every `Color` (the
  formatter is the inverse of the hex parser); a test asserts this.
- **Deferred**: `Color.formatEntry` (depends on the not-yet-ported config
  `EntryFormatter`), `cval` / the C extern struct, and the broader config parser
  and formatter (`loadCli` / per-field dispatch / file loading /
  `formatConfig`). (Consumed by later slices; this experiment lands the
  formatter core.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `Color::format_buf(self) -> String`.
2. Tests (in `config/mod.rs`):
   - mirror upstream's `formatEntry` example: `Color { r: 10, g: 11, b: 12 }` →
     `"#0a0b0c"`; plus `{0,0,0}` → `"#000000"`, `{255,255,255}` → `"#ffffff"`,
     `{0xAA,0xBB,0xCC}` → `"#aabbcc"`; and a round-trip
     (`Color::from_hex(&c.format_buf()) == Ok(c)`) for a few colors, tying the
     formatter to the Experiment 473 parser.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty format_buf
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Color::format_buf` renders `#rrggbb` (a `#`, then each channel as lowercase
  two-digit hex, in `r`, `g`, `b` order) — faithful to upstream's `formatBuf`;
- the tests pass (the upstream example; the extra colors; the round-trip through
  `from_hex`), and the existing tests still pass;
- `Color.formatEntry` and the broader config formatter/parser stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the formatted string differs from upstream (wrong
prefix, wrong case, missing zero-pad, wrong channel order), the round-trip does
not hold, an unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: the format is exactly `#`
plus lowercase, zero-padded, two-digit hex in `r`/`g`/`b` order
(`Config.zig:5468`), which `format!("#{:02x}{:02x}{:02x}", ...)` matches, and
the upstream formatter test confirms lowercase output like `#0a0b0c`
(`Config.zig:5524`); returning a `String` is the right Rust adaptation for this
layer — the Zig `buf`/`Allocator.Error` shape is driven by its formatter/buffer
API and modeling it now would add noise without improving faithfulness;
deferring `formatEntry` is correct (it depends on the not-yet-ported config
formatter abstraction). It judged the planned tests adequate, especially the
upstream `#0a0b0c` case and the round-trip through `from_hex`.

Review artifacts:

- Prompt: `logs/codex-review/20260604-130328-d475-prompt.md` (design)
- Result: `logs/codex-review/20260604-130328-d475-last-message.md` (design)
