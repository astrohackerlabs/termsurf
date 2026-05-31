# Experiment 56: Port PageList Prompt Iterator

## Description

Port the upstream PageList prompt-iteration layer.

Roastty already has row semantic prompt flags, point-to-pin conversion, pin
movement, row iteration, and cell iteration. Upstream `PromptIterator` uses the
row-level semantic prompt state to find prompt-start rows in either direction,
including prompt-continuation runs and trimmed-scrollback cases. This experiment
should add that prompt traversal behavior without also porting semantic
highlighting, diagrams, selection/search behavior, parser behavior, renderer
delivery, app behavior, or public ABI.

This is still PageList-only traversal work. `highlightSemanticContent` depends
on prompt traversal and should remain a later experiment.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/PageList.zig` for:
     - `PromptIterator`;
     - `PromptIterator.nextRightDown`;
     - `PromptIterator.nextLeftUp`;
     - `PageList.promptIterator`;
     - `Pin.promptIterator`.
   - Use upstream tests around `PageList promptIterator ...` as the behavioral
     source for prompt, prompt-continuation, orphan continuation, and inclusive
     limit cases.
   - Do not modify `vendor/ghostty/`.

2. Add an internal Rust `PromptIterator<'a>`.
   - Store:
     - `list: &'a PageList`;
     - `current: Option<Pin>`;
     - `limit: Option<Pin>`;
     - `direction: Direction`.
   - Implement `Iterator<Item = Pin>`.
   - Keep the type private/internal for now.
   - Yield prompt pins normalized to `x = 0`.
   - Do not expose mutable row, cell, page, or PageList access through the
     iterator.

3. Preserve upstream `RightDown` semantics.
   - Start from `current`; return `None` if current is absent.
   - Move downward one row at a time using the existing pin movement helper.
   - Ignore `SemanticPrompt::None` rows until a prompt row is found or the limit
     is reached.
   - Treat both `Prompt` and `PromptContinuation` as prompt starts when they are
     the first prompt-related row encountered while moving downward.
   - After yielding a prompt-related row, skip following continuation rows up to
     the limit.
   - If the prompt-related row is at the limit, yield it and end.
   - If a continuation run starts at the first available row, yield that
     continuation row as the prompt start, matching the trimmed-scrollback
     behavior.

4. Preserve upstream `LeftUp` semantics.
   - Start from `current`; return `None` if current is absent.
   - Move upward one row at a time using the existing pin movement helper.
   - Ignore `SemanticPrompt::None` rows until a prompt row is found or the limit
     is reached.
   - Yield `Prompt` rows directly.
   - When a `PromptContinuation` row is found, walk upward to find:
     - the nearest preceding `Prompt`, yielding that prompt row;
     - or the first preceding non-prompt row, yielding the last continuation row
       after that non-prompt row as an orphan/trimmed prompt start;
     - or, if there are no prior rows, yielding the starting continuation row as
       a trimmed prompt start.
   - Inclusive limits must behave like upstream: a prompt row at the limit is
     still yielded once, then iteration ends.

5. Add prompt-iterator constructors.
   - Add a private helper equivalent to upstream `Pin.promptIterator`, shaped as
     a `PageList` helper if that is cleaner in Rust.
   - Add `PageList::prompt_iterator(direction, top_left, bottom_left)`.
   - Match the point handling used by upstream and the existing iterator
     helpers:
     - resolve `top_left` with `pin`;
     - resolve explicit `bottom_left` with `pin`;
     - use `get_bottom_right(top_left.tag())` when no bottom-left point is
       supplied;
     - return an empty iterator if either endpoint cannot be pinned.
   - For `RightDown`, iterate from top-left toward bottom-left.
   - For `LeftUp`, iterate from bottom-left toward top-left.

6. Add tests.
   - Port the upstream single-page prompt traversal cases:
     - `left_up` finds normal prompts, prompt-with-continuation starts, and
       orphan continuations in reverse order.
     - `right_down` finds normal prompts, prompt-with-continuation starts, and
       orphan continuations in forward order.
     - `right_down` continuation at the first row yields that continuation row
       as a trimmed prompt start.
     - `right_down` starting in the middle of a continuation run yields the
       starting continuation row, not the prior prompt.
     - right/down inclusive limit yields a prompt at the limit.
     - left/up inclusive limit yields a prompt at the limit.
   - Add cross-page coverage in both directions.
   - Add cross-page continuation-run coverage in both directions:
     - a prompt starts on one page and continuation rows continue on the next
       page;
     - `RightDown` yields the prompt start once and skips the continuation run
       across the page boundary;
     - `LeftUp` resolves a continuation row on the later page back to the prompt
       start on the prior page.
   - Add active/history coordinate coverage where prompt iteration stops before
     active rows for history points.
   - Add inclusive-limit coverage where the limit lands on a
     `PromptContinuation` row:
     - for `RightDown`, the iterator yields the prompt or first continuation
       start once and ends without duplicating the limited continuation row;
     - for `LeftUp`, the iterator matches upstream behavior when the limit row
       itself is a continuation and yields that continuation as the prompt
       start.
   - Add invalid/unpinnable endpoint coverage returning an empty iterator.
   - Assert yielded prompt pins have `x = 0`.
   - Convert yielded pins back to expected points with `point_from_pin`.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - iterator shape;
     - direction behavior;
     - continuation/orphan-continuation behavior;
     - inclusive-limit behavior;
     - tests added;
     - verification command output summary;
     - independent result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `PromptIterator` yields prompt-start rows in upstream order for both
  directions;
- prompt continuation handling matches upstream, including orphan/trimmed
  continuation rows;
- prompt-continuation runs can cross page boundaries in both directions;
- inclusive prompt and continuation limits match upstream;
- prompt iteration crosses pages using existing pin movement helpers;
- history prompt iteration stops before active rows;
- invalid endpoints produce an empty iterator instead of panics;
- yielded prompt pins have `x = 0`;
- no semantic highlighting, diagram, parser, renderer, app, public ABI,
  resize/reflow, selection, or search work is introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- basic prompt iteration works, but a corner case around cross-page continuation
  runs, inclusive limits, or history/active coordinate conversion needs a
  follow-up experiment.

The experiment fails if:

- prompt iteration skips, duplicates, or misorders prompt starts;
- continuation rows are treated differently from upstream;
- inclusive prompt limits are exclusive or yield duplicates;
- invalid points panic;
- yielded prompt pins preserve caller `x` instead of normalizing to `x = 0`;
- the implementation expands into semantic highlighting, diagram output, parser,
  renderer, app, ABI, resize/reflow, selection, or search work;
- tests or formatting fail.
