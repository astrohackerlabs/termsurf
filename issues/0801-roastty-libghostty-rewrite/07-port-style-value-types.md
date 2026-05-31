# Experiment 7: Port Terminal Style Value Types

## Description

Port the terminal color, SGR underline, and style value types that `Page`,
`Cell`, and later `StyleSet` need.

Experiment 5 identified this as the next dependency after `BitmapAllocator`.
Experiment 6 completed the allocator. Do not run another broad Zig-to-Rust code
pattern pass here: Experiment 2 already defines the general translation policy,
including when `unsafe` Rust is acceptable. This experiment should apply that
policy to one concrete subsystem slice.

The goal is to create real, durable Roastty value types, not temporary stubs:

- `terminal::color` should contain the `RGB`/palette primitives needed by style
  formatting and future cell background handling.
- `terminal::sgr` should contain the underline value needed by style flags.
- `terminal::style` should contain `Id`, `DEFAULT_ID`, `Style`, style colors,
  flags, foreground/background/underline color helpers, and VT/HTML formatting
  behavior.

This experiment should not port `StyleSet`, `RefCountedSet`, `Page`, `Row`,
`Cell`, the full SGR parser, X11 color parsing, LAB palette generation, or
dynamic palette/config behavior. Those are later slices. If a helper is needed
only to make this slice compile, keep it faithful to the upstream shape and
document why it belongs here.

## Changes

1. Inspect upstream source and tests.
   - Use `vendor/ghostty/` as the source of truth.
   - Inspect at least:
     - `vendor/ghostty/src/terminal/color.zig`
     - `vendor/ghostty/src/terminal/sgr.zig`
     - `vendor/ghostty/src/terminal/style.zig`
     - `vendor/ghostty/src/terminal/page.zig` for `Cell` interactions with
       `Style::bgCell` / background helpers
   - Do not modify `vendor/ghostty/`.

2. Add terminal value modules.
   - Add `roastty/src/terminal/color.rs`.
   - Add `roastty/src/terminal/sgr.rs`.
   - Add `roastty/src/terminal/style.rs`.
   - Wire them from `roastty/src/terminal/mod.rs`.

3. Port the bounded color primitives.
   - Port `color.RGB` as `Rgb`.
   - Preserve the C-compatible representation decision:
     - use `#[repr(C)]` for the C-facing RGB value if a separate C type is
       needed;
     - use ordinary Rust layout only if the value never crosses an ABI boundary
       in this slice.
   - Add `from_c` / `cval` equivalents if the C-compatible value is included.
   - Port the default 256-color palette values needed by style tests.
   - It is acceptable to generate the default 256-color xterm palette directly
     from the same algorithm used by upstream, as long as tests verify key
     indexes.
   - Do not port LAB interpolation, X11 named-color parsing, dynamic palette
     enums, or color parser behavior in this experiment.

4. Port the bounded SGR underline value.
   - Add an `Underline` enum matching upstream `sgr.Attribute.Underline` values:
     `none`, `single`, `double`, `curly`, `dotted`, `dashed`.
   - Do not port the full SGR parser or `Attribute` union in this experiment.
   - The file should make clear that this is the style-value subset of SGR, not
     the complete parser port.

5. Port `style` value behavior.
   - Port `Id` as the existing `size::StyleCountInt`.
   - Port `DEFAULT_ID` from upstream `default_id`.
   - Port `Style`, `StyleColor`, style flags, `BoldColor`, and foreground
     options.
   - Preserve upstream equality/default semantics.
   - Preserve foreground selection behavior:
     - default foreground;
     - palette foreground;
     - RGB foreground;
     - bold-as-bright behavior for palette colors;
     - explicit bold color override behavior.
   - Preserve style-color-only background and underline color lookup behavior.
     Full cell-aware background behavior is deferred because upstream
     `Style::bg` depends on `page.Cell`.
   - Preserve VT formatting behavior and sequence order from upstream tests.
   - Preserve HTML formatting behavior from upstream tests where it does not
     require unported page/cell structures.
   - Defer `Style::bgCell` or implement it only if a faithful small cell
     dependency can be introduced without pulling in `Page`/`Cell`. Do not add a
     fake cell type just for this method.
   - Defer the cell-taking form of `Style::bg` until `Cell` is ported. Do not
     create a temporary cell surrogate to make the upstream signature compile.
   - Do not port `StyleSet` in this experiment.

6. Translate tests.
   - Port the upstream `Style VT formatting ...` tests that depend only on
     style/color values.
   - Port the upstream `Style HTML formatting ...` tests that depend only on
     style/color values.
   - Add direct tests for:
     - `Rgb` equality and C conversion if implemented;
     - selected default palette indexes used by upstream style formatting tests;
     - `Style::default` / equality;
     - foreground bold behavior.
   - Explicitly document deferred upstream tests:
     - `Set basic usage`;
     - `Set capacities`;
     - `color.zig` parser, dynamic color, LAB, generated palette, and X11
       named-color tests;
     - `sgr.zig` full parser and full `Attribute` union tests;
     - any tests blocked by `Page`, `Cell`, `StyleSet`, parser, X11 colors,
       dynamic colors, or LAB palette generation.

