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

# Experiment 451: the shell-integration config enum and its enabled predicate (ShellIntegration, enabled)

## Description

This experiment ports the `shell-integration` config enum —
`ShellIntegration { None, Detect, Bash, Elvish, Fish, Nushell, Zsh }` — **and
the predicate** the termio exec setup uses to decide whether shell integration
is active at all. Upstream's `Exec` switch disables integration entirely on
`none` and enables it for `detect` and the explicit shells; this experiment
captures the `!= .none` enabled check as a `ShellIntegration::enabled` method.
The forced-shell mapping (`detect` → auto-detect, each explicit shell → that
shell) needs a terminal `Shell` enum that roastty does not yet have, so it stays
deferred; this slice lands the config enum and the enabled predicate. It
diversifies the config-type family into the shell-integration / termio
subsystem.

## Upstream behavior

In `config/Config.zig`, the enum and its `Config` field (default `.detect`):

```zig
@"shell-integration": ShellIntegration = .detect,

pub const ShellIntegration = enum {
    none,
    detect,
    bash,
    elvish,
    fish,
    nushell,
    zsh,
};
```

In `termio/Exec.zig`, the setup decides the forced shell, disabling integration
on `none`:

```zig
const force: ?shell_integration.Shell = switch (cfg.shell_integration) {
    .none => {
        log.info("shell integration disabled by configuration", .{});
        break :shell default_shell_command; // disabled — no integration
    },
    .detect => null,        // enabled, auto-detect (no forced shell)
    .bash => .bash,
    .elvish => .elvish,
    .fish => .fish,
    .nushell => .nushell,
    .zsh => .zsh,           // enabled, forced to that shell
};
```

`none` disables shell integration entirely; `detect` enables it with auto-detect
(no forced shell); each explicit shell enables it and forces that shell. The
`none` case is the only one that disables — the enabled-or-not decision is
`!= .none`.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// The `shell-integration` config (upstream `ShellIntegration`): which shell's
/// integration to inject. The `Config` default is `Detect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellIntegration {
    /// Shell integration disabled.
    None,
    /// Auto-detect the shell.
    Detect,
    Bash,
    Elvish,
    Fish,
    Nushell,
    Zsh,
}

impl ShellIntegration {
    /// Whether shell integration is active at all (upstream's `Exec` setup
    /// `!= .none` decision): `None` disables it; `Detect` and the explicit shells
    /// enable it.
    pub(crate) fn enabled(self) -> bool {
        !matches!(self, ShellIntegration::None)
    }
}
```

`enabled` is the `!= .none` decision: `false` for `None`, `true` for `Detect`
and the explicit shells — exactly the upstream disable case. The `match` is
exhaustive (no wildcard).

## Scope / faithfulness notes

- **Ported (bridged)**: the `ShellIntegration` config enum (`config/Config.zig`)
  and its enabled predicate (`ShellIntegration::enabled`, upstream's `Exec`
  setup `!= .none` decision).
- **Faithful**: the enum has the seven upstream variants (`none`, `detect`,
  `bash`, `elvish`, `fish`, `nushell`, `zsh`); `enabled` returns `false` only
  for `None`, `true` for the rest — exactly the upstream disable case.
- **Faithful adaptation**: the consumer is modeled as a method (upstream inlines
  the switch in `Exec`); only the enabled-or-not decision is extracted (the part
  that needs no other type).
- **Deferred**: the `Config` struct / parsing (and the `.detect` field default),
  the forced-shell mapping (`detect` → auto-detect / `null`, each explicit shell
  → the terminal `shell_integration.Shell`, which roastty does not yet have),
  and the termio `Exec` setup that injects the integration. (Consumed by a later
  slice; this experiment lands the enum and the enabled predicate.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add
     `pub(crate) enum ShellIntegration { None, Detect, Bash, Elvish, Fish, Nushell, Zsh }`
     (derive `Debug, Clone, Copy, PartialEq, Eq`) and
     `ShellIntegration::enabled(self) -> bool`
     (`!matches!(self, ShellIntegration::None)`).
2. Tests (in `config/mod.rs`):
   - `enabled`: `None.enabled() == false`; `Detect`, `Bash`, `Elvish`, `Fish`,
     `Nushell`, `Zsh` each `enabled() == true`; the exact variant set (an array
     with `assert_eq!(len, 7)`); a representative `assert_ne!` and a `Copy`/`Eq`
     round-trip.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty shell_integration
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `ShellIntegration` has the seven upstream variants and `enabled` returns
  `false` only for `None` (`true` for the rest) via an exhaustive `match` —
  faithful to upstream's enum and the `Exec` `!= .none` decision;
- the tests pass (the predicate; the exact variant set), and the existing tests
  still pass;
- the `Config` struct, the forced-shell mapping, and the termio `Exec` setup
  stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant is missing/extra, `enabled` treats an
enabled variant as disabled (or `None` as enabled), an unrelated item changes,
or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: the seven variants match
exactly (`none`, `detect`, `bash`, `elvish`, `fish`, `nushell`, `zsh`,
`Config.zig:8661`); the default `.detect` is correctly documented as deferred to
the future `Config` field (`Config.zig:2813`); `enabled()` correctly extracts
the disable decision from `Exec.zig` (`Exec.zig:770`, only `.none` exits the
shell-integration setup path while `detect` and the explicit shells proceed);
deferring the forced-shell mapping is the right scope (roastty does not yet have
the terminal shell-integration `Shell` enum); and the exact-variant and
truth-table-style tests are adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-112138-d451-prompt.md` (design)
- Result: `logs/codex-review/20260604-112138-d451-last-message.md` (design)
