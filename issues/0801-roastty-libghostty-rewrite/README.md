+++
status = "open"
opened = "2026-05-31"
+++

# Issue 801: Reimplement libghostty as libroastty

## Goal

Reimplement the remaining `libghostty` library surface and behavior as
`libroastty`, a Rust library with Roastty naming throughout. The end state is a
macOS-only terminal implementation with Ghostty feature parity before TermSurf
browser-overlay features are added.

## Background

Issue 800 created the Roastty foundation:

- a top-level Cargo workspace containing `webtui`, `roamium`, and `roastty`
- a new `roastty/` Rust crate
- the first `roastty_*` C ABI skeleton
- `roastty/include/roastty.h`
- a small configuration subset exposed through `roastty_config_get`
- an ABI inventory comparing upstream Ghostty's C surface to Roastty's current
  skeleton

That issue also corrected an important naming mistake: Roastty is a faithful
Rust adaptation of Ghostty, but it is not allowed to expose `ghostty_*` app or
ABI names. Public functions, app-facing symbols, crate names, comments, and
documentation inside the implementation should use `roastty` / `Roastty`. The
word `ghostty` should appear only when citing the upstream project, vendored
source paths, research notes, or attribution.

This issue continues from that foundation and owns the rest of the library
rewrite.

## Scope

This issue is intentionally broad: it tracks the remaining work needed to turn
`roastty/` from an ABI skeleton into a real terminal library. The work must be
implemented incrementally through reviewed experiments, but the target is the
entire remaining `libghostty` behavior needed by the macOS app.

Roastty is macOS-only. The rewrite should not preserve Ghostty's Linux, FreeBSD,
Windows, GTK, Wayland, X11, OpenGL, or other non-macOS gates as live
implementation paths. When upstream Ghostty has code shaped like "if Linux do
this, else if macOS do that," the Roastty port should keep the macOS behavior
and omit the other branches unless an experiment documents a specific reason to
keep a stub for testability or source comparison.

The rewrite includes, at minimum:

- complete configuration loading, parsing, validation, finalization,
  diagnostics, key lookup, keybindings, and runtime config updates
- app/runtime lifecycle behavior: init, tick, focus, quit confirmation, color
  scheme changes, global keybinds, mailbox/event delivery, and surface
  coordination
- surface lifecycle behavior: creation, inherited config, draw/refresh hooks,
  sizing, content scale, display ID, focus, occlusion, quicklook, inspector
  integration, split coordination, selection reads, and text reads
- terminal core behavior: parser, screen/grid, scrollback, cursor state, style
  state, CSI/OSC/DCS handling, SGR, mouse modes, selection, reflow, input
  encoding, Kitty keyboard behavior, and Kitty graphics behavior
- PTY and IO behavior: macOS shell/process spawn, read/write loops, resize,
  foreground process ID, TTY name, process exit, and platform-specific macOS
  behavior
- rendering state and renderer boundary: enough structure to preserve Ghostty's
  rendering model while adapting it to Rust, with Metal renderer parity as a
  later subsystem slice
- font and text stack: CoreText integration, shaping, fallback, metrics, glyph
  atlas behavior, emoji behavior, and Nerd Font handling
- clipboard, keyboard translation, mouse input, IME/preedit, and platform input
  edge cases
- the renamed macOS Swift frontend integration once the Rust library has enough
  behavior to host it

## Architecture

Roastty should be a faithful macOS adaptation of Ghostty's architecture, not a
new terminal emulator invented beside it. The implementation may use idiomatic
Rust where Rust gives a clearer or safer representation, but the port should
preserve Ghostty's subsystem boundaries and macOS behavior unless an experiment
explicitly records why a different design is required.

The public library boundary is `libroastty`:

- exported C functions use `roastty_*`
- public headers use Roastty names
- Rust crates/modules use Roastty names where product identity is visible
- compatibility shims named `ghostty_*` are not allowed
- upstream Ghostty names are allowed only when describing the source material,
  vendored paths, test provenance, or attribution

The target is macOS-only, not macOS-first. Cross-platform terminal parity is not
part of this issue and should not shape the internal architecture.

## Test Parity

