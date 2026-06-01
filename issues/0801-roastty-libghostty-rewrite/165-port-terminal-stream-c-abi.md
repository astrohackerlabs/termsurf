# Experiment 165: Port Terminal Stream C ABI

## Description

Roastty has a substantial terminal core now: stream parsing, screen/page
mutation, scrollback, cursor state, style state, SGR, OSC/DCS handling, mouse
and key encoders, and formatter paths. But the C ABI still cannot create a
terminal, feed terminal bytes, or read observable terminal state.

This experiment ports the first terminal stream C ABI slice under
`roastty_terminal_*` names. This intentionally follows the upstream `libghostty`
terminal boundary, renamed to Roastty, instead of inventing a surface-level
terminal API. Surfaces may eventually own or reference a terminal, but that
app/surface integration should build on the terminal ABI rather than replace it.

The goal is not to implement PTY spawning, rendering, fonts, selection, resize
reflow, or the Swift frontend. The goal is to make the already-ported terminal
state reachable through a deterministic terminal handle so later PTY, renderer,
selection, and frontend experiments have a stable library boundary to drive.

This experiment intentionally ports only the first terminal stream slice. The
full upstream terminal getter and formatter ABI (`terminal_get`,
`terminal_get_multi`, formatter handles, selection handles, and richer metadata
queries) remains deferred. The inspection helpers below are narrow, test-focused
accessors for proving terminal state is reachable through the C boundary; they
must not be treated as the final complete terminal metadata API.

## Changes

### 1. Add terminal handle and ABI declarations

In `roastty/include/roastty.h`, add:

```c
typedef void* roastty_terminal_t;

ROASTTY_API roastty_result_e roastty_terminal_new(
    uint16_t columns,
    uint16_t rows,
    size_t max_scrollback_rows,
    roastty_terminal_t* out);

ROASTTY_API void roastty_terminal_free(roastty_terminal_t);

ROASTTY_API roastty_result_e roastty_terminal_vt_write(
    roastty_terminal_t,
    const uint8_t* bytes,
    size_t len);

ROASTTY_API roastty_result_e roastty_terminal_read_screen_plain(
    roastty_terminal_t,
    bool unwrap,
    roastty_string_s* out);

ROASTTY_API roastty_result_e roastty_terminal_title(roastty_terminal_t,
                                                    roastty_string_s* out);

ROASTTY_API roastty_result_e roastty_terminal_pwd(roastty_terminal_t,
                                                  roastty_string_s* out);

ROASTTY_API bool roastty_terminal_cursor_position(roastty_terminal_t,
                                                  uint16_t* column,
                                                  uint16_t* row);

ROASTTY_API roastty_result_e roastty_terminal_take_pty_response(
    roastty_terminal_t,
    roastty_string_s* out);
```

Do not add surface-level terminal-feed APIs in this experiment. Do not add
public `ghostty_*` compatibility names.

### 2. Make the terminal core usable from `lib.rs`

In `roastty/src/terminal/mod.rs` and `roastty/src/terminal/terminal.rs`:

- expose the `terminal` module to the crate with `pub(crate) mod terminal`;
- widen only the minimal terminal APIs needed by `lib.rs` from `pub(super)` to
  `pub(crate)`:
  - `Terminal`;
  - `Terminal::init`;
  - `Terminal::next_slice`;
  - title/PWD getters;
  - cursor position getter;
  - active-screen plain formatter helper or formatter types needed to produce
    plain text;
  - PTY-response drain helper.

Do not make terminal internals public outside the crate. Avoid broad visibility
changes to `Screen`, `Page`, style internals, or parser internals unless a
specific compile error proves that a narrow crate-level accessor is required.

### 3. Implement terminal construction/free

In `roastty/src/lib.rs`, add an opaque terminal wrapper around
`terminal::Terminal`.

`roastty_terminal_new` semantics:

- null `out` returns `ROASTTY_INVALID_VALUE`;
- `columns == 0` or `rows == 0` returns `ROASTTY_INVALID_VALUE` and writes a
  null handle to `out`;
- if `max_scrollback_rows == SIZE_MAX`, pass `None` to `Terminal::init`;
- otherwise pass `Some(max_scrollback_rows)`;
- map terminal allocation failure to `ROASTTY_OUT_OF_MEMORY`;
- on success, write a non-null terminal handle to `out`;
- on any failure, write a null handle to `out`.

