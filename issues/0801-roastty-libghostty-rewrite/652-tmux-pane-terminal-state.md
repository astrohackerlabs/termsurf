+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
session = "019e9ad7-04a6-7b20-823a-fa6e3d24129f"
verdict = "approved"

[review.result]
agent = "codex"
session = "019e9ad7-04a6-7b20-823a-fa6e3d24129f"
verdict = "approved"
+++

# Experiment 652: Tmux Pane Terminal State

## Description

Port the per-pane terminal-state part of upstream `initLayout`.

Experiment 651 tracks layout-derived pane IDs and queues capture/state commands,
but it does not store pane objects. Upstream `initLayout` preserves existing
pane entries, creates a new `Terminal` for each newly discovered pane using the
leaf layout's width and height, and prunes panes that disappear from the current
layout set. This experiment should add that pane-owned terminal state while
leaving pane command output handling for later.

## Changes

1. Replace the viewer's bare tracked pane ID set with ordered `TmuxPane`
   records:
   - `TmuxPane { id, terminal }`;
   - the terminal is `super::terminal::Terminal`;
   - pane order remains deterministic first-seen layout traversal order.
   - remove `Clone`, `PartialEq`, and `Eq` derives from `TmuxViewer` because
     `Terminal` is not cloneable or equality-comparable;
   - inspect pane state through test accessors instead of comparing whole
     viewers.
2. Extend layout traversal so leaf panes carry size:
   - collect `id`, `width`, and `height` from `LayoutContent::Pane` leaves;
   - keep the first occurrence if a duplicate pane ID appears;
   - preserve first-seen order across windows.
3. Update `sync_layouts`:
   - preserve existing `TmuxPane` records for panes that remain present;
   - convert layout `usize` width/height to `u16` / `CellCountInt` with checked
     conversion before calling `Terminal::init`;
   - create `Terminal::init(cols, rows, None)` for newly discovered panes;
   - if width/height conversion overflows or `Terminal::init` fails, move the
     viewer to `Defunct` and emit `Exit` from the caller path;
   - prune panes removed from the latest layout set;
   - queue new-pane capture/state commands exactly as Experiment 651 does.
4. Preserve existing command sequencing:
   - `ListWindows` emits `Windows` first, then the next queued command if sync
     queued one;
   - `LayoutChange` emits `Windows` first, and only emits a queued command when
     no command was already in flight before the notification.
5. Keep these upstream behaviors explicitly out of scope:
   - applying `PaneHistory`, `PaneVisible`, or `PaneState` command output;
   - pane output handling;
   - terminal resizing for an existing pane whose layout dimensions change;
   - PTY writes and App/Surface runtime integration.
6. Add tests for:
   - new panes create terminals with the layout leaf dimensions;
   - existing panes are preserved across sync and do not queue duplicate
     captures;
   - removed panes are pruned;
   - duplicate pane IDs use the first leaf dimensions and create one terminal;
   - oversized pane dimensions defunct and emit `Exit`;
   - `ListWindows` and `LayoutChange` still preserve command sequencing;
   - session change clears pane terminal state.
7. Keep the README's overall `tmux` checklist item unchecked, refining it after
   the result to say pane terminal state is initialized while pane command
   output, PTY, and App integration remain missing.
8. Update this experiment file with result and review records.

## Verification