The rewrite must include tests, not only implementation code. Each subsystem
experiment must identify the relevant upstream Ghostty tests, fixtures, or
behavioral checks and either:

- port those tests into the Roastty Rust test suite,
- create equivalent Roastty tests when a direct port is not appropriate, or
- explicitly document why a specific upstream test cannot be automated yet and
  what manual or future infrastructure would be required.

Feature parity is not considered achieved for a subsystem until both the
implementation and its corresponding tests or documented test-equivalent checks
exist.

## Process

This is a long parent issue. Do not attempt a big-bang rewrite. Each experiment
must choose one coherent subsystem slice, implement it, test it, record the
result, and let that result determine the next experiment.

Every experiment must follow the current project process:

- design exactly one experiment at a time
- have the experiment design reviewed by another AI agent before implementation
- fix all real design issues found by that review
- commit the reviewed experiment design separately
- implement and verify the experiment
- have the completed result reviewed by another AI agent before proceeding
- fix all real result issues found by that review
- commit the experiment result separately

No experiment may proceed to the next stage until the required review passes.

## Experiment Granularity

Early experiments may be small when they establish correctness-critical
foundation: page storage, managed metadata, pin tracking, parser state,
formatters, selection, and row/cell mutation primitives. These areas are allowed
to advance one primitive or one control sequence at a time because bugs are hard
to localize and can corrupt later layers.

Once the relevant foundation exists, experiments should grow to coherent
subsystem slices rather than one tiny behavior at a time. Prefer grouping
related features when they share the same implementation surface and can be
verified together, for example:

- remaining related CSI row/scroll mutations;
- SGR execution plus styled printing;
- OSC parsing and action dispatch;
- mouse modes plus mouse encoding;
- PTY spawn/read/write/resize;
- macOS input/key translation.

The goal is not to maximize experiment count. The goal is to preserve
reviewable, testable progress toward full `libroastty` parity. If an experiment
can cover a larger subsystem without blurring failure diagnosis or weakening
tests, choose the larger subsystem.

## Experiments

- [Experiment 1: Audit Dependencies and Platform Readiness](01-dependency-platform-audit.md)
  — **Pass**
- [Experiment 2: Define Zig-to-Rust Porting Patterns](02-zig-rust-porting-patterns.md)
  — **Pass**
- [Experiment 3: Port Terminal Tabstops](03-port-tabstops.md) — **Pass**
- [Experiment 4: Port Terminal Size Offsets](04-port-terminal-size.md) —
  **Pass**
- [Experiment 5: Decompose Page Storage Port](05-decompose-page-storage.md) —
  **Pass**
- [Experiment 6: Port Bitmap Allocator](06-port-bitmap-allocator.md) — **Pass**
- [Experiment 7: Port Terminal Style Value Types](07-port-style-value-types.md)
  — **Pass**
- [Experiment 8: Port Packed Row and Cell Values](08-port-row-cell-values.md) —
  **Pass**
- [Experiment 9: Port Page Capacity and Layout Arithmetic](09-port-page-layout.md)
  — **Pass**
- [Experiment 10: Port Basic Page Allocation and Access](10-port-page-init-access.md)
  — **Pass**
- [Experiment 11: Port Offset Hash Map Storage](11-port-offset-hash-map.md) —
  **Pass**
- [Experiment 12: Port Page Grapheme Storage](12-port-page-graphemes.md) —
  **Pass**
- [Experiment 13: Port Page Clone for Text and Graphemes](13-port-page-clone.md)
  — **Pass**
- [Experiment 14: Port Ref-Counted Set Storage](14-port-ref-counted-set.md) —
  **Pass**
- [Experiment 15: Port Style Hashing and Set Storage](15-port-style-set.md) —
  **Pass**
- [Experiment 16: Port Page Style Storage and Clone](16-port-page-style-clone.md)
  — **Pass**
- [Experiment 17: Port Page CloneFrom Plain Rows](17-port-page-clone-from-plain.md)
  — **Pass**
- [Experiment 18: Port Page CloneFrom Graphemes](18-port-page-clone-from-graphemes.md)
  — **Pass**
- [Experiment 19: Port Page CloneFrom Styles](19-port-page-clone-from-styles.md)
  — **Pass**
