+++
status = "closed"
opened = "2026-03-07"
closed = "2026-03-07"
+++

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

## Experiments

### Experiment 1: wgpu 25 → 26

Bump wgpu from 25.0.2 to 26.0.0. This version has one breaking change: a new
required field `depth_slice` on `RenderPassColorAttachment`.

#### Changes

1. **`wezboard/Cargo.toml`** (line 269): Change `wgpu = "25.0.2"` to
   `wgpu = "26.0.0"`.

2. **`wezboard-gui/src/termwindow/render/draw.rs`** (line 102): Add
   `depth_slice: None` after `resolve_target: None` in the
   `RenderPassColorAttachment` struct literal.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. `cargo run --bin wezboard-gui` — app launches and renders

#### Results

Success. Both changes applied cleanly. `cargo build -p wezboard-gui` compiled
with zero errors — wgpu 26.0.1 resolved (with naga 26.0.0, wgpu-hal 26.0.6,
metal 0.32.0). The app launched, rendered the terminal, and quit normally. The
only breaking change was the new `depth_slice: None` field on
`RenderPassColorAttachment`, exactly as predicted from the ts2 upgrade history.

### Experiment 2: wgpu 26 → 27

Bump wgpu from 26.0.0 to 27.0.0. This version has two breaking changes: a
removed lifetime parameter on `BufferViewMut` and a new required field
`experimental_features` on `DeviceDescriptor`.

#### Changes

1. **`wezboard/Cargo.toml`** (line 269): Change `wgpu = "26.0.0"` to
   `wgpu = "27.0.0"`.

2. **`wezboard-gui/src/renderstate.rs`** (line 194): Remove lifetime parameter
   from `BufferViewMut`. Change `mapping: wgpu::BufferViewMut<'static>,` to
   `mapping: wgpu::BufferViewMut,`.

3. **`wezboard-gui/src/termwindow/webgpu.rs`** (line 335): Add
   `experimental_features: wgpu::ExperimentalFeatures::default(),` after
   `trace: wgpu::Trace::Off,`.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. `cargo run --bin wezboard-gui` — app launches and renders

#### Results

Success. All three changes applied cleanly. `cargo build -p wezboard-gui`
compiled with zero errors — wgpu 27.0.1 resolved (with naga 27.0.3, wgpu-hal
27.0.4, wgpu-core 27.0.3). The app launched, rendered the terminal, and quit
normally. Both breaking changes were exactly as predicted from the ts2 upgrade
history: the removed `'static` lifetime on `BufferViewMut` and the new
`experimental_features` field on `DeviceDescriptor`.

### Experiment 3: wgpu 27 → 28

Bump wgpu from 27.0.0 to 28.0.0. This version has the most breaking changes:
async `enumerate_adapters`, `Surface` lifetime parameter, `MipmapFilterMode`
type change, field renames (`push_constant_ranges` → `immediate_size`,
`multiview` → `multiview_mask`), new `RenderPassDescriptor` fields, and
`request_adapter` returning `Result` instead of `Option`.

#### Changes

1. **`wezboard/Cargo.toml`** (line 269): Change `wgpu = "27.0.0"` to
   `wgpu = "28.0.0"`.

2. **`wezboard-gui/src/termwindow/webgpu.rs`** (line 194): Add lifetime
   parameter to `Surface`. Change `surface: &wgpu::Surface` to
   `surface: &wgpu::Surface<'_>` in `compute_compatibility_list`.

3. **`wezboard-gui/src/termwindow/webgpu.rs`** (lines 197, 238): Add `.await` to
   `enumerate_adapters` calls. Line 197 is inside `compute_compatibility_list`
   which must become `async fn`. Line 238 is inside `new_impl` which is already
   async.

4. **`wezboard-gui/src/termwindow/webgpu.rs`** (lines 280, 308): The two call
   sites of `compute_compatibility_list` must add `.await` since it is now
   async.

