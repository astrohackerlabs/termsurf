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

# Experiment 491: the config EntryFormatter and the first formatEntry (Color)

## Description

Every config value type's `formatEntry` (deferred across Experiments 475–490)
takes an `EntryFormatter` — upstream's `config/formatter.zig` object that writes
a single `name = value\n` config line. This experiment ports that
`EntryFormatter` (the primitive entry writers) into a new
`roastty/src/config/formatter.rs` module and **grounds it with its first
consumer**: `Color::format_entry` (upstream `Color.formatEntry`), which renders
`#rrggbb` via the already-ported `format_buf` (Experiment 475). This stands up
the config **formatter** layer that the remaining `formatEntry` ports will build
on.

The generic, comptime, field-dispatch `formatEntry(T, name, value, writer)`
(which auto-formats a field with no custom `formatEntry` —
bool/int/enum/optional/packed) is part of the per-field formatter dispatch and
stays deferred; this experiment ports the `EntryFormatter` **object** and its
typed entry writers that the custom `formatEntry` methods call.

## Upstream behavior

In `config/formatter.zig`:

```zig
pub fn entryFormatter(name: []const u8, writer: *std.Io.Writer) EntryFormatter {
    return .{ .name = name, .writer = writer };
}

pub const EntryFormatter = struct {
    name: []const u8,
    writer: *std.Io.Writer,

    pub fn formatEntry(self: @This(), comptime T: type, value: T) !void {
        return formatter.formatEntry(T, self.name, value, self.writer);
    }
};

pub fn formatEntry(comptime T: type, name: []const u8, value: T, writer: *std.Io.Writer) !void {
    switch (@typeInfo(T)) {
        .bool, .int => try writer.print("{s} = {}\n", .{ name, value }),
        .float       => try writer.print("{s} = {d}\n", .{ name, value }),
        .@"enum"     => try writer.print("{s} = {t}\n", .{ name, value }), // tag name
        .void        => try writer.print("{s} = \n", .{name}),
        // []const u8 / [:0]const u8:
        .pointer     => try writer.print("{s} = {s}\n", .{ name, value }),
        // optional → recurse on the inner value, or `name = \n`
        // struct/union with a formatEntry method → call it
        // packed struct → `name = [no-]field,[no-]field…\n`
        // ...
    }
}
```

`EntryFormatter` holds the field `name` and a writer, and
`formatEntry(T, value)` writes one `name = …\n` line. For the primitives the
custom `formatEntry` methods pass: a string (`[]const u8` / `[:0]const u8`) →
`name = value\n`; a `bool` → `name = true\n` / `name = false\n`; an int →
`name = <decimal>\n`; `void` → `name = \n`.

`Color.formatEntry` (upstream `Config.Color`) is the first consumer:

```zig
pub fn formatEntry(self: Color, formatter: formatterpkg.EntryFormatter) !void {
    var buf: [128]u8 = undefined;
    try formatter.formatEntry([]const u8, try self.formatBuf(&buf));
}
```

It renders the color to `#rrggbb` (via `formatBuf`) and writes it as a string
entry. Upstream's `Color` `formatConfig` test: a `Color{10,11,12}` under the
name `a` produces `a = #0a0b0c\n`.

## Rust mapping

New `roastty/src/config/formatter.rs`:

```rust
//! Config entry formatting (port of upstream `config/formatter.zig`).
//!
//! `EntryFormatter` writes one `name = value\n` config line. The comptime,
//! field-dispatch generic `formatEntry` (auto-formatting fields with no custom
//! `formatEntry`) is ported later; this is the object the custom `formatEntry`
//! methods call.
#![allow(dead_code)]

use std::fmt::{Display, Write as _};

/// Writes a single `name = value\n` config entry (upstream
/// `config.formatter.EntryFormatter`).
pub(crate) struct EntryFormatter<'a> {
    name: &'a str,
    out: &'a mut String,
}

impl<'a> EntryFormatter<'a> {
    pub(crate) fn new(name: &'a str, out: &'a mut String) -> Self {
        EntryFormatter { name, out }
    }

    /// `name = value\n` (upstream the `[]const u8` / `[:0]const u8` case).
    pub(crate) fn entry_str(&mut self, value: &str) {
        let _ = writeln!(self.out, "{} = {}", self.name, value);
    }

    /// `name = true|false\n` (upstream the `bool` case).
    pub(crate) fn entry_bool(&mut self, value: bool) {
        let _ = writeln!(self.out, "{} = {}", self.name, value);
    }

    /// `name = <decimal>\n` (upstream the `int` case).
    pub(crate) fn entry_int(&mut self, value: impl Display) {
        let _ = writeln!(self.out, "{} = {}", self.name, value);
    }

    /// `name = \n` (upstream the `void` case).
    pub(crate) fn entry_void(&mut self) {
        let _ = writeln!(self.out, "{} = ", self.name);
    }
}
```