- [Experiment 20: Port Page Hyperlink Storage](20-port-page-hyperlink-storage.md)
  — **Pass**
- [Experiment 21: Port Page CloneFrom Hyperlinks](21-port-page-hyperlink-row-copy.md)
  — **Pass**
- [Experiment 22: Port Page Exact Row Capacity](22-port-page-exact-row-capacity.md)
  — **Pass**
- [Experiment 23: Port Page Partial Row Clone](23-port-page-partial-row-clone.md)
  — **Pass**
- [Experiment 24: Port Page Move Cells](24-port-page-move-cells.md) — **Pass**
- [Experiment 25: Port Page Swap Cells](25-port-page-swap-cells.md) — **Pass**
- [Experiment 26: Port Page Clear Cells](26-port-page-clear-cells.md) — **Pass**
- [Experiment 27: Port Page Reinit](27-port-page-reinit.md) — **Pass**
- [Experiment 28: Port Page Integrity Checks](28-port-page-integrity-checks.md)
  — **Pass**
- [Experiment 29: Port Page Set Graphemes](29-port-page-set-graphemes.md) —
  **Pass**
- [Experiment 30: Port Page Move Grapheme](30-port-page-move-grapheme.md) —
  **Pass**
- [Experiment 31: Port Terminal Points](31-port-terminal-points.md) — **Pass**
- [Experiment 32: Port PageList Sizing](32-port-pagelist-sizing.md) — **Pass**
- [Experiment 33: Port PageList Init](33-port-pagelist-init.md) — **Pass**
- [Experiment 34: Port PageList Points](34-port-pagelist-points.md) — **Pass**
- [Experiment 35: Port PageList Tracked Pins](35-port-pagelist-tracked-pins.md)
  — **Pass**
- [Experiment 36: Port PageList Scrollbar State](36-port-pagelist-scrollbar.md)
  — **Pass**
- [Experiment 37: Port PageList Viewport Scrolling](37-port-pagelist-scroll.md)
  — **Pass**
- [Experiment 38: Port PageList Basic Growth](38-port-pagelist-grow.md) —
  **Pass**
- [Experiment 39: Port PageList Prune Growth](39-port-pagelist-prune.md) —
  **Pass**
- [Experiment 40: Port PageList Reset](40-port-pagelist-reset.md) — **Pass**
- [Experiment 41: Port PageList Page Iterator](41-port-pagelist-page-iterator.md)
  — **Pass**
- [Experiment 42: Port PageList Clone](42-port-pagelist-clone.md) — **Pass**
- [Experiment 43: Port PageList Dirty Helpers](43-port-pagelist-dirty-helpers.md)
  — **Pass**
- [Experiment 44: Port PageList Increase Capacity](44-port-pagelist-increase-capacity.md)
  — **Pass**
- [Experiment 45: Port PageList Compact](45-port-pagelist-compact.md) — **Pass**
- [Experiment 46: Port PageList Split](46-port-pagelist-split.md) — **Pass**
- [Experiment 47: Port PageList Viewport Fixup](47-port-pagelist-viewport-fixup.md)
  — **Pass**
- [Experiment 48: Port PageList Erase Row](48-port-pagelist-erase-row.md) —
  **Pass**
- [Experiment 49: Port PageList Erase Row Bounded](49-port-pagelist-erase-row-bounded.md)
  — **Pass**
- [Experiment 50: Port PageList Erase Page](50-port-pagelist-erase-page.md) —
  **Pass**
- [Experiment 51: Port PageList Erase Rows](51-port-pagelist-erase-rows.md) —
  **Pass**
- [Experiment 52: Port PageList Scroll Clear](52-port-pagelist-scroll-clear.md)
  — **Pass**
- [Experiment 53: Port PageList Cell Lookup](53-port-pagelist-cell-lookup.md) —
  **Pass**
- [Experiment 54: Port PageList Row Iterator](54-port-pagelist-row-iterator.md)
  — **Pass**
- [Experiment 55: Port PageList Cell Iterator](55-port-pagelist-cell-iterator.md)
  — **Pass**
- [Experiment 56: Port PageList Prompt Iterator](56-port-pagelist-prompt-iterator.md)
  — **Pass**
- [Experiment 57: Port Semantic Prompt Highlight](57-port-semantic-prompt-highlight.md)
  — **Pass**
