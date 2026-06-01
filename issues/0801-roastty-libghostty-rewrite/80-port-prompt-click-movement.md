# Experiment 80: Port Prompt Click Movement

## Description

Port the core line-based movement calculation behind upstream
`Screen.promptClickMove()` into Roastty's current PageList-centered terminal
model.

Experiments 56-60 ported semantic prompt/input/output row and content
highlighting. Experiments 65-79 ported selection geometry and copied-text
primitives. Upstream's next selection-adjacent helper is prompt-click movement:
when shell integration marks prompt input cells, a click inside that input area
can be translated into left/right cursor-key counts.

Upstream `promptClickMove()` lives on `Screen` because it reads the current
cursor semantic state and the configured OSC 133 click mode from screen state.
Roastty does not have `Screen`, cursor state, parser state, or semantic prompt
configuration yet. This experiment should therefore port the reusable core into
PageList with explicit inputs:

- cursor pin;
- current cursor semantic state;
- click pin;
- click mode.

That keeps the movement algorithm faithful and testable now, while leaving the
future `Screen` wrapper to supply the cursor semantic state from real
cursor/parser state. The cursor page-cell semantic content should be derived
from the supplied `cursor_pin`, because upstream treats current cursor semantic
state and cursor page-cell state differently.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/Screen.zig` for:
     - `PromptClickMove`;
     - `promptClickMove`;
     - `promptClickLine`;
     - tests named `Screen: promptClickMove ...`.
   - Use existing Roastty code:
     - `roastty/src/terminal/page_list.rs`;
     - `roastty/src/terminal/page.rs` for `SemanticContent` and
       `SemanticPrompt`;
     - existing PageList pin validation, row iteration, and semantic helpers.
   - Do not modify `vendor/ghostty/`.

2. Add private prompt-click value types.
   - Preferred shapes:

     ```rust
     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
     struct PromptClickMove {
         left: usize,
         right: usize,
     }

     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
     enum PromptClickMode {
         None,
         ClickEvents,
         Line,
         Multiple,
         ConservativeVertical,
         SmartVertical,
     }
     ```

   - `PromptClickMove::ZERO` should match upstream `.zero`.
   - Keep both types private. Do not add public API or C ABI exposure.

3. Add private PageList prompt-click movement helpers.
   - Preferred public-to-this-module shape:

     ```rust
     fn prompt_click_move(
         &self,
         cursor_pin: Pin,
         cursor_state_semantic: SemanticContent,
         click_pin: Pin,
         mode: PromptClickMode,
     ) -> PromptClickMove
     ```

   - The helper should match upstream `promptClickMove()`:
     - if either pin is invalid or garbage, return zero;
     - derive the cursor page-cell semantic content from `cursor_pin`;
     - if neither `cursor_state_semantic` nor the cursor page-cell semantic
       content is `SemanticContent::Input`, return zero, matching upstream's
       `cursor.semantic_content OR cursor.page_cell.semantic_content` gate;
     - if `mode` is `None` or `ClickEvents`, return zero;
     - if `mode` is `Line`, `Multiple`, `ConservativeVertical`, or
       `SmartVertical`, call the line-based helper.
   - Add a private `prompt_click_line()` implementing upstream
     `promptClickLine()` against PageList pins.

4. Match upstream line movement semantics.
   - If cursor and click pins are equal, return zero.
   - For cursor-before-click movement:
     - walk rows from cursor to click in screen order;
     - count only `SemanticContent::Input` cells;
     - on the cursor row, start after the cursor column;
     - on continuation rows, start at the first input cell;
     - skip prompt/output/blank cells;
     - stop before non-`SemanticPrompt::PromptContinuation` rows after the
       cursor row;
     - stop at hard row boundaries (`row.wrap() == false`);
     - when stopping at the end of a hard-bounded input row before reaching the
       click, add one movement if the cursor cell semantic content is input, so
       clicking to the right moves to the editor-style end position. This `+1`
       must use the cursor page-cell semantic only, not the separate cursor
       state semantic, matching upstream.
   - For cursor-after-click movement:
     - walk rows from cursor to click in reverse screen order;
     - count only `SemanticContent::Input` cells before the cursor on the cursor
       row and through previous wrapped rows;
     - skip prompt/output/blank cells;
     - stop when the previous row is not a wrap continuation.

5. Add upstream-equivalent tests.
   - Port the upstream prompt-click tests as direct PageList fixtures for:
     - line right basic;
     - cursor not on input;
     - click on same position;
     - line right skips non-input cells;
     - line right soft-wrapped line;
     - disabled when click mode is none;
     - line right stops at hard wrap;
     - line right stops at non-continuation row;
     - line left basic;
     - line left skips non-input cells;
     - line left soft-wrapped line;
     - line left stops at hard wrap;
     - click right of input on the same line;
     - click right of input when cursor is already at the end;
     - click right of input on a lower line;
     - click right of input when cursor is at end and click is on a lower line;
     - click right of input when cursor is on the last input char.
   - Add Roastty-specific guard tests:
     - invalid and garbage cursor/click pins return zero;
     - `ClickEvents` mode returns zero;
     - `Multiple`, `ConservativeVertical`, and `SmartVertical` currently reuse
       line movement, matching upstream;
     - cursor state `Input` plus a non-input/blank cursor cell still allows left
       movement back into input;
     - cursor state `Input` plus a non-input/blank cursor cell does not add the
       hard-row right-edge `+1`;
     - cursor state non-input plus cursor page-cell `Input` still allows
       movement, matching upstream's OR gate;
     - cross-page wrapped input works when cursor and click pins are in
       different PageList nodes.

6. Keep scope narrow.
   - Do not add `Screen`, `Terminal`, cursor structs, parser state, OSC 133
     parser/config parsing, keyboard event emission, mouse event handling,
     public ABI, app, renderer, clipboard, or UI wiring.
   - Do not make this helper public outside the terminal module.
   - Do not change existing selection-string or line-iterator behavior.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty prompt_click
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Record the design-review outcome in this experiment file before
     implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real findings before proceeding.

9. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - helper names and location;
     - which upstream prompt-click tests were ported;
     - which Screen/cursor/parser integration pieces are intentionally deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- Roastty has private PageList prompt-click movement helpers equivalent to the
  line-based core of upstream `Screen.promptClickMove()` / `promptClickLine()`;
- input-only counting, non-input skipping, same-position zero movement, disabled
  modes, line modes, soft-wrap continuation, hard-wrap stops, lower-line clicks,
  and editor-style end-position movement match the ported upstream tests;
- invalid or garbage cursor/click pins return zero instead of panicking;
- no `Screen`, `Terminal`, cursor structs, parser state, OSC 133 parser/config
  parsing, keyboard event emission, mouse event handling, public ABI, app,
  renderer, clipboard, or UI wiring is added;
- `cargo fmt`, targeted prompt-click tests, PageList tests, and full
  `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- the line movement core works for same-page fixtures, but a specific cross-page
  or continuation-row behavior exposes a missing lower-level PageList primitive
  that should be split into the next experiment.

The experiment fails if:

- prompt-click movement cannot be implemented without adding `Screen`, parser,
  cursor state, keyboard event emission, public ABI, app, renderer, clipboard,
  or UI behavior;
- movement counts non-input cells;
- hard row boundaries or prompt-continuation boundaries are crossed incorrectly;
- disabled modes move the cursor;
- invalid pins panic;
- tests or formatting fail.

## Design Review

Codex reviewed the initial design and found one real blocker: the helper took
only one cursor semantic value, but upstream uses both current cursor semantic
state and the page-cell semantic content under the cursor. Upstream's top-level
gate allows movement when either is input, while the hard-row right-edge `+1`
uses only the cursor page-cell semantic content.

The design now takes `cursor_state_semantic`, derives cursor page-cell semantic
content from `cursor_pin`, and requires split-state tests for:

- cursor state input plus a non-input/blank cursor cell;
- cursor state input not triggering the page-cell-only right-edge `+1`;
- non-input cursor state plus input cursor page-cell.

Follow-up Codex review approved the updated design with no remaining blockers.
