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

# Experiment 461: begin the aggregating Config struct (the clipboard config group)

## Description

The config layer so far is a set of leaf config types (enums, flag-structs,
color value types), each documented as deferring its default to "the deferred
`Config` struct". This experiment **begins that aggregating `Config` struct** —
the Rust analog of upstream's `config.Config` — with its first, coherent field
group: the **clipboard config** (`copy_on_select`, `clipboard_read`,
`clipboard_write`). It wires their documented defaults into a
`Config::default()`. The struct grows one coherent field group per later
experiment; this slice establishes the struct, its `Default`, and the pattern.
The full key set, the parser (`loadCli` / file loading), and the rest of
upstream `Config` stay deferred.

## Upstream behavior

Upstream's `config.Config` is one large struct: each setting is a field
`@"key-name": Type = default`, with the defaults inline. The clipboard group:

```zig
@"copy-on-select": CopyOnSelect = switch (builtin.os.tag) {
    .linux => .true,
    .macos => .true,
    else => .false,
},
@"clipboard-read": ClipboardAccess = .ask,
@"clipboard-write": ClipboardAccess = .allow,
```

`copy-on-select` defaults to `.true` on macOS (and Linux), `.false` elsewhere;
`clipboard-read` defaults to `.ask`; `clipboard-write` defaults to `.allow`.
roastty is macOS-only, so `copy-on-select` is `True`.

## Rust mapping (`roastty/src/config/mod.rs`)

The `Config` struct holds the (already-ported) field types; `Default` sets their
upstream defaults:

```rust
/// The aggregating config struct (upstream `config.Config`) — the home of the
/// config keys. Built up one field group per slice; this lands the clipboard
/// group. The full key set, the parser, and file loading are ported later.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Config {
    /// `copy-on-select`.
    pub copy_on_select: CopyOnSelect,
    /// `clipboard-read`.
    pub clipboard_read: ClipboardAccess,
    /// `clipboard-write`.
    pub clipboard_write: ClipboardAccess,
}

impl Default for Config {
    /// Upstream's `Config` field defaults for the clipboard group (macOS):
    /// `copy-on-select` is `True`, `clipboard-read` is `Ask`, `clipboard-write`
    /// is `Allow`.
    fn default() -> Self {
        Self {
            copy_on_select: CopyOnSelect::True,
            clipboard_read: ClipboardAccess::Ask,
            clipboard_write: ClipboardAccess::Allow,
        }
    }
}
```

`Default` is the upstream Config-field defaults for these three keys: the macOS
`copy-on-select` (`True`), `clipboard-read` (`Ask`), `clipboard-write`
(`Allow`). The struct derives `Clone`/`PartialEq` — not `Copy` (it will gain
`String`-backed fields like `Theme` as it grows), and not `Eq` (it will gain
float fields like `background_opacity` / `minimum_contrast` that cannot
implement `Eq`).

## Scope / faithfulness notes

- **Ported (bridged)**: the start of the aggregating `Config` struct (upstream
  `config.Config`) with the clipboard field group and its `Default`.
- **Faithful**: the three clipboard fields use the already-ported types
  (`CopyOnSelect`, `ClipboardAccess`); their `Default` values match upstream's
  Config-field defaults (macOS `copy-on-select` `True`, `clipboard-read` `Ask`,
  `clipboard-write` `Allow`).
- **Faithful adaptation**: the macOS-only `copy-on-select` default is `True`
  (upstream's `.macos => .true`); roastty is macOS-only, so the OS `switch` is
  resolved to the macOS arm. The struct is grown incrementally (one coherent
  field group per experiment), so this slice is a faithful partial of upstream's
  `Config`.
- **Deferred**: the rest of upstream `Config`'s fields (added group by group in
  later slices), the parser (`loadCli`, file loading, `parseCLI` per field), the
  `changeConfig` / clone-with-alloc machinery, and the conditional-config
  system. (Consumed by later slices; this experiment lands the struct skeleton
  and the clipboard group.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add
     `pub(crate) struct Config { pub copy_on_select: CopyOnSelect, pub clipboard_read: ClipboardAccess, pub clipboard_write: ClipboardAccess }`
     (derive `Debug, Clone, PartialEq` — not `Eq`, anticipating future float
     fields) and a `Default` impl (`copy_on_select: True`,
     `clipboard_read: Ask`, `clipboard_write: Allow`).
   - update the module doc: the `Config` struct is no longer wholly deferred —
     it now exists and is grown one field group per slice (the full key set /
     parser stay deferred).
2. Tests (in `config/mod.rs`):
   - `Config::default()` has `copy_on_select == CopyOnSelect::True`,
     `clipboard_read == ClipboardAccess::Ask`,
     `clipboard_write == ClipboardAccess::Allow`; a modified `Config` (e.g.
     `clipboard_read = Deny`) differs from the default and round-trips
     `Clone`/`PartialEq`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty config_default
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- the `Config` struct exists with the clipboard field group, and
  `Config::default()` sets the three upstream Config-field defaults (macOS
  `copy-on-select` `True`, `clipboard-read` `Ask`, `clipboard-write` `Allow`) —
  a faithful partial of upstream's `Config`;
- the tests pass (the defaults; a modified value; `Clone`/`PartialEq`), and the
  existing tests still pass;
- the rest of upstream `Config` and the parser stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a default is wrong (e.g. `clipboard-read` `Allow`),
a field uses the wrong type, an unrelated item changes, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **one
Low finding** (now folded in), no Required or Recommended findings. It confirmed
the design is faithful: incrementally growing `config::Config` by coherent field
groups is reasonable for this large upstream struct (as long as the module docs
say it is partial and expanding); the clipboard defaults are correct
(`copy_on_select = True` for macOS, matching the OS switch, `Config.zig:2416`;
`clipboard_read = Ask`, `Config.zig:2361`; `clipboard_write = Allow`,
`Config.zig:2362`); clipboard is a sensible first group (the leaf types exist
and the defaults are self-contained); deriving `Clone` and not `Copy` is right
for an aggregate that will soon contain `String`-backed fields like `Theme` /
`FontStyle`; and the default/modified/clone tests are adequate.

- **Low (fixed)**: derive `PartialEq` but **not** `Eq` on `Config`. This first
  clipboard group is `Eq`-able, but the aggregate will grow fields such as
  floats (`background_opacity`, `minimum_contrast`) that cannot implement `Eq`;
  dropping `Eq` now avoids later internal churn. Folded in: the struct derives
  `Debug, Clone, PartialEq` only, and the test uses a `Clone`/`PartialEq`
  round-trip.

Review artifacts:

- Prompt: `logs/codex-review/20260604-120259-d461-prompt.md` (design)
- Result: `logs/codex-review/20260604-120259-d461-last-message.md` (design)
