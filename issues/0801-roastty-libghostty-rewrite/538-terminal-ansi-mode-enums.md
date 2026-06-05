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

# Experiment 538: the remaining ANSI mode enums (CursorStyle / StatusDisplay / ModifyKeyFormat / ProtectedMode)

## Description

This experiment completes the `terminal::ansi` enum port (Experiments 536–537)
with the four remaining `terminal/ansi.zig` enums: `CursorStyle` (the `ESC [ q`
VT cursor style), `StatusDisplay` (DECSASD target), `ModifyKeyFormat` (the
`ESC [ > a;b m` modify-key format), and `ProtectedMode` (DECSCA / `ESC V,W`
protection). These are **exhaustive** enums used **by name** (their VT-parameter
mappings are parser-specific and live in the parser), so they are ported as
plain Rust enums — the building-block type definitions the VT layer will use.

(roastty's render-side `cursor::VisualStyle` — `Bar` / `Block` / `Underline` /
`BlockHollow` — is a different abstraction from the protocol-level
`ansi.CursorStyle`, which carries blink state, so `CursorStyle` is genuinely
unported.)

## Upstream behavior

`terminal/ansi.zig`:

```zig
/// Possible cursor styles (ESC [ q)
pub const CursorStyle = lib.Enum(lib.target, &.{
    "default", "blinking_block", "steady_block", "blinking_underline",
    "steady_underline", "blinking_bar", "steady_bar",
});

/// The display to target for status updates (DECSASD).
pub const StatusDisplay = lib.Enum(lib.target, &.{ "main", "status_line" });

/// The possible modify key formats to ESC[>{a};{b}m
pub const ModifyKeyFormat = lib.Enum(lib.target, &.{
    "legacy", "cursor_keys", "function_keys",
    "other_keys_none", "other_keys_numeric_except", "other_keys_numeric",
});

/// The protection modes (DECSCA and ESC V, W).
pub const ProtectedMode = enum { off, iso, dec };
```

`lib.Enum(keys)` makes an enum with those keys in order (sequential values
0..n-1). For the Zig target the numeric value is **not guaranteed stable**
("shouldn't be relied on for serialization"); the enums are used by name. So
each is a plain set of named variants:

- `CursorStyle`: `default`, `blinking_block`, `steady_block`,
  `blinking_underline`, `steady_underline`, `blinking_bar`, `steady_bar`.
- `StatusDisplay`: `main`, `status_line`.
- `ModifyKeyFormat`: `legacy`, `cursor_keys`, `function_keys`,
  `other_keys_none`, `other_keys_numeric_except`, `other_keys_numeric`.
- `ProtectedMode`: `off`, `iso`, `dec`.

## Rust mapping (`roastty/src/terminal/ansi.rs`)

Four plain Rust enums (PascalCase variants; no numeric value, since the Zig
values are not stable wire values — they are used by name):

```rust
/// Possible cursor styles (`ESC [ q`) (upstream `terminal.ansi.CursorStyle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorStyle {
    Default,
    BlinkingBlock,
    SteadyBlock,
    BlinkingUnderline,
    SteadyUnderline,
    BlinkingBar,
    SteadyBar,
}

/// The display to target for status updates (DECSASD) (upstream `terminal.ansi.StatusDisplay`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusDisplay {
    Main,
    StatusLine,
}

/// The modify-key format for `ESC [ > a;b m` (upstream `terminal.ansi.ModifyKeyFormat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModifyKeyFormat {
    Legacy,
    CursorKeys,
    FunctionKeys,
    OtherKeysNone,
    OtherKeysNumericExcept,
    OtherKeysNumeric,
}

