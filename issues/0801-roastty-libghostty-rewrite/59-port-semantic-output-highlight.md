# Experiment 59: Port Semantic Output Highlight

## Description

Port the final upstream `PageList.highlightSemanticContent` semantic branch:
`SemanticContent::Output`.

Experiments 57 and 58 ported the prompt and input branches as private `PageList`
helpers. The output branch has one important extra rule: empty cells default to
`SemanticContent::Output`, but they are not real command output and must not be
selected. Upstream therefore uses `cell.hasText()` both when finding the first
output cell and when extending the end of the highlight.

This experiment should add output semantic highlighting without expanding into
renderer highlight flattening/tracking, search selection, diagrams, parser
behavior, renderer delivery, app behavior, public ABI, resize/reflow, selection,
or search work.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/PageList.zig` for:
     - `highlightSemanticContent`;
     - the `.output` switch branch;
     - the shared prompt-zone end calculation;
     - upstream tests named `PageList highlightSemanticContent output...`.
   - Use `vendor/ghostty/src/terminal/highlight.zig` only to confirm the return
     type remains an untracked highlight.
   - Do not modify `vendor/ghostty/`.

2. Add output semantic highlighting.
   - Add a private helper equivalent to the `.output` branch of upstream
     `highlightSemanticContent`, such as `highlight_semantic_output`.
   - Input:
     - a prompt-start `Pin`.
   - Output:
     - `Option<UntrackedHighlight>`.
   - Use the same prompt-zone end calculation from Experiments 57 and 58.
   - Iterate cells from the provided prompt pin's `x` to the prompt-zone end
     with `cell_iterator_from_pin(Direction::RightDown, at, Some(end))`.
   - Find the start:
     - skip `SemanticContent::Prompt`;
     - skip `SemanticContent::Input`;
     - on `SemanticContent::Output`, skip the cell if `!cell.has_text()`;
     - on text-bearing `SemanticContent::Output`, set both `start` and `end` to
       that pin;
     - if no text-bearing output is found by the zone end, return `None`.
   - Find the end:
     - stop before `SemanticContent::Prompt`;
     - stop before `SemanticContent::Input`;
     - on `SemanticContent::Output`, extend `end` only if `cell.has_text()`;
     - empty output cells after output begins do not move `end` and do not stop
       scanning unless a later prompt/input stops the branch.

3. Keep API shape narrow.
   - Prefer a private helper such as `highlight_semantic_output`.
   - Do not expose a complete public semantic-highlight API yet.
   - If a local private dispatcher is useful in tests, keep it private and
     impossible to mistake for renderer/app integration.
   - Do not add renderer, parser, selection, search, diagram, app, ABI, or
     public API work.

4. Add tests.
   - Port upstream output-focused cases:
     - basic output on one row starts at the first text-bearing output cell and
       ends at the last text-bearing output cell before prompt/input;
     - multiline output spans rows and ends at the last text-bearing output cell
       before input;
     - output stops at the next prompt;
     - no following prompt scans through the screen-bottom prompt zone and ends
       at the last text-bearing output cell;
     - no text-bearing output returns `None`;
     - empty default-output cells are skipped before the start and do not become
       the highlight.
     - empty output cells after output begins do not advance `end`, do not stop
       scanning, and may lie inside the returned contiguous start/end range if
       later text-bearing output appears before prompt/input or zone end.
   - Add a nonzero-`at.x` test:
     - put earlier prompt/input/default-output cells before `at.x`;
     - put text-bearing output at and after `at.x`;
     - verify the earlier cells do not affect the result.
   - Add a cross-page output-zone test where text-bearing output starts on one
     page and extends into the next before input or prompt stops it.
   - Convert highlight start/end pins back to expected screen points with
     `point_from_pin`.
   - Verify no tracked-pin side effects are introduced.

5. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

6. Independent review.
   - Before implementation, get an independent agent review of this experiment
     design.
   - After implementation and verification, get an independent result review.
   - Fix all real findings before proceeding.

7. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - output branch behavior;
     - empty-cell skipping behavior;
     - null behavior;
     - tests added;
     - verification command output summary;
     - independent result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- output highlighting starts at the first text-bearing output cell in the prompt
  zone;
- output highlighting extends through later text-bearing output cells;
- empty default-output cells before the first real output do not become the
  start of the highlight;
- empty output cells after output begins do not advance `end`;
- prompt/input before output is skipped while finding the start;
- prompt/input after output begins stops the highlight;
- no text-bearing output by the prompt-zone end returns `None`;
- output highlighting scans from the provided pin's `x`;
- output highlighting works across rows and pages;
- returned output highlights are untracked;
- prompt and input semantic highlight behavior from Experiments 57 and 58 still
  passes;
- no renderer highlight flattening/tracking, search selection, diagram, parser,
  renderer, app, public ABI, resize/reflow, selection, or search work is
  introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- independent design and result reviews approve the experiment, or all real
  findings are fixed before proceeding.

The experiment is partial if:

- output highlighting works for single-page cases, but cross-page or
  empty-output-cell behavior needs a follow-up experiment.

The experiment fails if:

- output highlighting selects empty default-output cells as real output;
- output highlighting includes prompt or input cells after output begins;
- output highlighting returns a highlight when no text-bearing output exists;
- output highlighting scans from `x = 0` instead of the provided pin's `x`;
- prompt or input highlighting regresses;
- output branch support is presented as renderer/app-visible complete;
- the implementation expands into renderer highlights, search selection, diagram
  output, parser, renderer, app, ABI, resize/reflow, selection, or search work;
- tests or formatting fail.
