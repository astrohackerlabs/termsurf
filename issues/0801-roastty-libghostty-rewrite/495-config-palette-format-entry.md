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

# Experiment 495: the Palette config formatter (Palette::format_entry)

## Description

Continuing the config **formatter** layer (Experiments 491–494), this experiment
ports `Palette.formatEntry` (upstream `Config.Palette`) — the `palette` config:
it writes **all 256** entries, one `name = {index}=#rrggbb\n` line per palette
index. Grounded by the `EntryFormatter` from Experiment 491.

## Upstream behavior

In `config/Config.zig`, `Config.Palette.formatEntry`:

```zig
pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void {
    var buf: [128]u8 = undefined;
    for (0.., self.value) |k, v| {
        try formatter.formatEntry(
            []const u8,
            std.fmt.bufPrint(
                &buf,
                "{d}=#{x:0>2}{x:0>2}{x:0>2}",
                .{ k, v.r, v.g, v.b },
            ) catch return error.OutOfMemory,
        );
    }
}
```

- It iterates **every** palette index `0..256` (the mask is ignored), writing
  one string entry per index: `name = {decimal index}=#{rrggbb}\n` (the color
  channels as lowercase, two-digit hex). So it emits 256 lines.

Upstream's `formatConfig` test checks the first line for a default palette:
`a = 0=#1d1f21\n` (the default index-0 color). roastty's
`terminal::color::DEFAULT_PALETTE[0]` is `(0x1d, 0x1f, 0x21)`.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
impl Palette {
    /// Format as config entries (upstream `Palette.formatEntry`): one
    /// `index=#rrggbb` entry per palette index (all 256, mask ignored).
    pub(crate) fn format_entry(&self, formatter: &mut EntryFormatter) {
        for (k, rgb) in self.value.iter().enumerate() {
            formatter.entry_str(&format!("{}=#{:02x}{:02x}{:02x}", k, rgb.r, rgb.g, rgb.b));
        }
    }
}
```

`format_entry` mirrors upstream: it iterates all 256 `value` entries (ignoring
the `mask`), writing each as a string entry `name = {index}=#{rrggbb}\n`. The
format string `"{}=#{:02x}{:02x}{:02x}"` matches upstream's
`"{d}=#{x:0>2}{x:0>2}{x:0>2}"` (decimal index, lowercase two-digit hex per
channel — the same `#rrggbb` body as `Color::format_buf`). `format_entry` takes
`&self` (`Palette` is not `Copy`).

## Scope / faithfulness notes

- **Ported (bridged)**: `Palette::format_entry` (upstream
  `Palette.formatEntry`).
- **Faithful**: the iteration over all 256 indices (the mask is not consulted);
  one `index=#rrggbb` string entry per index, with the decimal index and
  lowercase two-digit hex channels — exactly upstream's `formatEntry`.
- **Faithful adaptation**: `formatEntry([]const u8, bufPrint(…))` →
  `entry_str(&format!(…))`; `"{d}=#{x:0>2}{x:0>2}{x:0>2}"` →
  `"{}=#{:02x}{:02x}{:02x}"`; the `OutOfMemory` path has no Rust analog (a
  `String` format cannot fail).
- **Deferred**: the remaining types' `formatEntry` (ported in later slices), the
  generic field-dispatch `formatEntry`, and the `Palette` `cval` C struct (FFI),
  and the broader config parser/formatter.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `Palette::format_entry` (in the existing
   `impl Palette`).
2. Tests (in `config/mod.rs`):
   - a default `Palette` formats to 256 lines; the first is `"a = 0=#1d1f21\n"`
     (upstream's `formatConfig` first line / the default index-0 color).
   - a `Palette` with index 0 set to `{0xAA,0xBB,0xCC}` → its first line is
     `"a = 0=#aabbcc\n"`; the output has 256 lines.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty palette_format_entry
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Palette::format_entry` writes one `index=#rrggbb` entry per palette index
  (all 256, mask ignored), with the decimal index and lowercase hex channels —
  faithful to upstream's `formatEntry`;
- the tests pass (the 256-line count; the `a = 0=#1d1f21\n` default and the set
  index-0 line), and the existing tests still pass;
- the other types' `formatEntry`, the generic field-dispatch, and `cval` stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a formatted entry differs from upstream (wrong index
format, wrong hex, missing/extra lines, mask consulted), an unrelated item
changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed `Palette::format_entry` is faithful: upstream iterates
every entry in `self.value`, ignores `mask`, and writes one `{index}=#rrggbb`
string entry per palette slot (`Config.zig:5890`); the Rust `enumerate()` over
`self.value.iter()` is the right equivalent of Zig's `for (0.., self.value)`,
and the lowercase two-digit hex formatting matches
`"{d}=#{x:0>2}{x:0>2}{x:0>2}"`; and the tests cover both the upstream first-line
expectation `a = 0=#1d1f21\n` and a modified index 0, plus the 256-line shape
(`:5953`).

Review artifacts:

- Prompt: `logs/codex-review/20260604-152731-d495-prompt.md` (design)
- Result: `logs/codex-review/20260604-152731-d495-last-message.md` (design)
