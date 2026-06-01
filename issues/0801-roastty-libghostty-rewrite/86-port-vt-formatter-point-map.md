# Experiment 86: Port VT Formatter Point Map

## Description

Port upstream `terminal/formatter.zig::PageFormatter.point_map` behavior for
Roastty's private VT PageList formatter path.

Experiments 84 and 85 completed byte-indexed point and pin maps for plain
output. Roastty's `page_string_with_point_map(...)` already accepts any
`PageOutputFormat`, but the current implementation only threads point maps
through `PlainPageFormat`; VT and HTML formatting ignore the map. Upstream maps
every byte emitted by VT output, including SGR sequences and `\r\n` newline
bytes, back to the source page coordinate that caused that byte.

This experiment ports VT point maps only. HTML point maps remain a separate
slice because HTML has wrapper tags, escaping, numeric entities, and hyperlink
tags that deserve focused tests.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/formatter.zig` for:
     - `PageFormatter.point_map`;
     - VT branches in `formatWithState`;
     - `formatStyleOpen`;
     - `formatStyleClose`;
     - `writeCell`;
     - `writeCodepointWithReplacement`;
     - VT point-map tests.
   - Use existing Roastty code:
     - `roastty/src/terminal/page_list.rs`;
     - `StyledPageFormat`;
     - `PageList::page_string_with_point_map(...)`;
     - `push_blank_cells_plain(...)` as the source of the upstream reverse-walk
       blank-cell mapping now fixed for screen-domain rows;
     - style VT formatting in `roastty/src/terminal/style.rs`.
   - Do not modify `vendor/ghostty/`.

2. Thread an optional point map through VT styled formatting.
   - Update `PageList::page_string_with_point_map(...)` so it is no longer
     plain-only. The current helper has a plain-only debug assertion; remove or
     replace it with an explicit supported-format policy:
     - `PageOutputFormat::Plain` keeps the Experiment 84 behavior;
     - `PageOutputFormat::Vt` is supported by this experiment;
     - `PageOutputFormat::Html` remains deferred and must be documented as
       producing no map entries or routed through a clearly named private
       deferred branch until a later HTML-map experiment.
   - Extend `StyledPageFormat` with enough context to map VT bytes:
     - the PageList screen-domain row base for the current page chunk;
     - an optional `&mut Vec<point::Coordinate>`.
   - In `PageList::page_string_between(...)`, when `PageOutputFormat::Vt` is
     requested and a point map is present, pass the chunk's screen-domain row
     base and point-map sink into `StyledPageFormat`.
   - Keep existing VT output text exactly unchanged.
   - Keep HTML point maps out of scope. If `PageOutputFormat::Html` receives a
     point-map sink, leave it unmapped for now or route through an explicit
     private helper that documents HTML as deferred. Do not partially implement
     HTML mapping in this experiment.

3. Map every emitted VT byte to a source coordinate.
   - Plain text bytes map to the source cell coordinate, one entry per UTF-8
     byte.
   - Grapheme bytes map to the base cell coordinate.
   - Codepoint-map replacement bytes map to the original source cell coordinate.
   - Generated blank-cell spaces map to the same reverse-walk source cells as
     plain point maps.
   - Background-only styled cells that emit a visible space map that byte to the
     background cell coordinate.
   - VT style-open bytes map to the cell that caused the style transition.
   - VT style-close bytes map to the previous emitted coordinate, matching
     upstream's `formatStyleClose` behavior.
   - VT pending blank-row newline bytes use `\r\n`, and both bytes in each
     newline sequence map to the same source coordinate that upstream would
     assign:
     - the first pending newline inherits the previous emitted coordinate;
     - later pending blank-row newlines map to the start of each blank row.
   - Multi-page chunks must use PageList screen-domain coordinates, not
     page-local rows, so rows crossing a page boundary continue to map
     correctly.

4. Preserve existing behavior and scope.
   - Existing `page_string(...)` VT output must not change.
   - Existing plain point maps and plain pin maps must not change.
   - Do not add VT pin maps yet. Pin maps depend on this point map and should be
     a later experiment.
   - Do not add HTML point maps or pin maps.
   - Do not add `ScreenFormatter`, `TerminalFormatter`, `Screen`, `Terminal`,
     parser state, cursor state, terminal extras, hyperlinks beyond existing
     HTML rendering, writer abstraction, public ABI, app, renderer, clipboard,
     PTY, or UI behavior.

5. Add upstream-equivalent tests.
   - Add VT point-map tests for:
     - unstyled single-line output;
     - bold/style-open bytes mapping to the first styled cell;
     - multiple style transitions preserving one map entry per output byte;
     - foreground/background palette style bytes mapping to the styled cell;
     - style-close bytes mapping to the previous emitted coordinate;
     - final formatter-end style-close bytes mapping to the last styled cell
       when non-default style remains active through the final text cell;
     - multiline output where VT emits `\r\n`;
     - pending blank rows, including style reset before blank-row newlines;
     - grapheme bytes mapping to the base cell;
     - wide character output from a spacer-tail selection start;
     - generated blank-cell spaces using reverse-walk order;
     - background-only cells that emit a styled space;
     - codepoint-map single-codepoint and string replacements;
     - multi-page output proving screen-domain coordinates cross PageList page
       chunks correctly;
     - invalid and garbage selection endpoints returning empty text/map.
   - Keep no-map regression assertions for existing VT `page_string`, plain
     point-map, and plain pin-map behavior.

6. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty vt_point_map
     cargo test -p roastty point_map
     cargo test -p roastty pin_map
     cargo test -p roastty page_string
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

7. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Record the design-review outcome in this experiment file before
     implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real findings before proceeding.

8. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - helper/type names and location;
     - whether VT point maps are byte-indexed;
     - how style-open and style-close bytes are mapped;
     - how VT `\r\n` pending newlines are mapped;
     - how multi-page screen-domain coordinates are preserved;
     - which HTML point-map, VT/HTML pin-map, `ScreenFormatter`, and
       `TerminalFormatter` behaviors remain deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- Roastty has a private VT point-map path that returns one screen-domain
  coordinate per output byte;
- the VT output string is unchanged from existing `page_string(...)` behavior;
- unstyled text, styled text, style opens, style closes, generated blanks,
  background-only cells, graphemes, wide cells, codepoint replacements, VT
  `\r\n` newlines, pending blank rows, and multi-page chunks map to the correct
  source coordinates;
- existing plain point maps and plain pin maps remain unchanged;
- invalid or unconvertible selections return empty output/map rather than a
  short or partially invalid map;
- no HTML point maps, VT/HTML pin maps, `ScreenFormatter`, `TerminalFormatter`,
  `Screen`, `Terminal`, parser state, cursor state, terminal extras, writer
  abstraction, public ABI, app, renderer, clipboard, PTY, or UI behavior is
  added;
- `cargo fmt`, targeted VT point-map tests, point-map and pin-map regression
  tests, page-string tests, PageList tests, and full `cargo test -p roastty`
  pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- single-page VT maps work, but multi-page VT mapping exposes a missing shared
  helper needed to keep styled and plain screen-domain coordinates identical.

The experiment fails if:

- VT point maps cannot be implemented without adding HTML maps, pin maps,
  higher-level formatter wrappers, public API, app, renderer, PTY, clipboard, or
  UI behavior;
- VT output changes while adding point maps;
- the map records one entry per character instead of one entry per output byte;
- style or newline bytes map to the wrong source coordinates;
- multi-page output loses screen-domain row identity;
- existing plain formatter maps regress;
- tests or formatting fail.

## Design Review

Codex reviewed the initial design and agreed that VT point maps are the right
next formatter slice after plain point maps and plain pin maps. It found one
blocker: `PageList::page_string_with_point_map(...)` still had a plain-only
debug assertion, so any VT point-map test would panic unless the experiment
explicitly removed or replaced that assertion and defined deferred HTML
behavior. It also requested explicit coverage for the final formatter-end
style-close reset bytes mapping to the last styled cell.

The design now requires both fixes. Follow-up Codex review found no blockers and
approved the experiment for implementation.
