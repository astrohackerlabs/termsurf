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

# Experiment 232: Port `Contents::add` / `clear` and `Key` (finish `cell.zig`)

## Description

Complete the `Contents` builder — and the whole `renderer/cell.zig` port — by
adding `add`, `clear`, and the `Key` enum from upstream. `add` appends a
foreground cell to the appropriate row list; `clear` empties one row's
background span and foreground list. This finishes `cell.zig` (predicates 227,
`is_symbol` 228, `constraint_width` 229, storage 230, cursor 231, and now row
mutation 232).

### Behavior to port

Upstream:

```
pub const Key = enum { bg, text, underline, strikethrough, overline,
    pub fn CellType(self) type { return switch (self) {
        .bg => CellBg, .text, .underline, .strikethrough, .overline => CellText }; }
};

pub fn add(self, alloc, comptime key: Key, cell: key.CellType()) !void {
    const y = cell.grid_pos[1];
    assert(y < self.size.rows);
    switch (key) {
        .bg => comptime unreachable,
        .text, .underline, .strikethrough, .overline =>
            try self.fg_rows.lists[y + 1].append(alloc, cell),
    }
}

pub fn clear(self, y: CellCountInt) void {
    assert(y < self.size.rows);
    @memset(self.bg_cells[y * cols ..][0..cols], .{0,0,0,0});
    self.fg_rows.lists[y + 1].clearRetainingCapacity();
}
```

### The `Key` / comptime-`CellType` translation

Upstream `add` is a comptime-generic: `Key.CellType(self)` returns `CellBg` for
`bg` and `CellText` for the four foreground kinds, and `add`'s `cell` parameter
has that type. The `bg` arm is `comptime unreachable` — background cells are
written via `bgCell`, never `add`. Every real caller passes a foreground key
(`.text`/`.underline`/`.strikethrough`/`.overline`), and all four route
identically to `fg_rows[y + 1]`.

Rust has no comptime type-returning function, so:

- Port `Key { Bg, Text, Underline, Strikethrough, Overline }` for call-site
  parity (callers name the buffer kind, e.g. `add(Key::Text, cell)`); document
  the conceptual `CellType` mapping (`Bg → CellBg`, foreground →
  `CellTextVertex`) in a comment.
- `add(&mut self, key: Key, cell: CellTextVertex)`: `cell` is always a
  `CellTextVertex` because `add` is only ever called with a foreground key. The
  `Key::Bg` arm is `unreachable!()` (the runtime analog of upstream's
  `comptime unreachable`), and the four foreground arms push to
  `fg_rows[y + 1]`. This keeps the upstream call shape and the bg-exclusion
  guarantee.

(Alternative considered: drop the `key` parameter since the foreground arms are
identical. Rejected to preserve the upstream call sites and the explicit
bg-is-never-`add` invariant; the reviewer should confirm this choice.)

### Behavior

- `add(&mut self, key: Key, cell: CellTextVertex)`: `y = cell.grid_pos[1]`;
  `assert!(y < rows)`; `Key::Bg` → `unreachable!()`; foreground keys →
  `fg_rows[y as usize + 1].push(cell)`. Adding the same cell twice duplicates it
  (upstream behavior — callers `clear` the row first).
- `clear(&mut self, y: u16)`: `assert!(y < rows)`; zero the background span
  `bg_cells[y * columns .. y * columns + columns]`; `fg_rows[y as usize + 1]`
  `clear()` (retaining capacity).

### Faithfulness and scope notes

- `cell.grid_pos[1]` is `u16` (the row); the list index is `y as usize + 1`, the
  `+ 1` skipping the reserved cursor list (`fg_rows[0]`).
- The `y < rows` bound uses an **always-on `assert!`**, not `debug_assert!`. A
  `debug_assert!` would be unsafe in release: since `fg_rows` has `rows + 2`
  lists, an invalid `y == rows` indexes `fg_rows[rows + 1]` — the reserved
  trailing cursor list — which is in bounds, so no panic occurs and cursor
  storage is silently corrupted. `assert!` matches upstream's `assert` invariant
  in all builds.
- Upstream `append` (which may allocate) and `clearRetainingCapacity` map to
  `Vec::push` and `Vec::clear`.
- `Key`, `add`, and `clear` are `pub(crate)`.
- This is the final `cell.zig` slice — no `Contents` methods remain after it.
- No C ABI, header, or ABI inventory changes; no new dependencies.

## Changes

1. `roastty/src/renderer/cell.rs`:
   - Add `pub(crate) enum Key { Bg, Text, Underline, Strikethrough, Overline }`
     (`Debug, Clone, Copy, PartialEq, Eq`) with the `CellType` mapping
     documented.
   - Implement `Contents::add` and `Contents::clear` as above.
   - Update the module doc comment to note `cell.zig` is fully ported.

2. Tests in `renderer/cell.rs` (helpers `grid`, `dummy_vertex` exist; add a
   `vertex_at(y)` helper setting `grid_pos = [0, y]`):
   - `add_routes_each_fg_key_to_row`: for each of `Text`, `Underline`,
     `Strikethrough`, `Overline`, `add(key, vertex_at(1))` pushes to
     `fg_rows[2]` (`y + 1`).
   - `add_appends_multiple`: two adds to the same row leave two cells in that
     list (duplication is upstream behavior).
   - `add_different_rows_route_separately`: `add(Text, vertex_at(0))` and
     `add(Text, vertex_at(1))` land in `fg_rows[1]` and `fg_rows[2]`
     respectively.
   - `clear_clears_row`: set a bg cell and add a fg cell in row `1`, `clear(1)`
     → that row's bg span is zero and `fg_rows[2]` is empty.
   - `clear_only_affects_its_row`: add a fg cell to rows `0` and `1` and set bg
     cells in both, `clear(1)` → row `0`'s bg cell **and** `fg_rows[1]` are
     untouched, while row `1`'s bg span is zero and `fg_rows[2]` is empty
     (proves both background and foreground row isolation, not a
     clear-everything bug).
   - `add_bg_key_panics` (`#[should_panic]`): `add(Key::Bg, vertex_at(0))` hits
     the `unreachable!()` invariant.

