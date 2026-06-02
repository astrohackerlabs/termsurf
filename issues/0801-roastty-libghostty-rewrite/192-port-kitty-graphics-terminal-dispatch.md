# Experiment 192: Port Kitty Graphics Terminal Dispatch

## Description

Experiments 186-191 ported Kitty graphics parsing, direct image loading, image
storage, query/transmit execution, placement storage, and storage-side display
execution. The remaining gap is that this machinery is not connected to terminal
input:

- `stream.rs` emits `Action::ApcStart`, `Action::ApcPut`, and `Action::ApcEnd`;
- `terminal.rs` currently ignores all APC actions;
- `Screen` has no `ImageStorage`;
- `graphics_exec::execute(storage, command)` can execute query/transmit but has
  no terminal state for display commands;
- `graphics_exec::display_with_location(...)` can store display placements once
  terminal state supplies a location.

The next coherent slice is terminal dispatch for Kitty graphics APC sequences.
This experiment should make `ESC_G...ESC\` commands mutate the active screen's
Kitty image storage and write Kitty responses to the existing PTY response
buffer.

This is still not renderer integration. It should not expose public C ABI,
render pixels, implement deletion/animation commands, or add a macOS app
surface. It is the terminal-core bridge between already-ported Kitty graphics
storage/execution and already-ported terminal stream decoding.

## Changes

1. Add Kitty graphics storage to `Screen`.
   - Add an `ImageStorage` field to `Screen`.
   - Initialize it in `Screen::init`.
   - Reset it in `Screen::reset`.
   - Add small internal accessors for active-screen Kitty storage, for example:
     - `kitty_images(&self) -> &ImageStorage`;
     - `kitty_images_mut(&mut self) -> &mut ImageStorage`.
   - Keep the storage screen-local, matching the existing primary/alternate
     screen split. Do not attach image storage globally to `Terminal`.

2. Add APC accumulation state to `Terminal`.
   - Add a small terminal-owned Kitty graphics APC parser/buffer state.
   - On `Action::ApcStart`, reset the state to "pending first byte".
   - On the first `Action::ApcPut { byte }` after `ApcStart`, select Kitty
     graphics only if `byte == b'G'`.
   - If the first APC byte is not `G`, switch to a "drain non-Kitty APC" state
     until `ApcEnd`.
   - On subsequent `Action::ApcPut { byte }` for a selected Kitty graphics APC,
     feed payload bytes to the Kitty graphics parser after the leading `G`.
   - On `Action::ApcEnd`, complete the parser and execute the command if the APC
     was a Kitty graphics APC.
   - Non-Kitty APC sequences must remain ignored, preserving current behavior.
   - Parser errors should not panic or dirty terminal text state. Ignore parse
     errors in this experiment rather than inventing partial responses; the
     current parser returns only `Result<Command, ParseError>` and does not
     expose partial `i` / `I` / `p` / `q` fields.
   - Use a concrete terminal APC payload limit equal to the existing direct
     loading limit used by `LoadingImageLimits::DIRECT`. If the parser reports
     `ParseError::OutOfMemory`, ignore the failed APC, clear the parser state,
     and leave terminal text state unchanged.

3. Execute query and transmit through existing storage execution.
   - For `CommandControl::Query` and `CommandControl::Transmit`, call the
     existing `graphics_exec::execute(storage, command)` path against the active
     screen's storage.
   - Encode any returned `Response` with `Response::encode(...)`.
   - Append the encoded bytes through `write_pty_response_bytes(...)` so the
     existing PTY response callback path is preserved.
   - Preserve quiet-mode filtering from `graphics_exec::execute`.

4. Execute storage-side display through terminal state.
   - For `CommandControl::Display`, call
     `graphics_exec::display_with_location(...)` through a new narrowly scoped
     terminal-display wrapper that preserves the same behavior as
     `graphics_exec::execute(...)` for:
     - `ImageStorage::enabled() == false`;
     - `Quiet::Ok` success suppression;
     - `Quiet::Failures` failure suppression.
   - For non-virtual display commands, use the active cursor cell as
     `PlacementLocation::Cell { x, y }`.
   - For virtual display commands, continue to let `display_with_location(...)`
     force `PlacementLocation::Virtual`.
   - Do not fake tracked pin ownership in this experiment. The stored cell
     location is a truthful but temporary terminal-coordinate bridge; tracked
     page-pin ownership belongs in a later experiment once placement cleanup and
     renderer access can share that source of truth.

5. Preserve cursor movement scope.
   - Keep `CursorMovement::After` deferred.
   - Do not move the cursor in this experiment.
   - Do not compute placement grid size from pixel metrics in this experiment.
     `Placement::grid_size(...)` already exists, but terminal APC dispatch does
     not yet own renderer/cell pixel metrics. Cursor-after movement should be a
     later experiment that wires those metrics deliberately.

6. Preserve unsupported command behavior.
   - Keep `TransmitAndDisplay` returning the existing unimplemented response
     through `graphics_exec::execute`.
   - Do not route only the transmit half. That would store an image while
     silently dropping display semantics.
   - If implementing `TransmitAndDisplay` correctly requires combining transmit
     and terminal display in one atomic path, stop and design a later experiment
     rather than quietly expanding scope.
   - Keep delete and animation commands unimplemented.

7. Add focused terminal dispatch tests.
   - Add tests proving that:
     - non-Kitty APC is ignored;
     - malformed Kitty APC does not panic or mutate text state;
     - an over-limit Kitty APC does not panic, clears parser state, and leaves
       terminal text state unchanged;
     - query APC writes the expected Kitty response;
     - direct transmit APC stores an image on the active screen;
     - quiet transmit suppresses the success response but stores the image;
     - display APC stores a placement at the current cursor cell;
     - quiet display success and failure filtering matches query/transmit quiet
       filtering;
     - disabled image storage suppresses display mutation and responses;
     - display by image number resolves the newest matching image;
     - virtual display stores `PlacementLocation::Virtual`;
     - display with `CursorMovement::After` stores placement but leaves cursor
       position unchanged;
     - switching to the alternate screen uses separate Kitty image storage;
     - full terminal reset clears Kitty image storage.
     - reset/full reset clears any partial APC parser state.
   - Use equivalent Roastty tests when upstream tests are tied to renderer
     internals not yet ported.

## Verification

Run:

```bash
cargo fmt -- roastty/src/terminal/terminal.rs roastty/src/terminal/screen.rs roastty/src/terminal/kitty/mod.rs roastty/src/terminal/kitty/graphics_command.rs roastty/src/terminal/kitty/graphics_exec.rs roastty/src/terminal/kitty/graphics_storage.rs
cargo test -p roastty kitty_graphics_command
cargo test -p roastty kitty_graphics_exec
cargo test -p roastty kitty_graphics_storage
cargo test -p roastty terminal_stream_kitty_graphics
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

