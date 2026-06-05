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

# Experiment 569: config conditional state (State / Key / Conditional)

## Description

This experiment ports upstream `config/conditional.zig` — the typed
state-of-the-world and the `Conditional` predicate that drive Ghostty's
conditional configuration (config that applies only when, e.g., the OS theme is
dark). roastty's config loader is heavily ported but has **no** conditional
machinery yet; its existing `config::Theme` is the unrelated `theme` _setting_
(light/dark theme names), not the conditional `State.Theme` (the OS desktop
theme). This port lands at `config::conditional`. macOS-only: the `os` state
resolves to the `macos` build target.

## Upstream behavior

`config/conditional.zig`:

- `State` — a static, typed snapshot: `theme: Theme` (the OS desktop theme,
  `light` / `dark`, default `light`) and `os: std.Target.Os.Tag` (the build
  target's OS). `match(cond)` looks up the state field named by `cond.key`,
  stringifies its enum tag (`@tagName`), and compares that string to
  `cond.value` with `==` (`.eq`) or `!=` (`.ne`).
- `Key` — an enum auto-derived from `State`'s field names (`theme`, `os`),
  naming which state field a conditional tests.
- `Conditional` — `{ key: Key, op: Op, value: []const u8 }` where `Op` is `eq` /
  `ne`. `clone` duplicates the `value` bytes into a new allocation.

Upstream test (`conditional enum match`): with `State{ .theme = .dark }`,
`match(.{ .theme, .eq, "dark" })` is true, `match(.{ .theme, .ne, "dark" })` is
false, and `match(.{ .theme, .ne, "light" })` is true.

## Rust mapping (`roastty/src/config/conditional.rs`)

A direct transcription. `Key` is written out explicitly (`Theme`, `Os`) rather
than comptime- derived; the comparison stays byte-oriented (`cond.value` is
`Vec<u8>`, mirroring upstream's `[]const u8`); and `clone` becomes the derived
`Clone` (which duplicates the `Vec`, exactly as upstream's `alloc.dupe`).

```rust
//! Conditional configuration state and predicates (port of upstream `config/conditional`).
//!
//! Conditionals test a static, typed snapshot of the world (`State`) so the implementation stays
//! simple and type-checked.

/// The OS desktop theme (upstream `State.Theme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Theme {
    Light,
    Dark,
}

impl Theme {
    /// The tag name compared against a conditional's value (upstream `@tagName`).
    fn name(self) -> &'static [u8] {
        match self {
            Theme::Light => b"light",
            Theme::Dark => b"dark",
        }
    }
}

/// The build-target OS (upstream `std.Target.Os.Tag`). roastty is macOS-only, so the only build
/// target is `macos`; a conditional comparing `os` against another OS name simply does not match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OsTag {
    Macos,
}

impl OsTag {
    fn name(self) -> &'static [u8] {
        match self {
            OsTag::Macos => b"macos",
        }
    }
}

/// A static, typed snapshot of the world a conditional tests against (upstream `State`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct State {
    pub(crate) theme: Theme,
    pub(crate) os: OsTag,
}

impl Default for State {
    fn default() -> Self {
        // Upstream: theme defaults to light, os to the build target (macos here).
        State {
            theme: Theme::Light,
            os: OsTag::Macos,
        }
    }
}

impl State {
    /// Test a conditional against this state (upstream `match`). Compares the named state field's
    /// tag name to the conditional's value.
    pub(crate) fn matches(&self, cond: &Conditional) -> bool {
        let value: &[u8] = match cond.key {
            Key::Theme => self.theme.name(),
            Key::Os => self.os.name(),
        };
        match cond.op {
            Op::Eq => value == cond.value.as_slice(),
            Op::Ne => value != cond.value.as_slice(),
        }
    }
}

/// Which state field a conditional tests (upstream `Key`, derived from `State`'s fields).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    Theme,
    Os,
}

/// The comparison a conditional applies (upstream `Conditional.Op`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Eq,
    Ne,
}

/// A single conditional predicate (upstream `Conditional`). `clone` is the derived `Clone`, which
/// duplicates `value` exactly as upstream's `alloc.dupe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Conditional {
    pub(crate) key: Key,
    pub(crate) op: Op,
    pub(crate) value: Vec<u8>,
}
```

## Scope / faithfulness notes

- **Ported (1:1)**: `config/conditional` → `config::conditional` (`State`,
  `Theme`, `Key`, `Op`, `Conditional`, and `State::matches`).
- **Faithful**: the `match` logic (look up the keyed state field, stringify its
  tag, compare with `eq` / `ne`), the `light`/`dark` theme tags, the `eq`/`ne`
  ops, the `light`/`macos` defaults, and the byte-string value comparison are
  reproduced exactly.
- **Faithful adaptation**: `Key` is written out (`Theme`, `Os`) instead of
  comptime-derived from the struct fields — the same two keys, just explicit;
  `Conditional.value` is `Vec<u8>` (mirroring `[]const u8`); upstream's
  `clone(alloc)` becomes the derived `Clone` (the `Vec` is duplicated, no
  allocator needed). `State::match` is renamed `matches` (`match` is a Rust
  keyword).
- **macOS-only**: upstream's `os` is the full `std.Target.Os.Tag`; roastty
  resolves it to the `macos` build target (`OsTag::Macos`). A conditional
  comparing `os` against any other OS name correctly does not match (`eq` →
  false, `ne` → true), which is the faithful macOS-arm behavior.
- **Deferred**: nothing — the upstream file is fully covered for the macOS
  target.
- No C ABI/header/ABI-inventory change (internal Rust). Adds
  `config::conditional`.

## Changes

1. `roastty/src/config/conditional.rs` (new): `Theme`, `OsTag`, `State`, `Key`,
   `Op`, `Conditional` as above.
2. `roastty/src/config/mod.rs`: add `#[allow(dead_code)] mod conditional;`
   (alphabetical, after `comma_splitter`).
3. Tests (in `conditional.rs`):
   - **the upstream theme test**: `State{ theme: Dark }` — `eq "dark"` true,
     `ne "dark"` false, `ne "light"` true.
   - **default state**: `light` theme and `macos` os.
   - **os matching**: the default `macos` os — `eq "macos"` true, `eq "linux"`
     false, `ne "linux"` true.
   - **clone**: a `Conditional` clones to an equal value (the derived `Clone`
     duplicates the bytes).
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty config::conditional
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config/conditional.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `State::matches` reproduces upstream's keyed-field / tag-name / `eq`-`ne`
  comparison, with the `light`/`dark` and `macos` tags and the `light`/`macos`
  defaults — faithful to `config/conditional.zig` for the macOS target;
- the tests pass (theme match / defaults / os match / clone), and the existing
  tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the `match` semantics, the tags/ops, the defaults,
or the value comparison diverge from upstream, an unrelated item changes, or any
public C API/ABI changes.

## Design Review

Codex reviewed the design and **approved it with no findings**. It confirmed
`State::matches` is faithful (it selects the keyed state field, uses the same
tag-name strings `light` / `dark` / `macos`, and applies byte
equality/inequality against `Conditional.value`), that the macOS-only
`OsTag::Macos` model is acceptable for this crate (`os == "linux"` is false,
`os != "linux"` is true — the macOS build-target behavior), and that the
explicit `Key { Theme, Os }`, the `Vec<u8>` conditional value, the derived
`Clone`, and the `matches` naming are all sound adaptations. The test plan
covers the upstream theme case, the defaults, the OS comparison, and clone
behavior.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d569-prompt.md`
- Result: `logs/codex-review/20260604-d569-last-message.md`
