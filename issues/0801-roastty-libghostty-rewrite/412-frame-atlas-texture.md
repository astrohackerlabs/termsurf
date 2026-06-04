+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 412: the modified-gated atlas sync (FrameAtlasTexture)

## Description

`sync_atlas_texture` (Experiment 411) re-uploads the whole font atlas to its GPU
texture. Doing that every frame would be wasteful — the atlas only changes when
new glyphs are rasterized. Upstream gates the sync on the atlas's `modified`
counter: a frame remembers the last-seen value per atlas and re-uploads only
when the counter has advanced. This experiment ports that gate as a small
per-frame wrapper, `FrameAtlasTexture` — the GPU texture plus its last-synced
`modified` value — with a `sync_if_modified` that runs `sync_atlas_texture` only
when the atlas changed. This is upstream's `drawFrame` `texture:` block,
factored into a reusable unit (the grayscale and color atlases each get one).

## Upstream behavior

In `drawFrame` (`renderer/generic.zig`), each atlas texture is synced under a
modified-counter gate (one block per atlas):

```zig
texture: {
    const modified = self.font_grid.atlas_grayscale.modified.load(.monotonic);
    if (modified <= frame.grayscale_modified) break :texture;     // unchanged — skip
    self.font_grid.lock.lockShared();
    defer self.font_grid.lock.unlockShared();
    frame.grayscale_modified = self.font_grid.atlas_grayscale.modified.load(.monotonic);
    try self.syncAtlasTexture(&self.font_grid.atlas_grayscale, &frame.grayscale);
}
```

