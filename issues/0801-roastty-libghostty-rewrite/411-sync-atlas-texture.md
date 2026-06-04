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

# Experiment 411: the font-atlas texture sync (sync_atlas_texture)

## Description

The texture region-upload primitive (`replace_region` — Experiment 410) is the
core of upstream's per-frame **font-atlas sync**. This experiment ports the
wrapper, `sync_atlas_texture` (upstream `syncAtlasTexture`), plus the
atlas-texture constructor it depends on, `init_atlas_texture` (upstream
`initAtlasTexture`): given a font `Atlas` and a GPU `MetalTexture`, reallocate
the texture when the atlas has outgrown it, then upload the atlas pixels. This
is the third part of `drawFrame`'s per-frame sync (uniforms, cells, **atlas
textures**); the per-frame call site that decides when to sync (the
`modified`/`resized` counters) stays deferred.

## Upstream behavior

`initAtlasTexture` (`renderer/Metal.zig`) builds a square GPU texture matching
the atlas's size and format:

```zig
pub fn initAtlasTexture(self, atlas: *const font.Atlas) Texture.Error!Texture {
    const pixel_format = switch (atlas.format) {
        .grayscale => .r8unorm,
        .bgra => .bgra8unorm_srgb,
        else => @panic("unsupported atlas format for Metal texture"),
    };
    return try Texture.init(.{ .device = …, .pixel_format = pixel_format,
        .resource_options = .{ .cpu_cache_mode = .write_combined, .storage_mode = … },
        .usage = .{ .shader_read = true } }, atlas.size, atlas.size, null);
}
```

`syncAtlasTexture` (`renderer/generic.zig`) reallocates only when the atlas has
grown past the texture, then re-uploads the whole atlas:

```zig
fn syncAtlasTexture(self, atlas: *const font.Atlas, texture: *Texture) !void {
    if (atlas.size > texture.width) {
        texture.deinit();
        texture.* = try self.api.initAtlasTexture(atlas);
    }
    try texture.replaceRegion(0, 0, atlas.size, atlas.size, atlas.data);
}
```

## Rust mapping (`roastty/src/renderer/metal/texture.rs`)

roastty's `Atlas` exposes `size()`, `format()`
(`Format::{Grayscale, Bgr, Bgra}`), and `data()`; `MetalTexture::new` and
`replace_region` (Experiment 410) provide the GPU side:

```rust
pub(crate) fn init_atlas_texture(
    device: &ProtocolObject<dyn MTLDevice>,
    storage_mode: MetalStorageMode,
    atlas: &Atlas,
) -> Result<MetalTexture, MetalTextureError> {
    let (format, srgb) = match atlas.format() {
        Format::Grayscale => (ImageTextureFormat::Gray, false), // r8unorm
        Format::Bgra => (ImageTextureFormat::Bgra, true),       // bgra8unorm_srgb
        Format::Bgr => return Err(MetalTextureError::UnsupportedAtlasFormat(atlas.format())),
    };
    let size = atlas.size() as usize;
    MetalTexture::new(
        device,
        image_texture_options(format, srgb, storage_mode),
        size,
        size,
        None,
    )
}

pub(crate) fn sync_atlas_texture(
    device: &ProtocolObject<dyn MTLDevice>,
    storage_mode: MetalStorageMode,
    texture: &mut MetalTexture,
    atlas: &Atlas,
) -> Result<(), MetalTextureError> {
    let size = atlas.size() as usize;
    // Reallocate only when the atlas has grown past the texture.
    if size > texture.width() {
        *texture = init_atlas_texture(device, storage_mode, atlas)?;
    }
    texture.replace_region(0, 0, size, size, atlas.data())
}
```

The format mapping matches upstream exactly (`grayscale → r8unorm`,
`bgra → bgra8unorm_srgb`; `Bgr` has no Metal pixel format and is rejected —
where upstream `@panic`s, roastty returns an error). The reallocate-then-upload
order and the `atlas.size > texture.width` condition are upstream's; the full
upload is `replace_region(0, 0, size, size, atlas.data())` (upstream's
`replaceRegion(0, 0, size, size, data)`).

## Scope / faithfulness notes

- **Ported (bridged)**: `init_atlas_texture` (the atlas-format → GPU texture
  constructor) and `sync_atlas_texture` (the reallocate-when-grown + re-upload
  wrapper) — the font-atlas half of `drawFrame`'s per-frame sync, composing the
  `replace_region` primitive with the font `Atlas`.