`roastty_terminal_free` must tolerate null.

Do not implement terminal resize/reflow in this experiment. A later experiment
should port resize semantics explicitly.

### 4. Implement terminal input

`roastty_terminal_vt_write` semantics:

- null terminal returns `ROASTTY_INVALID_VALUE`;
- `bytes == NULL && len > 0` returns `ROASTTY_INVALID_VALUE`;
- `bytes == NULL && len == 0` is a successful no-op;
- `TerminalStreamError::PageAlloc` maps to `ROASTTY_OUT_OF_MEMORY`;
- `TerminalStreamError::ManagedCellUnsupported` maps to `ROASTTY_INVALID_VALUE`;
- `TerminalStreamError::InvalidPoint` maps to `ROASTTY_INVALID_VALUE`;
- `TerminalStreamError::UnsupportedCodepoint(_)` maps to
  `ROASTTY_INVALID_VALUE`;
- successful writes return `ROASTTY_SUCCESS`.

This function feeds bytes into the existing `Terminal::next_slice` path. It must
not spawn a PTY, write to a real file descriptor, call runtime callbacks, or
perform rendering.

### 5. Implement copied string outputs with failure channels

For new string-returning terminal APIs, use result-returning functions with
`roastty_string_s* out` rather than returning `roastty_string_s` directly. The
new APIs must be able to report allocation failure.

Add a local helper for copied ABI strings that:

- writes the existing empty `roastty_string_s` shape to `out` before work when
  `out` is non-null;
- returns `ROASTTY_INVALID_VALUE` if `out` is null;
- uses fallible allocation, such as `Vec::try_reserve_exact`, before copying
  non-empty output bytes;
- returns `ROASTTY_OUT_OF_MEMORY` on allocation failure;
- returns `ROASTTY_SUCCESS` with an ABI-owned `roastty_string_s` on success;
- keeps ownership compatible with `roastty_string_free`.

Do not expose borrowed terminal memory through C.

### 6. Implement observable state APIs

`roastty_terminal_read_screen_plain`:

- formats the current active screen through the existing plain terminal
  formatter;
- follows the formatter's existing `unwrap` behavior;
- does not include inactive screen content unless the existing formatter already
  does;
- returns copied ABI-owned bytes through `out`.

`roastty_terminal_title` and `roastty_terminal_pwd`:

- return copied ABI-owned bytes through `out`;
- return an empty string for empty title/PWD;
- do not expose borrowed internal `String` memory.

`roastty_terminal_cursor_position`:

- returns false for null terminal or null output pointers;
- returns zero-indexed cell coordinates matching internal terminal coordinates.

`roastty_terminal_take_pty_response`:

- drains terminal query responses into copied ABI-owned bytes through `out`;
- a subsequent call returns an empty string until more input generates
  responses.

### 7. Extend the C ABI harness

In `roastty/tests/abi_harness.c`, add a terminal stream scenario:

1. Verify `roastty_terminal_new` rejects null output and zero dimensions.
2. Create a small terminal, such as 5x3.
3. Verify empty writes and null zero-length writes succeed.
4. Verify non-empty null writes fail.
5. Feed plain text and verify `roastty_terminal_read_screen_plain` returns the
   expected visible content.
6. Feed split UTF-8 across two writes and verify stream state survives across
   ABI calls.
7. Feed a split OSC title sequence across separate writes and verify
   `roastty_terminal_title`.
8. Feed a split OSC current-directory sequence across separate writes and verify
   `roastty_terminal_pwd`.
9. Feed a split CSI/query sequence across separate writes, drain
   `roastty_terminal_take_pty_response`, and verify a second drain is empty.
10. Verify cursor position output and null output-pointer behavior.
11. Free all strings and handles.

The C harness should prove that the header declarations, exported Rust symbols,
and dynamic library link together.

### 8. Add Rust tests

In `roastty/src/lib.rs`, add focused ABI tests for:

- terminal construction rejects null/zero dimensions and null output pointers;
- write input validation;
- empty writes succeed;
- plain text formatting returns ABI-owned memory;
- title, PWD, cursor position, and PTY-response draining work;
- split UTF-8 survives across writes;
- split OSC survives across writes;
- split CSI/query parser state survives across writes;
- invalid inputs never panic or dereference null.

