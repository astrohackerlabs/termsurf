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

# Experiment 441: the mouse-shift-capture config enum and its capture decision (MouseShiftCapture, capture_shift)

## Description

This experiment ports the `mouse-shift-capture` config enum —
`MouseShiftCapture { False, True, Always, Never }` — **and the decision** the
surface uses to decide whether the shift modifier may be captured by mouse
events. Upstream's `Surface.mouseShiftCapture` combines the config with the
terminal's own `mouse_shift_capture` request (a tri-state) to produce a bool;
this experiment captures that as a `MouseShiftCapture::capture_shift` method
that takes the terminal request as an `Option<bool>` parameter. It diversifies
the config-type family into the input/mouse subsystem (upstream `Surface.zig`);
the live terminal flag and the renderer-state lock stay deferred (the terminal
request is a parameter).

## Upstream behavior

In `config/Config.zig`, the enum and its `Config` field (default `.false`):

```zig
@"mouse-shift-capture": MouseShiftCapture = .false,

pub const MouseShiftCapture = enum {
    false,
    true,
    always,
    never,
};
```

In `Surface.zig`, `mouseShiftCapture` decides whether shift may be captured:

```zig
fn mouseShiftCapture(self: *const Surface, lock: bool) bool {
    // Handle our never/always case where we don't need a lock.
    switch (self.config.mouse_shift_capture) {
        .never => return false,
        .always => return true,
        .false, .true => {},
    }

    if (lock) self.renderer_state.mutex.lock();
    defer if (lock) self.renderer_state.mutex.unlock();

    // If the terminal explicitly requests it then we always allow it
    // since we processed never/always at this point.
    switch (self.io.terminal.flags.mouse_shift_capture) {
        .false => return false,
        .true => return true,
        .null => {},
    }

    // Otherwise, go with the user's preference
    return switch (self.config.mouse_shift_capture) {
        .false => false,
        .true => true,
        .never, .always => unreachable, // handled earlier
    };
}
```

`never`/`always` short-circuit (no terminal override). Otherwise the terminal's
own `mouse_shift_capture` flag (a tri-state `?bool`: `false` / `true` / `null`)
decides — and only when it is `null` does the config `false` / `true` provide
the default.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// The `mouse-shift-capture` config (upstream `MouseShiftCapture`): whether the
/// shift modifier may be captured by mouse events. The `Config` default is
/// `False`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseShiftCapture {
    /// Default off, but a terminal request can override.
    False,
    /// Default on, but a terminal request can override.
    True,
    /// Always capture (no terminal override).
    Always,
    /// Never capture (no terminal override).
    Never,
}

impl MouseShiftCapture {
    /// Whether the shift modifier may be captured, given the terminal's own
    /// `mouse_shift_capture` request (upstream `Surface.mouseShiftCapture`):
    /// `Never`/`Always` decide outright; otherwise the terminal request
    /// (`Some(false)` / `Some(true)`) decides, and only when it is `None` does
    /// the config `False`/`True` provide the default.
    pub(crate) fn capture_shift(self, terminal_request: Option<bool>) -> bool {
        match self {
            MouseShiftCapture::Never => false,
            MouseShiftCapture::Always => true,
            MouseShiftCapture::False | MouseShiftCapture::True => match terminal_request {
                Some(v) => v,
                None => matches!(self, MouseShiftCapture::True),
            },
        }
    }
}
```

`Never → false` / `Always → true` short-circuit; for `False`/`True`, a
`Some(terminal)` overrides and a `None` falls back to the config default
(`True → true`, `False → false`) — exactly upstream's two switches.

## Scope / faithfulness notes

- **Ported (bridged)**: the `MouseShiftCapture` config enum
  (`config/Config.zig`) and its capture decision
  (`MouseShiftCapture::capture_shift`, upstream's `Surface.mouseShiftCapture`).
- **Faithful**: the enum has the four upstream variants (`false`, `true`,
  `always`, `never`); `capture_shift` reproduces upstream exactly —
  `Never`/`Always` short-circuit, the terminal request decides next, and the
  config `False`/`True` is the fallback only when the terminal request is
  `None`.
- **Faithful adaptation**: the terminal's tri-state `mouse_shift_capture` flag
  (`?bool`: `false`/`true`/`null`) is the `terminal_request: Option<bool>`
  parameter (upstream reads `self.io.terminal.flags.mouse_shift_capture`); the
  `lock`/`renderer_state.mutex` guarding that read is the caller's concern (a
  Rust borrow), not part of the decision. The `unreachable` config arm in
  upstream's final switch is absorbed by Rust's `False | True` match arm.
- **Deferred**: the `Config` struct / parsing (and the `.false` field default),
  the live terminal `mouse_shift_capture` flag, and the surface call site (the
  lock and the situational checks — left button, mouse click — the caller still
  applies). (Consumed by a later slice; this experiment lands the enum and the
  decision.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `pub(crate) enum MouseShiftCapture { False, True, Always, Never }`
     (derive `Debug, Clone, Copy, PartialEq, Eq`) and
     `MouseShiftCapture::capture_shift(self, terminal_request: Option<bool>) -> bool`.
2. Tests (in `config/mod.rs`):
   - `capture_shift`: the full truth table over the four variants × the terminal
     request `∈ {None, Some(false), Some(true)}`:
     - `Never` → `false` for all three requests;
     - `Always` → `true` for all three;
     - `False` → `None`→`false`, `Some(false)`→`false`, `Some(true)`→`true`;
     - `True` → `None`→`true`, `Some(false)`→`false`, `Some(true)`→`true`;
   - plus the variants distinct and a `Copy`/`Eq` round-trip.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty mouse_shift_capture
cargo test -p roastty capture_shift
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `MouseShiftCapture` has the four upstream variants and `capture_shift`
  reproduces upstream's decision (`Never`/`Always` short-circuit; the terminal
  request next; the config default only when `None`) — faithful to upstream's
  enum and `mouseShiftCapture`;
- the tests pass (the full truth table; the distinct variants), and the existing
  tests still pass;
- the `Config` struct, the live terminal flag, and the surface call site stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant is missing/extra, `capture_shift`
mishandles a case (e.g. lets the terminal request override `Never`/`Always`, or
ignores it for `False`/`True`), an unrelated item changes, or any public C
API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream:
`MouseShiftCapture { False, True, Always, Never }` matches the enum exactly and
documenting the `.false` default while deferring the `Config` struct is
consistent with the existing pattern (`Config.zig:965` / `:9100`);
`capture_shift` correctly ports `Surface.mouseShiftCapture` (`Surface.zig:3689`)
— `Never`/`Always` short-circuit, then the terminal tri-state wins, then config
`False`/`True` is the fallback; `Option<bool>` is the right Rust shape for the
upstream `.false`/`.true`/`.null` terminal flag (which roastty already models as
`Option<bool>`); and treating the lock and situational mouse checks as caller
concerns is the right boundary for a pure config decision helper. It judged the
full truth-table test adequate to catch any ordering mistake between config
override, terminal request, and fallback.

Review artifacts:

- Prompt: `logs/codex-review/20260604-103716-d441-prompt.md` (design)
- Result: `logs/codex-review/20260604-103716-d441-last-message.md` (design)