5. **`wezboard-gui/src/termwindow/webgpu.rs`** (lines 307–313): Refactor
   `adapter.ok_or_else()` to a `match` with `anyhow::bail!`, because
   `request_adapter` now returns `Result` and the error closure would need
   `.await` (not allowed in closures). Replace:

   ```rust
   let adapter = adapter.ok_or_else(|| {
       let adapters = compute_compatibility_list(&instance, backends, &surface);
       anyhow!(
           "no compatible adapter found. Available:\n{}",
           adapters.join("\n")
       )
   })?;
   ```

   With:

   ```rust
   let adapter = match adapter {
       Some(adapter) => adapter,
       None => {
           let adapters = compute_compatibility_list(&instance, backends, &surface).await;
           anyhow::bail!(
               "no compatible adapter found. Available:\n{}",
               adapters.join("\n")
           );
       }
   };
   ```

6. **`wezboard-gui/src/termwindow/webgpu.rs`** (line 410): Change
   `mipmap_filter: wgpu::FilterMode::Nearest` to
   `mipmap_filter: wgpu::MipmapFilterMode::Nearest`.

7. **`wezboard-gui/src/termwindow/webgpu.rs`** (line 419): Change
   `mipmap_filter: wgpu::FilterMode::Linear` to
   `mipmap_filter: wgpu::MipmapFilterMode::Linear`.

8. **`wezboard-gui/src/termwindow/webgpu.rs`** (line 454): Change
   `push_constant_ranges: &[]` to `immediate_size: 0`.

9. **`wezboard-gui/src/termwindow/webgpu.rs`** (line 492): Change
   `multiview: None` to `multiview_mask: None`.

10. **`wezboard-gui/src/termwindow/render/draw.rs`** (lines 118–120): Replace
    the three explicit fields with `..Default::default()`:

    ```rust
    // Before:
    depth_stencil_attachment: None,
    occlusion_query_set: None,
    timestamp_writes: None,

    // After:
    ..Default::default()
    ```

11. **`wezboard-gui/src/scripting/mod.rs`** (line 89–91): Wrap
    `enumerate_adapters` in `smol::block_on()` since this is a sync Lua
    callback. Change:

    ```rust
    let gpus: Vec<GpuInfo> = instance
        .enumerate_adapters(backends)
        .into_iter()
    ```

    To:

    ```rust
    let gpus: Vec<GpuInfo> = smol::block_on(instance
        .enumerate_adapters(backends))
        .into_iter()
    ```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. `cargo run --bin wezboard-gui` — app launches and renders

#### Results

Success. All 11 changes applied cleanly across 4 files.
`cargo build -p wezboard-gui` compiled with zero errors — wgpu 28.0.0 resolved
(with naga 28.0.0, wgpu-hal 28.0.1, wgpu-core 28.0.1, metal 0.33.0). The app
launched, rendered the terminal, and quit normally. One additional change was
needed beyond the plan: replacing `use anyhow::anyhow` with `use anyhow::bail`
since the `ok_or_else` → `match` refactor switched from `anyhow!` to `bail!`.
All breaking changes matched the ts2 upgrade history.

## Conclusion

wgpu successfully upgraded from 25.0.2 to 28.0.0 in three experiments:

- **Exp 1 (25→26):** 1 breaking change — new `depth_slice` field
- **Exp 2 (26→27):** 2 breaking changes — `BufferViewMut` lifetime removal,
  `experimental_features` field
- **Exp 3 (27→28):** 11 changes across 4 files — async `enumerate_adapters`,
  `Surface` lifetime, `MipmapFilterMode`, field renames, `RenderPassDescriptor`
  defaults, `bail!` refactor, `smol::block_on()` wrapper

Total: 14 edits across 5 files. Each step built and ran cleanly. The upgrade
path was identical to the ts2 history, confirming that the Wezboard fork's wgpu
usage follows the same patterns.
