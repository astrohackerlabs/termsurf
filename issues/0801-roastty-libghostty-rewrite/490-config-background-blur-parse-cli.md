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

# Experiment 490: the config BackgroundBlur CLI parser (BackgroundBlur::parse_cli)

## Description

The `BackgroundBlur` enum is already ported in roastty (`False` / `True` /
`MacosGlassRegular` / `MacosGlassClear` / `Radius(u8)`, with `enabled` /
`is_macos_glass`), but its **parser** is not. This experiment adds
`BackgroundBlur::parse_cli` (upstream `BackgroundBlur.parseCLI`) — the
`background-blur` config: a missing value or a boolean resolves to on/off, the
two `macos-glass-*` keywords select a glass style, and any other value is parsed
as a base-0 `u8` blur radius. It reuses the shared `parse_bool` (Experiment 482)
and the base-0 `parse_uint` (Experiment 488). The `cval` C sentinel and
`formatEntry` formatter stay deferred.

## Upstream behavior

In `config/Config.zig`, `Config.BackgroundBlur.parseCLI`:

```zig
pub fn parseCLI(self: *BackgroundBlur, input: ?[]const u8) !void {
    const input_ = input orelse {
        // Emulate behavior for bools
        self.* = .true;
        return;
    };

    // Try to parse normal bools
    if (cli.args.parseBool(input_)) |b| {
        self.* = if (b) .true else .false;
        return;
    } else |_| {}

    // Try to parse enums (the void variants)
    if (std.meta.stringToEnum(std.meta.Tag(BackgroundBlur), input_)) |v| switch (v) {
        inline else => |tag| tag: {
            const info = std.meta.fieldInfo(BackgroundBlur, tag);
            if (info.type != void) break :tag; // skip `radius` (non-void)
            self.* = @unionInit(BackgroundBlur, @tagName(tag), {});
            return;
        },
    };

    self.* = .{ .radius = std.fmt.parseInt(u8, input_, 0) catch return error.InvalidValue };
}
```

- A **missing** value emulates the bool default: `.true` (not an error).
- A boolean (via `parseBool`) resolves to `.true` / `.false`.
- Otherwise the value is matched against the **void** variant names. Of those,
  `false` / `true` were already handled by `parseBool`, and `radius` is non-void
  (skipped), so this step effectively selects `macos-glass-regular` /
  `macos-glass-clear`.
- Anything else is a `radius`, a **base-0** `u8` (`parseInt(u8, _, 0)`); a parse
  error (bad digit or overflow) is `error.InvalidValue`.

Upstream's tests: `null` → `true`; `"true"` → `true`; `"false"` → `false`;
`"42"` → `radius 42`; `"macos-glass-regular"` / `"macos-glass-clear"` → those
variants; `""` / `"aaaa"` → `error.InvalidValue`; `"420"` → `error.InvalidValue`
(overflows `u8`).

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// An error parsing `BackgroundBlur` (upstream `error.InvalidValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundBlurParseError {
    /// The value is neither a boolean, a glass keyword, nor a base-0 `u8` radius.
    InvalidValue,
}

