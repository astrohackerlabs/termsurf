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

# Experiment 519: the bool / string type-magic parse paths (parse_bool_field / parse_string_field)

## Description

Continuing the config loader, this experiment ports two of the **type-magic**
parse paths from upstream `parseIntoField` (`cli/args.zig`) — the branches that
parse a field which has **no `parseCLI`** based purely on its Rust type. This
slice ports the `bool` and string (`[]const u8` / `[:0]const u8`) paths as the
reusable helpers `parse_bool_field` and `parse_string_field`. They are ported
ahead of the per-field dispatch that will call them (the same approach used for
`from_keyword` / `parse_cli`).

The int paths (`u8`…`isize`) and the float paths are **not** included: the int
span is broad (signed + unsigned, base-0) with no current raw-int `Config`
field, and float is float-formatting/parsing blocked (Experiment 509). Both stay
deferred.

## Upstream behavior

The type-magic switch in `parseIntoField` (`cli/args.zig:399`):

```zig
@field(dst, field.name) = switch (Field) {
    []const u8 => value: {                       // string: copy the slice
        const slice = value orelse return error.ValueRequired;
        const buf = try alloc.alloc(u8, slice.len);
        @memcpy(buf, slice);
        break :value buf;
    },
    [:0]const u8 => value: {                      // sentinel string: copy + NUL
        const slice = value orelse return error.ValueRequired;
        // … alloc, copy, NUL-terminate …
    },
    bool => try parseBool(value orelse "t"),      // bare flag ⇒ true
    inline u8, …, isize => … parseInt(…, 0) …,    // (deferred)
    f32, f64 => … parseFloat(…) …,                // (deferred, blocked)
    else => switch (fieldInfo) { … },             // enum/struct/union (done)
};
```

So:

- **bool**: `parseBool(value orelse "t")` — a missing value (a bare `--flag`) is
  `"t"` ⇒ `true`; otherwise `parseBool` of the value (`1`/`t`/`T`/`true` ⇒
  `true`, `0`/`f`/`F`/`false` ⇒ `false`, anything else ⇒ `error.InvalidValue`).
  A bool field is never `ValueRequired`.
- **string** (`[]const u8` / `[:0]const u8`): a missing value is
  `error.ValueRequired`; otherwise an owned **copy** of the value bytes.

(The set-but-empty `value == ""` reset-to-default rule is a _separate_ earlier
branch of `parseIntoField`, not part of this type-magic switch; it stays
deferred.)

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// An error from a type-magic field parse (upstream `error.InvalidValue` /
/// `error.ValueRequired` from `cli.args.parseIntoField`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicParseError {
    InvalidValue,
    ValueRequired,
}

/// Parse a `bool` field (upstream `parseIntoField`'s `bool => parseBool(value
/// orelse "t")`): a missing value is a bare flag ⇒ `true`; otherwise `parse_bool`
/// of the value, with `InvalidValue` for an unrecognized value.
pub(crate) fn parse_bool_field(value: Option<&str>) -> Result<bool, MagicParseError> {
    match value {
        None => Ok(true),
        Some(v) => parse_bool(v).ok_or(MagicParseError::InvalidValue),
    }
}

