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

# Experiment 515: more enum-keyword config parsers (from_keyword: the misc + fullscreen enums)

## Description

Continuing the config loader (Experiments 513–514), this experiment adds
`from_keyword(value) -> Option<Self>` — the `std.meta.stringToEnum` parse — to
the next batch of plain enums: the small misc enums (Experiment 505) and the
fullscreen-style enums (Experiment 503). These are the parse-side inverse of
their already-validated `keyword()`.

## Upstream behavior

`parseIntoField` (`cli/args.zig:302`) parses an enum field with no custom
`parseCLI` via `std.meta.stringToEnum(Field, value)` — the variant whose tag
name equals `value`, else an error. The seven enums in this batch have no custom
upstream `parseCLI` (verified), so they parse purely by tag name. Their tags (=
their `keyword()` values, validated in Experiments 503 / 505):

- `OscColorReportFormat` (`osc-color-report-format`): `none`, `8-bit`, `16-bit`.
- `ConfirmCloseSurface` (`confirm-close-surface`): `false`, `true`, `always`.
- `LinkPreviews` (`link-previews`): `false`, `true`, `osc8`.
- `WindowSubtitle` (`window-subtitle`): `false`, `working-directory`.
- `WindowPaddingColor` (`window-padding-color`): `background`, `extend`,
  `extend-always`.
- `Fullscreen` (`fullscreen`): `false`, `true`, `non-native`,
  `non-native-visible-menu`, `non-native-padded-notch`.
- `NonNativeFullscreen` (`macos-non-native-fullscreen`): `false`, `true`,
  `visible-menu`, `padded-notch`.

`stringToEnum` matches the exact tag — the bool-like `false` / `true` tags (of
`ConfirmCloseSurface` / `LinkPreviews` / `WindowSubtitle` / `Fullscreen` /
`NonNativeFullscreen`) and the digit-led `8-bit` / `16-bit` are matched only as
their literal tag strings (the enum-tag path, not the `bool` `parseBool` path).

## Rust mapping (`roastty/src/config/mod.rs`)

Each enum gets `from_keyword(value: &str) -> Option<Self>`, the inverse of its
`keyword()` — an exact `match` on the tag string, else `None` (mirroring
`std.meta.stringToEnum`'s `?Field`). For example:

```rust
impl OscColorReportFormat {
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "none" => Some(OscColorReportFormat::None),
            "8-bit" => Some(OscColorReportFormat::Bits8),
            "16-bit" => Some(OscColorReportFormat::Bits16),
            _ => None,
        }
    }
}
// … the same shape for the other six enums (each arm = a keyword() value).
```

## Scope / faithfulness notes

- **Ported (bridged)**: the `stringToEnum` enum parse, as `from_keyword`, for
  the seven enums.
- **Faithful**: each maps the exact upstream tag name to its variant and returns
  `None` otherwise — exactly `std.meta.stringToEnum`. The bool-like `false` /
  `true` and digit-led `8-bit` / `16-bit` tags match only as literal tag
  strings.
- **Faithful adaptation**: `std.meta.stringToEnum(Field, value)` → an explicit
  `match value { … }` returning `Option<Self>`.
- **Deferred**: `from_keyword` for the remaining enums (shell-integration /
  notify groups); the enums with custom upstream `parseCLI`; the empty-string
  reset rule; the bool / int / float / string magic paths; the per-field
  `parseIntoField` dispatch and the `loadCli` / file loader.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `from_keyword` to the seven enums (each in
   its existing `impl`).
2. Tests (in `config/mod.rs`): for each enum, every tag round-trips
   (`from_keyword(v.keyword()) == Some(v)`) and an unknown string is `None`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty from_keyword
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- each enum's `from_keyword` returns the variant for the exact tag and `None`
  otherwise — faithful to `std.meta.stringToEnum`;
- the tests pass (round-trip every tag + an unknown → `None`), and the existing
  tests still pass;
- the remaining loader pieces stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a tag mapping diverges from upstream, an unrelated
item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the seven enums are plain definitions with the proposed
tag names upstream
(`Config.zig:5235`/`:5253`/`:5263`/`:5271`/`:5277`/`:5282`/`:8966`) and have no
custom `parseCLI`, so the generic enum path applies. `parseIntoField` dispatches
actual `bool` fields through `parseBool` but enum fields through exact
`std.meta.stringToEnum` (`args.zig:416`/`:442`), so `false` / `true` and `8-bit`
/ `16-bit` are literal enum tags only (no `1`/`t`/`0`/`f` aliasing). The
mappings are exact — including `OSCColorReportFormat` `none` / `8-bit` /
`16-bit`, `LinkPreviews::Osc8 -> "osc8"`, and the fullscreen kebab-case tags —
and the round-trip plus unknown-rejection tests are adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-170913-d515-prompt.md` (design)
- Result: `logs/codex-review/20260604-170913-d515-last-message.md` (design)
