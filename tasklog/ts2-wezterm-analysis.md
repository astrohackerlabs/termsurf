# TermSurf 2.0 Architecture

TermSurf 2.0 is built on **WezTerm + cef-rs** for cross-platform terminal-browser
integration.

## Executive Summary

The WezTerm + cef-rs approach offers significant advantages over TermSurf 1.x:

- Single language (Rust) vs three (Zig + Swift + Objective-C)
- True cross-platform (Linux, Windows, macOS) vs macOS-only
- Full browser API (CEF) vs limited (WKWebView)
- Both projects use wgpu for GPU rendering, enabling clean integration
- cef-rs already has working OSR (Off-Screen Rendering) with hardware
  acceleration

## TermSurf 1.x Architecture (Context)

### Stack

```
┌─────────────────────────────────────────┐
│           Swift UI Layer                │  termsurf-macos/ (~33k lines)
│   (WebViewOverlay, ControlBar, etc.)    │
├─────────────────────────────────────────┤
│           WKWebView (WebKit)            │  Apple's WebKit framework
├─────────────────────────────────────────┤
│         libghostty (Zig)                │  src/ (~213k lines)
│   Terminal emulation, GPU rendering     │
├─────────────────────────────────────────┤
│      Metal (macOS GPU)                  │
└─────────────────────────────────────────┘
```

### Strengths

- Working product (TermSurf 1.0 released)
- Ghostty is high-quality terminal emulator
- WKWebView is lightweight and native

### Weaknesses

- **macOS only** - No path to Linux/Windows without rewrite
- **Limited browser API** - WKWebView lacks:
  - Proper visited link handling
  - Full cookie/storage control
  - Extension support
  - DevTools API (only Safari Web Inspector)
  - Robust download handling
- **Three languages** - Zig + Swift + Objective-C increases complexity
- **No upstream path** - TermSurf changes unlikely to merge into Ghostty

## TermSurf 2.0 Architecture

### Stack

```
┌─────────────────────────────────────────┐
│           Rust Application              │
│   (TermSurf-specific UI, integration)   │
├─────────────────────────────────────────┤
│     WezTerm Core        │   CEF (cef-rs)│  Both render to wgpu textures
│  Terminal emulation     │   Browser     │
│     wgpu rendering      │   OSR mode    │
├─────────────────────────────────────────┤
│              wgpu (unified)             │  WebGPU abstraction
├─────────────────────────────────────────┤
│   Metal │ Vulkan │ DX12 │ OpenGL       │  Platform GPU APIs
└─────────────────────────────────────────┘
```

### Key Insight: Shared GPU Path

Both WezTerm and cef-rs use **wgpu** for GPU rendering:

- WezTerm: `wgpu = "28"` for terminal rendering
- cef-rs: `wgpu = "28"` for CEF texture import

