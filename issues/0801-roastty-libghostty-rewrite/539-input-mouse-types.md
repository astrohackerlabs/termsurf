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

# Experiment 539: the mouse input types

## Description

This experiment ports upstream `input/mouse.zig` — the **mouse input types** —
into a new `roastty::input::mouse` module. These are the GUI-side mouse event
types the frontend and input layer use: the action (`Action`), button state
(`ButtonState`), the button itself (`Button`), the scroll momentum phase
(`Momentum`), the trackpad pressure stage (`PressureStage`), and the
scroll-event modifier bitmask (`ScrollMods`). The file is fully self-contained
upstream (it imports only `std`), so it is a clean bounded slice.

(Background: an earlier candidate for this slot — porting `terminal/apc.zig`'s
`Handler` — was abandoned at design review because roastty already implements
the APC handler as `KittyGraphicsApc` in `terminal/terminal.rs` (with `start` /
`put` / `end`, a parser state, max-byte handling, and full `ApcStart` / `ApcPut`
/ `ApcEnd` dispatch wiring). Porting a second `apc::Handler` would duplicate
working code, so that direction was dropped and this self-contained,
genuinely-unported input slice chosen instead.)

## Upstream behavior

`input/mouse.zig`:

```zig
pub const Action = enum(c_int) { press, release, motion };

pub const ButtonState = enum(c_int) { release, press };  // mirrored in include/ghostty.h

pub const Button = enum(c_int) {
    pub const max = …;  // the largest field value (11)
    unknown = 0, left = 1, right = 2, middle = 3, four = 4, five = 5,
    six = 6, seven = 7, eight = 8, nine = 9, ten = 10, eleven = 11,
};  // mirrored in include/ghostty.h

pub const Momentum = enum(u3) {  // matches macOS NSEventPhase
    none = 0, began = 1, stationary = 2, changed = 3, ended = 4, cancelled = 5, may_begin = 6,
};

pub const PressureStage = enum(u2) { none = 0, normal = 1, deep = 2 };  // macOS stages

pub const ScrollMods = packed struct(u8) {
    precision: bool = false,   // bit 0: high-precision (Magic Mouse, trackpads)
    momentum: Momentum = .none,// bits 1-3
    _padding: u4 = 0,          // bits 4-7
};
```

