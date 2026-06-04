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

# Experiment 460: the clipboard-access config enum and its predicates (ClipboardAccess, denied / needs_confirm)

## Description

This experiment ports the `clipboard-access` config enum —
`ClipboardAccess { Allow, Deny, Ask }` — used for both `clipboard-read` and
`clipboard-write` — **and the two predicates** the surface uses to gate a
clipboard operation. Upstream's `Surface` denies the operation when the config
is `deny` (`== .deny`) and requires confirmation when it is `ask` (`== .ask`);
`allow` proceeds. This experiment captures those two checks as
`ClipboardAccess::denied` and `ClipboardAccess::needs_confirm`. The surface
clipboard call sites (the actual read / write, the confirmation prompt) stay
deferred. It diversifies the config-type family into the clipboard-permission
config.

## Upstream behavior

In `config/Config.zig`, the enum and its two `Config` fields:

```zig
@"clipboard-read": ClipboardAccess = .ask,
@"clipboard-write": ClipboardAccess = .allow,

pub const ClipboardAccess = enum {
    allow,
    deny,
    ask,
};
```

In `Surface.zig`, the surface gates clipboard operations on the config:

```zig
if (self.config.clipboard_read == .deny) { ... deny ... }
// ...
if (self.config.clipboard_write == .deny) { ... deny ... }
const confirm = self.config.clipboard_write == .ask;
```

`deny` denies the operation (`== .deny`); `ask` requires a confirmation prompt
(`== .ask`); `allow` proceeds without asking. The same enum gates reads and
writes (with different defaults: `clipboard-read` is `ask`, `clipboard-write` is
`allow`).

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// The `clipboard-read` / `clipboard-write` config (upstream `ClipboardAccess`):
/// whether a clipboard operation is allowed, denied, or confirmed. The `Config`
/// defaults are `Ask` for read and `Allow` for write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipboardAccess {
    /// Proceed without asking.
    Allow,
    /// Deny the operation.
    Deny,
    /// Require a confirmation prompt.
    Ask,
}

impl ClipboardAccess {
    /// Whether the clipboard operation is denied (upstream's `== .deny` check).
    pub(crate) fn denied(self) -> bool {
        matches!(self, ClipboardAccess::Deny)
    }

    /// Whether the clipboard operation needs a confirmation prompt (upstream's
    /// `== .ask` check).
    pub(crate) fn needs_confirm(self) -> bool {
        matches!(self, ClipboardAccess::Ask)
    }
}
```

`denied` is the `== .deny` check (`true` only for `Deny`); `needs_confirm` is
the `== .ask` check (`true` only for `Ask`); `Allow` is neither (proceeds). Both
`matches!` are exhaustive.

## Scope / faithfulness notes

- **Ported (bridged)**: the `ClipboardAccess` config enum (`config/Config.zig`)
  and its two predicates (`denied` / `needs_confirm`, upstream's `Surface`
  `== .deny` / `== .ask` checks).
- **Faithful**: the enum has the three upstream variants (`allow`, `deny`,
  `ask`); `denied` returns `true` only for `Deny`, `needs_confirm` returns
  `true` only for `Ask` — exactly the upstream gates; `Allow` is neither.
- **Faithful adaptation**: the consumer is modeled as two methods (upstream
  inlines the two comparisons at the read / write call sites); each returns the
  positive decision. The same enum is used for both `clipboard-read` and
  `clipboard-write` (with different `Config` field defaults, documented but kept
  off the enum).
- **Deferred**: the `Config` struct / parsing (and the field defaults — `ask`
  for read, `allow` for write), and the surface clipboard call sites (the actual
  read / write, the confirmation prompt, the OSC 52 handling) that consume the
  decisions. (Consumed by a later slice; this experiment lands the enum and the
  two predicates.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `pub(crate) enum ClipboardAccess { Allow, Deny, Ask }` (derive
     `Debug, Clone, Copy, PartialEq, Eq`) and
     `ClipboardAccess::denied(self) -> bool`
     (`matches!(self, ClipboardAccess::Deny)`) and
     `ClipboardAccess::needs_confirm(self) -> bool`
     (`matches!(self, ClipboardAccess::Ask)`).
2. Tests (in `config/mod.rs`):
   - the two predicates over the three variants: `Allow` → `false`/`false`,
     `Deny` → `true`/`false`, `Ask` → `false`/`true` (denied / needs_confirm);
     the variants distinct and a `Copy`/`Eq` round-trip.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty clipboard_access
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `ClipboardAccess` has the three upstream variants; `denied` returns `true`
  only for `Deny` and `needs_confirm` returns `true` only for `Ask` — faithful
  to upstream's two checks;
- the tests pass (the two predicates over the three variants; the distinct
  variants), and the existing tests still pass;
- the `Config` struct and the surface clipboard call sites stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant is missing/extra, a predicate maps a
variant the wrong way (e.g. `Allow` denied, or `Deny`/`Ask` swapped), an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: the variants match
exactly (`allow`, `deny`, `ask`, `Config.zig:9210`); the defaults are correctly
documented as deferred Config-field defaults (read `.ask`, write `.allow`,
`Config.zig:2361`); `denied()` is the exact `.deny` check used by the read/write
paths (`Surface.zig:1044` / `:2163`); `needs_confirm()` is the exact `.ask`
check used by the write/read confirmation gating (`Surface.zig:2200` / `:5907`);
splitting into two predicates is the right modeling (deny and ask are
independent decisions in the call sites); and the 3-variant behavior table is
adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-115851-d460-prompt.md` (design)
- Result: `logs/codex-review/20260604-115851-d460-last-message.md` (design)
