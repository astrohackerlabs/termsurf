+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "default"
reasoning = "medium"

[review.result]
agent = "codex"
model = "default"
reasoning = "medium"
+++

# Experiment 811: IOSurfaceLayer Sync Presentation

## Description

Port the first useful slice of upstream `renderer/metal/IOSurfaceLayer.zig` into
Roastty: a retained CoreAnimation layer that can synchronously present an
IOSurface when the surface matches the layer's current pixel size.

Experiment 810 added the IOSurface-backed `MetalTarget` resource layer. The next
missing Metal primitive is the presentation layer that receives that target's
surface. Upstream `IOSurfaceLayer.zig` also contains a custom CALayer subclass,
display callback ivars, async main-thread dispatch, and animation suppression.
Those are important, but they are separable from the first resource/presentation
contract. This experiment adds the synchronous wrapper and size guard without
starting the live render loop or implementing the subclass/display-callback
path.

## Changes

- `roastty/Cargo.toml`
  - Add `objc2-quartz-core` with `CALayer` and `objc2-core-foundation` features
    so Roastty can create and inspect CoreAnimation layers.
  - Enable the `objc2` feature on `objc2-io-surface` so the IOSurface CF type is
    available to Objective-C APIs. The implementation will still use
    `IOSurfaceRef` because `MetalTarget` owns the surface through the existing
    CoreFoundation path.
- `roastty/src/renderer/metal/iosurface_layer.rs`
  - Add `MetalIOSurfaceLayer` owning a retained `CALayer`.
  - Create the layer with `CALayer::layer()`.
  - Set `contentsGravity` to `kCAGravityTopLeft`, matching upstream's resize
    behavior that avoids stretching stale frame contents.
  - Expose `layer() -> &CALayer` for later window/view integration.
  - Add a small unsafe bridge helper that views `&IOSurfaceRef` as `&AnyObject`
    for `CALayer::setContents`. The safety invariant is that IOSurface is
    toll-free bridged/CoreFoundation-backed and `objc2-io-surface` declares the
    CF type with Objective-C ref encoding when the `objc2` feature is enabled.
  - Add `set_surface_sync(&self, surface: &IOSurfaceRef)` that directly sets the
    layer `contents` to the IOSurface. This mirrors upstream `setSurfaceSync`
    and intentionally does not dispatch to the main thread.
  - Add `set_surface_if_size_matches(&self, surface: &IOSurfaceRef) -> bool`
    that computes the layer pixel size from `bounds * contentsScale`, assigns
    contents only when the IOSurface width/height match, and returns whether the
    assignment happened. This ports the discard logic from upstream's async
    callback in a testable synchronous form.
  - Add tests that create a layer, verify `contentsGravity`, set bounds and
    contents scale, present a matching `MetalTarget` surface, verify layer
    contents points at the same IOSurface object, verify a mismatched surface is
    rejected without replacing the previous contents, and verify scaled bounds
    math such as `1.5 × 2.0` bounds with `contentsScale = 2.0` accepting a
    `3 × 4` surface.
- `roastty/src/renderer/metal/mod.rs`
  - Add the `iosurface_layer` module.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - After implementation, update the Metal checklist row to mention the
    synchronous IOSurfaceLayer wrapper while keeping the custom subclass,
    async/main-thread presentation, display callback, and full live frame
    orchestration open.

## Verification

- Inspect:
  - `vendor/ghostty/src/renderer/metal/IOSurfaceLayer.zig`
  - `roastty/src/renderer/metal/target.rs`
  - local `objc2-quartz-core` generated `CALayer` bindings