- `Action` / `ButtonState` / `Button` are `enum(c_int)` because they are used
  as-is by the C embedding API (upstream notes "Any changes here update
  include/ghostty.h").
- `Button.max` is a comptime fold over the fields giving the largest value (11),
  used to size densely-packed arrays.
- `Momentum` (`u3`) and `PressureStage` (`u2`) are small backed enums matching
  macOS event phases / pressure stages.
- `ScrollMods` is a `packed struct(u8)`: bit 0 = `precision`, bits 1-3 =
  `momentum`, bits 4-7 = padding. Its `@bitCast` to `u8` is `0` when default,
  `0b0000_0001` when `precision = true` (the upstream test).

## Rust mapping (`roastty/src/input/mouse.rs`)

`enum(c_int)` → `#[repr(i32)]` (so the types stay ABI-compatible for when the
embedding API exposes them, matching upstream's C-ABI intent — but no header is
touched here); `enum(u3)` / `enum(u2)` → `#[repr(u8)]` (the smallest Rust repr,
values unchanged); `Button.max` → an associated `const MAX: i32 = 11`; the
`packed struct(u8)` → a plain struct plus an `int(self) -> u8` bit-encoder
mirroring the packed layout (the same shape as the existing
`input::key_mods::ModSides::int`):

```rust
//! Mouse input types (port of upstream `input/mouse`).

/// The type of action associated with a mouse event (upstream `input.mouse.Action`).
/// Backed by `c_int` for the embedding API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub(crate) enum Action {
    Press,
    Release,
    Motion,
}

/// The state of a mouse button (upstream `input.mouse.ButtonState`). Backed by `c_int`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub(crate) enum ButtonState {
    Release,
    Press,
}

/// Possible mouse buttons; we track up to 11 (upstream `input.mouse.Button`). Backed by
/// `c_int`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub(crate) enum Button {
    Unknown = 0,
    Left = 1,
    Right = 2,
    Middle = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Eleven = 11,
}

impl Button {
    /// The maximum value in this enum (upstream `Button.max`), e.g. to size a densely
    /// packed array.
    pub(crate) const MAX: i32 = 11;
}

/// The "momentum" of a mouse scroll event (upstream `input.mouse.Momentum`), matching
/// the macOS `NSEventPhase` used for inertial scrolling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Momentum {
    #[default]
    None = 0,
    Began = 1,
    Stationary = 2,
    Changed = 3,
    Ended = 4,
    Cancelled = 5,
    MayBegin = 6,
}

/// The pressure stage of a pressure-sensitive input device (upstream
/// `input.mouse.PressureStage`); macOS stages only.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum PressureStage {
    #[default]
    None = 0,
    Normal = 1,
    Deep = 2,
}

/// The modifier bitmask for scroll events (upstream `input.mouse.ScrollMods`, a
/// `packed struct(u8)`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScrollMods {
    /// True for a high-precision scroll event (Apple Magic Mouse, trackpads).
    pub(crate) precision: bool,
    /// The momentum phase of the scroll event (inertial scrolling).
    pub(crate) momentum: Momentum,
}

impl ScrollMods {
    /// The `u8` bit-encoding of this mask, mirroring upstream's `packed struct(u8)`:
    /// bit 0 = `precision`, bits 1-3 = `momentum`, bits 4-7 = padding (0).
    pub(crate) fn int(self) -> u8 {
        (self.precision as u8) | ((self.momentum as u8) << 1)
    }
}
```

Zig packs `packed struct(u8)` fields from the least-significant bit in
declaration order, so `precision` is bit 0 and `momentum` is bits 1-3 — exactly
what `int` reproduces. The `Default` derive gives `precision = false`,
`momentum = None`, i.e. `int() == 0`.

## Scope / faithfulness notes

- **Ported (bridged)**:
  `input.mouse.{Action, ButtonState, Button, Momentum, PressureStage, ScrollMods}`
  into `input::mouse`, including `Button::MAX` and `ScrollMods::int`.
- **Faithful**: the exact variant sets and discriminants (`Button` `0..=11`,
  `Momentum` `0..=6`, `PressureStage` `0..=2`); the `ScrollMods` bit layout
  (precision = bit 0, momentum = bits 1-3); `Button.max == 11`.
- **Faithful adaptation**: `enum(c_int)` → `#[repr(i32)]` (ABI-compatible
  internal types; no header change); `enum(u3)` / `enum(u2)` → `#[repr(u8)]`
  (values unchanged); the comptime `Button.max` → `const MAX`; the
  `packed struct(u8)` → a struct plus an `int(self) -> u8` encoder (the same
  pattern as `key_mods::ModSides::int`).
- **Deferred**: wiring these types into the frontend / input dispatch
  (consumers); any C embedding-API exposure (`roastty.h` is untouched — these
  are internal types until wired).
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/input/mouse.rs` (new): `Action`, `ButtonState`, `Button` (+
   `MAX`), `Momentum`, `PressureStage`, `ScrollMods` (+ `int`).
2. `roastty/src/input/mod.rs`: add `pub(crate) mod mouse;` (the module already
   has a crate-level `#![allow(dead_code)]`).
3. Tests (in `mouse.rs`):
   - **scroll_mods bit layout**: `ScrollMods::default().int() == 0`;
     `ScrollMods { precision: true, .. }.int() == 0b0000_0001` (the upstream
     test); `momentum = Began` ⇒ `0b0000_0010`;
     `precision = true, momentum = MayBegin` ⇒ `0b0000_1101` (6 `<< 1` `| 1`).
   - **discriminants**: `Action::Motion as i32 == 2`;
     `ButtonState::Press as i32 == 1`; `Button::Eleven as i32 == 11`;
     `Button::MAX == 11`; `Momentum::MayBegin as u8 == 6`;
     `PressureStage::Deep as u8 == 2`.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty mouse
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/input/mouse.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `input::mouse` has `Action` / `ButtonState` / `Button` / `Momentum` /
  `PressureStage` / `ScrollMods` with the exact upstream variant sets and
  discriminants, `Button::MAX == 11`, and `ScrollMods::int` reproducing the
  packed-`u8` layout;
- the tests pass (bit layout + discriminants), and the existing tests still
  pass;
- the consumer wiring and any C-ABI exposure stay deferred (`roastty.h`
  untouched);
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant set, discriminant, or the `ScrollMods` bit
layout diverges from upstream, an unrelated item changes, or any public C
API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. (The earlier APC-handler direction for this slot was abandoned at
Codex's prompting — roastty already implements the APC handler as
`KittyGraphicsApc` in `terminal.rs`, so a second `apc::Handler` would duplicate
it; this self-contained, genuinely-unported input slice was chosen instead.)
Codex confirmed: upstream `mouse.zig` defines exactly these six types, with
`Button` `unknown=0..eleven=11`, `Momentum` `0..=6`, `PressureStage` `0..=2`;
`ScrollMods::int()` correctly adapts the `packed struct(u8)` layout (Zig packs
declaration order from the low bits, so `precision` is bit 0 — confirmed by
upstream's own bitcast test — and `(momentum as u8) << 1` places the 3-bit
`Momentum` in bits 1-3 with no collision into the padding bits 4-7);
`#[repr(i32)]` is appropriate for the `enum(c_int)` types on macOS and
`#[repr(u8)]` is the right carrier for the explicit `u3` / `u2` values; leaving
`roastty.h` untouched is correct while these remain internal; and
`Button::MAX = 11` is a faithful stand-in for the upstream comptime `max` fold.
The proposed tests are adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d539-prompt.md` (design)
- Result: `logs/codex-review/20260604-d539-last-message.md` (design)

## Result

**Result:** Pass

`input::mouse` was created with the six mouse input types: `Action` /
`ButtonState` / `Button` as `#[repr(i32)]` (the `enum(c_int)` types), `Momentum`
/ `PressureStage` as `#[repr(u8)]` (the `u3` / `u2` enums) with their exact
discriminants, `Button::MAX = 11` (the comptime `max` fold), and `ScrollMods` as
a struct with `int(self) -> u8` reproducing upstream's `packed struct(u8)`
layout (precision = bit 0, momentum = bits 1-3, padding = bits 4-7). The module
is registered in `input/mod.rs` (which already carries crate-level
`#![allow(dead_code)]`). Two new tests verify the `ScrollMods` bit encoding
(including upstream's `default == 0` and `precision == 0b1` cases) and the enum
discriminants.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3031 passed, 0 failed (two new tests; no regressions,
  up from 3029).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + input/mouse.rs +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean; `roastty.h`
  untouched.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **one Nit** (no
Required or Optional findings): the doc had `## Result` but no `## Conclusion`,
which the issue process expects — fixed by adding the conclusion below. Codex
confirmed the six type definitions match upstream `mouse.zig` (including
`Button` `0..=11`, `Momentum` `0..=6`, `PressureStage` `0..=2`, and
`Button::MAX = 11`); `ScrollMods::int()` correctly reproduces the packed-`u8`
layout (`precision` at bit 0, `momentum` in bits 1-3, padding zero);
`#[repr(i32)]` for the `enum(c_int)` types is correct on macOS and `#[repr(u8)]`
is the right carrier for the `u3` / `u2` enums; leaving `roastty.h` untouched is
the right scope while these remain internal; and the tests adequately cover the
slice.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r539-prompt.md` (result)
- Result: `logs/codex-review/20260604-r539-last-message.md` (result)

## Conclusion

`input::mouse` now holds the GUI-side mouse input types (`Action`,
`ButtonState`, `Button`, `Momentum`, `PressureStage`, `ScrollMods`), faithfully
ported from `input/mouse.zig` — the first slice of the input layer beyond the
already-ported `key` / `key_encode` / `key_mods` / `paste`. The `enum(c_int)`
types use `#[repr(i32)]` so they stay ABI-compatible for when the embedding API
exposes them, but `roastty.h` is untouched until that wiring happens (deferred).
This also recorded a dead end: the APC handler is already ported as
`terminal::terminal::KittyGraphicsApc`, so `apc.zig` needs no separate port.
Natural next input slices are the other self-contained upstream input files
(`keyboard.zig`'s `Layout`, `function_keys.zig`, `keycodes.zig`,
`mouse_encode`), each genuinely unported. The config `loadDefaultFiles` stays
deferred pending roastty's naming decision; `background-image-opacity` stays
float-blocked.
