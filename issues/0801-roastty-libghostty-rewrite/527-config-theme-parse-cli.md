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

# Experiment 527: Theme::parse_cli and the theme Config::set arm

## Description

This experiment wraps `Theme::parse_auto_struct` (Experiment 526) as
`Theme::parse_cli` — upstream `Theme.parseCLI` — and wires the `theme` field
into `Config::set` via the existing `set_optional_value_field`. `theme` is the
last parseable field; with it, `Config::set` routes **43 of 44** fields (only
the float-blocked `background-image-opacity` remains).

## Upstream behavior

`Theme.parseCLI` (`Config.zig:9852`):

```zig
pub fn parseCLI(self: *Theme, alloc, input_: ?[]const u8) !void {
    const input = input_ orelse return error.ValueRequired;
    if (input.len == 0) return error.ValueRequired;
    // (Windows: a colon at index 1 is a drive letter; macOS: any ':' counts.)
    const has_colon = std.mem.indexOf(u8, input, ":") != null;
    if (std.mem.indexOf(u8, input, ",") != null or
        std.mem.indexOf(u8, input, "=") != null or
        has_colon)
    {
        self.* = try cli.args.parseAutoStruct(Theme, alloc, input, null);
        return;
    }
    const trimmed = std.mem.trim(u8, input, cli.args.whitespace);  // " \t"
    self.* = .{ .light = try alloc.dupeZ(u8, trimmed), .dark = self.light };
}
```

So `Theme.parseCLI`:

- a missing value (`None`) or an empty value (`""`) ⇒ `error.ValueRequired`.
- if the value contains `,`, `=`, or `:` ⇒ the **light/dark pair** form, parsed
  by `parseAutoStruct` (Experiment 526).
- otherwise ⇒ the **single-name** form: trim `" \t"`, and set
  `light = dark = trimmed`.

(On macOS `has_colon` is simply "contains `:`"; per the macOS-only directive,
the Rust port resolves to that arm.)

In `Config::set`, `theme` is an **optional** field (`?Theme`), so the dispatch
uses the optional-as-child + empty-reset path: `value == Some("")` ⇒ reset to
the default (`None`); otherwise `Some(Theme::parse_cli(value))`. (`parseCLI`'s
own `len == 0` check is for direct calls; the dispatch intercepts `""` with the
reset first.)

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
// Extend ThemeParseError with ValueRequired (Experiment 526 added `Invalid`).
pub(crate) enum ThemeParseError {
    Invalid,
    ValueRequired,
}

impl Theme {
    /// Parse the `theme` value (upstream `Theme.parseCLI`): a missing or empty
    /// value is `ValueRequired`; a value with `,` / `=` / `:` is the light/dark
    /// pair (`parse_auto_struct`); otherwise the single-name form (`light = dark =
    /// trimmed`).
    pub(crate) fn parse_cli(value: Option<&str>) -> Result<Theme, ThemeParseError> {
        let input = value.ok_or(ThemeParseError::ValueRequired)?;
        if input.is_empty() {
            return Err(ThemeParseError::ValueRequired);
        }
        if input.contains(',') || input.contains('=') || input.contains(':') {
            return Theme::parse_auto_struct(input);
        }
        let trimmed = input.trim_matches(|c: char| c == ' ' || c == '\t');
        Ok(Theme::single(trimmed.to_string()))
    }
}

impl From<ThemeParseError> for ConfigSetError {
    fn from(e: ThemeParseError) -> Self {
        match e {
            ThemeParseError::Invalid => ConfigSetError::InvalidValue,
            ThemeParseError::ValueRequired => ConfigSetError::ValueRequired,
        }
    }
}
```

New `Config::set` arm (added before the `_ =>` catch-all), reusing
`set_optional_value_field`:

```rust
"theme" => self.theme = set_optional_value_field(value, default.theme, Theme::parse_cli)?,
```

`set_optional_value_field` (Experiment 523) gives `Some("")` ⇒ reset to
`default.theme` (`None`), and otherwise `Some(Theme::parse_cli(value)?)` —
exactly upstream's optional-as-child + empty-reset, with `parse_cli` handling
`None` ⇒ `ValueRequired`.

## Scope / faithfulness notes

- **Ported (bridged)**: `Theme.parseCLI`, as `Theme::parse_cli`; the `theme`
  `Config::set` arm; `From<ThemeParseError> for ConfigSetError`.
- **Faithful**: `None`/empty ⇒ `ValueRequired`; a `,`/`=`/`:` value ⇒ the pair
  form (`parse_auto_struct`); otherwise the single-name form (trim `" \t"`,
  `light = dark = trimmed`); the `theme` dispatch is optional-as-child with the
  empty-reset to `None`. The macOS `has_colon` is "contains `:`". The
  `ThemeParseError` ⇒ `ConfigSetError` mapping preserves `InvalidValue` /
  `ValueRequired`.
- **Deferred**: the `loadCli` / config-file loader; `background-image-opacity`
  stays float-blocked. With `theme` wired, `Config::set` routes 43 of 44 fields.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `ValueRequired` to `ThemeParseError`,
   `Theme::parse_cli`, `From<ThemeParseError> for ConfigSetError`, and the
   `theme` `Config::set` arm.
2. Tests (in `config/mod.rs`): `parse_cli` — a single name (`catppuccin-mocha` ⇒
   `light = dark`); whitespace trimmed; a pair (`light:day,dark:night`); a value
   with `=` routes to the pair parser (and fails, since `=` is not a valid
   auto-struct separator); `None` / `""` ⇒ `ValueRequired`. The `theme`
   `Config::set` arm: a single name and a pair route (verified via
   `format_config`); `Some("")` resets to `None` (the void line); `None` ⇒
   `ValueRequired`; an invalid pair ⇒ `InvalidValue`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty theme_parse_cli
cargo test -p roastty config_set
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Theme::parse_cli` matches upstream `parseCLI` (`None`/empty ⇒
  `ValueRequired`; `,`/`=`/`:` ⇒ pair; else single-name), and the `theme`
  `Config::set` arm routes via `set_optional_value_field` with the reset /
  `ValueRequired` / `InvalidValue` semantics;