The per-frame `grayscale_modified` / `color_modified` start at `0` (and are
reset to `0` when the frame's textures are reinitialized). The atlas `modified`
counter is bumped on every data change (`set`, `clear`, `grow`), so the first
frame (after glyphs are rasterized) always syncs, and a frame with no atlas
change skips.

## Rust mapping (`roastty/src/renderer/metal/texture.rs`)

`FrameAtlasTexture` bundles the texture with its last-synced `modified` value:

```rust
pub(crate) struct FrameAtlasTexture {
    texture: MetalTexture,
    last_modified: usize,
}

impl FrameAtlasTexture {
    /// Create the frame's atlas texture, sized/formatted to `atlas` but not yet
    /// uploaded (`last_modified = 0`, so the first `sync_if_modified` runs).
    pub(crate) fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        storage_mode: MetalStorageMode,
        atlas: &Atlas,
    ) -> Result<Self, MetalTextureError> {
        Ok(Self {
            texture: init_atlas_texture(device, storage_mode, atlas)?,
            last_modified: 0,
        })
    }

    /// Upload the atlas only if its `modified` counter advanced past the last
    /// sync. Returns whether a sync happened (upstream's `texture:` gate).
    pub(crate) fn sync_if_modified(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        storage_mode: MetalStorageMode,
        atlas: &Atlas,
    ) -> Result<bool, MetalTextureError> {
        let modified = atlas.modified();
        if modified <= self.last_modified {
            return Ok(false);
        }
        self.last_modified = modified;
        sync_atlas_texture(device, storage_mode, &mut self.texture, atlas)?;
        Ok(true)
    }

    /// The GPU texture (bound at the cell-text draw step).
    pub(crate) fn texture(&self) -> &MetalTexture {
        &self.texture
    }
}
```

The gate is upstream's `modified <= last → skip`; otherwise record the new value
and sync. `last_modified` starts at `0`, matching `frame.grayscale_modified`.

## Scope / faithfulness notes

- **Ported (bridged)**: `FrameAtlasTexture` — the per-frame atlas texture plus
  its last-synced `modified` value, with `sync_if_modified` gating
  `sync_atlas_texture` (Experiment 411) on the atlas's `modified` counter.
  Upstream's `drawFrame` `texture:` block, one per atlas.
- **Faithful**: the gate compares `atlas.modified() <= last_modified` (skip when
  not advanced), records the new value before syncing, and starts
  `last_modified` at `0`; the sync is the full `sync_atlas_texture`
  (reallocate-if-grown + re-upload).
- **Faithful adaptation**: roastty reads the atlas `modified` counter once
  (`atlas.modified()`) and stores that value; upstream loads it twice under a
  shared `font_grid` lock (to capture a concurrent write between the gate and
  the store). The `font_grid` shared lock around the sync is deferred (it
  belongs to the live font-grid threading model); with no concurrent writer the
  single-load gate has the same net effect. `sync_if_modified` returns a `bool`
  (did it sync) for testability; upstream's block has no return.
- **Deferred**: the live wiring that owns the grayscale and color
  `FrameAtlasTexture`s in the frame state and calls `sync_if_modified` each
  frame under the `font_grid` lock, and the texture reinit that resets the
  counters. (Consumed by a later slice; this experiment lands and tests the
  gate.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/metal/texture.rs`:
   - add a `FrameAtlasTexture` struct (`texture: MetalTexture`,
     `last_modified: usize`) with `new(device, storage_mode, atlas)`,
     `sync_if_modified(device, storage_mode, atlas) -> Result<bool, MetalTextureError>`,
     and a `texture()` accessor.
2. Tests (in `texture.rs`, live Metal device, grayscale atlas):
   - **first sync runs, second skips**: a grayscale `Atlas::new(4, Grayscale)`
     (its `modified` already advanced past `0` via `clear`) with a reserved
     pixel `set`; `FrameAtlasTexture::new` then `sync_if_modified` returns
     `true` and the texture holds `atlas.data()`; an immediate second
     `sync_if_modified` (no atlas change) returns `false`;
   - **change re-triggers**: after a further `set` on the atlas (its `modified`
     advances), `sync_if_modified` returns `true` again and the texture reflects
     the new `atlas.data()`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty frame_atlas
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `sync_if_modified` syncs only when the atlas's `modified` counter advanced
  past the last sync, recording the new value and returning whether it synced —
  faithful to upstream's `drawFrame` `texture:` gate;
- the tests pass (the first sync runs and uploads, the unchanged second skips, a
  subsequent atlas change re-triggers), and the existing tests still pass;
- the live frame-state wiring and the `font_grid` lock stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the gate syncs when unchanged (or skips a real
change), the counter is recorded incorrectly, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the gate is faithful: `modified <= last_modified` skips
unchanged atlases, `last_modified` starts at `0`, and recording the observed
value before `sync_atlas_texture` matches upstream's store-before-sync order.
Reusing the same single loaded `modified` value for both the compare and the
store is the right scoped adaptation while the live font-grid lock is deferred.
It confirmed the first sync runs with the current `Atlas` behavior (`Atlas::new`
and `set` both bump `modified`), the unchanged second sync skips, and a later
`set` retriggers; that returning `bool` is a harmless testability addition; and
that deferring the shared lock and the live grayscale/color frame-state wiring
is reasonable for this slice.

Review artifacts:

- Prompt: `logs/codex-review/20260604-074947-d412-prompt.md` (design)
- Result: `logs/codex-review/20260604-074947-d412-last-message.md` (design)

## Result

**Result:** Pass

The modified-gated atlas sync is now live.

- `roastty/src/renderer/metal/texture.rs`: a `FrameAtlasTexture` struct
  (`texture: MetalTexture`, `last_modified: usize`) with `new` (the texture via
  `init_atlas_texture`, `last_modified = 0`), `sync_if_modified` (reads the
  atlas `modified` counter once, skips when `<= last_modified`, else records it
  and runs `sync_atlas_texture`, returning whether it synced), and a `texture()`
  accessor.

Tests (in `texture.rs`, live Metal device, grayscale atlas):

- `frame_atlas_texture_syncs_first_then_skips_unchanged` — a grayscale
  `Atlas::new(4, Grayscale)` with a reserved pixel `set`;
  `FrameAtlasTexture::new` then `sync_if_modified` returns `true` and the
  texture holds `atlas.data()`; an immediate second `sync_if_modified` returns
  `false`.
- `frame_atlas_texture_resyncs_after_change` — after the first sync, a further
  `set` advances the atlas `modified` counter, so `sync_if_modified` returns
  `true` again and the texture reflects the new `atlas.data()`.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2886 passed, 0 failed (+2, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer + `lib.rs`/header/`abi_harness.c`)
  clean; `git diff --check` clean.

## Conclusion

The font-atlas texture now re-uploads only when the atlas actually changed —
upstream's `drawFrame` `texture:` gate, factored as a reusable per-frame unit
(the grayscale and color atlases each get one). With the cell buffers
(`FrameCells`), the atlas textures (`FrameAtlasTexture`), the draw sequence
(`draw_cells`), and the upload/sync primitives in place, the renderer bridge has
all the per-frame GPU building blocks. The remaining renderer-bridge work is the
frame-state struct that owns these (`FrameCells` + the grayscale/color
`FrameAtlasTexture`s + the uniform buffer) and the per-frame orchestration that
acquires the target, runs the syncs, and issues `draw_cells` — which depends on
the live render `State` and the `begin_frame` target plumbing — plus the
deferred bg-image / kitty / overlay draws and the `rebuild_viewport`
cursor/preedit assembly.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the implementation matches the approved design and
upstream's texture gate: `sync_if_modified` uses the correct
`modified <= last_modified` skip condition, starts `last_modified` at `0`,
records the observed value before calling `sync_atlas_texture`, and returns
`true` only when a sync occurred; the single-load adaptation is consistent with
the scoped design while the live font-grid shared lock remains deferred. It
judged the tests sufficient (first sync uploads, the unchanged second sync
skips, a later atlas mutation retriggers the sync and updates the texture bytes
to `atlas.data()`). Internal Rust only — no public C ABI/header impact; nothing
needed to change before the result commit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-075155-r412-prompt.md` (result)
- Result: `logs/codex-review/20260604-075155-r412-last-message.md` (result)