- [Experiment 58: Port Semantic Input Highlight](58-port-semantic-input-highlight.md)
  — **Pass**
- [Experiment 59: Port Semantic Output Highlight](59-port-semantic-output-highlight.md)
  — **Pass**
- [Experiment 60: Port Semantic Highlight Dispatcher](60-port-semantic-highlight-dispatcher.md)
  — **Pass**
- [Experiment 61: Port Highlight Untracked Module](61-port-highlight-untracked-module.md)
  — **Pass**
- [Experiment 62: Port Highlight Flattened Shape](62-port-highlight-flattened-shape.md)
  — **Pass**
- [Experiment 63: Port Flattened Highlight Constructor](63-port-flattened-highlight-constructor.md)
  — **Pass**
- [Experiment 64: Port Tracked Highlight](64-port-tracked-highlight.md) —
  **Pass**
- [Experiment 65: Port Selection Codepoints](65-port-selection-codepoints.md) —
  **Pass**
- [Experiment 66: Port Selection Value Shape](66-port-selection-value-shape.md)
  — **Pass**
- [Experiment 67: Port Selection Ordering](67-port-selection-ordering.md) —
  **Pass**
- [Experiment 68: Port Selection Containment](68-port-selection-containment.md)
  — **Pass**
- [Experiment 69: Port Selection Contained Row](69-port-selection-contained-row.md)
  — **Pass**
- [Experiment 70: Port Selection Adjustment](70-port-selection-adjustment.md) —
  **Pass**
- [Experiment 71: Port Selection Tracking Ownership](71-port-selection-tracking-ownership.md)
  — **Pass**
- [Experiment 72: Port Selection Pin Navigation](72-port-selection-pin-navigation.md)
  — **Pass**
- [Experiment 73: Port Cell Drag Selection](73-port-cell-drag-selection.md) —
  **Pass**
- [Experiment 74: Port Word Selection](74-port-word-selection.md) — **Pass**
- [Experiment 75: Port Line Selection](75-port-line-selection.md) — **Pass**
- [Experiment 76: Port Select All](76-port-select-all.md) — **Pass**
- [Experiment 77: Port Select Output](77-port-select-output.md) — **Pass**
- [Experiment 78: Port Line Iterator](78-port-line-iterator.md) — **Pass**
- [Experiment 79: Port Plain Selection String](79-port-plain-selection-string.md)
  — **Pass**
- [Experiment 80: Port Prompt Click Movement](80-port-prompt-click-movement.md)
  — **Pass**
- [Experiment 81: Port Dump String Helpers](81-port-dump-string.md) — **Pass**
- [Experiment 82: Port Styled Page Formatter Core](82-port-styled-page-formatter-core.md)
  — **Pass**
- [Experiment 83: Port Formatter Codepoint Map](83-port-codepoint-map.md) —
  **Pass**
- [Experiment 84: Port Plain Formatter Point Map](84-port-plain-formatter-point-map.md)
  — **Pass**
- [Experiment 85: Port Plain Formatter Pin Map](85-port-plain-formatter-pin-map.md)
  — **Pass**
- [Experiment 86: Port VT Formatter Point Map](86-port-vt-formatter-point-map.md)
  — **Pass**
- [Experiment 87: Port HTML Formatter Point Map](87-port-html-formatter-point-map.md)
  — **Pass**
- [Experiment 88: Port Styled Formatter Pin Maps](88-port-styled-formatter-pin-maps.md)
  — **Pass**
- [Experiment 89: Port Screen Formatter Content](89-port-screen-formatter-content.md)
  — **Pass**
- [Experiment 90: Port Terminal Formatter Content](90-port-terminal-formatter-content.md)
  — **Pass**
- [Experiment 91: Port Screen Formatter Cursor and Style Extras](91-port-screen-formatter-cursor-style-extras.md)
  — **Pass**
- [Experiment 92: Port Screen Formatter Protection Extra](92-port-screen-formatter-protection-extra.md)
  — **Pass**
- [Experiment 93: Port Screen Formatter Charset Extra](93-port-screen-formatter-charset-extra.md)
  — **Pass**
