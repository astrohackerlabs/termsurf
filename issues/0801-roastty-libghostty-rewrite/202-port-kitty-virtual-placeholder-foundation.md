# Experiment 202: Port Kitty Virtual Placeholder Foundation

## Description

Experiments 186-201 ported most of the Kitty graphics command, storage, media,
and image-loading surface. The remaining Kitty graphics gap is not just image
data; it is how virtual placements become renderable.

Ghostty supports Kitty's Unicode placeholder feature in
`vendor/ghostty/src/terminal/kitty/graphics_unicode.zig`. A virtual placement is
not rendered directly from the `Display` command's stored placement. Instead:

- the display command stores a virtual placement definition in image storage;
- the terminal prints placeholder cells (`U+10EEEE`) with style colors and
  optional row/column diacritics;
- the renderer scans the visible terminal cells, decodes placeholder runs, and
  derives concrete render placements from the combination of terminal cells,
  styles, graphemes, and image storage.

Roastty already has several prerequisites:

- row metadata contains a `kitty_virtual_placeholder` flag;
- page/cell storage supports grapheme metadata;
- `ImageStorage` supports virtual placement definitions;
- render-info ABI can report placement geometry once a placement exists.

But Roastty does not yet port the placeholder decoder/iterator or mark printed
placeholder rows. This experiment ports that decoder foundation. It does not add
a GPU renderer, does not validate decoded virtual placement records against
`ImageStorage`, does not port Ghostty's `renderPlacement` path, and does not try
to solve the final app rendering ABI in one pass.

## Changes

1. Add a Kitty virtual placeholder module.

   Create `roastty/src/terminal/kitty/graphics_unicode.rs` and expose it from
   `roastty/src/terminal/kitty/mod.rs`.

   Port the behavior from
   `vendor/ghostty/src/terminal/kitty/graphics_unicode.zig` that is independent
   of renderer upload:
   - placeholder codepoint constant `0x10EEEE`;
   - sorted diacritic table and `get_index()` lookup;
   - color-to-id conversion using the upper 24 bits of style color, matching
     Ghostty's RGB/palette behavior;
   - incomplete placement parsing from a cell's codepoint, style, underline
     color, and grapheme diacritics;
   - run continuation rules:
     - same image low bits;
     - same placement id;
     - omitted row/col/high bits may continue;
     - explicit col may continue only when it equals previous col + width;
     - explicit high bits may continue only when they match;
   - completed virtual placement values:
     - image id from low 24 bits plus optional high 8 bits;
     - placement id defaulting to 0;
     - row/col defaulting to 0;
     - width as run length;
     - height fixed to 1.

2. Mark placeholder rows during printing.

   Update the terminal/page write path so printing `U+10EEEE` sets the row's
   `kitty_virtual_placeholder` metadata flag. Because this is a row-level bit,
   every partial row mutation must keep it truthful:
   - overwriting a non-last placeholder keeps the row flagged;
   - overwriting the last placeholder clears the row flag;
   - clearing part of a row recomputes or otherwise preserves the correct flag;
   - inserting or deleting cells inside a row moves the placeholder cells and
     preserves the flag only while at least one placeholder remains;
   - moving, cloning, scrolling, or clearing rows preserves or clears the flag
     with the row content, not with the old row index.

   Prefer a small helper that recomputes a row's placeholder bit from its cells
   after operations that may remove or shift placeholders. Do not rely on a
   write-only "set once" bit that can become stale after mutation.

   Do not treat arbitrary graphemes as placeholders. Only the base cell
   codepoint `U+10EEEE` marks the row as containing Kitty virtual placeholders.

3. Add an internal visible-range iterator.

   Add an internal API that can scan a terminal's visible range and return
   decoded virtual placement records. The API should be internal Rust for now,
   not public C ABI:
   - iterate visible rows efficiently by first checking the row-level
     `kitty_virtual_placeholder` flag;
   - skip rows without the flag;
   - inspect cells for `U+10EEEE`;
   - read any grapheme diacritics already associated with the placeholder cell;
   - return decoded placement structs in the same order Ghostty would discover
     them.

   The iterator must return records in top-to-bottom, left-to-right visible-cell
   order and must not include placeholders outside the visible viewport, even
   when scrollback exists. This internal API is the foundation for a later
   experiment that decides the renderer/app ABI shape for virtual placements.
   That ABI decision should not be hidden inside this experiment.

4. Preserve existing storage/render-info ABI behavior.

   Do not change the public `roastty_kitty_graphics_*` ABI in this experiment.
   The current placement iterator remains storage-backed. A later experiment
   will decide how the app asks for a terminal-scoped render placement list that
   includes virtual placeholders.

