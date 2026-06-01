# Experiment 168: Port Terminal Metadata Setters C ABI

## Description

Experiment 167 added direct terminal mode-control ABI functions. The next
upstream terminal C ABI surface is `terminal_set`, but upstream's option table
mixes several unrelated concerns:

- embedder callbacks (`write_pty`, bell, DA, ENQ, XTVERSION, size, title
  changed, color scheme);
- text metadata (`title`, `pwd`);
- color defaults and palette;
- Kitty graphics limits;
- APC byte limits;
- selection state.

Roastty should not expose one large `roastty_terminal_set` function and silently
pretend all of those surfaces work. This experiment ports the first isolated
slice: text metadata setters for title and current working directory. These are
already represented in `Terminal`, already observable through
`roastty_terminal_title`, `roastty_terminal_pwd`, and OSC-generated formatter
output, and do not require stream effect callbacks, selection ownership, Kitty
graphics storage, resize/reflow, or renderer integration.

This is intentionally a narrow ABI foundation experiment. Later experiments can
extend the same `roastty_terminal_set` entry point with callbacks, colors,
palette, APC limits, Kitty graphics, and selection once their backing subsystems
are ready.

## Changes

### 1. Add terminal option constants for the implemented slice

In `roastty/include/roastty.h`, add:

```c
typedef enum {
  ROASTTY_TERMINAL_OPTION_TITLE = 9,
  ROASTTY_TERMINAL_OPTION_PWD = 10,
} roastty_terminal_option_e;
```

The numeric values must match upstream Ghostty's option table:

- `title = 9`;
- `pwd = 10`.

Do not expose unimplemented option constants in this experiment. In particular,
do not add callback, color, Kitty graphics, APC, or selection options until
their behavior is implemented and verified. This avoids advertising a public ABI
surface that returns placeholder results for known upstream options.

### 2. Add `roastty_terminal_set`

In `roastty/include/roastty.h`, add:

```c
ROASTTY_API roastty_result_e roastty_terminal_set(roastty_terminal_t,
                                                  roastty_terminal_option_e,
                                                  const void*);
```

Semantics for this experiment:

- null terminal returns `ROASTTY_INVALID_VALUE`;
- unknown option returns `ROASTTY_INVALID_VALUE`;
- `ROASTTY_TERMINAL_OPTION_TITLE` accepts `const roastty_string_s*`;
- `ROASTTY_TERMINAL_OPTION_PWD` accepts `const roastty_string_s*`;
- null value for title clears the title, matching upstream's `value == null`
  behavior;
- null value for PWD clears the PWD, matching upstream's `value == null`
  behavior;
- `{ ptr = NULL, len = 0 }` is accepted as an empty string and produces the same
  visible state as clearing the field;
- `{ ptr = NULL, len > 0 }` returns `ROASTTY_INVALID_VALUE` and must not mutate
  the prior value;
- non-null strings are copied into terminal-owned storage;
- the terminal does not retain the input pointer;
- interior NUL bytes are valid payload bytes and should not truncate the copied
  value;
- invalid UTF-8 returns `ROASTTY_INVALID_VALUE` and must not mutate existing
  terminal state.
- allocation failure while copying the input string returns
  `ROASTTY_OUT_OF_MEMORY` and must not mutate the prior value.

Implementation note: `roastty_string_s` carries a pointer plus length. Do not
use `strlen`.

### 3. Add private terminal setters

In `roastty/src/terminal/terminal.rs`, add private/public-crate helpers such as:

```rust
pub(crate) fn set_title(&mut self, value: Option<String>);
pub(crate) fn set_pwd(&mut self, value: Option<String>);
```

These helpers should update the existing `TerminalTitle` and `TerminalPwd`
storage. They should not simulate OSC parsing and should not emit callbacks.
Title-changed callbacks are a separate effect-callback experiment.

The fallible allocation boundary belongs before these helpers are called:
`lib.rs` must validate the `roastty_string_s`, decode UTF-8, and stage an owned
`String` before mutating terminal state. If allocation fails while staging that
owned value, return `ROASTTY_OUT_OF_MEMORY` and leave the previous title/PWD
unchanged. The terminal helper may then take ownership of the already-staged
string and commit it infallibly.

### 4. Keep unsupported surfaces out of scope

Do not implement or stub behavior for:

- `userdata`;
- `write_pty`;
- bell;
- enquiry;
- XTVERSION;
- title-changed callback;
- size callback;
- color-scheme callback;
- device-attributes callback;
- foreground/background/cursor/palette color options;
- Kitty image options;
- APC byte-limit options;
- selection options.