- [Experiment 94: Port Screen Formatter Kitty Keyboard Extra](94-port-screen-formatter-kitty-keyboard-extra.md)
  — **Pass**
- [Experiment 95: Port Screen Formatter Hyperlink Extra](95-port-screen-formatter-hyperlink-extra.md)
  — **Pass**
- [Experiment 96: Port Terminal Formatter Screen Extra Forwarding](96-port-terminal-formatter-screen-extra-forwarding.md)
  — **Pass**
- [Experiment 97: Port Terminal Formatter Palette Extra](97-port-terminal-formatter-palette-extra.md)
  — **Pass**
- [Experiment 98: Port Terminal Modes and Formatter Extra](98-port-terminal-modes-extra.md)
  — **Pass**
- [Experiment 99: Port Terminal Scrolling Region Formatter Extra](99-port-terminal-scrolling-region-extra.md)
  — **Pass**
- [Experiment 100: Port Terminal Tabstops Formatter Extra](100-port-terminal-tabstops-extra.md)
  — **Pass**
- [Experiment 101: Port Terminal Keyboard and Pwd Formatter Extra](101-port-terminal-keyboard-pwd-extra.md)
  — **Pass**
- [Experiment 102: Port Stream UTF-8 Print Core](102-port-stream-utf8-print-core.md)
  — **Pass**
- [Experiment 103: Port Basic Stream Print Mutation](103-port-basic-stream-print-mutation.md)
  — **Pass**
- [Experiment 104: Port Basic Pending Wrap](104-port-basic-pending-wrap.md) —
  **Pass**
- [Experiment 105: Port Basic Wrap Scroll](105-port-basic-wrap-scroll.md) —
  **Pass**
- [Experiment 106: Port Basic LF and CR](106-port-basic-lf-cr.md) — **Pass**
- [Experiment 107: Port Basic Backspace](107-port-basic-backspace.md) — **Pass**
- [Experiment 108: Port Basic Horizontal Tab](108-port-basic-horizontal-tab.md)
  — **Pass**
- [Experiment 109: Port VT and FF Linefeed Aliases](109-port-vt-ff-linefeed-aliases.md)
  — **Pass**
- [Experiment 110: Port Escape Tab Set](110-port-escape-tab-set.md) — **Pass**
- [Experiment 111: Port CSI Tab Set](111-port-csi-tab-set.md) — **Pass**
- [Experiment 112: Port CSI Tab Clear and Reset](112-port-csi-tab-clear-reset.md)
  — **Pass**
- [Experiment 113: Port Escape Index and Next Line](113-port-escape-index-next-line.md)
  — **Pass**
- [Experiment 114: Port Basic CSI Cursor Movement](114-port-basic-csi-cursor-movement.md)
  — **Pass**
- [Experiment 115: Port CSI Next and Previous Line](115-port-csi-next-previous-line.md)
  — **Pass**
- [Experiment 116: Port CSI Horizontal Absolute](116-port-csi-horizontal-absolute.md)
  — **Pass**
- [Experiment 117: Port CSI Vertical Positioning](117-port-csi-vertical-positioning.md)
  — **Pass**
- [Experiment 118: Port CSI Cursor Position](118-port-csi-cursor-position.md) —
  **Pass**
- [Experiment 119: Port CSI Horizontal Tabulation](119-port-csi-horizontal-tabulation.md)
  — **Pass**
- [Experiment 120: Port CSI Erase Display](120-port-csi-erase-display.md) —
  **Pass**
- [Experiment 121: Port CSI Erase Line](121-port-csi-erase-line.md) — **Pass**
- [Experiment 122: Port CSI Delete Character](122-port-csi-delete-character.md)
  — **Pass**
- [Experiment 123: Port CSI Insert Lines](123-port-csi-insert-lines.md) —
  **Pass**
- [Experiment 124: Port CSI Delete Lines](124-port-csi-delete-lines.md) —
  **Pass**
- [Experiment 125: Port CSI Scroll Up and Down](125-port-csi-scroll-up-down.md)
  — **Pass**
- [Experiment 126: Port CSI Insert and Erase Characters](126-port-csi-insert-erase-characters.md)
  — **Pass**
- [Experiment 127: Port CSI Horizontal Tab Back](127-port-csi-horizontal-tab-back.md)
  — **Pass**
