# Experiment 29: Port Page Set Graphemes

## Description

Port upstream `Page.setGraphemes` to Roastty.

Roastty currently supports appending grapheme codepoints to a cell and
moving/clearing existing grapheme entries. Upstream also has a distinct
`setGraphemes` primitive that installs an entire grapheme slice for a cell in
one operation. That primitive is used by clone paths and is an important Page
operation in its own right.

This experiment should add the internal set-graphemes operation only. It should
not rework clone paths, parser behavior, screen lifecycle, public ABI, or
automatic integrity-check wiring.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/page.zig` as the source of truth for:
     - `Page.setGraphemes`;
     - `Page.appendGrapheme`;
     - `Page.lookupGrapheme`;
     - nearby grapheme tests.
   - Preserve upstream preconditions:
     - the target cell must contain a non-zero codepoint;
     - the target cell must be a plain codepoint cell;
     - the target cell must not already have grapheme data.
   - Do not modify `vendor/ghostty/`.

2. Add an internal set-graphemes method.
   - Add an internal method shaped like:

     ```rust
     fn set_graphemes_at(
         &mut self,
         x: usize,
         y: usize,
         cps: &[u32],
     ) -> Result<(), GraphemeError>
     ```

   - If a lower-level offset helper better matches existing Page internals, add:

     ```rust
     fn set_graphemes_at_offset(
         &mut self,
         row_index: usize,
         cell_offset: Offset<Cell>,
         cps: &[u32],
     ) -> Result<(), GraphemeError>
     ```

     and keep the public-to-module coordinate wrapper small.

   - Validate all input codepoints are `<= 0x10ffff`.
   - Assert the cell has a non-zero codepoint and plain `ContentTag::Codepoint`.
   - Assert `cps` is not empty. Upstream accepts a slice, but Roastty's current
     bitmap allocator asserts non-empty allocation requests, so this method
     should make the precondition explicit before calling the allocator.
   - Allocate exactly one grapheme slice sized to `cps.len()`.
   - Copy all supplied codepoints into the allocated slice.
   - Insert a no-clobber map entry for the target cell.
   - Set the cell content tag to `ContentTag::CodepointGrapheme`.
   - Set the row grapheme flag.

3. Preserve rollback behavior on failure.
   - If grapheme allocation succeeds but map insertion fails, free the allocated
     grapheme slice and leave the cell and row flags unchanged.
   - If allocation fails, leave the cell and row flags unchanged.
   - Empty `cps` is rejected by assertion before allocation. Document that the
     Page model expects at least one additional codepoint for a grapheme entry.

4. Keep existing append behavior intact.
   - Do not rewrite `append_grapheme_at` to call `set_graphemes_at` unless that
     falls out naturally without changing observable behavior.
   - Existing append growth, chunk reuse, and rollback tests must keep passing.

5. Add focused tests.
   - Basic set:
     - start with a plain codepoint cell;
     - call `set_graphemes_at` with multiple codepoints;
     - verify lookup returns the full codepoint list;
     - verify cell and row grapheme flags are set;
     - verify `verify_integrity()` passes.
   - Single codepoint:
     - set one additional codepoint;
     - verify count, used bytes, lookup, and integrity.
   - Preconditions:
     - zero base codepoint panics;
     - empty `cps` panics;
     - target cell already containing grapheme data panics;
     - invalid codepoint panics.
   - Map out-of-memory rollback:
     - create a Page with enough grapheme allocator bytes for the requested
       slice;
     - shrink `page.size.rows` so one or more capacity rows are outside the
       visible integrity-checked area;
     - fill the grapheme map to capacity with test-only dummy entries keyed to
       hidden-row cell offsets and default slices, leaving allocator space free;
     - verify the method returns `GraphemeMapOutOfMemory`;
     - verify allocated grapheme bytes are released;
     - verify the cell tag and row flag are unchanged;
     - verify `verify_integrity()` passes.
   - Allocation out-of-memory rollback:
     - use grapheme byte capacity too small for the requested slice;
     - verify the method returns `GraphemeAllocOutOfMemory`;
     - verify the cell tag, row flag, map count, and used bytes are unchanged;
     - verify `verify_integrity()` passes.

6. Preserve scope.
   - Do not implement:
     - parser/screen lifecycle;
     - public ABI or app-facing APIs;
     - automatic integrity checks after mutation;
     - clone-path rewrites unless required to make the primitive correct.
   - Do not change existing grapheme layout constants or allocator semantics.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - set-graphemes API added;
     - rollback behavior;
     - tests added;
     - any deferred clone-path wiring;
     - verification command output summary.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `set_graphemes_at` installs an exact grapheme slice on a plain codepoint cell;
- cell and row grapheme flags are updated correctly;
- lookup returns exactly the provided additional codepoints;
- allocation and map-insertion failures roll back allocated memory and flags;
- invalid preconditions are rejected before corrupting Page state;
- existing append, lookup, move, clear, clone, reinit, and integrity tests do
  not regress;
- `cargo fmt`, targeted Page tests, and full `cargo test -p roastty` pass;
- Codex reviews the completed result and approves it or all real findings are
  fixed.

The experiment is partial if:

- the set operation works for normal cases, but one failure rollback case needs
  a focused follow-up;
- implementation shows existing allocator semantics require a separate
  zero-length grapheme decision.

The experiment fails if:

- the method leaks grapheme allocations on map insertion failure;
- the method leaves cell or row grapheme flags set after a failed operation;
- it permits clobbering existing grapheme data;
- it changes append behavior unexpectedly;
- the implementation expands into parser/screen lifecycle, public ABI, or
  unrelated behavior.