- Run:
  - `cargo fmt -p roastty`
  - `cargo test -p roastty metal::iosurface_layer -- --nocapture --test-threads=1`
  - `cargo test -p roastty metal::target -- --nocapture --test-threads=1`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/811-iosurface-layer-sync.md`
- Run:
  - `git diff --check`

The experiment passes if Roastty has a retained CALayer wrapper that can
synchronously assign a matching IOSurface as layer contents and reject
mismatched surface sizes. It is Partial if layer creation lands but contents
assignment or size checks need follow-up. It fails if the synchronous layer
wrapper cannot be cleanly expressed with the current `objc2` bindings.

## Design Review

Codex reviewed the initial design and found one blocking gap before
implementation: `CALayer::setContents` takes `Option<&AnyObject>`, not
`&IOSurfaceRef`, so the plan needed to specify how the IOSurface is bridged into
the Objective-C object expected by CoreAnimation. The review also asked for
stronger verification: prove the assigned layer contents is the same IOSurface,
prove a rejected mismatched surface does not replace existing contents, and
cover scaled bounds math rather than only scale `1.0`.

The plan was updated to enable the `objc2` feature on `objc2-io-surface`, add an
explicit unsafe `IOSurfaceRef` to `AnyObject` bridge helper with its safety
invariant, and strengthen the tests with contents identity, unchanged contents
after mismatch, and a scaled `1.5 × 2.0` bounds / `contentsScale = 2.0` case.

## Result

**Result:** Pass

Roastty now has a synchronous IOSurfaceLayer presentation foundation:

- `roastty/Cargo.toml` adds `objc2-quartz-core` with `CALayer` and
  `objc2-core-foundation`, and enables `objc2` on `objc2-io-surface` for the
  IOSurface Objective-C bridge.
- `roastty/src/renderer/metal/iosurface_layer.rs` adds `MetalIOSurfaceLayer`,
  owning a retained `CALayer` created by `CALayer::layer()`.
- The layer initializes `contentsGravity` to `kCAGravityTopLeft`.
- `set_surface_sync` assigns an `IOSurfaceRef` as layer contents through a
  narrow unsafe `IOSurfaceRef` to `AnyObject` bridge.
- `set_surface_if_size_matches` computes `bounds * contentsScale`, assigns only
  matching IOSurfaces, and rejects mismatched sizes without replacing existing
  contents.
- Tests verify top-left gravity, direct surface contents identity, scaled bounds
  math (`1.5 × 2.0` with scale `2.0` accepts `3 × 4`), and unchanged contents
  after a mismatch.

Verification:

- Inspected `vendor/ghostty/src/renderer/metal/IOSurfaceLayer.zig`.
- Inspected `roastty/src/renderer/metal/target.rs`.
- Inspected local `objc2-quartz-core` generated `CALayer` bindings.
- `cargo fmt -p roastty` — passed.
- `cargo test -p roastty metal::iosurface_layer -- --nocapture --test-threads=1`
  — passed, 3 tests.
- `cargo test -p roastty metal::target -- --nocapture --test-threads=1` —
  passed, 5 tests.
- `git diff --check` — passed.

## Conclusion

Experiment 811 completes the synchronous CALayer/IOSurface presentation wrapper
needed by later live Metal presentation. It intentionally leaves the upstream
custom CALayer subclass, display callback, animation suppression, async
main-thread presentation, and full live frame orchestration for follow-up
experiments.

## Completion Review

Codex reviewed the staged result and approved it with no blocking findings. The
review confirmed that `MetalIOSurfaceLayer` owns a retained `CALayer`, sets
`kCAGravityTopLeft`, exposes the layer for later integration, and faithfully
ports upstream `setSurfaceSync` by directly assigning the IOSurface as layer
contents without main-thread dispatch. It also confirmed that the unsafe
IOSurface-to-`AnyObject` bridge is narrow and backed by the `objc2` feature on
`objc2-io-surface`.

The review also approved the synchronous size-guard behavior and tests: scaled
`bounds * contentsScale` math is covered, matching IOSurfaces are assigned by
identity, mismatched IOSurfaces are rejected, and previous layer contents remain
unchanged after rejection. The README keeps the custom subclass, display
callback, async presentation, animation suppression, and full live orchestration
open for follow-up work.