If implementation reveals that adding the `roastty_terminal_set` function
requires representing the full option enum for C compatibility, stop and mark
the experiment Partial. Do not quietly add placeholder behavior for the rest of
the option table.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/terminal.rs
cargo test -p roastty terminal_metadata_setters_abi
cargo test -p roastty terminal_get_abi
cargo test -p roastty terminal_mode_control_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty terminal_stream
cargo test -p roastty
! rg -n "ghostty|Ghostty|ghostty_" roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c
```

Rust tests must cover:

- option discriminants are stable (`TITLE == 9`, `PWD == 10`);
- null terminal returns `ROASTTY_INVALID_VALUE`;
- unknown option returns `ROASTTY_INVALID_VALUE`;
- upstream-known but unsupported options return `ROASTTY_INVALID_VALUE`, with
  representative checks for `0` (`userdata`) and `11` (`color_foreground`);
- title set/get through `roastty_terminal_title`;
- PWD set/get through `roastty_terminal_pwd`;
- null title clears the title;
- null PWD clears the PWD;
- `{ ptr = NULL, len = 0 }` is accepted as empty;
- `{ ptr = NULL, len > 0 }` is rejected without mutating existing state;
- input strings are copied, not borrowed;
- explicit lengths are respected and interior NUL bytes do not truncate storage;
- invalid UTF-8 returns `ROASTTY_INVALID_VALUE` and leaves prior state intact;
- setting title/PWD directly does not mutate screen contents, cursor position,
  modes, or active screen;
- OSC title/PWD updates still work after direct `terminal_set`, and direct
  `terminal_set` still works after OSC updates.

C harness coverage must include:

- enum discriminants for title and PWD;
- successful title and PWD set/get through the public header;
- null value clears both fields;
- invalid option returns `ROASTTY_INVALID_VALUE`;
- representative unsupported upstream options `0` and `11` return
  `ROASTTY_INVALID_VALUE`;
- null terminal returns `ROASTTY_INVALID_VALUE`;
- `{ ptr = NULL, len = 0 }` is accepted as empty;
- `{ ptr = NULL, len > 0 }` is rejected without mutation;
- copied-storage behavior by mutating the input buffer after set.

## Non-Negotiable Invariants

- Use Roastty names only for public ABI and implementation-facing text.
- Do not add `ghostty_*` compatibility names.
- Do not expose unimplemented `roastty_terminal_option_e` constants.
- Do not implement callback/effects behavior in this experiment.
- Do not implement color, palette, Kitty graphics, APC, selection, resize,
  viewport scrolling, render state, PTY/process, renderer, font, IME, Swift
  frontend, browser, or non-macOS behavior.
- Do not make `roastty_terminal_set` parse escape sequences; it directly updates
  terminal-owned metadata.
- Do not borrow caller-provided string memory.

## Failure Criteria

This experiment fails if:

- public option values do not match upstream's `title = 9` and `pwd = 10`;
- invalid UTF-8 mutates existing terminal state;
- `{ ptr = NULL, len > 0 }` is accepted or dereferenced;
- allocation failure mutates the previous title/PWD value before returning
  `ROASTTY_OUT_OF_MEMORY`;
- string storage borrows caller memory;
- interior NUL bytes are truncated because the implementation uses `strlen`;
- title/PWD direct setters accidentally mutate visible terminal contents, cursor
  state, modes, or active screen;
- callback, color, Kitty graphics, APC, selection, resize, or renderer behavior
  is added as placeholder scope creep;
- the C harness does not exercise `roastty_terminal_set` from the public header;
- the design or result proceeds without the required Codex review gate.

## Codex Design Review

**Result:** Approved after revision.

Codex's first review accepted the narrow `TITLE = 9` / `PWD = 10`
`roastty_terminal_set` slice as a reasonable staged ABI step, but found real
gaps in the string-input contract:

- the design needed explicit handling for inner `roastty_string_s` null-pointer
  cases: `{ ptr = NULL, len = 0 }` is valid empty input, while
  `{ ptr = NULL, len > 0 }` is invalid and must not mutate existing state;
- the design needed an explicit allocation-failure contract:
  `ROASTTY_OUT_OF_MEMORY` with no prior-state mutation;
- unsupported upstream-known option values, such as `0` (`userdata`) and `11`
  (`color_foreground`), needed representative rejection tests.

The design was updated with those rules. Codex's second review found one
remaining contradiction: the proposed terminal helper signatures were infallible
even though `lib.rs` must report allocation failure. The design was revised so
`lib.rs` stages a validated owned `String` before mutating terminal state, and
the terminal helpers take ownership of that staged value.

Codex's final review found no blocking findings and approved the experiment for
implementation.
