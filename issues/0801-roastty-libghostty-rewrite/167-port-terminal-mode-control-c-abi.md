# Experiment 167: Port Terminal Mode Control C ABI

## Description

Experiment 166 added generic scalar terminal getters. The next narrow upstream
terminal C ABI slice is terminal control state that already exists in Roastty:
full reset and mode get/set.

This experiment ports:

- `roastty_terminal_reset`;
- `roastty_terminal_mode_get`;
- `roastty_terminal_mode_set`;
- the public packed mode-tag representation used by the mode get/set ABI.

It also repairs one compatibility detail made visible by adding
`roastty_terminal_mode_set`: upstream `terminal_get(MOUSE_TRACKING)` reads the
terminal mode table, not the cached mouse runtime field. Roastty currently keeps
a runtime mouse-event cache for input encoding, but the public scalar getter
should report the mode table so direct C ABI mode changes and escape-sequence
mode changes produce the same query result.

This experiment does not port resize, `terminal_set`, effects callbacks, encoder
`setopt_from_terminal`, viewport scrolling, selection, render state, or
formatter ownership. Those are larger surfaces with separate ownership and side
effect questions.

## Changes

### 1. Add public mode tag ABI shape

In `roastty/include/roastty.h`, add:

```c
typedef uint16_t roastty_mode_tag_t;

enum {
  ROASTTY_MODE_TAG_VALUE_MASK = 0x7fff,
  ROASTTY_MODE_TAG_ANSI_BIT = 0x8000,
};
```

Upstream Ghostty's `ModeTag` is a packed `u16`:

- low 15 bits: mode numeric value;
- high bit: ANSI-family flag.

Examples:

- DEC cursor keys `?1` is `0x0001`;
- ANSI insert mode `4` is `0x8004`;
- DEC wraparound `?7` is `0x0007`;
- DEC bracketed paste `?2004` is `0x07d4`;
- ANSI linefeed mode `20` is `0x8014`.

Do not expose a `ghostty_*` name. Do not invent a different struct layout.

### 2. Add terminal reset

Add:

```c
ROASTTY_API void roastty_terminal_reset(roastty_terminal_t);
```

Semantics:

- null terminal is a no-op;
- reset uses the same state transition as RIS / full terminal reset;
- preserve terminal dimensions;
- reset screens, modes, scrolling region, tabstops, title, PWD, DCS state,
  mouse/key runtime flags, and previous character state;
- do not add PTY, process, renderer, font, selection, frontend, or browser
  behavior.

Implementation note: upstream `terminal_reset` calls `ZigTerminal.fullReset()`
and does not reset the persistent C wrapper stream parser. Roastty should match
that behavior unless implementation reveals a direct contradiction in current
Roastty stream ownership. If a shared helper is needed to avoid duplicating RIS
logic, factor the reset logic into a private terminal helper and have both RIS
and the C ABI call it.

### 3. Add mode get/set

Add:

```c
ROASTTY_API roastty_result_e
roastty_terminal_mode_get(roastty_terminal_t,
                          roastty_mode_tag_t,
                          bool* out);

ROASTTY_API roastty_result_e
roastty_terminal_mode_set(roastty_terminal_t,
                          roastty_mode_tag_t,
                          bool value);
```

Semantics:

- null terminal returns `ROASTTY_INVALID_VALUE`;
- null `out` for mode get returns `ROASTTY_INVALID_VALUE`;
- unknown mode tag returns `ROASTTY_INVALID_VALUE`;
- known mode get writes the current value and returns `ROASTTY_SUCCESS`;
- known mode set updates the mode table and returns `ROASTTY_SUCCESS`;
- mode set follows upstream's direct C ABI behavior: it updates the mode table
  only, not the higher-level escape-sequence side effects used by stream mode
  commands.

Examples that must work:

- `0x8004` maps to ANSI insert mode;
- `0x8014` maps to ANSI linefeed mode;
- `0x0007` maps to DEC wraparound;
- `0x07d4` maps to DEC bracketed paste;
- `0x8009` is invalid because mouse mode 9 is DEC-only;
- `0x270f` is invalid because mode 9999 is unknown.

### 4. Align mouse-tracking scalar getter

Update `roastty_terminal_get(..., ROASTTY_TERMINAL_DATA_MOUSE_TRACKING, ...)` to
report:

```text
mouse_event_x10 || mouse_event_normal || mouse_event_button || mouse_event_any
```

