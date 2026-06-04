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

# Experiment 483: the config RepeatableString CLI parser (RepeatableString::parse_cli)

## Description

This experiment ports `RepeatableString` (upstream `Config.RepeatableString`) —
the accumulating string-list value used by repeatable config keys (e.g.
`font-family`). Unlike the comma-split `ColorList`, each `parseCLI` call appends
one whole string; repeated keys accumulate. An empty value resets the list, and
an `overwrite_next` flag (used so a CLI `font-family` replaces the config-file
list once) clears before the next append. The `formatEntry` formatter and the
C/FFI null-terminated string detail stay deferred.

(`QuickTerminalSize` — the originally-considered slice — was deferred: its `%`
path needs a faithful `std.fmt.parseFloat` port, including hex floats and `_`
separators, which is a substantial sub-task on its own; the design review
flagged a narrowed `f32` parse as a real divergence.)

## Upstream behavior

In `config/Config.zig`, `Config.RepeatableString`:

```zig
pub const RepeatableString = struct {
    // Allocator for the list is the arena for the parent config.
    list: std.ArrayListUnmanaged([:0]const u8) = .{},

    // If true, then the next value will clear the list and start over
    // rather than append. This is a bit of a hack but is here to make
    // the font-family set of configurations work with CLI parsing.
    overwrite_next: bool = false,

    pub fn parseCLI(self: *Self, alloc: Allocator, input: ?[]const u8) !void {
        const value = input orelse return error.ValueRequired;

        // Empty value resets the list
        if (value.len == 0) {
            self.list.clearRetainingCapacity();
            return;
        }

        // If we're overwriting then we clear before appending
        if (self.overwrite_next) {
            self.list.clearRetainingCapacity();
            self.overwrite_next = false;
        }

        const copy = try alloc.dupeZ(u8, value);
        try self.list.append(alloc, copy);
    }

    pub fn count(self: Self) usize {
        return self.list.items.len;
    }
    // ...
};
```

- A missing value is `error.ValueRequired`.
- An **empty** value (`""`) resets the list (`clearRetainingCapacity`) and
  returns — it does _not_ append an empty string.
- If `overwrite_next` is set, the list is cleared and the flag reset before the
  append (so the first post-overwrite value starts a fresh list).
- Otherwise the whole value is appended (duplicated into the parent arena).
  There is **no** comma splitting — each call appends exactly one string;
  repeated config keys accumulate.
- `count` returns the number of items.

(`overwrite_next` is set elsewhere — by the CLI argument machinery — so a
`font-family` given on the command line replaces, rather than extends, the
config-file list. This experiment ports the flag's effect in `parseCLI`; nothing
in this slice sets it yet.)

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// An error parsing a `RepeatableString` (upstream `error.ValueRequired`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepeatableStringParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
}

/// An accumulating string-list config (upstream `Config.RepeatableString`): each
/// parse appends one value; an empty value resets the list; `overwrite_next`
/// clears before the next append. The `formatEntry` formatter is ported later.
#[derive(Debug, Default)]
pub(crate) struct RepeatableString {
    pub list: Vec<String>,
    pub overwrite_next: bool,
}

// `Clone` and `PartialEq`/`Eq` follow upstream's `clone` / `equal` (which copy /
// compare only `list`, dropping / ignoring `overwrite_next`), so they are
// implemented manually rather than derived.
impl Clone for RepeatableString {
    fn clone(&self) -> Self {
        // Upstream `clone` returns `.{ .list = list }`: `overwrite_next` resets to
        // its default `false`.
        RepeatableString {
            list: self.list.clone(),
            overwrite_next: false,
        }
    }
}

impl PartialEq for RepeatableString {
    fn eq(&self, other: &Self) -> bool {
        // Upstream `equal` compares only the list contents.
        self.list == other.list
    }
}

impl Eq for RepeatableString {}