7. Preserve the unsafe policy.
   - Prefer safe Rust for this slice.
   - If any `unsafe` is used for ABI/layout conversion, keep it localized,
     include a safety comment, and add a layout or conversion test proving the
     invariant.
   - Do not introduce unsafe code merely to mimic Zig packed structs where the
     layout is not externally observed in this slice.

8. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::color
     cargo test -p roastty terminal::sgr
     cargo test -p roastty terminal::style
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

9. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - modules added;
     - upstream tests ported;
     - upstream tests deferred and why;
     - any unsafe code used and why;
     - verification command output summary.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- the bounded color, SGR underline, and style value modules exist and are wired
  into `terminal::mod`;
- VT formatting tests ported from upstream pass;
- HTML formatting tests ported from upstream pass, except for cases explicitly
  blocked by deferred dependencies;
- foreground, style-color-only background, and underline color helpers match
  upstream behavior for the included value types;
- deferred tests are listed with concrete blockers;
- `cargo fmt` has been run and accepted;
- all targeted tests and full `cargo test -p roastty` pass;
- Codex reviews the completed result and approves it or all real findings are
  fixed.

The experiment is partial if:

- most value behavior is ported, but one subset such as HTML formatting or
  default palette handling proves larger than expected and is cleanly deferred
  with the next slice identified.

The experiment fails if:

- it introduces placeholder style/color types that will need to be thrown away;
- it starts porting `StyleSet`, `Page`, `Row`, `Cell`, or the full SGR parser;
- it leaves style formatting behavior untested;
- it uses `unsafe` without a narrow invariant and a test.

## Codex Review

This experiment design must be reviewed by Codex before implementation. Any real
design issues must be fixed before committing the plan or implementing the
slice.

## Result

**Result:** Pass

Experiment 7 ported the bounded terminal style value layer needed before the
next page-storage slices. The implementation added:

- `roastty/src/terminal/color.rs`
- `roastty/src/terminal/sgr.rs`
- `roastty/src/terminal/style.rs`
- module wiring in `roastty/src/terminal/mod.rs`

The slice is safe Rust. No `unsafe` code was needed because no ABI pointer
boundary or packed page-memory layout is exposed by these value types yet.

### Upstream Tests Ported

The implementation ports the relevant value-only behavior from upstream
`style.zig`:

- VT reset formatting
- individual style flag VT formatting
- underline style VT formatting for single, double, curly, dotted, and dashed
- palette and RGB VT formatting for foreground, background, and underline color
- palette-to-RGB VT formatting when a palette is provided
- combined flags/colors VT formatting
- HTML formatting for bold, RGB foreground, palette background, combined
  colors/flags, text decoration line/style, and palette-to-RGB output

Additional direct tests cover:

- `Rgb` C-value conversion
- C RGB layout
- selected default palette indexes used by the style formatting tests
- default xterm cube/grayscale palette indexes
- `Style` default/equality behavior
- foreground bold behavior
- style-color-only background and underline lookup behavior

### Deferred Upstream Tests

The following upstream tests remain intentionally deferred:

| Deferred area                                     | Reason                                                                 |
| ------------------------------------------------- | ---------------------------------------------------------------------- |
| `style.zig` `Set basic usage` / `Set capacities`  | Requires `StyleSet` and `RefCountedSet`, which are a later page slice. |
| `Style::bg` and `Style::bgCell` cell-aware checks | Require `page.Cell`; this experiment explicitly avoided fake cells.    |
| `color.zig` RGB parser tests                      | Parser behavior is not needed by this value slice.                     |
| `color.zig` dynamic palette tests                 | Dynamic palette/config behavior is a later terminal/config slice.      |
| `color.zig` LAB and generated palette tests       | LAB interpolation was explicitly out of scope for this slice.          |
| `color.zig` X11 named-color tests                 | X11 named-color parsing is not part of the current value dependency.   |
| `sgr.zig` parser and full `Attribute` union tests | Only `Underline` is needed by `Style` flags in this slice.             |

### Verification

Ran and passed:

```bash
cargo fmt
cargo test -p roastty terminal::color
cargo test -p roastty terminal::sgr
cargo test -p roastty terminal::style
cargo test -p roastty
```

The full `cargo test -p roastty` run passed 63 Rust unit tests, the C ABI
harness, and doc tests.

## Conclusion

Experiment 7 succeeds. Roastty now has durable terminal color, SGR underline,
and style value types with behavior tests covering the formatting and lookup
surface needed by future `Cell`, `Page`, and `StyleSet` work.

The next experiment should port the packed `Row` / `Cell` value model from
`page.zig`, using these style/color primitives and explicit tests for zero-cell
behavior, helper methods, and layout-sensitive decisions.
