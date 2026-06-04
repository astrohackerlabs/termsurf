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

# Experiment 516: the last enum-keyword config parsers (from_keyword: ShellIntegration / NotifyOnCommandFinish)

## Description

Completing the plain-enum `from_keyword` sweep (Experiments 513–515), this
experiment adds `from_keyword(value) -> Option<Self>` — the
`std.meta.stringToEnum` parse — to the two remaining plain config enums:
`ShellIntegration` and `NotifyOnCommandFinish`. These are the parse-side inverse
of their already-validated `keyword()`.

The other two members of the shell / notify group, `ShellIntegrationFeatures`
and `NotifyOnCommandFinishAction`, are **packed structs** (bool-flag types, like
`ScrollToBottom`), not enums — they parse via their own flag parser, not
`stringToEnum`, and stay deferred to the packed-struct parse work.

## Upstream behavior

`parseIntoField` (`cli/args.zig:302`) parses an enum field with no custom
`parseCLI` via `std.meta.stringToEnum(Field, value)` — the variant whose tag
name equals `value`, else an error. Both enums have no custom upstream
`parseCLI` (verified), so they parse purely by tag name:

- `ShellIntegration` (`shell-integration`, `Config.zig:8661`): `none`, `detect`,
  `bash`, `elvish`, `fish`, `nushell`, `zsh`.
- `NotifyOnCommandFinish` (`notify-on-command-finish`, `Config.zig:10214`):
  `never`, `unfocused`, `always`.

## Rust mapping (`roastty/src/config/mod.rs`)

Each enum gets `from_keyword(value: &str) -> Option<Self>`, the inverse of its
`keyword()` — an exact `match` on the tag string, else `None` (mirroring
`std.meta.stringToEnum`'s `?Field`):

```rust
impl NotifyOnCommandFinish {
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "never" => Some(NotifyOnCommandFinish::Never),
            "unfocused" => Some(NotifyOnCommandFinish::Unfocused),
            "always" => Some(NotifyOnCommandFinish::Always),
            _ => None,
        }
    }
}
// … the same shape for ShellIntegration (each arm = a keyword() value).
```

## Scope / faithfulness notes

- **Ported (bridged)**: the `stringToEnum` enum parse, as `from_keyword`, for
  the two enums.
- **Faithful**: each maps the exact upstream tag name to its variant and returns
  `None` otherwise — exactly `std.meta.stringToEnum`.
- **Faithful adaptation**: `std.meta.stringToEnum(Field, value)` → an explicit
  `match value { … }` returning `Option<Self>`.
- **Deferred**: the packed-struct parse for `ShellIntegrationFeatures` /
  `NotifyOnCommandFinishAction` (and `ScrollToBottom` / `FontShapingBreak`); the
  enums with custom upstream `parseCLI`; the empty-string reset rule; the bool /
  int / float / string magic paths; the per-field `parseIntoField` dispatch and
  the `loadCli` / file loader.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `from_keyword` to `ShellIntegration` and
   `NotifyOnCommandFinish` (each in its existing `impl`).
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
- the packed-struct parse and the remaining loader pieces stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a tag mapping diverges from upstream, an unrelated
item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed `ShellIntegration` and `NotifyOnCommandFinish` are
plain enums with the exact proposed tags upstream (`Config.zig:8661`/`:10214`)
and, with no custom `parseCLI`, use the generic enum branch
`std.meta.stringToEnum` from `parseIntoField` (`args.zig:442`); the exclusions
are correct — `ShellIntegrationFeatures` and `NotifyOnCommandFinishAction` are
packed structs, not enums, so they should not get `from_keyword`
(`Config.zig:8672`/`:10221`); and the tag mappings are exact with adequate
round-trip + unknown-rejection tests.

Review artifacts:

- Prompt: `logs/codex-review/20260604-171315-d516-prompt.md` (design)
- Result: `logs/codex-review/20260604-171315-d516-last-message.md` (design)

## Result

**Result:** Pass

`from_keyword(value: &str) -> Option<Self>` was added to `ShellIntegration`
(`none`/`detect`/`bash`/`elvish`/`fish`/`nushell`/`zsh`) and
`NotifyOnCommandFinish` (`never`/`unfocused`/`always`), each an exact tag match
(the inverse of `keyword()`) returning `None` otherwise — the
`std.meta.stringToEnum` parse. The new test
`enum_from_keyword_round_trips_shell_notify` round-trips every variant and
rejects an unknown string. With this, every plain config enum now has
`from_keyword`.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3002 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches the approved design — exact
`stringToEnum`-style matching, inverse of `keyword()`, for the two remaining
plain enums; the packed structs are correctly left out of this sweep; the test
is adequate (every tag round-trips, unknowns reject) and the gates are clean
with no deferred-scope spillover. "Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-171444-r516-prompt.md` (result)
- Result: `logs/codex-review/20260604-171444-r516-last-message.md` (result)

## Conclusion

The plain-enum `from_keyword` sweep is complete — every plain config enum (24
across Experiments 513–516) now parses its keyword via `std.meta.stringToEnum`.
The remaining loader work is: the **packed-struct parse**
(`ShellIntegrationFeatures`, `NotifyOnCommandFinishAction`, `ScrollToBottom`,
`FontShapingBreak` — the `[no-]flag` comma-list parser); the bool / int / float
/ string "magic" parse paths; the empty-string reset-to-default rule; and the
per-field `parseIntoField` dispatch (`Config::set(key, value)`) + the `loadCli`
/ file loader — the inverse of `Config::format_config`.