/// The terminal protection modes (DECSCA and `ESC V, W`) (upstream `terminal.ansi.ProtectedMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtectedMode {
    Off,
    Iso,
    Dec,
}
```

## Scope / faithfulness notes

- **Ported (bridged)**: the four remaining `terminal.ansi.zig` enums, as plain
  Rust enums in `terminal::ansi`.
- **Faithful**: the exact variant sets, in upstream order. No numeric value is
  exposed because upstream's `lib.Enum` Zig-target values are explicitly not
  stable / not for serialization — these enums are used by name (the
  VT-parameter mappings are parser-specific). `ProtectedMode` is likewise a
  by-name plain enum.
- **Faithful adaptation**: `lib.Enum(keys)` and the plain Zig `enum` → plain
  Rust enums with PascalCase variants matching the upstream keys.
- **Deferred**: the VT-parameter mappings (e.g. the `ESC [ q` parameter →
  `CursorStyle` map) which live in the parser; the rest of the VT layer (`csi`
  types not already in `stream`, `apc` handler, `parse_table`, `Parser`).
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/terminal/ansi.rs`: add `CursorStyle`, `StatusDisplay`,
   `ModifyKeyFormat`, and `ProtectedMode`.
2. Tests (in `ansi.rs`): construct and compare each variant (the exact upstream
   sets) — e.g. each `CursorStyle` variant is distinct, the `ModifyKeyFormat`
   set is the six named formats, `ProtectedMode` is `Off`/`Iso`/`Dec`,
   `StatusDisplay` is `Main`/`StatusLine`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty ansi_mode_enums
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/ansi.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `CursorStyle` / `StatusDisplay` / `ModifyKeyFormat` / `ProtectedMode` have
  exactly the upstream variant sets (in order) — faithful to
  `terminal.ansi.zig`;
- the tests pass (each variant constructible and distinct), and the existing
  tests still pass;
- the VT-parameter mappings and the rest of the VT layer stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant set diverges from upstream, an unrelated
item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. The four variant sets and their order match upstream exactly
(`ansi.zig:54-106`). `lib.Enum` explicitly documents that the Zig enum values
are not guaranteed stable and should not be relied on for serialization, so
omitting `value()` / `from_value()` for these by-name semantic enums is the
right call. Codex confirmed `cursor::VisualStyle` is a different render-side
abstraction (visual shape only) from `ansi::CursorStyle` (protocol-level
cursor-style choices including blink / default variants), so keeping both is
appropriate. Porting the bare type definitions now is acceptable — the
parameter→enum mappings remain deferred to the parser/dispatch consumers where
the actual VT semantics live.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d538-prompt.md` (design)
- Result: `logs/codex-review/20260604-d538-last-message.md` (design)

## Result

**Result:** Pass

`CursorStyle` (7 variants), `StatusDisplay` (2), `ModifyKeyFormat` (6), and
`ProtectedMode` (3) were added to `terminal::ansi` as plain Rust enums with
PascalCase variants matching the upstream keys in order. No numeric `value()` /
`from_value()` is exposed — these enums are used by name and upstream's
`lib.Enum` Zig-target value is not a stable wire value. This completes the
`terminal::ansi` enum port. The new `ansi_mode_enums` test constructs every
variant of each enum, checks the exact variant counts (7 / 2 / 6 / 3), and
confirms all variants are mutually distinct.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3029 passed, 0 failed (one new test; no regressions,
  up from 3028).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/ansi.rs +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **one Nit** (no
Required or Optional findings on the implementation). The Nit: the doc had
`## Result` but no `## Conclusion`, which the issue workflow expects — fixed by
adding the conclusion below. Codex confirmed the four variant sets are faithful
to upstream (`ansi.zig:54-106`), including order (`CursorStyle` 7,
`StatusDisplay` 2, `ModifyKeyFormat` 6, `ProtectedMode` 3); the by-name port
with no `value()` / `from_value()` is correct because `lib.Enum` values are not
stable wire values; `cursor::VisualStyle` remains a separate render-side
abstraction, not a duplicate of `ansi::CursorStyle`; the `ansi_mode_enums` test
adequately verifies the variant sets; and the VT-parameter mappings remain
properly deferred.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r538-prompt.md` (result)
- Result: `logs/codex-review/20260604-r538-last-message.md` (result)

## Conclusion

The `terminal::ansi` enum port is now complete: all six `ansi.zig` enums are
bridged — the three non-exhaustive value enums with stable wire values (`C0`,
`RenditionAspect`, `StatusLineType`; Experiments 536–537) and the four by-name
mode enums (`CursorStyle`, `StatusDisplay`, `ModifyKeyFormat`, `ProtectedMode`;
this experiment). The next VT slices move to the parser/dispatch consumers that
give these enums their parameter mappings — the `csi` types not already in
`stream`, the `apc` handler, `parse_table`, and the `Parser`. The config
`loadDefaultFiles` stays deferred pending roastty's naming decision;
`background-image-opacity` stays float-blocked.