These versions are now aligned (see [WezTerm Fork Modifications](#wezterm-fork-modifications)).

CEF's accelerated OSR mode renders to shared textures:

- **macOS**: IOSurface → Metal → wgpu
- **Linux**: DMA-BUF → Vulkan → wgpu
- **Windows**: D3D11 → Vulkan/DX12 → wgpu

This means we can composite terminal and browser content in a unified GPU
pipeline.

## WezTerm Analysis

### Codebase Stats

- 452 Rust files, ~410k lines
- Mature, feature-rich terminal emulator
- Active development, good community

### Key Components

| Component       | Purpose                           | Files                            |
| --------------- | --------------------------------- | -------------------------------- |
| `wezterm-gui/`  | Main GUI application              | termwindow, rendering            |
| `mux/`          | Multiplexer (tabs, panes, splits) | pane.rs (~29k), tab.rs (~85k)    |
| `window/`       | Cross-platform windowing          | macos/, windows/, x11/, wayland/ |
| `termwiz/`      | Terminal emulation library        | VT parsing, cell representation  |
| `wezterm-font/` | Font rendering                    | HarfBuzz, FreeType integration   |

### Rendering Pipeline

1. Terminal content → glyph atlas → wgpu textures
2. WebGPU shader (`shader.wgsl`) composites glyphs
3. Platform backend (Metal/Vulkan/DX12/OpenGL) presents

### Pane System

The `Pane` trait (`mux/src/pane.rs:167`) is terminal-oriented but extensible:

```rust
pub trait Pane: Downcast + Send + Sync {
    fn pane_id(&self) -> PaneId;
    fn get_cursor_position(&self) -> StableCursorPosition;
    fn get_lines(&self, lines: Range<StableRowIndex>) -> (StableRowIndex, Vec<Line>);
    fn resize(&self, size: TerminalSize) -> anyhow::Result<()>;
    fn key_down(&self, key: KeyCode, mods: KeyModifiers) -> anyhow::Result<()>;
    fn mouse_event(&self, event: MouseEvent) -> anyhow::Result<()>;
    // ... many more terminal-specific methods
}
```

A browser pane would need a different rendering path, not terminal lines.

### Platform Support

| Platform | Windowing    | GPU            |
| -------- | ------------ | -------------- |
| macOS    | Cocoa        | Metal          |
| Linux    | X11, Wayland | OpenGL, Vulkan |
| Windows  | Win32        | DX12, OpenGL   |

## cef-rs Analysis

### Codebase Stats

- CEF version: 143.7.0 (recent Chromium)
- Full CEF API bindings (~2.3MB per platform)
- Active Tauri project maintenance

### Key Components

| Component             | Purpose                                  |
| --------------------- | ---------------------------------------- |
| `cef/`                | High-level Rust API                      |
| `sys/`                | Low-level FFI bindings                   |
| `osr_texture_import/` | GPU texture sharing                      |
| `examples/osr/`       | Working hardware-accelerated OSR example |

### OSR (Off-Screen Rendering) Pipeline

```rust
// From examples/osr/src/webrender.rs
fn on_accelerated_paint(
    &self,
    _browser: Option<&mut Browser>,
    type_: PaintElementType,
    _dirty_rects: Option<&[Rect]>,
    info: Option<&AcceleratedPaintInfo>,
) {
    let shared_handle = SharedTextureHandle::new(info);
    let texture = shared_handle.import_texture(&device)?;
    // texture is now a wgpu::Texture ready for rendering
}
```

### Platform-Specific Texture Import

| Platform | Mechanism            | File           |
| -------- | -------------------- | -------------- |
| macOS    | IOSurface → Metal    | `iosurface.rs` |
| Linux    | DMA-BUF → Vulkan     | `dmabuf.rs`    |
| Windows  | D3D11 shared texture | `d3d11.rs`     |

### CEF Multi-Process Model

CEF uses multiple processes (browser, renderer, GPU, etc.):

```rust
// Main process check
let is_browser_process = cmd.has_switch(Some(&"type".into())) != 1;
let ret = execute_process(Some(args), Some(&mut app), sandbox_info);
if is_browser_process {
    // Initialize CEF, create browser windows
} else {
    // Subprocess exits after execute_process
}
```

### Browser API Coverage

CEF provides full Chromium API including:

- Navigation, history, cookies, storage
- JavaScript execution and message passing
- DevTools protocol
- Extensions (limited)
- Downloads, uploads, permissions
- Certificate handling
- Print preview
- All HTML5 features

## Integration Strategy

### Phase 1: Fork WezTerm ✓

- Fork WezTerm as TermSurf base
- Remove/disable features not needed (SSH multiplexing, etc.)
- Understand rendering pipeline

### Phase 2: Add CEF Integration (In Progress)

- Add cef-rs dependency
- Create `BrowserPane` type (not implementing terminal `Pane` trait)
- Implement CEF OSR handlers
- Import CEF textures into wgpu pipeline

### Phase 3: Unified Compositor

- Modify WezTerm's render pass to support mixed pane types
- Terminal panes: existing glyph rendering
- Browser panes: CEF texture blit
- Handle pane splitting between types

### Phase 4: CLI Integration

- Implement `web` command similar to TermSurf 1.0
- Console bridging (stdout/stderr routing)
- JavaScript API (`window.termsurf.exit()`)

### Phase 5: Polish

- Profile isolation
- Bookmarks
- DevTools integration
- Platform-specific packaging

## WezTerm Fork Modifications

This section tracks all modifications made to our WezTerm fork (ts2/) to facilitate
merging upstream changes in the future.

### Dependency Alignment

These dependencies were upgraded to align with cef-rs versions:

| Dependency | Original | Updated | Reason |
| ---------- | -------- | ------- | ------ |
| wgpu | 25.0.2 | 28 | Match cef-rs for GPU texture sharing |
| thiserror | 1.0 | 2 | Match cef-rs |
| libloading | 0.8 | 0.9 | Match cef-rs |
| objc2 | 0.6 | 0.6.3 | Match cef-rs |
| objc2-foundation | 0.3 | 0.3.2 | Match cef-rs |

#### wgpu Upgrade Details

The wgpu upgrade required code changes across multiple files:

**25 → 26:**
- Added `depth_slice: None` to `RenderPassColorAttachment` in `draw.rs`

**26 → 27:**
- Removed lifetime from `BufferViewMut` in `renderstate.rs`
- Added `experimental_features` field to `DeviceDescriptor` in `webgpu.rs`

**27 → 28:**
- Made `enumerate_adapters` calls async (now returns a future)
- Made `compute_compatibility_list` function async in `webgpu.rs`
- Wrapped `enumerate_adapters` in `smol::block_on` for Lua `enumerate_gpus` function
- Added `Surface<'_>` lifetime parameter
- Renamed `push_constant_ranges` to `immediate_size` in `PipelineLayoutDescriptor`
- Renamed `multiview` to `multiview_mask` in `RenderPipelineDescriptor`
- Changed `mipmap_filter` type from `FilterMode` to `MipmapFilterMode`
- Used `..Default::default()` for `RenderPassDescriptor` optional fields

### Deferred Dependency Updates

These dependencies have version mismatches but are deferred:

| Dependency | WezTerm | cef-rs | Reason for Deferral |
| ---------- | ------- | ------ | ------------------- |
| syn | 1.0 | 2 | Major API changes in proc-macros; doesn't affect runtime |
| windows | 0.33.0 | 0.62 | Windows-only; cannot test on macOS |

### Feature Additions

**`web-open` CLI command:**
- Added PDU (Protocol Data Unit) plumbing for browser pane creation
- Preparatory work for CEF integration

### Files Modified

Key files changed from upstream WezTerm:

| File | Changes |
| ---- | ------- |
| `Cargo.toml` | Dependency version updates |
| `wezterm-gui/src/termwindow/webgpu.rs` | wgpu 28 API changes |
| `wezterm-gui/src/termwindow/render/draw.rs` | wgpu 26+ API changes |
| `wezterm-gui/src/renderstate.rs` | wgpu 27 buffer lifetime changes |
| `wezterm-gui/src/scripting/mod.rs` | Async enumerate_adapters wrapper |

## Code Changes Required

### WezTerm Modifications

**New pane type** (`src/browser_pane.rs`):

```rust
pub struct BrowserPane {
    id: PaneId,
    browser: cef::Browser,
    texture: RefCell<Option<wgpu::Texture>>,
    size: RefCell<Size>,
}

impl BrowserPane {
    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        // Blit CEF texture to render target
    }
}
```

**Render pipeline modification** (`wezterm-gui/src/termwindow/render/`):

```rust
// In render loop
match pane {
    PaneType::Terminal(term_pane) => {
        // Existing terminal rendering
        self.render_terminal_pane(term_pane, ...);
    }
    PaneType::Browser(browser_pane) => {
        // New browser rendering
        browser_pane.render(encoder, target);
    }
}
```

**Event routing**:

```rust
// In input handling
if let Some(browser_pane) = self.get_active_browser_pane() {
    // Route keyboard/mouse to CEF
    browser_pane.browser.host().send_key_event(...);
    browser_pane.browser.host().send_mouse_event(...);
}
```

### CEF Setup

**Initialization** (in main or startup):

```rust
fn init_cef() {
    let args = cef::args::Args::new();
    let settings = cef::Settings {
        windowless_rendering_enabled: true,
        external_message_pump: true, // Integrate with WezTerm event loop
        ..Default::default()
    };
    cef::initialize(Some(args.as_main_args()), Some(&settings), ...);
}
```

**Message pump integration**:

```rust
// In WezTerm's main event loop
loop {
    // Process WezTerm events
    process_wezterm_events();

    // Pump CEF messages
    cef::do_message_loop_work();

    // Render frame
    render();
}
```

## Risk Assessment

### Technical Risks

| Risk                       | Likelihood | Impact | Mitigation                                |
| -------------------------- | ---------- | ------ | ----------------------------------------- |
| ~~wgpu version mismatch~~  | ~~Medium~~ | ~~Medium~~ | ✓ Resolved - both now use wgpu 28     |
| CEF message pump conflicts | Medium     | High   | Study WezTerm event loop, prototype early |
| Performance overhead       | Low        | Medium | CEF OSR is hardware-accelerated           |
| CEF binary size (~100MB)   | Certain    | Low    | Accept as tradeoff for full browser       |
| Cross-platform CEF quirks  | Medium     | Medium | Test on all platforms early               |

### Project Risks

| Risk                         | Likelihood | Impact | Mitigation                        |
| ---------------------------- | ---------- | ------ | --------------------------------- |
| Large codebase to understand | Certain    | Medium | Start with minimal changes        |
| Upstream WezTerm changes     | Medium     | Low    | Periodic merge, maintain fork     |
| cef-rs API changes           | Low        | Medium | Pin versions, contribute upstream |

## Comparison: 1.x vs 2.0

| Aspect                 | TermSurf 1.x (Ghostty) | TermSurf 2.0 (WezTerm) |
| ---------------------- | ---------------------- | ---------------------- |
| **Languages**          | Zig + Swift + ObjC     | Rust                   |
| **Platforms**          | macOS only             | Linux, Windows, macOS  |
| **Browser API**        | Limited (WKWebView)    | Full Chromium (CEF)    |
| **Terminal quality**   | Excellent              | Excellent              |
| **GPU rendering**      | Metal only             | wgpu (all backends)    |
| **Codebase size**      | ~246k lines            | ~410k lines            |
| **Binary size**        | ~20MB                  | ~150MB+ (with CEF)     |
| **Community**          | Ghostty growing        | WezTerm established    |

## References

- WezTerm: https://github.com/wezterm/wezterm
- cef-rs: https://github.com/tauri-apps/cef-rs
- CEF Documentation: https://bitbucket.org/chromiumembedded/cef/wiki/Home
- wgpu: https://wgpu.rs/

---

## Appendix: Superseded Approaches

### CEF + Zig (Abandoned)

Before choosing WezTerm + cef-rs, we explored integrating CEF directly into
Ghostty's Zig codebase. This approach was abandoned due to complexity.

**Why Swift + CEF Failed**

CEF has a C API, but integrating it with Swift failed due to struct marshalling:

- Swift class memory layout doesn't match what CEF expects
- The CEF C-to-C++ wrapper validates struct sizes and rejects Swift-created
  structs
- See `docs/ts2-cef.md` for detailed documentation of the Swift integration
  challenges

**Why Zig + CEF Was Considered**

Zig doesn't have Swift's marshalling problems:

- Direct C interop with zero overhead
- Exact memory layout control
- Proven pattern (Ghostty already calls Objective-C APIs from Zig)

**Why We Chose WezTerm Instead**

- Single language (Rust) vs two (Zig + Swift)
- cef-rs already has working OSR with hardware acceleration
- WezTerm is already cross-platform
- Less integration work than adding CEF to Ghostty

### CEF Handler Reference

For reference, CEF requires implementing these handler structs:

| Handler                    | Purpose                       |
| -------------------------- | ----------------------------- |
| `cef_app_t`                | Application lifecycle         |
| `cef_client_t`             | Browser event routing         |
| `cef_life_span_handler_t`  | Browser creation/destruction  |
| `cef_render_handler_t`     | Off-screen rendering          |
| `cef_display_handler_t`    | Console messages, title       |
| `cef_request_handler_t`    | Navigation, downloads         |
| `cef_context_menu_handler` | Context menu (suppress/custom)|

These are implemented in Rust via cef-rs rather than manually in Zig.