- [Experiment 128: Port CSI Mode Set and Reset](128-port-csi-mode-set-reset.md)
  — **Pass**
- [Experiment 129: Port Basic Print Mode Effects](129-port-basic-print-mode-effects.md)
  — **Pass**
- [Experiment 130: Port CSI Mode Save and Restore](130-port-csi-mode-save-restore.md)
  — **Pass**
- [Experiment 131: Port DECRQM Mode Reports](131-port-decrqm-mode-reports.md) —
  **Pass**
- [Experiment 132: Port SGR and Styled Printing](132-port-sgr-styled-printing.md)
  — **Pass**
- [Experiment 133: Port Basic OSC Runtime](133-port-basic-osc-runtime.md) —
  **Pass**
- [Experiment 134: Port OSC 8 Printed Cell Hyperlinks](134-port-osc8-cell-hyperlinks.md)
  — **Pass**
- [Experiment 135: Port OSC ANSI Palette Operations](135-port-osc-ansi-palette.md)
  — **Pass**
- [Experiment 136: Port OSC Dynamic Colors](136-port-osc-dynamic-colors.md) —
  **Pass**
- [Experiment 137: Port RGB Parser Parity](137-port-rgb-parser-parity.md) —
  **Pass**
- [Experiment 138: Port Kitty OSC 21 Colors](138-port-kitty-osc21-colors.md) —
  **Pass**
- [Experiment 139: Port OSC 22 Mouse Shape](139-port-osc22-mouse-shape.md) —
  **Pass**
- [Experiment 140: Port Kitty OSC 66 Text Sizing](140-port-kitty-osc66-text-sizing.md)
  — **Pass**
- [Experiment 141: Port OSC Notifications](141-port-osc-notifications.md) —
  **Pass**
- [Experiment 142: Port OSC Clipboard Protocols](142-port-osc-clipboard-protocols.md)
  — **Pass**
- [Experiment 143: Port OSC 3008 Context Signals](143-port-osc3008-context-signals.md)
  — **Pass**
- [Experiment 144: Port OSC 133 Semantic Prompts](144-port-osc133-semantic-prompts.md)
  — **Pass**
- [Experiment 145: Port iTerm2 OSC 1337](145-port-iterm2-osc1337.md) — **Pass**
- [Experiment 146: Port Terminal Query Responses](146-port-terminal-query-responses.md)
  — **Pass**
- [Experiment 147: Port DCS and APC Framing](147-port-dcs-apc-framing.md) —
  **Pass**
- [Experiment 148: Port DCS Command Handling](148-port-dcs-command-handling.md)
  — **Pass**
- [Experiment 149: Port Cursor Visual Style](149-port-cursor-visual-style.md) —
  **Pass**
- [Experiment 150: Port ESC Cursor State Controls](150-port-esc-cursor-state-controls.md)
  — **Pass**
- [Experiment 151: Port RIS Full Reset](151-port-ris-full-reset.md) — **Pass**
- [Experiment 152: Port Charset Escape Controls](152-port-charset-escape-controls.md)
  — **Pass**
- [Experiment 153: Port CSI Repeat Previous Character](153-port-csi-repeat-previous-character.md)
  — **Pass**
- [Experiment 154: Port Alternate-Screen Modes](154-port-alternate-screen-modes.md)
  — **Pass**
- [Experiment 155: Port Kitty Keyboard Protocol](155-port-kitty-keyboard-protocol.md)
  — **Pass**
- [Experiment 156: Port Mouse Event Encoding](156-port-mouse-event-encoding.md)
  — **Pass**
- [Experiment 157: Port Mouse Mode Runtime State](157-port-mouse-mode-runtime-state.md)
  — **Pass**
- [Experiment 158: Port Mouse Encoder C ABI](158-port-mouse-encoder-c-abi.md) —
  **Pass**
- [Experiment 159: Port Key Event Value Types](159-port-key-event-value-types.md)
  — **Pass**
- [Experiment 160: Port Key Encoder Core](160-port-key-encoder-core.md) —
  **Pass**
- [Experiment 161: Complete Key Encoder Tables](161-complete-key-encoder-tables.md)
  — **Pass**