impl BackgroundBlur {
    /// Parse the `background-blur` value (upstream `parseCLI`): a missing value or a
    /// boolean resolves on/off (`true` / `false`); `macos-glass-regular` /
    /// `macos-glass-clear` select a glass style; anything else is a base-0 `u8`
    /// radius (a bad digit or overflow is `InvalidValue`).
    pub(crate) fn parse_cli(&mut self, input: Option<&str>) -> Result<(), BackgroundBlurParseError> {
        let Some(input) = input else {
            *self = BackgroundBlur::True; // emulate the bool default
            return Ok(());
        };

        if let Some(b) = parse_bool(input) {
            *self = if b { BackgroundBlur::True } else { BackgroundBlur::False };
            return Ok(());
        }

        match input {
            "macos-glass-regular" => {
                *self = BackgroundBlur::MacosGlassRegular;
                return Ok(());
            }
            "macos-glass-clear" => {
                *self = BackgroundBlur::MacosGlassClear;
                return Ok(());
            }
            _ => {}
        }

        let radius = parse_uint(input, 0, 0xFF).map_err(|_| BackgroundBlurParseError::InvalidValue)?;
        *self = BackgroundBlur::Radius(radius as u8);
        Ok(())
    }
}
```

`parse_cli` mirrors upstream: the missing-value `True` default, the
boolean-first resolution (`parse_bool`), the two `macos-glass-*` keyword
variants (the only void tags not already a boolean), and the base-0 `u8` radius
fallback with every parse error → `InvalidValue`. The radius uses the shared
`parse_uint(_, 0, 0xFF)` — the exact `parseInt(u8, _, 0)` equivalent (Experiment
488). The existing `enabled` / `is_macos_glass` methods are unchanged.

## Scope / faithfulness notes

- **Ported (bridged)**: `BackgroundBlur::parse_cli` (upstream
  `BackgroundBlur.parseCLI`) and `BackgroundBlurParseError`. The
  `BackgroundBlur` enum and `enabled` / `is_macos_glass` already exist
  (unchanged).
- **Faithful**: the missing-value `True` default (not an error); the
  boolean-first resolution; the `macos-glass-regular` / `macos-glass-clear`
  keyword selection (the void tags that survive after `parseBool`); the base-0
  `u8` radius fallback with a parse error → `InvalidValue` — exactly upstream's
  `parseCLI`. Note `"0"` / `"1"` are booleans (via `parse_bool`) → `False` /
  `True`, not `Radius(0/1)`, matching upstream's `parseBool`-first order.
- **Faithful adaptation**: `?[]const u8` → `Option<&str>`; `cli.args.parseBool`
  → the shared `parse_bool`; the `stringToEnum` over void tags → an explicit
  match on the two glass keywords (the others are booleans / non-void);
  `parseInt(u8, _, 0)` → `parse_uint(_, 0, 0xFF)`; the one upstream error →
  `BackgroundBlurParseError`.
- **Deferred**: `BackgroundBlur.cval` (the `i16` C sentinel: `false`→0,
  `true`→20, `radius`→v, glass→-1/-2; FFI) and `BackgroundBlur.formatEntry`
  (depends on the not-yet-ported config `EntryFormatter`), and the broader
  config parser/formatter. (Consumed by later slices; this experiment lands the
  parser.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `BackgroundBlurParseError { InvalidValue }`
   and `BackgroundBlur::parse_cli` (in the existing `impl BackgroundBlur`).
2. Tests (in `config/mod.rs`):
   - mirror upstream's `parse BackgroundBlur` test: `None` → `True`; `"true"` →
     `True`; `"false"` → `False`; `"42"` → `Radius(42)`; `"macos-glass-regular"`
     → `MacosGlassRegular`; `"macos-glass-clear"` → `MacosGlassClear`; `""` /
     `"aaaa"` → `InvalidValue`; `"420"` → `InvalidValue` (overflow).
   - the `parse_bool`-first order and base-0 radius: `"1"` → `True`, `"0"` →
     `False` (booleans, not `Radius`); `"5"` → `Radius(5)`; `"0x10"` →
     `Radius(16)` (base-0).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty background_blur
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `BackgroundBlur::parse_cli` resolves a missing value / boolean to `True` /
  `False`, the two glass keywords to their variants, and anything else to a
  base-0 `u8` `Radius` (a bad digit / overflow → `InvalidValue`) — faithful to
  upstream's `parseCLI`;
- the tests pass (the upstream cases; the bool-first / base-0 radius cases), and
  the existing tests still pass;
- `cval` / `formatEntry` and the broader config parser/formatter stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a value is parsed wrong (wrong missing/bool default,
wrong glass keyword, a non-base-0 radius, a `"0"`/`"1"` taken as a radius), an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the design matches upstream `BackgroundBlur.parseCLI`:
`None` sets `True`, `parseBool` runs first, only the two macOS glass void tags
need explicit string handling, and the fallback radius uses
`parseInt(u8, input, 0)` semantics with all parse errors collapsed to
`InvalidValue` (`Config.zig:9676`/ `:9683`/`:9707`); the `parseBool`-first
ordering correctly makes `"0"` / `"1"` booleans rather than radii; reducing the
void-tag enum path to the two glass strings is faithful (`false` / `true`
handled earlier, `radius` non-void); and deferring `cval` / `formatEntry` is the
right scope.

Review artifacts:

- Prompt: `logs/codex-review/20260604-150020-d490-prompt.md` (design)
- Result: `logs/codex-review/20260604-150020-d490-last-message.md` (design)

## Result

**Result:** Pass

`BackgroundBlur::parse_cli` and `BackgroundBlurParseError` were added to the
existing `impl BackgroundBlur` exactly as designed — the missing-value `True`
default, the `parse_bool`-first resolution, the two `macos-glass-*` keyword
variants, and the base-0 `u8` radius fallback (via `parse_uint(_, 0, 0xFF)`)
with every error → `InvalidValue`. The existing `enabled` / `is_macos_glass` are
unchanged. The new test
`background_blur_parse_cli_resolves_bool_glass_and_radius` covers the upstream
cases plus the `parse_bool`-first `"0"` / `"1"` and the base-0 radius behavior.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 2974 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches upstream `BackgroundBlur.parseCLI`
(`None` → `True`, bool parsing first, the two macOS glass tags handled as void
variants, and the radius fallback using base-0 `u8` parsing with all failures →
`InvalidValue` — `Config.zig:9676`/`:9707`); the tests cover the upstream cases
plus the bool-first `"0"` / `"1"` and the base-0 radius behavior; gates are
clean. "Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-150302-r490-prompt.md` (result)
- Result: `logs/codex-review/20260604-150302-r490-last-message.md` (result)

## Conclusion

`BackgroundBlur` now parses — the second consumer of the shared `parse_bool`
(Experiment 482) and another consumer of the base-0 `parse_uint` (Experiment
488), combining a bool, two keywords, and an integer radius. The config parse
layer now spans fourteen value types plus the reusable parsing helpers. The next
slice can port another self-contained value type, the font `CodepointMap`
storage (toward `RepeatableCodepointMap`), or begin the per-field parser
dispatch / the config `EntryFormatter` (which unblocks the deferred
`formatEntry` / `cval` sides), continuing toward the full config loader.