impl RepeatableString {
    /// Parse one repeatable string value (upstream `RepeatableString.parseCLI`):
    /// a missing value is `ValueRequired`; an empty value resets the list; an
    /// `overwrite_next` clears before appending (and resets the flag); otherwise
    /// the value is appended.
    pub(crate) fn parse_cli(&mut self, input: Option<&str>) -> Result<(), RepeatableStringParseError> {
        let value = input.ok_or(RepeatableStringParseError::ValueRequired)?;

        // An empty value resets the list.
        if value.is_empty() {
            self.list.clear();
            return Ok(());
        }

        // If we're overwriting, clear before appending.
        if self.overwrite_next {
            self.list.clear();
            self.overwrite_next = false;
        }

        self.list.push(value.to_string());
        Ok(())
    }

    /// The number of items in the list (upstream `RepeatableString.count`).
    pub(crate) fn count(&self) -> usize {
        self.list.len()
    }
}
```

`parse_cli` mirrors upstream: the `ValueRequired` guard, the empty-value reset
(no append), the `overwrite_next` clear-and-reset before the append, and the
plain append otherwise (one whole value per call, no splitting). `count` mirrors
upstream's `count`.

## Scope / faithfulness notes

- **Ported (bridged)**: the config `RepeatableString` struct (`list` +
  `overwrite_next`), `RepeatableString::parse_cli` (upstream `parseCLI`) and
  `count`, plus `RepeatableStringParseError`.
- **Faithful**: the `ValueRequired` guard; the empty-value reset (which does not
  append); the `overwrite_next` clear-and-reset before the append; the
  one-value-per-call append (no comma splitting); `count` — exactly upstream's
  `parseCLI` / `count`.
- **Faithful adaptation**: `?[]const u8` → `Option<&str>`; the arena-backed
  `std.ArrayListUnmanaged([:0]const u8)` → `Vec<String>` (the null-terminated C
  string detail is a deferred FFI concern); `clearRetainingCapacity` →
  `Vec::clear`; the one upstream error → `RepeatableStringParseError`.
- **Faithful value semantics (manual `Clone` / `PartialEq` / `Eq`)**: upstream's
  `clone` copies only the list (so `overwrite_next` resets to `false`), and
  `equal` compares only the list contents (ignoring `overwrite_next`). A derived
  `Clone` would preserve `overwrite_next` and a derived `PartialEq`/`Eq` would
  include it, so these are implemented by hand to match upstream (folded in from
  the design review).
- **Deferred**: `RepeatableString.formatEntry` (renders each item, or an empty
  field; depends on the not-yet-ported config `EntryFormatter`), the
  null-terminated (`[:0]`) C-string representation, and the machinery elsewhere
  that _sets_ `overwrite_next`; and the broader config parser/formatter.
  (Consumed by later slices; this experiment lands the parser.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `RepeatableStringParseError { ValueRequired }`, the `RepeatableString`
     struct (`list: Vec<String>`, `overwrite_next: bool`,
     `derive(Debug, Default)` with manual `Clone` / `PartialEq` / `Eq` matching
     upstream's `clone` / `equal`), `RepeatableString::parse_cli`, and `count`.
2. Tests (in `config/mod.rs`):
   - accumulation: `parse_cli(Some("a"))` then `Some("b")` → `["a", "b"]`,
     `count() == 2`.
   - missing value: `None` → `Err(ValueRequired)`.
   - empty-value reset: after building a list, `parse_cli(Some(""))` → empty
     list, and a reset does not append an empty string.
   - `overwrite_next`: set the flag, `parse_cli(Some("c"))` clears then appends
     (`["c"]`) and resets the flag, so the next `parse_cli(Some("d"))` appends
     (`["c", "d"]`).
   - empty-reset order (design-review Low): with `overwrite_next = true`,
     `parse_cli(Some(""))` clears the list but leaves `overwrite_next` still
     `true` (upstream returns before the overwrite block); the next non-empty
     parse then clears-and-resets.
   - value semantics (design-review Required): `clone` of a list with
     `overwrite_next = true` yields the same list with
     `overwrite_next == false`; two `RepeatableString`s with equal lists but
     different `overwrite_next` compare **equal**.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty repeatable_string
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `RepeatableString::parse_cli` appends one value per call, resets on an empty
  value, clears-and-resets on `overwrite_next`, and returns `ValueRequired` on a
  missing value (with `count` returning the length) — faithful to upstream's
  `parseCLI` / `count`;
- the tests pass (accumulation; missing-value; empty-reset; `overwrite_next`),
  and the existing tests still pass;
- `formatEntry`, the C-string detail, and the broader config parser/formatter
  stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the list behavior is wrong (wrong
append/reset/overwrite semantics, comma-splitting introduced, an empty value
appended), a missing value does not error, an unrelated item changes, or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation across two rounds.

**Round 1 — one Required finding (fixed) + one Low (folded in).** Codex
confirmed the parser behavior faithful (`None` → `ValueRequired`; empty resets
without appending; non-empty appends one whole string with no comma splitting;
`overwrite_next` clears before append then resets) and `Vec<String>` the right
adaptation (C nul-termination deferred), but flagged that the derived
`Clone`/`PartialEq` are **not** faithful: upstream `clone` copies only the list
(so `overwrite_next` resets to `false`, `Config.zig:6024`) and `equal` compares
only the list contents (ignoring `overwrite_next`, `:6048`). Fixed by
implementing `Clone` / `PartialEq` / `Eq` by hand to match. The Low (an
`overwrite_next = true` + `""` parse clears the list but leaves the flag set,
since upstream returns before the overwrite block, `:6008`) was folded into the
test plan.

**Round 2 — approved, no findings.** Codex confirmed the manual `Clone` matches
upstream `clone` (deep-copies the list, leaves `overwrite_next` at the default
`false`) and the manual `PartialEq` matches upstream `equal` (list contents
only, ignoring `overwrite_next`), and that the added empty-reset-order and
value-semantics tests cover the edge behavior, with the parser scope and
`Vec<String>` adaptation faithful. "Approved with no findings."

Review artifacts:

- Round 1 prompt: `logs/codex-review/20260604-140303-d483-prompt.md`
- Round 1 result: `logs/codex-review/20260604-140303-d483-last-message.md`
- Round 2 prompt: `logs/codex-review/20260604-140439-d483b-prompt.md`
- Round 2 result: `logs/codex-review/20260604-140439-d483b-last-message.md`

## Result

**Result:** Pass

`RepeatableString` was implemented exactly as the (Round-2-approved) design —
the struct (`list: Vec<String>`, `overwrite_next: bool`), the manual `Clone`
(drops `overwrite_next` to `false`) and `PartialEq`/`Eq` (compare only `list`),
`parse_cli` (the `ValueRequired` guard, the empty-value reset, the
`overwrite_next` clear-and-reset before the append, the one-value-per-call
append), and `count`. Two tests were added:
`repeatable_string_parse_cli_accumulates_and_resets` (accumulation,
missing-value, empty-reset, `overwrite_next`, and the empty-reset ordering) and
`repeatable_string_clone_and_eq_match_upstream` (the custom clone / equality
semantics).

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 2964 passed, 0 failed (two new tests; no
  regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches upstream `RepeatableString` (parse
appends one whole non-empty value, empty input clears without appending,
`overwrite_next` clears before append then resets, `count` is the list length,
`Clone` drops `overwrite_next`, and equality ignores it —
`Config.zig:6005`/`:6024`/`:6048`); the added tests cover the important stateful
cases, including the empty-reset ordering and the custom clone/equality
semantics; gates are clean. "Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-140731-r483-prompt.md` (result)
- Result: `logs/codex-review/20260604-140731-r483-last-message.md` (result)

## Conclusion

`RepeatableString` now parses — an accumulating, empty-resetting,
`overwrite_next`- aware string list with upstream-faithful clone/equality
semantics. This is the first repeatable (accumulating) config value type ported,
and the manual clone/equal pattern (copy/compare a subset of fields) will recur
for other `Required by Config` types. The next slice can port another
self-contained config value type's `parseCLI` (e.g. `RepeatablePath`,
`SelectionWordChars`, or — once a faithful `parseFloat` lands — the deferred
`QuickTerminalSize`), continuing toward the per-field parser dispatch and the
full config loader.