5. Add tests.

   Port or adapt the upstream `graphics_unicode.zig` behavior tests:
   - diacritic table is sorted;
   - spot-check diacritic indexes;
   - no placeholder cells yields no virtual placements;
   - single placeholder with row/col diacritics decodes row 0 / col 0;
   - continuation break when explicit col jumps;
   - continuation with explicit sequential diacritics combines into one run;
   - continuation with omitted col combines into one run;
   - continuation with no diacritics combines into one run;
   - run ends when non-placeholder text appears;
   - image id low 24 bits come from foreground color;
   - image id high 8 bits come from the third diacritic;
   - placement id comes from underline color and defaults to 0;
   - invalid diacritics are ignored rather than fatal.

   Add terminal-stream tests proving:
   - printing `U+10EEEE` marks the row flag;
   - clearing or overwriting the placeholder clears the row flag when no
     placeholders remain;
   - scrolling and row movement preserve the flag with the placeholder row;
   - decoded visible placements can be read from a real `Terminal`;
   - decoded visible placements are returned top-to-bottom and left-to-right
     across multiple rows;
   - placeholders in scrollback or otherwise outside the visible viewport are
     excluded;
   - removing one of multiple placeholders keeps the row flag set;
   - removing the last placeholder clears the row flag;
   - partial clear, insert-cell, delete-cell, row move, and scroll operations
     keep the row flag truthful.

6. Preserve formatting and review rules.

   Run:

   ```bash
   cargo fmt -- roastty/src/terminal/kitty/graphics_unicode.rs roastty/src/terminal/kitty/mod.rs roastty/src/terminal/page.rs roastty/src/terminal/page_list.rs roastty/src/terminal/terminal.rs
   prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/202-port-kitty-virtual-placeholder-foundation.md
   ```

## Verification

Run:

```bash
cargo fmt -- roastty/src/terminal/kitty/graphics_unicode.rs roastty/src/terminal/kitty/mod.rs roastty/src/terminal/page.rs roastty/src/terminal/page_list.rs roastty/src/terminal/terminal.rs
cargo test -p roastty kitty_graphics_unicode
cargo test -p roastty terminal_stream_kitty_virtual_placeholder
cargo test -p roastty --test abi_harness
cargo test -p roastty kitty_graphics_render_info_c_abi
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

The experiment passes when:

- Roastty can decode Kitty Unicode placeholder runs from terminal cells;
- decoded values match upstream behavior for row, column, width, image id, and
  placement id;
- row-level placeholder metadata is set, moved, and cleared correctly;
- existing storage-backed placement/render-info ABI tests still pass unchanged;
- no public ABI is changed prematurely, and `roastty/include/roastty.h` remains
  unchanged;
- Codex approves the experiment design before implementation and approves the
  result before the experiment is recorded.

## Non-Negotiable Invariants

- Do not add GPU rendering, Metal, texture upload, or app drawing in this
  experiment.
- Do not change public C ABI for placement iteration in this experiment.
- Do not pretend storage-only placement iteration is enough for true virtual
  placeholder rendering; this experiment must explicitly leave the terminal-
  scoped render ABI decision for later.
- Do not weaken existing managed-memory preservation in page, page-list, row,
  scroll, insert, delete, or clear paths.
- Do not weaken Kitty graphics storage behavior from Experiments 188-201.
- Do not expose any `ghostty_*` ABI names.
- Do not skip Codex design review or Codex result review.

## Result

**Result:** Pass

Roastty now has an internal Kitty Unicode virtual placeholder foundation:

- `graphics_unicode` contains the terminal-internal placeholder constant,
  complete 297-entry Kitty diacritic table, index lookup, color-to-id decoding,
  incomplete placement parsing, run continuation, and completed virtual
  placement records.
- Printing `U+10EEEE` marks the row's `kitty_virtual_placeholder` bit through
  both the normal styled path and the fast basic-cell path.
- Page row metadata recomputes the placeholder flag after overwrites, clears,
  inserts, deletes, swaps, partial clones, and other row/cell mutation paths.
- Page integrity now rejects both missing and stale Kitty placeholder row flags.
- `PageList` can scan the visible viewport and return decoded terminal-internal
  virtual placement records in top-to-bottom, left-to-right order.
- The public C ABI was not changed; `roastty/include/roastty.h` remained
  unchanged.

Verification run:

```bash
cargo fmt -- roastty/src/terminal/kitty/graphics_unicode.rs roastty/src/terminal/kitty/mod.rs roastty/src/terminal/page.rs roastty/src/terminal/page_list.rs roastty/src/terminal/screen.rs roastty/src/terminal/terminal.rs
cargo test -p roastty kitty_graphics_unicode
cargo test -p roastty terminal_stream_kitty_virtual_placeholder
cargo test -p roastty --test abi_harness
cargo test -p roastty kitty_graphics_render_info_c_abi
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

All commands passed.

Codex reviewed the implementation twice. The first review found a real blocker:
the initial diacritic table was truncated. The implementation was corrected to
match the full upstream 297-entry table, with tests for the full length and
representative spot checks. The second Codex review found no remaining blocking
correctness, regression, missing-test, or scope issues and approved recording
the experiment as Pass.

## Conclusion

Experiment 202 establishes the terminal-side decoder and row metadata foundation
needed for true Kitty Unicode virtual placement rendering. The next experiment
should use this internal visible-placement list to design the terminal-scoped
render/app ABI that combines decoded virtual placements with image storage and
existing render-info geometry.
