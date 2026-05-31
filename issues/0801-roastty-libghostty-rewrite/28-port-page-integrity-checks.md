# Experiment 28: Port Page Integrity Checks

## Description

Port upstream `Page.verifyIntegrity` to Roastty as an internal Page consistency
checker.

Experiment 27 added `Page::reinit`, matching the upstream Page lifecycle order.
The next upstream Page primitive is `verifyIntegrity`, which validates that row
flags, cell flags, grapheme maps, style refcounts, hyperlink maps/refcounts, and
wide-character spacer invariants agree with each other.

This experiment should add the integrity checker only. It should not wire the
checker into every Page mutation, add runtime-safety feature flags, add parser
or screen lifecycle integration, or expose a public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/page.zig` as the source of truth for:
     - `Page.IntegrityError`;
     - `Page.verifyIntegrity`;
     - the upstream integrity tests around `Page verifyIntegrity ...`;
     - the disabled zombie-style note.
   - Preserve upstream semantics where they are meaningful in Rust.
   - Do not modify `vendor/ghostty/`.

2. Add an internal integrity error enum.
   - Add an internal Rust enum equivalent to upstream's `IntegrityError`.
   - Include the upstream variants currently relevant to Roastty's Page model:
     - zero row count;
     - zero column count;
     - unmarked grapheme row;
     - missing grapheme data;
     - invalid grapheme count;
     - unmarked grapheme cell;
     - missing style;
     - unmarked style row;
     - mismatched style refcount;
     - invalid style count;
     - missing hyperlink data;
     - mismatched hyperlink refcount;
     - unmarked hyperlink cell;
     - unmarked hyperlink row;
     - invalid spacer tail location;
     - invalid spacer head location;
     - unwrapped spacer head.
   - `InvalidStyleCount` should be kept for upstream enum parity, but should be
     documented as not currently produced because upstream intentionally does
     not check exact style counts.
   - If any other variant cannot be produced yet because a corresponding Page
     subsystem is not ported, keep the variant if upstream has it and document
     the reason in tests or comments.

3. Add `Page::verify_integrity`.
   - Add an internal method:

     ```rust
     fn verify_integrity(&self) -> Result<(), IntegrityError>
     ```

   - The method should validate:
     - `self.size.rows != 0`;
     - `self.size.cols != 0`;
     - every checked row is inside this Page's row storage;
     - every row's cell offset points to a valid cell range;
     - any grapheme-marked cell has map data;
     - cells with grapheme map data are marked as graphemes;
     - any row containing grapheme cells has the row grapheme flag set;
     - grapheme cells seen do not exceed the grapheme map count, matching
       upstream's `graphemes_seen > graphemeCount()` behavior;
     - any styled cell references an existing style, returning `MissingStyle`
       instead of panicking on invalid style IDs;
     - any row containing styled cells has the row styled flag set;
     - each style refcount is at least the number of visible cell references
       seen by the checker, matching upstream's deliberately non-exact refcount
       rule;
     - any hyperlink-marked cell has map data;
     - any row containing hyperlink cells has the row hyperlink flag set;
     - any cell with hyperlink map data is marked as a hyperlink;
     - each referenced hyperlink ID exists in the set;
     - each hyperlink refcount is at least the number of visible cell references
       seen by the checker;
     - spacer tails are not at column 0 and follow a wide cell;
     - spacer heads are at the last column and require row wrap.
   - Do not enable the disabled upstream zombie-style check in this experiment.
     Record that it remains disabled for the same reason as upstream: fast paths
     can leave extra live style refs that are not visible in the checked cells.

4. Avoid mutation-path wiring.
   - Do not add `verify_integrity` calls to clone, move, clear, swap, or reinit
     methods in this experiment.
   - Do not add a `pause_integrity_checks` field or runtime-safety feature gate
     yet.
   - Those are separate wiring concerns after the checker itself exists and has
     focused tests.

5. Add focused tests.
   - Port or create equivalents for upstream's current integrity tests:
     - graphemes good;
     - grapheme row not marked;
     - styles good;
     - style refcount mismatch;
     - zero rows;
     - zero cols.
   - Add Roastty-specific equivalents that cover currently ported Page state:
     - missing grapheme data when a cell is marked but the map entry is gone;
     - unmarked grapheme cell when map data exists but the cell flag is clear;
     - missing style when a cell references a style ID that is not present in
       the style set;
     - unmarked style row;
     - missing hyperlink data when a cell is marked but the map entry is gone;
     - unmarked hyperlink cell when map data exists but the cell flag is clear;
     - unmarked hyperlink row;
     - hyperlink refcount mismatch;
     - invalid spacer tail at column 0;
     - invalid spacer tail after a non-wide cell;
     - invalid spacer head away from the last column;
     - unwrapped spacer head at the last column;
     - a fresh Page and a reinitialized Page pass integrity.
   - Tests may intentionally corrupt Page internals because this checker exists
     to detect invalid internal state.

6. Preserve scope.
   - Do not implement:
     - `assert_integrity`;
     - pause/resume integrity checks;
     - automatic integrity checks after mutations;
     - parser/screen lifecycle;
     - public ABI or app-facing APIs.
   - Do not change existing Page mutation behavior except for helper visibility
     that tests need.

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
     - integrity API added;
     - which upstream checks are implemented;
     - which wiring pieces are deferred;
     - tests added;
     - verification command output summary.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `Page::verify_integrity` returns `Ok(())` for valid fresh, mutated, and
  reinitialized Pages;
- every implemented error variant has at least one focused test or is explicitly
  justified as not yet producible;
- grapheme, style, hyperlink, and spacer invariants match upstream semantics;
- style and hyperlink refcount checks use upstream's "at least visible refs"
  rule, not exact equality;
- no mutation path is wired to call `verify_integrity` automatically;
- `cargo fmt`, targeted Page tests, and full `cargo test -p roastty` pass;
- Codex reviews the completed result and approves it or all real findings are
  fixed.

The experiment is partial if:

- core grapheme/style/hyperlink checks work, but one smaller invariant needs a
  follow-up;
- the checker works in tests but needs a small follow-up to make corruption test
  setup cleaner.

The experiment fails if:

- the checker misses stale map entries that upstream would catch;
- the checker requires exact style or hyperlink refcounts instead of upstream's
  lower-bound rule;
- the implementation wires integrity checks into mutation paths and changes
  behavior outside the checker;
- the implementation expands into parser/screen lifecycle, public ABI, or
  unrelated behavior.

## Result

**Result:** Pass

Implemented internal `Page::verify_integrity` in `roastty/src/terminal/page.rs`,
plus the supporting `IntegrityError` enum.

The checker validates the upstream Page invariants that Roastty can now model:
zero size rejection, grapheme cell/map/row consistency, style existence and row
flag consistency, hyperlink cell/map/set/row consistency, lower-bound style and
hyperlink refcount checks, and wide-character spacer placement rules.

Added a non-panicking `contains_id` helper to `RefCountedSet` and exposed it
through `style::Set` so the integrity checker can return `MissingStyle` and
`MissingHyperlinkData` instead of relying on assertion failures for corrupt
internal IDs.

The upstream `InvalidStyleCount` variant is retained for enum parity but is not
currently produced, matching upstream's deliberate choice not to check exact
style counts. The disabled zombie-style check also remains disabled.

Added focused tests covering:

- fresh and reinitialized Pages passing integrity;
- zero row and zero column errors;
- valid grapheme rows and unmarked/missing grapheme corruption;
- valid styled cells, missing styles, unmarked style rows, and style refcount
  mismatches;
- extra live style refs being accepted by the lower-bound refcount rule;
- valid hyperlink cells, missing hyperlink data, unmarked hyperlink cells,
  unmarked hyperlink rows, and hyperlink refcount mismatches;
- extra live hyperlink refs being accepted by the lower-bound refcount rule;
- invalid spacer tails and spacer heads.

`InvalidGraphemeCount` is retained and implemented for upstream parity, but it
is not directly covered by a corruption test. In the current Roastty data model,
every visible grapheme lookup is backed by a map entry, and `grapheme_count()`
is the same map's entry count, so producing "seen graphemes > grapheme count"
without corrupting private map header metadata would require an artificial
memory-level corruption path outside this experiment's scope.

Verification passed:

```bash
cargo fmt
cargo test -p roastty terminal::page
cargo test -p roastty
```

The targeted Page suite reported 153 passing tests. The full `roastty` suite
reported 262 unit tests, the ABI harness, and doc tests passing.

## Conclusion

Roastty now has an internal Page integrity checker equivalent to the currently
ported upstream Page state. This experiment deliberately did not wire the
checker into mutation paths or add pause/assert runtime-safety plumbing. Those
integration pieces should be considered only after the next Page or screen
lifecycle slices require them.