- **Faithful**: the pixel-format mapping (`grayscale → r8unorm`,
  `bgra → bgra8unorm_srgb`); the square `atlas.size × atlas.size` texture with
  shader-read usage and no initial data; the reallocation condition
  (`atlas.size > texture.width`) and the
  reallocate-then-`replaceRegion(0, 0, size, size, data)` order.
- **Faithful adaptation**: reallocation assigns a fresh `MetalTexture` (Rust's
  RAII drops the old one — upstream's explicit `deinit` + reassign);
  `storage_mode` is a parameter (upstream reads `self.default_storage_mode`);
  the unsupported `Bgr` format returns
  `MetalTextureError::UnsupportedAtlasFormat` instead of panicking.
- **Deferred**: the per-frame call site that calls `sync_atlas_texture` for the
  grayscale and color atlases only when their `modified` counter advanced past
  the frame's last-seen value (upstream's `texture:` blocks in `drawFrame`), and
  the surrounding frame/atlas-lock plumbing. (Consumed by a later slice; this
  experiment lands and tests the sync.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/metal/texture.rs`:
   - add a `MetalTextureError::UnsupportedAtlasFormat(Format)` variant;
   - add
     `init_atlas_texture(device, storage_mode, atlas) -> Result<MetalTexture, MetalTextureError>`
     and
     `sync_atlas_texture(device, storage_mode, texture: &mut MetalTexture, atlas) -> Result<(), MetalTextureError>`.
   - import `crate::font::atlas::{Atlas, Format}`.
2. Tests (in `texture.rs`, live Metal device):
   - **reallocation**: a grayscale `Atlas::new(4, Grayscale)` with a reserved
     pixel `set` to a known value; an initial 2×2 grayscale texture →
     `sync_atlas_texture` reallocates to 4×4 and `read_bytes()` equals
     `atlas.data()` (16 bytes);
   - **no reallocation (sub-region upload)**: the same atlas (size 4) and an
     initial 6×6 grayscale texture → the texture stays 6×6 (no realloc), and the
     4×4 atlas pixels land in the top-left (`read_bytes()` is `atlas.data()` in
     the `[0,4) × [0,4)` block, zero elsewhere);
   - **format mapping**: `init_atlas_texture` for a `Grayscale` atlas yields a
     texture whose **Metal pixel format** is `R8Unorm` (via
     `texture.texture().pixelFormat() == MetalPixelFormat::R8Unorm.to_objc()`),
     `bytes_per_pixel == 1`, and the atlas size; for a `Bgra` atlas, the pixel
     format is `Bgra8UnormSrgb` and `bytes_per_pixel == 4`; for a `Bgr` atlas it
     returns `UnsupportedAtlasFormat`. (Asserting the concrete pixel format —
     not just the depth — guards against mapping `Bgra` to a wrong 4-byte format
     such as `Rgba8UnormSrgb` or non-sRGB `Bgra8Unorm`.)
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty atlas_texture
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `init_atlas_texture` builds a square texture with the upstream pixel-format
  mapping, and `sync_atlas_texture` reallocates only when the atlas grew past
  the texture then uploads the whole atlas via `replace_region` — faithful to
  upstream's `initAtlasTexture` / `syncAtlasTexture`;
- the tests pass (the reallocation upload; the no-realloc sub-region upload; the
  format mapping and the `Bgr` rejection), and the existing tests still pass;
- the per-frame atlas-sync call site (the `modified`-counter gate) stays
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the format mapping is wrong, the reallocation
condition or order differs from upstream, the upload writes the wrong region, or
any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Low** finding (no Required), now addressed:

- **Low (addressed):** the format-mapping test should assert the concrete Metal
  pixel format, not just `bytes_per_pixel` — checking only depth would not catch
  mapping `Format::Bgra` to a wrong 4-byte format (e.g. `Rgba8UnormSrgb` or
  non-sRGB `Bgra8Unorm`). The test now asserts `texture.texture().pixelFormat()`
  equals `R8Unorm` for the grayscale atlas and `Bgra8UnormSrgb` for the bgra
  atlas.

Codex confirmed the rest is faithful: `size > texture.width()` and the
reallocate-then-upload order match upstream;
`replace_region(0, 0, size, size, atlas.data())` is correct; the no-realloc
larger-texture top-left sub-region behavior matches upstream; `Bgr` as an error
is a reasonable Rust adaptation of upstream's panic; and deferring the
modified-counter call site is well scoped.

Review artifacts:

- Prompt: `logs/codex-review/20260604-074324-d411-prompt.md` (design)
- Result: `logs/codex-review/20260604-074324-d411-last-message.md` (design)
