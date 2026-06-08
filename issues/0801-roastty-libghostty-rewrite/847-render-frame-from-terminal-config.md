+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.result]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"
+++

# Experiment 847: FrameRenderer::render_frame from (terminal, config)

## Description

Exps 842–846 made every piece of the render input derivable:
`FrameRenderState:: from_terminal` (colors/palette/cursor/row_never_extend) and
`FrameRenderKnobs:: from_config`. A caller can now assemble a complete
`FramePreparedRebuildInput` from `(terminal, config)` — but still has to wire
the four steps together by hand. This experiment adds the **single composed
entry point** the live draw path (`surface.draw()`) will call:
`FrameRenderer::render_frame(terminal, config)` and
`render_and_present_frame(terminal, config, presentation)`, which build the
state, knobs, and input internally and drive the existing `update_frame` /
`update_and_present_frame`.

These are **additive** — the input-taking `update_frame` /
`update_and_present_frame` stay (the tests that inject malformed inputs for
error paths, e.g. the `PaddingExtend` and present-error tests, still need to
pass a raw input). The new methods are the convenience the live path uses.

**Known perf cost (pre-existing):** `render_frame` builds the state via
`FrameRenderState::from_terminal`, which shapes the rows for `row_never_extend`,
and `update_frame`'s snapshot `collect` shapes them again — two shapings per
frame. This double-shaping was introduced by Exp 844's `from_terminal` (not by
this experiment, which only composes pre-existing pieces); the shared-shaping
refactor is deferred (Exp 846+, per the in-source comment).

## Changes

`roastty/src/renderer/frame_renderer.rs` (production code + tests).

- Add to `impl FrameRenderer`:

  ```rust
  /// Compose the full render input from (terminal, config) and rebuild a frame —
  /// the single entry point the live draw path uses (Issue 801, Exp 847).
  pub(crate) fn render_frame(
      &mut self,
      terminal: &Terminal,
      grid: &mut SharedGrid,
      dirty: RenderDirty,
      preedit: Option<Preedit>,
      config: &Config,
  ) -> Result<FramePreparedRebuildApplication, FramePreparedRebuildError> {
      let state = FrameRenderState::from_terminal(terminal);
      let knobs = FrameRenderKnobs::from_config(config);
      let input = state.rebuild_input(&knobs);
      self.update_frame(terminal, grid, dirty, preedit, input)
  }

  /// The Metal-presenting variant (Issue 801, Exp 847).
  pub(crate) fn render_and_present_frame(
      &mut self,
      terminal: &Terminal,
      grid: &mut SharedGrid,
      dirty: RenderDirty,
      preedit: Option<Preedit>,
      config: &Config,
      presentation: FramePreparedPresentationInput<'_>,
  ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
      let state = FrameRenderState::from_terminal(terminal);
      let knobs = FrameRenderKnobs::from_config(config);
      let input = state.rebuild_input(&knobs);
      self.update_and_present_frame(terminal, grid, dirty, preedit, input, presentation)
  }
  ```

`state` and `knobs` are locals that outlive the borrow `input` takes;
`update_frame` consumes `input` before the method returns. No other change.

## Verification

Per the bounded-run convention (15-min cap, Central-stamped, single tracked
task, no poll-watcher). Tests in `frame_renderer.rs`:

- **`render_frame` rebuilds from (terminal, config)** (non-Metal): a fresh
  `FrameRenderer` + a 4×3 terminal + `Config::default()` rebuilds the full frame
  (`reset_contents`, rows `[0,1,2]`, `current_grid` → 4×3) — equivalent to the
  hand-wired `update_frame(state.rebuild_input(from_config(...)))` path but via
  the one call.
- **`render_frame` equals the hand-wired path:**
  `render_frame(&term, …, &config)` produces the same application
  (`reset_contents`, `rebuilt_rows`) as the explicit
  `update_frame(&term, …, FrameRenderState::from_terminal(&term) .rebuild_input(&FrameRenderKnobs::from_config(&config)))`
  on an equivalent fresh renderer — proving the composition is exactly the four
  hand-wired steps, not just "doesn't panic".
- **`render_and_present_frame` presents** (Metal-guarded,
  `let Some(device) = metal_device() else { return; }`): composes and presents
  at the drawable size, reporting both rebuild and present halves;
  `current_grid` → 4×3.
- The input-taking `update_frame` / `update_and_present_frame` tests (840–846)
  still pass (untouched).
- `cargo build -p roastty` — no warnings. `cargo fmt -p roastty -- --check` —
  clean. Full suite via `scripts/bounded-run.sh` (default parallelism) stays
  green. No-ghostty grep on changed lines — clean. `git diff --check` — clean.

**Pass** = the new `render_frame` / `render_and_present_frame` tests pass, the
prior tests still pass, and the full suite stays green. **Partial/Fail** = any
test fails or the suite regresses.

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: APPROVED, no Required findings.** Confirmed: the
borrow/lifetime is sound (`state`/`knobs` declared before `input` outlive it;
the return/error types carry no lifetimes/borrows, so returning after the locals
drop is fine); the composition matches the
`update_frame`/`update_and_present_frame` signatures exactly (no parameter
mis-ordering) and is identical to the passing `from_config_knobs_drive_a_frame`
path; additive is justified (the `PaddingExtend` error tests inject an empty
`row_never_extend` that `render_frame` provably cannot produce, since
`from_terminal` always sizes it to the terminal rows); the double-shaping cost
is honestly disclosed and pre-existing (Exp 844). Two minors, both adopted:

- **Optional — weak config test.** The "font-thicken set → still rebuilds" check
  was a smoke test. **Fixed:** replaced with an **equivalence** assertion —
  `render_frame` produces the same application as the explicit four-step
  hand-wired path on an equivalent renderer.
- **Nit — cross-ref drift.** The perf note said "Exp 844" while the in-source
  comment says "Exp 846+". **Fixed:** the note now states the double-shaping was
  introduced by Exp 844 and the refactor is deferred to Exp 846+.

## Conclusion

_(to be written after the run)_