The experiment passes when terminal `next_slice(...)` can receive Kitty graphics
APC sequences, mutate active-screen image/placement storage for query, transmit,
and display commands, and write encoded Kitty responses through the existing PTY
response path, while leaving rendering, C ABI, tracked placement pins,
cursor-after movement, deletion, animation, and combined transmit-and-display
semantics deferred.

## Non-Negotiable Invariants

- Do not expose any `ghostty_*` ABI names.
- Do not add public Kitty graphics C ABI in this experiment.
- Do not render images.
- Do not add renderer-facing placement iteration APIs.
- Do not mutate terminal cursor position for Kitty graphics commands.
- Do not add tracked terminal pin ownership.
- Do not compute cursor-after movement without renderer/cell pixel metrics.
- Do not add Unicode virtual placement rendering.
- Do not implement delete or animation execution.
- Do not silently expand this experiment into a full Kitty graphics renderer.
- Do not break Experiment 189 query/transmit behavior.
- Do not break Experiment 190 placement storage or eviction behavior.
- Do not break Experiment 191 storage-side display behavior.
- Do not skip Codex design review. If the design review finds a real issue, fix
  it and re-review before committing this experiment design.
- Do not skip Codex result review after implementation.

## Result

**Result:** Pass

Implemented terminal dispatch for Kitty graphics APC sequences.

The implementation adds screen-local Kitty image storage and resets it with the
screen. Terminal stream handling now accumulates APC payloads, selects Kitty
graphics only when the first APC byte is `G`, drains non-Kitty APCs, ignores
malformed/over-limit Kitty payloads without mutating text state, and executes
completed Kitty graphics commands against the active screen.

Query, transmit, unsupported combined transmit/display, delete, and animation
commands route through the existing `graphics_exec::execute(...)` path. Display
commands use the active cursor cell and the storage-side
`display_with_location(...)` helper, with a narrow terminal wrapper preserving
storage-disabled behavior and quiet filtering. `CursorMovement::After` remains
deferred and does not move the cursor.

Added focused terminal-stream tests for:

- non-Kitty APC drain behavior;
- malformed and over-limit Kitty APC handling;
- query responses;
- direct transmit storage;
- quiet transmit response suppression;
- display placement at the current cursor cell;
- display quiet filtering, including successful `q=2` suppression;
- disabled image storage;
- display by newest image number;
- virtual display placement;
- deferred cursor-after behavior;
- separate primary/alternate screen storage;
- reset behavior for image storage and partial APC parser state.

Codex result review initially found a real quiet-filtering bug: display `q=2`
was suppressing failures only, while query/transmit suppress all responses for
`Quiet::Failures`. The implementation was fixed to mirror `graphics_exec`, a
successful `q=2` display test was added, and Codex re-review found no remaining
issues.

Verification passed:

```bash
cargo fmt -- roastty/src/terminal/terminal.rs roastty/src/terminal/screen.rs roastty/src/terminal/kitty/mod.rs roastty/src/terminal/kitty/graphics_command.rs roastty/src/terminal/kitty/graphics_exec.rs roastty/src/terminal/kitty/graphics_storage.rs
cargo test -p roastty kitty_graphics_command
cargo test -p roastty kitty_graphics_exec
cargo test -p roastty kitty_graphics_storage
cargo test -p roastty terminal_stream_kitty_graphics
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

The focused Kitty terminal dispatch suite passed with 14 tests. The full Roastty
suite passed with 1,994 Rust tests plus the C ABI harness.

## Conclusion

Roastty terminal input now reaches Kitty graphics storage. A terminal stream can
receive Kitty APC sequences, store/query/transmit images, create display
placements at the active cursor cell, and emit Kitty protocol responses through
the existing PTY response path.

The next experiment should move from terminal dispatch toward renderer-facing
state access and/or tracked placement ownership. Rendering, public C ABI,
tracked page pins, cursor-after movement, deletion execution, animation
execution, Unicode virtual placement rendering, and atomic transmit-and-display
semantics are still deferred.
