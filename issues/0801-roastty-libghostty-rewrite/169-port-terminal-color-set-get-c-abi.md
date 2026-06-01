# Experiment 169: Port Terminal Color Set/Get C ABI

## Description

Experiment 168 added the first `roastty_terminal_set` slice for text metadata.
The next coherent upstream terminal C ABI slice is terminal color configuration:

- `color_foreground`;
- `color_background`;
- `color_cursor`;
- `color_palette`;
- corresponding `terminal_get` values for current/effective colors;
- corresponding `terminal_get` values for configured/default colors.

This cannot be implemented as a thin set/get wrapper over Roastty's current
storage. Upstream Ghostty's terminal color model is:

- foreground/background/cursor are `DynamicRGB` values with separate `override`
  and `default` fields;
- a newly-created terminal has all three dynamic colors unset;
- `terminal_set(color_foreground/background/cursor, value)` changes the
  `default` field, not the current override;
- OSC/Kitty dynamic-color operations change the `override` field;
- OSC/Kitty color reset operations restore `override = default`, which may still
  be unset;
- palette is a `DynamicPalette` with `current`, `original`, and a mask of
  runtime-overridden entries;
- `terminal_set(color_palette, value)` changes `original` while preserving
  masked runtime overrides in `current`;
- `terminal_set(color_palette, NULL)` changes `original` back to the built-in
  default palette while still preserving masked runtime overrides.

Roastty currently initializes foreground/background defaults at terminal
creation and stores palette as a plain `[Rgb; 256]`. That made earlier OSC query
tests convenient, but it does not match upstream's terminal C ABI. This
experiment ports the upstream color storage model and the C ABI color set/get
surface together so the state transition is coherent.

## Changes

### 1. Add public color C types

In `roastty/include/roastty.h`, add:

```c
typedef struct {
  uint8_t r;
  uint8_t g;
  uint8_t b;
} roastty_rgb_s;

typedef roastty_rgb_s roastty_palette_t[256];
```

The RGB struct must stay exactly three bytes with one-byte alignment, matching
the existing internal `CRgb` layout and upstream `color.RGB.C`.

### 2. Add implemented color option constants

Extend `roastty_terminal_option_e` with the upstream-compatible values:

```c
ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND = 11,
ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND = 12,
ROASTTY_TERMINAL_OPTION_COLOR_CURSOR = 13,
ROASTTY_TERMINAL_OPTION_COLOR_PALETTE = 14,
```

Do not add unrelated option constants (`kitty_image_*`, APC, selection, effects
callbacks) in this experiment.

### 3. Port dynamic palette storage

In `roastty/src/terminal/color.rs`, add a `DynamicPalette` equivalent with:

- `current: Palette`;
- `original: Palette`;
- `mask` tracking runtime-overridden entries.

Required methods:

- initialize from a default palette;
- set a runtime palette entry and mark its mask bit;
- reset one runtime palette entry to its original value and clear its mask bit;
- reset all runtime palette entries to original and clear the mask;
- change the default palette while preserving masked runtime overrides.

Use a compact fixed-size bitset representation such as `[u64; 4]` or an
equivalent local type. Do not add an external dependency for this.

Update `TerminalColors` to store:

- `foreground: DynamicRgb::unset()`;
- `background: DynamicRgb::unset()`;
- `cursor: DynamicRgb::unset()`;
- `palette: DynamicPalette::init(DEFAULT_PALETTE)`.

### 4. Keep OSC and Kitty color operations on runtime overrides

Update terminal stream handling so:

- OSC palette set uses `DynamicPalette::set`;
- OSC palette reset uses `DynamicPalette::reset`;
- OSC reset-all palette uses `DynamicPalette::reset_all`;
- OSC/Kitty foreground/background/cursor set calls `DynamicRgb::set`;
- OSC/Kitty foreground/background/cursor reset calls `DynamicRgb::reset`;
- palette formatters and query responses read `palette.current`;
- palette default getters read `palette.original`.

Because dynamic foreground/background now start unset, update tests that assumed
initial hardcoded foreground/background defaults. If an OSC/Kitty dynamic-color
query targets an unset value, it should emit the same empty response behavior
the existing writer uses for `None`; do not reintroduce hardcoded defaults in
the query path.

Keep cursor query behavior as-is relative to upstream semantics:

- xterm OSC cursor query may fall back to foreground via the existing
  `dynamic_color` helper;
- Kitty OSC 21 cursor query must not add a foreground fallback.

### 5. Extend `roastty_terminal_set`