`roastty/src/config/mod.rs` — wire the module and the first consumer:

```rust
mod formatter;
use crate::config::formatter::EntryFormatter;

impl Color {
    /// Format the color as a config entry (upstream `Color.formatEntry`): write the
    /// `#rrggbb` string (via [`Color::format_buf`]) as the value.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(&self.format_buf());
    }
}
```

`EntryFormatter` mirrors upstream: a `name` + an output target, with the
primitive entry writers producing the exact `name = …\n` lines (Rust's `{}` on a
`bool` is `true`/`false` and on an int is decimal, matching Zig's `{}`; the
string and void forms match the `{s}` / empty forms). `Color::format_entry`
mirrors `Color.formatEntry` — write the `format_buf` `#rrggbb` string. The
writer is a `&mut String` (the Rust analog of upstream's `*std.Io.Writer`); a
returned error is not modeled (writing to a `String` cannot fail).

## Scope / faithfulness notes

- **Ported (bridged)**: the config `EntryFormatter` object and its typed entry
  writers (`entry_str` / `entry_bool` / `entry_int` / `entry_void`, upstream's
  `formatEntry` primitive cases), and `Color::format_entry` (upstream
  `Color.formatEntry`).
- **Faithful**: the `name = value\n` line shape for a string;
  `name = true|false\n` for a bool; `name = <decimal>\n` for an int; `name = \n`
  for void — exactly upstream's `formatEntry` primitive cases.
  `Color::format_entry` writes the `format_buf` `#rrggbb` string, exactly
  upstream's `Color.formatEntry`.
- **Faithful adaptation**: the comptime generic `formatEntry(T, …)` (type-driven
  dispatch) → an `EntryFormatter` with one typed writer per primitive case the
  custom `formatEntry` methods use (Rust has no comptime type switch);
  `*std.Io.Writer` → `&mut String` (writing cannot fail, so the `!void` error is
  dropped); Rust's `{}` matches Zig's `{}` for bool/int.
- **Deferred**: the generic field-dispatch `formatEntry(T, name, value, writer)`
  for fields **without** a custom `formatEntry` (the enum-`{t}`, float-`{d}`,
  optional-recurse, and packed-struct `[no-]field` cases — part of the per-field
  formatter dispatch), and the remaining types' `formatEntry` methods (ported in
  later slices, each grounded by `EntryFormatter`). This experiment lands the
  object and the first consumer.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/formatter.rs` (new): the module doc, `EntryFormatter`
   with `new` / `entry_str` / `entry_bool` / `entry_int` / `entry_void`, and a
   test.
2. `roastty/src/config/mod.rs`: add `mod formatter;` and
   `use crate::config::formatter::EntryFormatter;`; add `Color::format_entry`.
3. Tests:
   - in `config/formatter.rs`: an `EntryFormatter` test — `entry_str("v")` →
     `"a = v\n"`; `entry_bool(true)` → `"a = true\n"`; `entry_int(42u8)` →
     `"a = 42\n"`; `entry_void()` → `"a = \n"`.
   - in `config/mod.rs`: `Color { r: 10, g: 11, b: 12 }.format_entry` under the
     name `a` produces `"a = #0a0b0c\n"` (upstream's `Color` `formatConfig`).
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty entry_formatter
cargo test -p roastty color_format_entry
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `EntryFormatter` writes the `name = …\n` lines for a string / bool / int /
  void exactly as upstream's `formatEntry` primitives, and `Color::format_entry`
  writes the `#rrggbb` string entry — faithful to upstream;
- the tests pass (the `EntryFormatter` primitives; the `Color` `a = #0a0b0c\n`),
  and the existing tests still pass;
- the generic field-dispatch `formatEntry` and the other types' `formatEntry`
  methods stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a formatted entry differs from upstream (wrong
separator/newline/value), `Color::format_entry` writes the wrong string, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the formatter shape is a faithful Rust adaptation for
this slice — upstream's comptime-dispatched
`EntryFormatter.formatEntry(T, value)` becomes explicit typed entry writers, and
the line formats match the upstream primitive cases (`name = value\n` for
strings/bools/ints, `name = \n` for void — `formatter.zig:16`/`:41`/`:57`);
`Color::format_entry` is faithful (upstream formats `Color.formatBuf` as a
string entry, output exactly `a = #0a0b0c\n` — `Config.zig:5459`/`:5524`); using
`&mut String` and dropping the writer error is reasonable (`String` formatting
has no I/O-failure surface); and deferring the generic dispatch and the other
`formatEntry` consumers is the right scope.

Review artifacts:

- Prompt: `logs/codex-review/20260604-150634-d491-prompt.md` (design)
- Result: `logs/codex-review/20260604-150634-d491-last-message.md` (design)