3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo test -p roastty renderer::cell
cargo test -p roastty renderer
cargo test -p roastty
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/renderer/cell.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `add` routes a foreground cell to `fg_rows[y + 1]` (`y` from `grid_pos[1]`)
  for every foreground `Key`, and `Key::Bg` is `unreachable!()`;
- `clear` zeroes the row's background span and clears `fg_rows[y + 1]`;
- the routing, append-duplication, and clear tests pass;
- no `Contents` behavior is left unported (this finishes `cell.zig`);
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the `Key`/`CellType` translation needs a
different shape once a real renderer caller exists.

The experiment **fails** if `add` routes to the wrong list (e.g. forgets the
`+ 1` cursor offset), if `clear` clears the wrong span, or if any public C
API/ABI changes.

## Design Review

Codex reviewed this design before implementation.

Review artifacts:

- Prompt: `logs/codex-review/20260602-080503-338144-prompt.md`
- Result: `logs/codex-review/20260602-080503-338144-last-message.md`

Codex confirmed keeping `Key` for call-site parity is the right call, that
`Key::Bg => unreachable!()` is an acceptable runtime analog of the comptime
`unreachable`, and that the routing/clear formulas are otherwise correct.

Findings fixed in the design above before this commit:

1. **(High)** `debug_assert!(y < rows)` was unsafe in release: because `fg_rows`
   has `rows + 2` lists, `y == rows` would index the reserved trailing cursor
   list `fg_rows[rows + 1]` (in bounds — no panic) and silently corrupt cursor
   storage. Changed to an always-on `assert!` for both `add` and `clear`,
   matching upstream.
2. **(Medium)** the clear tests now also prove foreground-row isolation
   (`clear(1)` leaves `fg_rows[1]` populated while emptying `fg_rows[2]`), so a
   clear-everything bug cannot pass.
3. **(Low)** added an `add_bg_key_panics` `#[should_panic]` test for the
   `Key::Bg` invariant.

## Result

**Result:** Pass

Added `pub(crate) enum Key { Bg, Text, Underline, Strikethrough, Overline }` and
implemented `Contents::add` and `Contents::clear` in
`roastty/src/renderer/cell.rs`, and updated the module doc comment to note
`cell.zig` is fully ported. `add` reads `y = cell.grid_pos[1]`, asserts
`y < rows` (always-on `assert!`), routes `Key::Bg` to `unreachable!()`, and
pushes the foreground keys to `fg_rows[y + 1]`. `clear` asserts `y < rows`,
zeroes the background span `bg_cells[y * columns .. y * columns + columns]`, and
clears `fg_rows[y + 1]`.

Tests added (6): `add_routes_each_fg_key_to_row` (all four foreground keys),
`add_appends_multiple`, `add_different_rows_route_separately`,
`clear_clears_row`, `clear_only_affects_its_row` (background and foreground row
isolation), and `add_bg_key_panics` (`#[should_panic]`).

### Verification

```bash
cargo fmt -p roastty
cargo test -p roastty renderer::cell
cargo test -p roastty renderer
cargo test -p roastty
```

Observed:

- `renderer::cell`: 40 passed (34 prior + 6 new).
- Full `roastty`: 2276 unit tests passed (2270 prior + 6 new), plus the C ABI
  harness passed.
- `cargo fmt -p roastty -- --check`: clean.
- `cargo build -p roastty`: no warnings.
- No-`ghostty`-name gates passed for `roastty/src/renderer/cell.rs` and for
  `roastty/src/lib.rs`, `roastty/include/roastty.h`,
  `roastty/tests/abi_harness.c`.
- `git diff --check`: clean.

No C ABI, header, or ABI inventory changes. This completes the
`renderer/cell.zig` port; no `Contents` methods remain unported.

### Completion Review

Codex reviewed the completed implementation and found **no issues** ("nothing
should change before the result commit").

Review artifacts:

- Prompt: `logs/codex-review/20260602-080823-205085-prompt.md`
- Result: `logs/codex-review/20260602-080823-205085-last-message.md`

Codex confirmed `add`/`clear` match upstream (the always-on `assert!(y < rows)`
closes the release-mode cursor-list corruption risk, `Key::Bg` panics,
foreground keys route to `fg_rows[y + 1]`, `clear` zeroes the row span and
clears the row list), that the 6 tests cover the routing/isolation/panic cases,
and — explicitly — that this **completes the `renderer/cell.zig` port**:
predicates, `is_symbol`, `constraint_width`, `Key`, and all `Contents`
storage/cursor/add/clear behavior are now represented.

## Conclusion

Experiment 232 succeeds and **completes the port of `renderer/cell.zig`** across
Experiments 227–232: the codepoint-classification predicates, `is_symbol`,
`constraint_width`, and the full `Contents` cell-render-data builder (storage,
cursor lists, and row mutation), with 40 `renderer::cell` tests. Both Codex
gates passed (the High `assert!` finding and two others fixed at design time;
zero result findings).

The renderer foundation now landed across Experiments 223–232 — cursor style,
the sizing/coordinate model, preedit, and the cell builder — feeds the live
renderer. The natural next directions are the renderer's frame/draw path
(`renderer/generic.zig` / `Metal.zig`, which consume `Contents`) and the font
stack (`font/` + CoreText) that produces the glyphs `Contents` holds — both
large subsystems that will span many experiments.