Keep existing terminal-core tests intact. This experiment should be mostly ABI
wiring over already-tested terminal behavior, not a rewrite of stream semantics.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/mod.rs roastty/src/terminal/terminal.rs
cargo test -p roastty terminal_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty terminal_stream
cargo test -p roastty
! rg -n "ghostty|Ghostty|ghostty_" roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c
```

Required evidence:

- C callers can create and free `roastty_terminal_t`.
- C callers can feed terminal bytes through `roastty_terminal_vt_write`.
- C callers can read visible active-screen plain content through copied
  `roastty_string_s` memory.
- string-returning helpers report allocation failure instead of hiding it behind
  a direct `roastty_string_s` return.
- UTF-8 state survives across multiple `roastty_terminal_vt_write` calls.
- OSC parser state survives across multiple `roastty_terminal_vt_write` calls.
- CSI/query parser state survives across multiple `roastty_terminal_vt_write`
  calls.
- OSC title and PWD mutations are visible through the terminal inspection
  helpers.
- terminal query responses can be drained exactly once.
- cursor position is exposed as zero-indexed cell coordinates.
- invalid handles, null pointers, zero dimensions, and invalid input buffers
  fail predictably without panicking or dereferencing null.
- the C ABI harness compiles and links against `roastty/include/roastty.h` and
  the built library.
- existing terminal stream tests and the full `roastty` test suite still pass.
- no surface-level terminal feed API, PTY spawn/read/write loop, renderer,
  font/text stack, selection API, clipboard IO, runtime callback dispatch,
  Swift/macOS frontend code, browser behavior, or non-macOS platform branch is
  added.
- Codex design and result reviews both pass before moving to the next stage.

## Non-Negotiable Invariants

- Use Roastty names in public ABI, implementation-facing comments, tests, and
  modules.
- Do not add public `ghostty_*` compatibility names.
- Keep this experiment at the terminal stream C ABI boundary.
- Do not add surface-level terminal feed APIs in this experiment.
- Do not implement PTY spawning or real IO.
- Do not implement renderer, font, glyph atlas, Metal, selection, IME, or Swift
  frontend behavior.
- Do not implement terminal resize/reflow.
- Do not expose borrowed internal string memory through C; return copied
  `roastty_string_s` values through result-returning APIs.
- Run `cargo fmt` and accept its output.
- Pass Codex design and result reviews before moving to the next stage.

## Failure Criteria

This experiment fails if:

- any public `ghostty_*` or compatibility ABI names are introduced;
- terminal writes can dereference null pointers or panic on invalid inputs;
- string-returning helpers expose borrowed terminal memory instead of copied
  ABI-owned strings;
- string-returning helpers cannot report allocation failure;
- terminal parser state is recreated on every write, causing split UTF-8, split
  OSC, or split CSI/query sequences to fail;
- terminal query responses are not drained or are duplicated after draining;
- resize/reflow semantics are added implicitly rather than as their own
  experiment;
- surface-level terminal feed APIs, PTY, renderer, font, selection, runtime
  callback, Swift frontend, browser, or non-macOS platform behavior is added;
- existing terminal, key, mouse, OSC, formatter, or C ABI tests regress;
- the design or result proceeds without the required Codex review gate.

## Codex Design Review

**Result:** Approved after revision.

Codex's first review found blocking design issues: the initial draft invented a
surface-level terminal API instead of following the upstream terminal C ABI
boundary; the verification did not require split escape/parser-state coverage
across ABI writes; the `TerminalStreamError` mapping was incomplete; copied
string APIs had no allocation-failure channel; and plain screen output semantics
were underspecified.

The design was revised to use a standalone `roastty_terminal_t` handle, require
split UTF-8/OSC/CSI coverage, map every current `TerminalStreamError` variant,
use result-returning copied string APIs, and define active-screen plain
formatter semantics.

Codex's second review found one remaining blocking ABI parity issue: the write
entrypoint should preserve the upstream `terminal_vt_write` shape. The design
was updated from `roastty_terminal_write` to `roastty_terminal_vt_write` and
also added an explicit note that fuller upstream `terminal_get`,
`terminal_get_multi`, formatter handles, selection handles, and richer metadata
APIs remain deferred.

Codex's final review found no remaining blocking design issues and approved the
experiment for implementation.