- [Experiment 162: Port Legacy Ctrl/CSI-u/Alt Matrix](162-port-legacy-ctrl-csiu-alt-matrix.md)
  — **Pass**
- [Experiment 163: Port Key Encoder C ABI](163-port-key-encoder-c-abi.md) —
  **Pass**
- [Experiment 164: Port OSC Parser C ABI](164-port-osc-parser-c-abi.md) —
  **Pass**
- [Experiment 165: Port Terminal Stream C ABI](165-port-terminal-stream-c-abi.md)
  — **Pass**
- [Experiment 166: Port Terminal Scalar Getters C ABI](166-port-terminal-scalar-getters-c-abi.md)
  — **Pass**
- [Experiment 167: Port Terminal Mode Control C ABI](167-port-terminal-mode-control-c-abi.md)
  — **Pass**
- [Experiment 168: Port Terminal Metadata Setters C ABI](168-port-terminal-metadata-setters-c-abi.md)
  — **Pass**
- [Experiment 169: Port Terminal Color Set/Get C ABI](169-port-terminal-color-set-get-c-abi.md)
  — **Pass**
- [Experiment 170: Port Terminal Basic Effects C ABI](170-port-terminal-basic-effects-c-abi.md)
  — **Pass**
- [Experiment 171: Port Terminal Query Callback C ABI](171-port-terminal-query-callbacks-c-abi.md)
  — **Pass**
- [Experiment 172: Port Terminal Grid Reference C ABI](172-port-terminal-grid-reference-c-abi.md)
  — **Pass**
- [Experiment 173: Port Terminal Selection C ABI](173-port-terminal-selection-c-abi.md)
  — **Pass**
- [Experiment 174: Port Selection Gesture C ABI](174-port-selection-gesture-c-abi.md)
  — **Pass**
- [Experiment 175: Port Tracked Grid Reference C ABI](175-port-tracked-grid-reference-c-abi.md)
  — **Pass**
- [Experiment 176: Port Row and Cell C ABI](176-port-row-cell-c-abi.md) —
  **Pass**
- [Experiment 177: Port Style C ABI](177-port-style-c-abi.md) — **Pass**
- [Experiment 178: Port Render State Scalar C ABI](178-port-render-state-scalar-c-abi.md)
  — **Pass**
- [Experiment 179: Port Render State Row Iterator C ABI](179-port-render-state-row-iterator-c-abi.md)
  — **Pass**
- [Experiment 180: Port Render State Row Cells Basic C ABI](180-port-render-state-row-cells-basic-c-abi.md)
  — **Pass**
- [Experiment 181: Complete Render State Row Cells Selectors](181-complete-render-state-row-cells-selectors.md)
  — **Pass**
- [Experiment 182: Port Grid Ref Accessors C ABI](182-port-grid-ref-accessors-c-abi.md)
  — **Pass**
- [Experiment 183: Port Terminal Formatter C ABI](183-port-terminal-formatter-c-abi.md)
  — **Pass**
- [Experiment 184: Port Standalone Terminal Encoding C ABI](184-port-standalone-terminal-encoding-c-abi.md)
  — **Pass**
- [Experiment 185: Port Support C ABI](185-port-support-c-abi.md) — **Pass**
- [Experiment 186: Port Kitty Graphics Command Parser](186-port-kitty-graphics-command-parser.md)
  — **Pass**
- [Experiment 187: Port Direct Kitty Image Loading](187-port-direct-kitty-image-loading.md)
  — **Pass**
- [Experiment 188: Port Kitty Image Storage Foundation](188-port-kitty-image-storage-foundation.md)
  — **Pass**
- [Experiment 189: Port Kitty Transmit and Query Execution](189-port-kitty-transmit-query-execution.md)
  — **Pass**
- [Experiment 190: Port Kitty Placement Storage Foundation](190-port-kitty-placement-storage-foundation.md)
  — **Pass**
- [Experiment 191: Port Kitty Display Storage Execution](191-port-kitty-display-storage-execution.md)
  — **Pass**
- [Experiment 192: Port Kitty Graphics Terminal Dispatch](192-port-kitty-graphics-terminal-dispatch.md)
  — **Pass**