- `cargo test -p roastty terminal::tmux`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/652-tmux-pane-terminal-state.md`
- compare/read the Rust pane terminal-state logic against:
  - `vendor/ghostty/src/terminal/tmux/viewer.zig` `initLayout`
  - `vendor/ghostty/src/terminal/tmux/viewer.zig` `syncLayouts`
  - `roastty/src/terminal/terminal.rs` `Terminal::init`
- `git diff --check`

Pass = Roastty's standalone tmux viewer owns deterministic per-pane terminal
state, preserves existing panes, creates new terminals from layout leaf sizes,
prunes removed panes, keeps new-pane command queueing intact, and leaves pane
command output/runtime integration open.

Fail = pane terminals are not created with checked layout dimensions, oversized
dimensions are accepted silently, existing panes are reinitialized
unnecessarily, removed panes remain stored, duplicate pane IDs create duplicate
terminals, command sequencing regresses, pane command output is implemented
prematurely, or the README overclaims full tmux support.

## Design Review

Initial Codex design review session `019e9ad7-04a6-7b20-823a-fa6e3d24129f`
requested revisions:

- require checked conversion from layout `usize` dimensions to
  `Terminal::init`'s `CellCountInt` / `u16` parameters, defuncting on overflow;
- account for `Terminal` not implementing `Clone`, `PartialEq`, or `Eq` by
  removing those derives from `TmuxViewer` and using test accessors instead of
  whole-viewer equality.

The plan was revised to address those findings before implementation.

Follow-up review in the same session found no blocking issues and approved the
revised design. The reviewer confirmed that checked dimension conversion,
defunct-on-overflow behavior, oversized-dimension testing, `TmuxViewer` derive
changes, and the intended pane-terminal-state scope are now specified.

## Result

**Result:** Pass

Implemented ordered `TmuxPane` records in `roastty/src/terminal/tmux.rs`.
`TmuxViewer` now owns per-pane `Terminal` state instead of a bare pane ID list.
Layout traversal collects pane leaf IDs with their leaf width/height, preserves
first-seen order, and ignores duplicate pane IDs after the first occurrence.

`sync_layouts` now preserves existing pane records, creates `Terminal` instances
for newly discovered panes with checked `usize` to `u16` dimension conversion,
prunes removed panes, and keeps the new-pane capture/state command queueing from
Experiment 651. Oversized pane dimensions or terminal initialization failure
defunct the viewer and emit `Exit` from the caller path.

`TmuxViewer` no longer derives `Clone`, `PartialEq`, or `Eq` because it owns
`Terminal` values. Tests inspect pane IDs and terminal dimensions through
accessors instead of comparing whole viewers.

The intended upstream boundary remains intact. This experiment does not apply
`PaneHistory`, `PaneVisible`, or `PaneState` output, does not handle live pane
output, does not resize existing pane terminals, and does not integrate with
PTY, App, or Surface runtime code.

Verification performed:

- `cargo fmt -p roastty`
- `cargo test -p roastty terminal::tmux` — 101 passed, 0 failed

Source comparison was against `vendor/ghostty/src/terminal/tmux/viewer.zig`
`initLayout` and `syncLayouts`, plus `roastty/src/terminal/terminal.rs`
`Terminal::init`.

## Completion Review

Codex completion review session `019e9ad7-04a6-7b20-823a-fa6e3d24129f` found no
blocking issues and approved the completed experiment. The reviewer confirmed
that `TmuxViewer` now owns ordered `TmuxPane` records with Roastty `Terminal`
state, bare pane ID storage is replaced, layout traversal collects first-seen
pane `id`/`width`/`height`, existing pane terminals are preserved, removed panes
are pruned, new panes use checked `u16` conversion before `Terminal::init`,
oversized dimensions defunct the viewer, new-pane command queueing and
sequencing remain intact, `TmuxViewer` clone/equality derives were removed, and
pane command output, live pane output, resize, PTY, App, and Surface integration
remain out of scope.

The reviewer also ran:

- `cargo test -p roastty terminal::tmux` — 101 passed
- `cargo fmt -p roastty -- --check`
- `prettier --check ... README.md ... 652-tmux-pane-terminal-state.md`
- `git diff --check`

## Conclusion

Roastty's standalone tmux viewer now owns initialized terminal state for tracked
panes. The next tmux experiment should consume pane capture output into those
terminals, starting with `PaneHistory` / `PaneVisible` before `PaneState` and
live `%output`.
