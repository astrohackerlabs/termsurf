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

# Experiment 536: the C0 control-character enum (terminal::ansi::C0)

## Description

The config subsystem's remaining piece (`loadDefaultFiles`) is blocked on a
product naming decision, so this experiment pivots to the **non-config** rewrite
— the terminal core's VT layer, which roastty has not yet ported (`ansi`, `csi`,
`apc`, `parse_table`, `Parser`). It starts with the most fundamental,
self-contained unit: the **C0 control-character** enum from upstream
`terminal/ansi.zig`, the named 7-bit ANSI control codes the VT stream parser
dispatches on. A new `terminal::ansi` module is introduced to house it (later
ANSI/VT enums join it).

## Upstream behavior

`terminal/ansi.zig` `C0` (a non-exhaustive `enum(u7)`):

```zig
pub const C0 = enum(u7) {
    NUL = 0x00, SOH = 0x01, STX = 0x02, ENQ = 0x05, BEL = 0x07, BS = 0x08,
    HT = 0x09, LF = 0x0A, VT = 0x0B, FF = 0x0C, CR = 0x0D, SO = 0x0E, SI = 0x0F,
    // Non-exhaustive so that @enumFromInt never fails since the inputs are
    // user-generated.
    _,
};
```

It is the named set of C0 control codes; the trailing `_` makes it
**non-exhaustive** so `@enumFromInt(byte)` (over user-generated input) never
fails — an unrecognized byte is simply a value with no named tag. The stream
parser does `const c0: C0 = @enumFromInt(c)` then `switch`es, with the named
tags handled and `else` for the rest (`terminal/stream.zig`).

## Rust mapping (`roastty/src/terminal/ansi.rs`, new module)

Rust enums are exhaustive, so the non-exhaustive `@enumFromInt` is modeled with
a `from_byte(u8) -> Option<C0>` (a recognized control code ⇒ `Some(variant)`, an
unrecognized byte ⇒ `None`), and `value()` returns the byte:

```rust
/// C0 (7-bit) ANSI control characters (upstream `terminal.ansi.C0`). Only the
/// control codes the terminal handles are named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum C0 {
    /// Null.
    Nul = 0x00,
    /// Start of heading.
    Soh = 0x01,
    /// Start of text.
    Stx = 0x02,
    /// Enquiry.
    Enq = 0x05,
    /// Bell.
    Bel = 0x07,
    /// Backspace.
    Bs = 0x08,
    /// Horizontal tab.
    Ht = 0x09,
    /// Line feed.
    Lf = 0x0A,
    /// Vertical tab.
    Vt = 0x0B,
    /// Form feed.
    Ff = 0x0C,
    /// Carriage return.
    Cr = 0x0D,
    /// Shift out.
    So = 0x0E,
    /// Shift in.
    Si = 0x0F,
}

impl C0 {
    /// The byte value of this control code.
    pub(crate) fn value(self) -> u8 {
        self as u8
    }

    /// The named C0 control code for a byte, or `None` for an unrecognized byte
    /// (upstream's non-exhaustive `@enumFromInt`: a parser matches the named codes and
    /// treats the rest as "not a recognized C0").
    pub(crate) fn from_byte(byte: u8) -> Option<C0> {
        Some(match byte {
            0x00 => C0::Nul,
            0x01 => C0::Soh,
            0x02 => C0::Stx,
            0x05 => C0::Enq,
            0x07 => C0::Bel,
            0x08 => C0::Bs,
            0x09 => C0::Ht,
            0x0A => C0::Lf,
            0x0B => C0::Vt,
            0x0C => C0::Ff,
            0x0D => C0::Cr,
            0x0E => C0::So,
            0x0F => C0::Si,
            _ => return None,
        })
    }
}
```

`value` reads the `#[repr(u8)]` discriminant (the control byte); `from_byte`
maps a byte to its named code or `None` — the faithful Rust shape for the
parser's "match the named C0s, else treat as not-a-C0" dispatch.

## Scope / faithfulness notes

- **Ported (bridged)**: the `terminal.ansi.C0` named control codes, as
  `terminal::ansi::C0` + `value` / `from_byte`.
- **Faithful**: the 13 named C0 codes with their exact byte values; the
  non-exhaustive behavior (an unrecognized byte ⇒ `None`, never a failure).
- **Faithful adaptation**: Zig's non-exhaustive `enum(u7)` + `@enumFromInt` → a
  Rust `#[repr(u8)]` enum (u7 fits in u8) + `from_byte(u8) -> Option<C0>`; the
  `_` tag → the `None` arm.
- **Deferred**: the other `ansi.zig` enums (`RenditionAspect`, `CursorStyle`,
  `StatusLineType`, `StatusDisplay`, `ModifyKeyFormat`, `ProtectedMode`); the
  rest of the VT layer (`csi`, `apc`, `parse_table`, `Parser`); the stream
  parser that consumes `C0`.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/terminal/ansi.rs` (new): the `C0` enum + `value` / `from_byte`.
2. `roastty/src/terminal/mod.rs`: add `#[allow(dead_code)] mod ansi;`.
3. Tests (in `ansi.rs`): each named code round-trips
   (`C0::from_byte(c.value()) == Some(c)`) with the exact byte
   (`C0::Bel.value() == 0x07`, `C0::Lf.value() == 0x0A`); an unrecognized byte
   (`0x03` ETX, `0x20` space, `0x7F` DEL) ⇒ `None`.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty c0
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/ansi.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `C0` has the 13 named control codes with their exact byte values, `value`
  returns the byte, and `from_byte` maps a byte to its named code or `None` —
  faithful to upstream's non-exhaustive `C0`;
- the tests pass (round-trip each code + unrecognized bytes ⇒ `None`), and the
  existing tests still pass;
- the other ANSI enums and the VT layer stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a code's value diverges from upstream, an unrelated
item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. The 13 named C0 codes and byte values match upstream exactly —
`NUL`, `SOH`, `STX`, `ENQ`, `BEL`, `BS`, `HT`, `LF`, `VT`, `FF`, `CR`, `SO`,
`SI` (`ansi.zig:7`). Modeling Zig's non-exhaustive `enum(u7)` as a Rust
`#[repr(u8)]` enum plus `from_byte -> Option<C0>` is a reasonable faithful
adaptation for the planned consumer — upstream uses `@enumFromInt(c)` and
switches named tags with `else` for unhandled values (`stream.zig:764`), and a
future Rust consumer can treat `None` as that else path. Porting this building
block ahead of the stream parser is consistent with the approach used throughout
the rewrite, and the PascalCase variants (`Nul` / `Soh`) are idiomatic Rust as
long as `value()` / `from_byte()` preserve the exact numeric mapping.

Review artifacts:

- Prompt: `logs/codex-review/20260604-193707-d536-prompt.md` (design)
- Result: `logs/codex-review/20260604-193707-d536-last-message.md` (design)