- [Experiment 193: Port Kitty Tracked Placement Ownership](193-port-kitty-tracked-placement-ownership.md)
  — **Pass**
- [Experiment 194: Port Kitty Delete Execution](194-port-kitty-delete-execution.md)
  — **Pass**
- [Experiment 195: Port Kitty Transmit-Display and Cursor-After](195-port-kitty-transmit-display-cursor-after.md)
  — **Pass**
- [Experiment 196: Port Kitty Graphics C ABI Handles](196-port-kitty-graphics-c-abi-handles.md)
  — **Pass**
- [Experiment 197: Port Kitty Placement Render Info ABI](197-port-kitty-placement-render-info-abi.md)
  — **Pass**
- [Experiment 198: Port Kitty Graphics Terminal Options](198-port-kitty-graphics-terminal-options.md)
  — **Pass**
- [Experiment 199: Port Kitty Graphics File Media](199-port-kitty-graphics-file-media.md)
  — **Pass**
- [Experiment 200: Port Kitty Graphics Shared Memory](200-port-kitty-graphics-shared-memory.md)
  — **Pass**
- [Experiment 201: Port Kitty Graphics PNG Decode Hook](201-port-kitty-graphics-png-decode-hook.md)
  — **Pass**
- [Experiment 202: Port Kitty Virtual Placeholder Foundation](202-port-kitty-virtual-placeholder-foundation.md)
  — **Pass**
- [Experiment 203: Port Kitty Terminal Render Placement ABI](203-port-kitty-terminal-render-placement-abi.md)
  — **Pass**
- [Experiment 204: Attach Kitty Render Placements to Render State](204-attach-kitty-render-placements-to-render-state.md)
  — **Pass**
- [Experiment 205: Port Renderer Image State Foundation](205-port-renderer-image-state-foundation.md)
  — **Pass**
- [Experiment 206: Port Renderer Image Upload and Draw Contract](206-port-renderer-image-upload-draw-contract.md)
  — **Pass**
- [Experiment 207: Port Metal Image Texture Values](207-port-metal-image-texture-values.md)
  — **Pass**
- [Experiment 208: Port Metal Texture Upload Backend](208-port-metal-texture-upload-backend.md)
  — **Pass**
- [Experiment 209: Port Image Draw Command Contract](209-port-image-draw-command-contract.md)
  — **Pass**
- [Experiment 210: Port Metal Buffer Wrapper](210-port-metal-buffer-wrapper.md)
  — **Pass**
- [Experiment 211: Port Metal Vertex Descriptor Mapping](211-port-metal-vertex-descriptor-mapping.md)
  — **Pass**
- [Experiment 212: Port Metal Pipeline Attachment Values](212-port-metal-pipeline-attachment-values.md)
  — **Pass**
- [Experiment 213: Port Standard Metal Pipeline Descriptions](213-port-standard-metal-pipeline-descriptions.md)
  — **Pass**
- [Experiment 214: Port Metal Pipeline State Builder](214-port-metal-pipeline-state-builder.md)
  — **Pass**
- [Experiment 215: Port Standard Metal Shader Library](215-port-standard-metal-shader-library.md)
  — **Pass**
- [Experiment 216: Port Offscreen Metal Render Pass Readback](216-port-offscreen-metal-render-pass-readback.md)
  — **Designed**

## Non-Goals

- Do not add TermSurf browser overlay behavior in this issue. Roastty must first
  become a real terminal foundation.
- Do not preserve or expose `ghostty_*` compatibility ABI names.
- Do not rewrite the vendored Ghostty source. The vendor copy is source material
  and attribution context.
- Do not retrofit older issue documents to the one-file-per-experiment
  structure.
- Do not implement Linux, FreeBSD, Windows, GTK, Wayland, X11, OpenGL, or other
  non-macOS support paths.
- Do not keep platform abstraction layers merely because upstream Ghostty has
  them. Keep an abstraction only when it is useful for Roastty's macOS design or
  for tests.

## Closure Criteria

This issue can close when `libroastty` implements the remaining macOS library
behavior required by the renamed app with test coverage or documented
test-equivalent checks for each ported subsystem. At that point Roastty should
be ready for a follow-up issue focused on the Swift app integration and then,
later, TermSurf browser overlay integration.