from the terminal mode table, matching upstream `terminal_get`. Do not read the
runtime mouse-event cache for the scalar getter. The runtime cache still exists
for mouse encoder behavior and should not be removed here.

Verification must prove both paths:

- escape-sequence mode changes still make `MOUSE_TRACKING` true/false;
- direct `roastty_terminal_mode_set` changes for each of the four DEC mouse
  tracking tags make `MOUSE_TRACKING` true:
  - DEC X10 mouse `?9`;
  - DEC normal mouse `?1000`;
  - DEC button-event mouse `?1002`;
  - DEC any-event mouse `?1003`;
- `MOUSE_TRACKING` returns false again only after all four tracking modes are
  reset.

### 5. Extend Rust tests

In `roastty/src/lib.rs`, add `terminal_mode_control_abi` tests covering:

- mode tag packing constants and representative tags;
- reset null terminal no-op;
- mode get validation: null terminal, null `out`, invalid ANSI/DEC family, and
  unknown mode value;
- mode get defaults for representative modes:
  - ANSI insert false;
  - ANSI send/receive true;
  - DEC wraparound true;
  - DEC bracketed paste false;
- mode set/get round trips for ANSI insert, ANSI linefeed, DEC wraparound, and
  DEC bracketed paste;
- direct mode-set of DEC 1049 updates the mode table as observed by
  `roastty_terminal_mode_get`, but does not switch the active screen;
- reset restores default modes, clears title/PWD, returns to the primary screen,
  clears pending wrap, clears Kitty keyboard flags, and clears mouse tracking;
- reset preserves dimensions;
- reset restores full scrolling region;
- reset restores default tabstops;
- reset clears previous-character state, proven by writing a character, issuing
  reset, then sending REP and verifying the pre-reset character is not repeated;
- `ROASTTY_TERMINAL_DATA_MOUSE_TRACKING` reports true after both CSI mouse-mode
  input and direct `roastty_terminal_mode_set`.
- `ROASTTY_TERMINAL_DATA_MOUSE_TRACKING` reports true for direct mode-set of all
  four upstream mouse tracking modes (`?9`, `?1000`, `?1002`, `?1003`) and false
  after all four are reset.

### 6. Extend the C ABI harness

In `roastty/tests/abi_harness.c`, extend the terminal scenario to verify:

- `roastty_mode_tag_t` has `sizeof(uint16_t)`;
- `ROASTTY_MODE_TAG_VALUE_MASK == 0x7fff`;
- `ROASTTY_MODE_TAG_ANSI_BIT == 0x8000`;
- mode get/set round trips from C for ANSI insert and DEC wraparound;
- invalid tags return `ROASTTY_INVALID_VALUE`;
- reset restores default mode values and clears visible terminal state.
- reset preserves dimensions.
- direct mode-set of DEC 1049 updates mode state while leaving active screen
  unchanged.
- direct mode-set of each mouse tracking mode is reflected by
  `ROASTTY_TERMINAL_DATA_MOUSE_TRACKING`.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/terminal.rs roastty/src/terminal/modes.rs