/// Parse a string field (upstream `parseIntoField`'s `[]const u8` / `[:0]const u8`
/// copy): a missing value is `ValueRequired`; otherwise an owned copy of the value.
pub(crate) fn parse_string_field(value: Option<&str>) -> Result<String, MagicParseError> {
    match value {
        None => Err(MagicParseError::ValueRequired),
        Some(v) => Ok(v.to_string()),
    }
}
```

`parse_bool_field` reuses the existing `parse_bool` (which matches upstream
`parseBool` exactly, Experiment 482); the `value orelse "t"` shortcut is the
`None => Ok(true)` arm. `parse_string_field` returns an owned `String` (upstream
allocs a copy into the config arena; Rust owns the `String`). Upstream's
`error.OutOfMemory` on the allocation has no Rust analog (`String` allocation is
infallible here).

## Scope / faithfulness notes

- **Ported (bridged)**: the `bool` and string type-magic paths of
  `parseIntoField`, as `parse_bool_field` / `parse_string_field`.
- **Faithful**: bare flag (no value) ⇒ `true` for bool; missing value ⇒
  `ValueRequired` for strings; `parse_bool` semantics for a present bool value;
  an owned copy for a present string.
- **Faithful adaptation**: `parseBool(value orelse "t")` →
  `match value { None => Ok(true), Some(v) => parse_bool(v)… }`; the alloc-copy
  → `v.to_string()`; `error.{InvalidValue,ValueRequired}` → `MagicParseError`.
- **Deferred**: the int (`u8`…`isize`, base-0) and float type-magic paths (float
  blocked, Experiment 509); the set-but-empty reset-to-default rule; the
  per-field `parseIntoField` dispatch (`Config::set(key, value)`) and the
  `loadCli` / file loader.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `MagicParseError`, `parse_bool_field`, and
   `parse_string_field`.
2. Tests (in `config/mod.rs`): `parse_bool_field(None) == Ok(true)`; `Some("1")`
   / `Some("true")` ⇒ `true`; `Some("0")` / `Some("false")` ⇒ `false`;
   `Some("x")` ⇒ `Err(InvalidValue)`.
   `parse_string_field(None) == Err(ValueRequired)`;
   `Some("hi") == Ok("hi".into())`; `Some("") == Ok("".into())`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty parse_bool_field
cargo test -p roastty parse_string_field
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `parse_bool_field` / `parse_string_field` match the upstream `bool` / string
  type-magic paths (bare flag ⇒ `true`; missing string ⇒ `ValueRequired`;
  `parse_bool` semantics; owned copy);
- the tests pass (bare flag, true/false values, invalid value, missing string,
  present + empty string), and the existing tests still pass;
- the int / float magic, the empty-reset rule, and the dispatch stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a path diverges from upstream, an unrelated item
changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Low** finding (folded into this experiment's tests): add direct helper tests
for the two edge cases that differ once the later dispatch adds the empty-string
reset — `parse_bool_field(Some("")) -> InvalidValue` and
`parse_string_field(Some("")) -> Ok("")`, plus the `None` cases for both.

Codex confirmed everything else is faithful: upstream has the empty-string reset
_before_ the type-magic switch (`args.zig:326`); the string branches require
`value` and copy the slice (`args.zig:401`/`:408`); the bool branch is exactly
`parseBool(value orelse "t")` (`args.zig:416`), with `parseBool` exact-token
matching for `1`/`t`/`T`/`true` and `0`/`f`/`F`/`false` (`args.zig:654`). So
`None -> true` for bool (bool never `ValueRequired`), `None -> ValueRequired`
for string, and the `Some("")` reset belonging to the outer dispatch (not these
helpers in isolation) are all correct; omitting Zig's allocator `OutOfMemory` is
acceptable for this Rust helper shape.

Review artifacts:

- Prompt: `logs/codex-review/20260604-180052-d519-prompt.md` (design)
- Result: `logs/codex-review/20260604-180052-d519-last-message.md` (design)

## Result

**Result:** Pass

`MagicParseError`, `parse_bool_field`, and `parse_string_field` were added. The
bool path matches `parseBool(value orelse "t")` — `None` (a bare flag) is
`true`, a recognized value parses, and an unrecognized value is `InvalidValue`;
the string path requires a value (`ValueRequired` when absent) and otherwise
returns an owned copy. The new test `parse_bool_field_and_string_field` covers
the bare flag, every recognized bool token, an invalid value, the missing
string, and the folded `Some("")` edge cases (bool → `InvalidValue`, string →
`Ok("")`) that distinguish these helpers from the future empty-string reset
branch.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3005 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches the two upstream type-magic paths — bool
uses `parseBool(value orelse "t")` (so `None` is `true` and invalid/empty
explicit values are `InvalidValue`), and string requires a value and copies it
(incl. an explicit empty string); the folded edge tests cover the important
separation from the future empty-string reset branch; the gates are clean and
the deferred scope is intact. "Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-180347-r519-prompt.md` (result)
- Result: `logs/codex-review/20260604-180347-r519-last-message.md` (result)

## Conclusion

The bool and string type-magic parse paths are ported. The remaining loader work
is: the **int** type-magic path (`u8`…`isize`, base-0 — to port when a raw-int
`Config` field exists; float stays blocked); the set-but-empty
**reset-to-default** rule; and the per-field `parseIntoField` **dispatch**
(`Config::set(key, value)`) + the `loadCli` / file loader — the inverse of
`Config::format_config`, wiring together `from_keyword`, the packed-struct
`parse_cli`, the leaf `parse_cli`, and these magic helpers.
