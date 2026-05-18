+++
status = "closed"
opened = "2026-02-05"
closed = "2026-02-05"
+++

# TermSurf 3.0: Electron FrameSinkVideoCapturer Study

## Problem

TermSurf's browser renders at ~20fps instead of 60fps. Scrolling, mouse
interactions, and animations are visibly laggy compared to native Chrome. Users
expect browser-quality smoothness; we're delivering slideshow-quality
choppiness.

The goal: achieve 60fps rendering with no perceptible lag.

## What We Learned from Issue 338

Issue 338 documented five experiments attempting to fix the lag through CEF
configuration. All failed. The key findings:

### CEF's Architecture is the Problem

The throttling occurs in `CefCopyFrameGenerator::GenerateCopyFrame()`:

```cpp
if (frame_in_progress_)
    return;  // Discards >66% of frames
```

This is baked into CEF's code. No setting, flag, or API call can bypass it:

| Experiment | Approach                       | Result                                      |
| ---------- | ------------------------------ | ------------------------------------------- |
| 1          | Cache IOSurface texture        | Disproven — import is fast (0.37ms)         |
| 2          | Investigate frame pacing       | Found CEF's 30fps cap                       |
| 3          | `multi_threaded_message_loop`  | Failed — incompatible with OSR on macOS     |
| 4          | `external_begin_frame_enabled` | Partial — reduced stutters, still ~20fps    |
| 5          | Chrome command-line flags      | Marginal — flags don't affect OSR code path |

### The Proven Solution Exists

Electron achieves **240fps** GPU-accelerated off-screen rendering. They hit the
same CEF limitations we did, then bypassed them entirely by using a different
Chromium API: `FrameSinkVideoCapturer`.

|                  | CEF (current)            | Electron                 |
| ---------------- | ------------------------ | ------------------------ |
| Capture API      | `OnAcceleratedPaint`     | `FrameSinkVideoCapturer` |
| Frame throttling | Hard-coded 30fps cap     | None                     |
| Texture sharing  | `shared_texture_enabled` | `kGpuMemoryBuffer`       |
| Measured fps     | ~20                      | 240+                     |

This isn't a hack or workaround — it's the architecturally correct solution that
ships in production today.

## Why Mimic Electron

### It's Proven at Scale

Electron powers VS Code, Slack, Discord, Figma, and thousands of other apps.
Their GPU-accelerated OSR implementation handles billions of renders daily. When
they say it works at 240fps, it's battle-tested.

### Same Constraints, Same Solution

Electron faced identical constraints:

- Need GPU-accelerated rendering (can't use software fallback)
- Need shared textures for compositor integration
- Need high frame rates for smooth UX
- CEF's APIs weren't sufficient

They solved it. We should learn from their solution rather than reinvent it.

### Clear Implementation Path

[Electron PR #42953](https://github.com/electron/electron/pull/42953) provides a
complete reference implementation. The code is open source. We can study exactly
how they:

- Set up `FrameSinkVideoCapturer`
- Configure `kGpuMemoryBuffer` for zero-copy texture sharing
- Integrate with their compositor
- Handle frame pacing

### Lower Risk Than Alternatives

| Alternative           | Risk                                        |
| --------------------- | ------------------------------------------- |
| Fork CEF              | Maintenance burden of entire CEF codebase   |
| Use OBS fork          | May not have macOS support, different goals |
| Use Chromium directly | Enormous complexity, no abstraction layer   |
| **Mimic Electron**    | **Well-documented, proven, focused scope**  |

## Architecture Overview

### Current: CEF OnAcceleratedPaint

```
CEF Browser
    │
    ▼
CefCopyFrameGenerator
    │ (throttled to ~30fps)
    ▼
OnAcceleratedPaint callback
    │
    ▼
IOSurface → Mach port → GUI
```

### Target: FrameSinkVideoCapturer

```
Chromium Browser
    │
    ▼
viz::FrameSinkVideoCapturer
    │ (no throttle)
    ▼
GPU Memory Buffer callback
    │
    ▼
IOSurface → Mach port → GUI
```

The key difference: `FrameSinkVideoCapturer` operates at the compositor level,
capturing frames directly from the GPU without the frame-dropping logic in CEF's
`CefCopyFrameGenerator`.

## Key Resources

### Electron Implementation

- [PR #42953: GPU OSR with FrameSinkVideoCapturer](https://github.com/electron/electron/pull/42953)
- [Electron OSR documentation](https://www.electronjs.org/docs/latest/tutorial/offscreen-rendering)

### Chromium APIs

- `viz::FrameSinkVideoCapturer` — Frame capture from compositor
- `media::VideoFrame` — Frame container with GPU memory buffer
- `gpu::GpuMemoryBufferHandle` — Cross-process texture handle

### CEF Reference (what we're replacing)

- `CefRenderHandler::OnAcceleratedPaint` — Current callback
- `CefCopyFrameGenerator::GenerateCopyFrame` — The throttling code
- [CEF Issue #1368: OSR frame rate limit](https://bitbucket.org/chromiumembedded/cef/issues/1368)

### Previous Work

- [Issue 338: Browser lag investigation](./338-lag.md) — Full context on why CEF
  doesn't work

## Success Criteria

| Metric                 | Current | Target        |
| ---------------------- | ------- | ------------- |
| Frame rate             | ~20fps  | 60fps         |
| Frame interval         | 50ms    | 16ms          |
| Stutter frames (>50ms) | 14%     | <1%           |
| Scrolling feel         | Laggy   | Chrome-smooth |
| Input latency          | ~100ms  | <32ms         |

## Timeline

This is a significant undertaking. Rough phases:

1. **Research**: Understand Electron's approach, choose integration strategy
2. **Prototype**: Prove feasibility with standalone demo
3. **Integrate**: Bring into TermSurf
4. **Polish**: Production-ready quality

Each phase should be completed before starting the next. Findings from earlier
phases may change the approach for later ones.

## Ideas for Future Experiments

These ideas may become experiments after we complete the initial research. What
we learn from studying Electron's implementation will inform which approaches
are viable.

- **Assess integration options** — Evaluate paths: patch CEF, use libcef +
  Chromium APIs, embed Electron, or custom Chromium embedding
- **Prototype FrameSinkVideoCapturer** — Standalone proof-of-concept that
  captures frames and renders via wgpu
- **Integrate into TermSurf** — Replace CEF's `OnAcceleratedPaint` with the new
  capture API in termsurf-profile
- **Optimize and polish** — Frame pacing, vsync alignment, edge cases, production
  quality

## Experiments

### Experiment 1: Study Electron's Implementation

**Status:** COMPLETE

**Goal:** Understand exactly how Electron implements `FrameSinkVideoCapturer`
and what Chromium APIs it uses.

**Local repo:** `/electron/` (cloned for local study, gitignored)

**Tasks:**

1. Study [PR #42953](https://github.com/electron/electron/pull/42953) using local
   source code:
   - Identify the key files implementing GPU OSR
   - Understand what Chromium headers are included
   - Document how the capturer is configured

2. Read the key source files in `/electron/shell/browser/osr/`:
   - `osr_video_consumer.cc` — Frame capture consumer
   - `osr_host_display_client.cc` — Display client integration
   - `osr_render_widget_host_view.cc` — Render widget host

3. Study the key Chromium classes (referenced in Electron code):
   - `viz::FrameSinkVideoCapturer` — What is it? How is it created?
   - `media::VideoFrame` — How are frames delivered?
   - `kGpuMemoryBuffer` — How does zero-copy work on macOS?

4. Trace the data flow through the code:
   - Browser renders to compositor
   - Compositor → FrameSinkVideoCapturer
   - Capturer → VideoFrame callback
   - VideoFrame → texture extraction
   - Texture → application

5. Document dependencies:
   - What Chromium components are required?
   - Are there macOS-specific APIs?
   - What's the minimum Chromium version?

**Deliverable:** Architecture document explaining Electron's approach in detail.

---

#### Findings

##### Key Source Files

Electron's OSR implementation lives in `/electron/shell/browser/osr/`:

| File | Purpose |
| ---- | ------- |
| `osr_video_consumer.cc` | Implements `viz::mojom::FrameSinkVideoConsumer`, receives captured frames |
| `osr_render_widget_host_view.cc` | Creates the video capturer, manages the render widget |
| `osr_paint_event.h` | Defines `OffscreenSharedTextureValue` struct for frame data |
| `osr_host_display_client_mac.mm` | macOS-specific IOSurface handling |
| `README.md` | Excellent documentation of the entire architecture |

##### How Electron Creates the Capturer

In `OffScreenVideoConsumer` constructor (`osr_video_consumer.cc:37-73`):

```cpp
video_capturer_(view->CreateVideoCapturer()) {
  video_capturer_->SetAutoThrottlingEnabled(false);  // No throttling!
  video_capturer_->SetMinSizeChangePeriod(base::TimeDelta());
  video_capturer_->SetFormat(format);
  video_capturer_->SetAnimationFpsLockIn(false, 1);  // Prevent stutter
  video_capturer_->SetResolutionConstraints(
      gfx::Size(1, 1),
      gfx::Size(media::limits::kMaxDimension, media::limits::kMaxDimension),
      false);
  SetFrameRate(view_->frame_rate());
}
```

Key configuration:
- `SetAutoThrottlingEnabled(false)` — Disables frame rate throttling
- `SetAnimationFpsLockIn(false, 1)` — Prevents animation-based frame dropping
- No resolution constraints — Adapts to any window size

##### Starting Capture with GPU Texture Sharing

In `OffScreenVideoConsumer::SetActive()` (`osr_video_consumer.cc:77-87`):

```cpp
video_capturer_->Start(
    this,
    view_->offscreen_use_shared_texture()
        ? viz::mojom::BufferFormatPreference::kPreferMappableSharedImage
        : viz::mojom::BufferFormatPreference::kDefault);
```

The `kPreferMappableSharedImage` preference tells Chromium to use GPU memory
buffers (IOSurface on macOS, D3D11 texture on Windows).

##### Frame Delivery Flow

1. **Chromium captures frame** → `FrameSinkVideoCapturerImpl` creates a
   `GpuMemoryBuffer` via `GmbVideoFramePoolContext`

2. **GPU process creates texture** → Platform-specific (IOSurface on macOS)

3. **Frame delivered to consumer** → `OnFrameCaptured()` callback with
   `media::mojom::VideoBufferHandlePtr`

4. **Extract platform handle** (`osr_video_consumer.cc:104-141`):

```cpp
// macOS
texture.shared_texture_handle =
    reinterpret_cast<uintptr_t>(gmb_handle.io_surface().get());
```

5. **Application imports texture** — Using `IOSurfaceRef` directly

##### macOS IOSurface Handling

In `osr_host_display_client_mac.mm`, Electron shows how to import IOSurface:

```cpp
base::apple::ScopedCFTypeRef<IOSurfaceRef> io_surface(
    IOSurfaceLookupFromMachPort(ca_layer_params.io_surface_mach_port.get()));
void* pixels = static_cast<void*>(IOSurfaceGetBaseAddress(io_surface.get()));
size_t stride = IOSurfaceGetBytesPerRow(io_surface.get());
```

##### Frame Pool Architecture

From `README.md`: The capturer uses a pool of **10 frames**
(`kFramePoolCapacity`). This is critical for high frame rates:

- CEF: Single frame, blocks if previous frame not consumed
- Electron: 10-frame pool, can queue frames without blocking

##### Key Chromium Dependencies

| Component | Header | Purpose |
| --------- | ------ | ------- |
| `viz::ClientFrameSinkVideoCapturer` | `components/viz/host/client_frame_sink_video_capturer.h` | Creates and manages the capturer |
| `viz::mojom::FrameSinkVideoConsumer` | `services/viz/privileged/mojom/compositing/frame_sink_video_capture.mojom` | Interface for receiving frames |
| `gfx::GpuMemoryBufferHandle` | `ui/gfx/gpu_memory_buffer.h` | Cross-process texture handle |
| `media::VideoFrame` | `media/base/video_frame.h` | Frame container |

##### Why This Works (and CEF Doesn't)

| Aspect | CEF | Electron |
| ------ | --- | -------- |
| Capture point | `CefCopyFrameGenerator` | `FrameSinkVideoCapturerImpl` |
| Frame dropping | `if (frame_in_progress_) return;` | 10-frame pool, no dropping |
| Throttling | Hard-coded in OSR code | `SetAutoThrottlingEnabled(false)` |
| API level | CEF abstraction layer | Direct Chromium viz API |

##### Minimum Requirements

- **Chromium version:** 134+ (based on README.md timestamp)
- **macOS APIs:** IOSurface, Mach ports
- **Build dependency:** Must link against Chromium's viz and media components

##### Critical Insight: Electron Bypasses CEF Entirely

Electron doesn't use CEF at all. It embeds Chromium directly and accesses
`viz::ClientFrameSinkVideoCapturer` through Chromium's internal APIs. This means:

1. **We cannot add this to CEF easily** — CEF would need to expose these APIs
2. **Patching CEF is significant work** — Would need to add new callback type
3. **Best path: Use Chromium directly** — Either via Electron or custom embedding

##### Recommendation

Based on this research, the viable options are:

1. **Embed Electron** — Use Electron's proven OSR implementation directly
2. **Fork Electron's approach** — Extract the OSR code and adapt for TermSurf
3. **Patch CEF** — Add `FrameSinkVideoCapturer` support (significant C++ work)

Option 1 (Embed Electron) is the lowest risk since it's already working.
Option 2 requires understanding Chromium's build system.
Option 3 requires maintaining a CEF fork.

---

## Conclusion: The CEF Impasse

**We have reached a fundamental impasse with CEF.**

The evidence is now conclusive:

1. **Issue 338** — Five experiments confirmed CEF's frame throttling is hard-coded
   in `CefCopyFrameGenerator::GenerateCopyFrame()`. No configuration can bypass it.

2. **Experiment 1** — Electron achieves 240fps by using Chromium's
   `FrameSinkVideoCapturer` API directly. This API is *internal to Chromium* and
   not exposed by CEF.

3. **Steam's trajectory** — Valve hit the same wall and spent years migrating
   from CEF to direct Chromium embedding. Steam is now effectively its own
   browser platform.

### The Ceiling is Architectural, Not Configurational

CEF was designed as a simplified embedding layer. It deliberately hides
Chromium's internal compositor APIs behind abstractions like `OnAcceleratedPaint`.
The frame-dropping logic is part of that abstraction — CEF's authors considered
it a feature, not a bug.

The `FrameSinkVideoCapturer` API that Electron uses is internal to Chromium's
`viz` layer. CEF does not expose it, and adding support would require forking
CEF itself.

### The Steam Lesson

Steam's migration away from CEF is instructive:

> *"Use CEF until you are forced not to. The day CEF blocks your vision is the
> day you earn the pain."*

Steam used CEF to ship fast (2010–2017), then gradually replaced it with direct
Chromium embedding (2017–2020) once their UI became mission-critical. Today,
Steam is effectively its own browser platform.

This trajectory is rare. Most apps never outgrow CEF. But TermSurf has hit the
wall: browser-quality smoothness is core to the product, and CEF cannot deliver it.

### Paths Forward

| Path | Effort | Risk | Maintenance |
| ---- | ------ | ---- | ----------- |
| **Patch CEF** | High | Medium | Fork forever |
| **Use OBS's CEF fork** | Medium | Unknown | Depends on OBS |
| **Embed Electron** | Medium | Low | Electron updates |
| **Embed Chromium directly** | Very High | High | Chromium updates |

### Recommendation

**Embed Electron** is the recommended path forward.

Rationale:
- Proven 240fps GPU-accelerated OSR in production
- Active maintenance by a large team
- Well-documented APIs (`webPreferences.offscreen`)
- Lower risk than patching CEF or embedding Chromium directly
- Electron's OSR code can serve as reference even if we later diverge

The day CEF blocks your vision is the day you earn the pain. That day has arrived.