Implement these additional options:

- `ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND` accepts `const roastty_rgb_s*` or
  `NULL`;
- `ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND` accepts `const roastty_rgb_s*` or
  `NULL`;
- `ROASTTY_TERMINAL_OPTION_COLOR_CURSOR` accepts `const roastty_rgb_s*` or
  `NULL`;
- `ROASTTY_TERMINAL_OPTION_COLOR_PALETTE` accepts `const roastty_palette_t*` or
  `NULL`.

Semantics:

- null terminal returns `ROASTTY_INVALID_VALUE`;
- non-null RGB pointers update the `default` dynamic color;
- null RGB pointers clear that dynamic color default;
- non-null palette pointer copies all 256 entries into the default/original
  palette while preserving masked runtime overrides;
- null palette pointer resets the default/original palette to `DEFAULT_PALETTE`
  while preserving masked runtime overrides;
- all successful color option changes mark the terminal palette dirty flag if
  such a flag remains represented in Roastty. If Roastty has no externally
  observable dirty flag yet, document that it is deferred with render state.

The C ABI must copy palette data immediately. It must not retain caller memory.

### 6. Implement the existing color `roastty_terminal_get` selectors

The public header and Rust constants already reserve these upstream-compatible
`roastty_terminal_data_e` selectors, but they currently return
`ROASTTY_NO_VALUE` as declared-but-deferred placeholders. Implement them:

- `ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND`;
- `ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND`;
- `ROASTTY_TERMINAL_DATA_COLOR_CURSOR`;
- `ROASTTY_TERMINAL_DATA_COLOR_PALETTE`;
- `ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT`;
- `ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT`;
- `ROASTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT`;
- `ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT`.

Semantics:

- effective foreground/background/cursor return `ROASTTY_NO_VALUE` when unset;
- default foreground/background/cursor return `ROASTTY_NO_VALUE` when unset;
- effective foreground/background/cursor return the runtime override when set,
  otherwise their configured default;
- palette returns `current`;
- palette default returns `original`;
- foreground/background/cursor selectors write `roastty_rgb_s` to
  `roastty_rgb_s*` output pointers;
- palette selectors write all 256 entries to `roastty_palette_t*` output
  pointers;
- null output pointer still returns `ROASTTY_INVALID_VALUE`;
- unset dynamic RGB selectors return `ROASTTY_NO_VALUE` without mutating the
  output buffer;
- selectors outside the implemented set keep their existing behavior;
- existing discriminant tests should remain in place and be extended only where
  needed for the new value types.

### 7. Extend tests

Port or mirror the relevant upstream C tests from
`vendor/ghostty/src/terminal/c/terminal.zig`:

- `set and get color_foreground`;
- `set and get color_background`;
- `set and get color_cursor`;
- `set and get color_palette`;
- `get color default vs effective with override`;
- `get color default returns no_value when unset`;
- `get color_palette_default vs current`.

Also port or mirror the relevant `DynamicPalette` tests from
`vendor/ghostty/src/terminal/color.zig`:

- init;
- set;
- reset;
- reset all;
- change default with no changes;
- change default preserves one changed entry;
- change default preserves multiple changed entries.

Rust ABI tests must cover:

- public `roastty_rgb_s` layout through Rust-side struct layout;
- color option discriminants `11..14`;
- initial foreground/background/cursor effective and default getters return
  `ROASTTY_NO_VALUE`;
- color set/get/clear for foreground/background/cursor;
- runtime OSC/Kitty overrides affect effective getters but not default getters;
- runtime foreground/background/cursor overrides survive a later
  `roastty_terminal_set` default change, so the effective getter continues to
  report the override while the default getter reports the new configured
  default;
- runtime foreground/background/cursor overrides survive a later
  `roastty_terminal_set(..., NULL)` default clear, so the effective getter
  continues to report the override while the default getter returns
  `ROASTTY_NO_VALUE`;
- after a runtime foreground/background/cursor override is reset, the effective
  getter falls back to the configured default or returns `ROASTTY_NO_VALUE` if
  no default remains;
- palette set/get/reset and copied-storage behavior;
- palette default changes preserve runtime-overridden entries in current;
- palette default getter returns the configured original palette, not the
  current runtime-overridden palette;
- each successful color default/palette option change sets
  `terminal.flags.dirty.palette` if the dirty flag is still represented in
  Roastty;
- existing OSC and Kitty color tests updated for the unset initial dynamic color
  model.

C harness coverage must include:

- `roastty_rgb_s` size and alignment;
- color option discriminants `11..14`;
- foreground/background/cursor set/get/clear from C;
- initial no-value behavior for unset dynamic colors;
- palette set/get and copied-storage behavior from C;
- palette default getter after `terminal_set(color_palette, ...)`;
- unsupported option values still reject.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/terminal.rs roastty/src/terminal/color.rs
cargo test -p roastty dynamic_palette
cargo test -p roastty terminal_color_set_get_abi
cargo test -p roastty terminal_get_abi
cargo test -p roastty terminal_metadata_setters_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty terminal_stream
cargo test -p roastty
! rg -n "ghostty|Ghostty|ghostty_" roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c
```

Required evidence:

- Roastty's newly-created terminal color state matches upstream: dynamic
  foreground/background/cursor are unset;
- config/default colors can be injected through `roastty_terminal_set`;
- OSC/Kitty runtime overrides remain separate from defaults;
- palette has current/original/mask semantics;
- palette default changes preserve masked runtime overrides;
- successful color default and palette setter calls dirty the palette when the
  dirty flag is represented;
- terminal formatter and palette query paths use current palette values;
- terminal color default getters use original/default palette values;
- the C ABI copies RGB/palette data and never borrows caller memory;
- C harness exercises the new public header types and functions;
- existing terminal stream, getter, metadata setter, and full crate tests pass;
- Codex design and result reviews both pass before moving to the next stage.

## Non-Negotiable Invariants

- Use Roastty names only for public ABI and implementation-facing text.
- Do not add `ghostty_*` compatibility names.
- Preserve upstream option/data discriminants.
- Do not expose unrelated terminal option constants.
- Do not implement callbacks, Kitty graphics limits, APC limits, selection,
  resize, viewport scrolling, render state, PTY/process, renderer, font, IME,
  Swift frontend, browser, or non-macOS behavior.
- Do not keep hardcoded foreground/background defaults in terminal
  initialization to satisfy older tests; tests should follow upstream's unset
  dynamic-color model.
- Do not collapse palette current and default/original into one array.
- Do not borrow caller palette memory.

## Failure Criteria

This experiment fails if:

- new color option values or data selectors differ from upstream;
- initial dynamic foreground/background/cursor getters return hardcoded colors
  instead of `ROASTTY_NO_VALUE`;
- `terminal_set` color defaults overwrite runtime OSC/Kitty overrides instead of
  preserving effective override state;
- palette default changes lose masked runtime palette overrides;
- palette default getter returns the current runtime-overridden palette;
- palette or RGB setters retain caller memory;
- existing OSC/Kitty color behavior regresses outside the intentional
  unset-initial-default correction;
- the C harness does not exercise the new ABI from C;
- the design or result proceeds without the required Codex review gate.

## Result

**Result:** Pass

Implemented the terminal color C ABI slice:

- added public `roastty_rgb_s` and `roastty_palette_t` types;
- added implemented terminal color option values `11..14`;
- ported `DynamicPalette` with current/original/mask semantics;
- changed newly-created terminals so foreground/background/cursor dynamic colors
  start unset, matching upstream terminal core behavior;
- routed OSC and Kitty palette operations through runtime palette overrides;
- implemented color default setters through `roastty_terminal_set`;
- implemented existing color `roastty_terminal_get` selectors `18..25`;
- kept effective RGB getters distinct from default RGB getters;
- kept current palette getters distinct from default/original palette getters;
- copied all C RGB and palette inputs at the ABI boundary;
- updated Rust tests and the C harness.

Roastty does not currently represent the upstream renderer `flags.dirty.palette`
field, so there was no dirty-palette flag to set or test in this experiment. The
color state and ABI behavior are otherwise covered by the new tests.

Verification run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/terminal.rs roastty/src/terminal/color.rs
cargo test -p roastty dynamic_palette
cargo test -p roastty terminal_color_set_get_abi
cargo test -p roastty terminal_get_abi
cargo test -p roastty terminal_metadata_setters_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty terminal_stream
cargo test -p roastty
! rg -n "ghostty|Ghostty|ghostty_" roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c roastty/src/terminal/color.rs
```

All commands passed. `cargo test -p roastty` passed with 1817 Rust unit tests,
the C ABI harness, and doc tests.

The implementation result must still pass Codex result review before the result
commit.

## Conclusion

Experiment 169 completes the terminal color defaults/palette C ABI slice. The
next experiment can move to the next coherent upstream terminal ABI surface
without carrying unresolved color-storage debt.