- the tests pass (single, pair, whitespace, reset, missing, invalid), and the
  existing tests still pass;
- the loader stays deferred and `background-image-opacity` stays the only
  unrouted field;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the parse diverges from upstream, a key is
mis-mapped, an unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed: the macOS colon logic is correct (upstream
special-cases drive-letter colons only on Windows; non-Windows `has_colon` is
"contains `:`", `Config.zig:9863`); the pair trigger is exactly `,` / `=` / `:`,
where `=` intentionally routes into `parseAutoStruct` and then fails because the
pair parser requires `:` (`Config.zig:9856`/`:9867`); the single-name branch is
faithful — trim `" \t"` and set `light` and `dark` to the trimmed value
(`Config.zig:9880`); and the `Config::set` arm via `set_optional_value_field` is
correct — upstream optional fields are parsed as the child type and the
empty-string reset runs before `parseCLI`, so `Some("") -> None`,
`None -> Theme::parse_cli(None) -> ValueRequired`, and non-empty values wrap as
`Some(Theme)` (`args.zig:314`/`:326`/`:381`).

Review artifacts:

- Prompt: `logs/codex-review/20260604-185141-d527-prompt.md` (design)
- Result: `logs/codex-review/20260604-185141-d527-last-message.md` (design)

## Result

**Result:** Pass

`ThemeParseError` gained `ValueRequired`; `Theme::parse_cli`,
`From<ThemeParseError> for ConfigSetError`, and the `theme` `Config::set` arm
were added. `parse_cli` matches upstream `Theme.parseCLI` (missing/empty ⇒
`ValueRequired`; `,`/`=`/`:` ⇒ the pair form; else the single-name form, trim
`" \t"` and `light = dark = trimmed`); the `theme` arm uses
`set_optional_value_field` (reset to `None` on `Some("")`, `None` ⇒
`ValueRequired`, else `Some(parse)`). `theme` was the last parseable field —
`Config::set` now routes **43 of 44** fields (only the float-blocked
`background-image-opacity` remains). Two new tests cover `parse_cli` (single /
pair / whitespace / `=` typo route / missing / empty) and the `theme`
`Config::set` arm (single, pair, reset, missing, invalid).

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3016 passed, 0 failed (two new tests; no
  regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches upstream `Theme.parseCLI` for macOS —
missing/empty ⇒ `ValueRequired`, any `,` / `=` / `:` routes to the pair parser,
otherwise the single-name branch trims `" \t"` and sets both light/dark; the
`theme` `Config::set` arm is faithful to optional-as-child + empty-reset
(`Some("")` ⇒ `None`, `None` ⇒ `ValueRequired` via `parse_cli`, non-empty ⇒
`Some(Theme)`); the tests cover the key behaviors (the `=` typo route,
reset-vs-missing, invalid pair, format verification); gates are clean. "Approved
with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-185355-r527-prompt.md` (result)
- Result: `logs/codex-review/20260604-185355-r527-last-message.md` (result)

## Conclusion

`Config::set` now routes **43 of the 44** `Config` fields — every field except
the float-blocked `background-image-opacity`. The per-field config **loader** is
complete: both directions (`Config::format_config` and `Config::set`) plus all
the leaf parsers/formatters. The remaining config work is the top-level
**`loadCli` / config-file loader** — splitting a config source into
`key = value` lines (comments, trimming, the `--key=value` CLI form) and driving
`Config::set` per line. After that, the entire non-config rewrite remains.