cargo test -p roastty terminal_mode_control_abi
cargo test -p roastty terminal_get_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty terminal_stream
cargo test -p roastty
! rg -n "ghostty|Ghostty|ghostty_" roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c
```

Required evidence:

- mode tag constants match upstream packed `ModeTag` layout;
- raw mode tags are decoded before mode lookup;
- invalid tags are rejected without mutating terminal state;
- mode get and set work for both ANSI and DEC mode families;
- direct mode set changes the mode table but does not run escape-sequence-only
  side effects such as active-screen switching;
- terminal reset matches RIS-visible terminal state reset while preserving
  dimensions;
- terminal reset restores full scrolling region, default tabstops, and clears
  previous-character state;
- `MOUSE_TRACKING` scalar getter reads the mode table and remains correct for
  both CSI-driven and direct C ABI mode changes;
- `MOUSE_TRACKING` scalar getter is true for every upstream mouse tracking mode
  individually (`?9`, `?1000`, `?1002`, `?1003`) and false after all four are
  reset;
- direct DEC 1049 mode-set updates mode state but does not switch active screen;
- existing scalar getter tests, terminal stream tests, and full `roastty` suite
  still pass;
- C harness compiles and exercises the new ABI from C;
- Codex design and result reviews both pass before moving to the next stage.

## Non-Negotiable Invariants

- Use Roastty names in public ABI, implementation-facing comments, tests, and
  modules.
- Do not add public `ghostty_*` compatibility names.
- Do not implement resize, terminal options/effects callbacks, encoder
  `setopt_from_terminal`, viewport scrolling, selection, formatter, render
  state, PTY/process, renderer, font, IME, Swift frontend, browser, or non-macOS
  platform behavior.
- Preserve upstream packed mode-tag ABI semantics.
- Keep `roastty_terminal_mode_set` as a direct mode-table update, not a replay
  of CSI mode-command side effects.
- Keep the runtime mouse-event cache in place for existing stream/input
  behavior; only the public scalar `MOUSE_TRACKING` getter changes to read
  modes.

## Failure Criteria

This experiment fails if:

- any public `ghostty_*` or compatibility ABI names are introduced;
- mode tag packing differs from upstream's low-15-bit value plus high ANSI bit;
- invalid mode tags are accepted or mutate state;
- mode get/set cannot distinguish ANSI and DEC families;
- mode set runs escape-sequence-only side effects such as alternate-screen
  switching;
- reset changes terminal dimensions or adds process/PTY/frontend behavior;
- `MOUSE_TRACKING` still reads only the runtime cache and misses direct
  `roastty_terminal_mode_set` changes;
- existing terminal getter, stream, key, mouse, OSC, formatter, or C ABI tests
  regress;
- the design or result proceeds without the required Codex review gate.

## Codex Design Review

**Result:** Approved after revision.

Codex's first review agreed with the scope and direct mode-set direction, but
found three real verification gaps:

- `MOUSE_TRACKING` needed explicit direct-mode-set coverage for all four
  upstream mouse tracking modes (`?9`, `?1000`, `?1002`, `?1003`), not just a
  generic true/false check.
- Reset verification named more state than it proved. The design needed concrete
  checks for preserved dimensions, full scrolling region, default tabstops, and
  cleared previous-character state.
- The DEC 1049 side-effect test needed to prove both halves of the contract:
  direct `mode_set` updates the mode table as observed by `mode_get`, while the
  active screen remains unchanged.

The design was updated with those required checks. Codex's second review found
no remaining blocking issues and approved the experiment for implementation.

## Result

**Result:** Pass

Experiment 167 implemented the terminal mode-control C ABI slice:

- added `roastty_mode_tag_t` plus the packed mode-tag constants
  `ROASTTY_MODE_TAG_VALUE_MASK` and `ROASTTY_MODE_TAG_ANSI_BIT`;
- added `roastty_terminal_reset`;
- added `roastty_terminal_mode_get`;
- added `roastty_terminal_mode_set`;
- aligned `ROASTTY_TERMINAL_DATA_MOUSE_TRACKING` with upstream getter semantics
  by reading the terminal mode table instead of the runtime mouse-event cache.

The direct mode setter intentionally updates the mode table only. DEC 1049
therefore becomes observable through `roastty_terminal_mode_get`, but it does
not switch the active screen. Escape-sequence processing remains responsible for
the higher-level alternate-screen side effects.

The reset entry point uses the same terminal-state reset surface as RIS-visible
full reset while preserving dimensions and the wrapper stream parser.

Verification passed:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/terminal.rs roastty/src/terminal/modes.rs
cargo test -p roastty terminal_mode_control_abi
cargo test -p roastty terminal_get_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty terminal_stream
cargo test -p roastty
! rg -n "ghostty|Ghostty|ghostty_" roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c
```

Observed results:

- `terminal_mode_control_abi`: 6 passed;
- `terminal_get_abi`: 6 passed;
- C ABI harness: passed;
- `terminal_stream`: 381 passed;
- full `roastty`: 1799 unit tests, C harness, and doc-tests passed;
- forbidden public/source name grep over touched ABI files passed.

## Codex Result Review

**Result:** Approved.

Codex reviewed the completed diff and found no blocking implementation issues.
It confirmed that:

- the packed `roastty_mode_tag_t` ABI and Rust decoding match upstream's
  low-15-bit value plus high ANSI bit layout;
- reset preserves dimensions, resets terminal state, and does not reset the
  wrapper stream parser;
- `MOUSE_TRACKING` now reads the mode table while leaving the runtime mouse
  cache available for stream/input behavior;
- Rust tests and the C harness cover the experiment requirements.

## Conclusion

Roastty now exposes the next terminal control-state ABI slice needed by the
macOS frontend: direct full reset, mode get, and mode set. The implementation
keeps mode-table mutation separate from CSI side effects, matching upstream's C
ABI behavior and preserving the active-screen semantics already owned by stream
processing.
