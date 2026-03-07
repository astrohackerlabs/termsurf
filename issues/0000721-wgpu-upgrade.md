# Issue 721: Upgrade wgpu from 25 to 28

## Goal

Upgrade Wezboard's wgpu dependency from 25.0.2 to 28.0.0, the latest stable
release. This keeps the rendering backend on maintained, current APIs and
matches the version used in ts2/ts3.

## Background

Wezboard (the WezTerm fork) uses wgpu as one of three rendering backends
(OpenGL, WebGpu, Software). The WebGpu backend powers terminal text rendering
via a WGSL shader pipeline with texture atlases, vertex/index buffers, and
alpha-blended render passes.

The current pinned version is `wgpu = "25.0.2"` in `wezboard/Cargo.toml`. The
latest stable release is 28.0.0. We've done this exact upgrade twice before — in
ts2 (commits `af4c82274`, `55e0250bc`, `9422a1258`) and ts3 (commit
`97b98f3679`). Both times the upgrade went 25→26→27→28 in three steps, each with
a small number of breaking changes.

## Breaking changes by version

These are the exact changes required, verified against the ts2 upgrade diffs.

### 25 → 26 (1 change)

**`draw.rs`**: Add `depth_slice: None` to `RenderPassColorAttachment`.

```rust
// Before:
wgpu::RenderPassColorAttachment {
    view: &view,
    resolve_target: None,
    ops: wgpu::Operations { ... },
}

// After:
wgpu::RenderPassColorAttachment {
    view: &view,
    resolve_target: None,
    depth_slice: None,
    ops: wgpu::Operations { ... },
}
```

### 26 → 27 (2 changes)

1. **`renderstate.rs`**: Remove lifetime parameter from `BufferViewMut`.

   ```rust
   // Before:
   mapping: wgpu::BufferViewMut<'static>,
   // After:
   mapping: wgpu::BufferViewMut,
   ```

2. **`webgpu.rs`**: Add `experimental_features` field to `DeviceDescriptor`.

   ```rust
   // Before:
   trace: wgpu::Trace::Off,
   })
   // After:
   trace: wgpu::Trace::Off,
   experimental_features: wgpu::ExperimentalFeatures::default(),
   })
   ```

### 27 → 28 (6 changes)

1. **`webgpu.rs`**: `enumerate_adapters` is now async. Add `.await` in async
   contexts, wrap in `smol::block_on()` for sync contexts (Lua
   `enumerate_gpus`).

2. **`webgpu.rs`**: `Surface` now requires lifetime parameter. Change
   `wgpu::Surface` to `wgpu::Surface<'_>` in `compute_compatibility_list`.

3. **`webgpu.rs`**: `mipmap_filter` field type changed from `FilterMode` to
   `MipmapFilterMode`. Change `wgpu::FilterMode::Nearest` →
   `wgpu::MipmapFilterMode::Nearest` (and `Linear`).

4. **`webgpu.rs`**: `push_constant_ranges` renamed to `immediate_size`. Change
   `push_constant_ranges: &[]` → `immediate_size: 0`.

5. **`webgpu.rs`**: `multiview` renamed to `multiview_mask`. Change
   `multiview: None` → `multiview_mask: None`.

6. **`draw.rs`**: `RenderPassDescriptor` gains new fields. Replace explicit
   `depth_stencil_attachment: None, occlusion_query_set: None, timestamp_writes: None`
   with `..Default::default()`.

7. **`webgpu.rs`**: `adapter.ok_or_else()` pattern no longer works because the
   closure inside needs to `.await`. Refactor to `match` with `anyhow::bail!`.

8. **`scripting/mod.rs`**: `enumerate_adapters` call in sync Lua function needs
   `smol::block_on()` wrapper.

## Files affected

| File                                         | Changes                                                                                                       |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `wezboard/Cargo.toml`                        | Version bump (3 times)                                                                                        |
| `wezboard-gui/src/termwindow/render/draw.rs` | `depth_slice`, `..Default::default()`                                                                         |
| `wezboard-gui/src/renderstate.rs`            | `BufferViewMut` lifetime removal                                                                              |
| `wezboard-gui/src/termwindow/webgpu.rs`      | Async adapters, `MipmapFilterMode`, `immediate_size`, `multiview_mask`, `Surface<'_>`, `ExperimentalFeatures` |
| `wezboard-gui/src/scripting/mod.rs`          | `smol::block_on()` for `enumerate_adapters`                                                                   |

## Approach

Three experiments, one per version bump. Each is a mechanical replay of the
corresponding ts2 commit, adapted to the Wezboard file paths (which differ from
ts2's `ts2/wezterm-gui/` paths). Build and verify after each step.

### Verification for each experiment

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. `cargo run --bin wezboard-gui` with `front_end = "WebGpu"` in config — app
   launches, terminal renders correctly
3. If WebGpu config isn't set up, at minimum verify the build succeeds (the
   OpenGL backend is the default and doesn't use wgpu)
