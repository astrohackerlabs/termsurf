# CEF Integration

This document covers the Chromium Embedded Framework (CEF) integration for
TermSurf 2.0 browser panes, using the cef-rs Rust bindings.

## Overview

**Status:** CEF integration validated via cef-rs. WezTerm integration pending.

TermSurf 2.0 integrates CEF via **cef-rs** (Rust bindings) into a WezTerm fork:

- Full Chromium browser capabilities
- Cross-platform (macOS, Linux, Windows)
- Chrome DevTools support
- Rust has predictable C-compatible memory layouts (unlike Swift)

See [termsurf2.md](termsurf2.md) for the full architecture.

### Why cef-rs?

cef-rs was imported into the TermSurf monorepo at `cef-rs/` because:

- Provides complete CEF API bindings for Rust
- Has working OSR (Off-Screen Rendering) with hardware acceleration
- Uses wgpu for GPU rendering (same as WezTerm)
- Maintained by the Tauri project

### CEF Resources

- [CEF Builds](https://cef-builds.spotifycdn.com/index.html) - Official binary
  distributions
- [CEF Wiki](https://bitbucket.org/chromiumembedded/cef/wiki/Home) - General
  usage guide
- [cef-rs upstream](https://github.com/tauri-apps/cef-rs) - Original Rust
  bindings

## Validation Status

The OSR example (`cef-rs/examples/osr/`) serves as our validation testbed:

| Feature                              | Status     | Commit      |
| ------------------------------------ | ---------- | ----------- |
| IOSurface texture import (macOS)     | Working    | `d8b58edea` |
| Purple flash fix                     | Working    | `e6f8a2e4c` |
| Input handling (keyboard, mouse)     | Working    | `88ab04355` |
| Multi-browser instances              | Working    | `40f2a55cc` |
| Context menu suppression             | Working    | `25def7592` |
| Resize handling                      | Working    | —           |
| Performance (event-driven rendering) | Working    | `150cb7775` |
| Fullscreen                           | Broken     | winit issue |

**Key validation:** Multiple CEF browser instances run successfully in a single
process with independent texture storage and event routing. This is critical for
WezTerm integration where browser panes coexist with terminal panes.

## Modifications

### 1. Initial Import (`5075cc44c`)

Moved cef-rs files into `cef-rs/` folder for TermSurf integration.

---

### 2. Fix macOS IOSurface Texture Import Crash (`d8b58edea`)

**File:** `cef-rs/cef/src/osr_texture_import/iosurface.rs`

**Problem:** The original code used `std::mem::transmute` to cast raw pointers
to Metal API types, causing crashes at memory address 0x1f00000080.

**Root cause:** Transmuting raw device/descriptor pointers to
`&metal::NSObject` references was incorrect. The Metal-rs crate expects properly
typed references that implement the `Message` trait for Objective-C message
sending.

**Fix:** Replace unsafe transmutes with proper typed references via the objc
crate:

```rust
// Before (crashed):
let texture: metal::Texture = std::mem::transmute(objc::msg_send![
    std::mem::transmute::<_, &metal::NSObject>(raw_device),
    newTextureWithDescriptor:std::mem::transmute::<_, &metal::NSObject>(metal_desc.as_ptr())
    iosurface:self.handle
    plane:0usize
]);

// After (working):
let device_ref: &metal::DeviceRef = raw_device;
let desc_ref: &metal::TextureDescriptorRef = metal_desc.as_ref();
let texture: metal::Texture = objc::msg_send![
    device_ref,
    newTextureWithDescriptor:desc_ref
    iosurface:self.handle
    plane:0usize
];
```

**Additional fixes:**

- Added IOSurface validation via C functions (`IOSurfaceGetWidth`,
  `IOSurfaceGetHeight`)
- Removed `metal_desc.set_storage_mode()` call - Metal determines storage mode
  from the IOSurface itself
- Added dimension mismatch warnings

---

### 3. Fix Purple Flash on Startup (`e6f8a2e4c`)

**Files:** `cef-rs/examples/osr/src/main.rs`,
`cef-rs/cef/src/osr_texture_import/iosurface.rs`

**Problem:** Uninitialized GPU memory displayed as purple/magenta color before
CEF rendered its first frame.

**Fix:** Clear the render pass to black before any CEF content:

```rust
ops: wgpu::Operations {
    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
    store: wgpu::StoreOp::Store,
}
```

---

### 4. Add Input Handling (`88ab04355`)

**File:** `cef-rs/examples/osr/src/main.rs`

**Problem:** The OSR example had no input handling - browsers were
non-interactive.

**Added:**

- Mouse move, click, drag events
- Mouse wheel scrolling
- Keyboard input with proper key codes
- Modifier key tracking (Shift, Ctrl, Alt, Cmd)
- Mouse button state tracking

**Key implementation details:**

1. **CEF event flags** for modifier/button state:

   ```rust
   const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
   const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
   const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
   const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;
   const EVENTFLAG_MIDDLE_MOUSE_BUTTON: u32 = 1 << 5;
   const EVENTFLAG_RIGHT_MOUSE_BUTTON: u32 = 1 << 6;
   const EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;
   ```

2. **Platform-specific native key codes:**

   - macOS: Uses native key codes (0x00-0x7E range)
   - Windows/Linux: Uses Windows Virtual Key codes (0x08-0x5A range)

3. **Text input:** Sends `CHAR` events after `KEYDOWN` for actual character
   input

---

### 5. Window Config Cleanup (`c4bbf909d`)

**File:** `cef-rs/examples/osr/src/main.rs`

**Changes:**

- Added descriptive window titles: `format!("CEF Browser - {}", url)`
- Set explicit default size: `LogicalSize::new(800.0, 600.0)`
- Documented fullscreen limitation (winit issue, not cef-rs)

---

### 6. Add Multi-Browser Instance Support (`40f2a55cc`)

**Files:** `cef-rs/examples/osr/src/main.rs`,
`cef-rs/examples/osr/src/webrender.rs`

**Problem:** Original code used a global `thread_local!` texture holder, meaning
only one browser could render at a time.

**Solution:** Per-browser texture storage with HashMap-based window management.

**Architecture:**

```rust
/// Per-browser instance state
struct BrowserInstance {
    state: State,                    // wgpu rendering state
    browser: cef::Browser,           // CEF browser handle
    size: Rc<RefCell<LogicalSize>>,  // Shared with RenderHandler
    texture_holder: TextureHolder,   // Per-instance texture storage
    cursor_pos: (f64, f64),          // Mouse position for this window
    closing: bool,
}

/// Application manages multiple browser windows
struct App {
    instances: HashMap<WindowId, BrowserInstance>,
    key_modifiers: u32,   // Shared modifier state
    mouse_buttons: u32,   // Shared button state
    urls_to_open: Vec<&'static str>,
}
```

**Key changes to `webrender.rs`:**

```rust
/// Return type includes per-browser texture holder
pub struct RenderHandlerParts {
    pub handler: OsrRenderHandler,
    pub size: Rc<RefCell<LogicalSize<f32>>>,
    pub texture_holder: Rc<RefCell<Option<wgpu::BindGroup>>>,
}

/// Each RenderHandler stores to its own texture_holder
impl OsrRenderHandler {
    // In on_accelerated_paint:
    *self.handler.texture_holder.borrow_mut() = Some(bind_group);
}
```

**Event routing:** All window events are routed by `WindowId` to the correct
browser instance.

---

### 7. Suppress Context Menu to Prevent Crash (`25def7592`)

**File:** `cef-rs/examples/osr/src/webrender.rs`

**Problem:** Right-clicking triggered CEF to display a native context menu,
which called `NSApplication.isHandlingSendEvent` - a method that winit's
NSApplication subclass doesn't implement, causing a crash.

**Fix:** Implement `ContextMenuHandler` that clears the menu model before
display:

```rust
#[derive(Clone)]
pub struct OsrContextMenuHandler {}

wrap_context_menu_handler! {
    pub(crate) struct ContextMenuHandlerBuilder {
        handler: OsrContextMenuHandler,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            // Clear the menu model to suppress the context menu
            if let Some(model) = model {
                model.clear();
            }
        }
    }
}
```

**Integration:** Added `context_menu_handler()` to the `Client` implementation:

```rust
wrap_client! {
    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> { ... }
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(self.context_menu_handler.clone())
        }
    }
}
```

---

### 8. Remove Fixed 16ms Sleep (`790a38aa1`)

**File:** `cef-rs/examples/osr/src/main.rs`

**Problem:** The event loop had a hardcoded 16ms sleep after each iteration,
adding unnecessary latency to every frame.

**Root cause:** The sleep was likely added as a simple frame rate limiter, but
it caused input lag because:

1. User types/scrolls → event processed
2. Sleep 16ms (unnecessary delay)
3. Next frame renders

**Fix:** Remove the sleep entirely and use a minimal 1ms timeout for
`pump_app_events`:

```rust
// Before:
let timeout = Some(Duration::ZERO);
let status = event_loop.pump_app_events(timeout, &mut app);
sleep(Duration::from_millis(16));

// After:
let timeout = Some(Duration::from_millis(1));
let status = event_loop.pump_app_events(timeout, &mut app);
// No sleep - let the event loop handle timing
```

---

### 9. Add Event-Driven Rendering (`150cb7775`)

**Files:** `cef-rs/examples/osr/src/main.rs`,
`cef-rs/examples/osr/src/webrender.rs`

**Problem:** Even after removing the sleep, there was still perceptible lag. The
render loop was continuously calling `request_redraw()` and
`send_external_begin_frame()`, rendering frames whether or not CEF had new
content.

**Root cause:** The continuous rendering approach meant:

1. We'd often render the same frame multiple times
2. We'd sometimes render one frame behind (CEF paints, we render old, then
   render new)
3. CPU/GPU constantly busy even when nothing changed

**Solution:** Event-driven rendering - only render when CEF signals a new frame
is ready.

**Architecture:**

```
┌─────────────────────────────────────────────────────────────┐
│                      Event Loop                              │
│                                                              │
│  ┌──────────────┐    UserEvent::FrameReady    ┌──────────┐  │
│  │ CEF paints   │ ─────────────────────────▶  │ user_    │  │
│  │ new frame    │      (via proxy)            │ event()  │  │
│  └──────────────┘                             └────┬─────┘  │
│         │                                          │        │
│         ▼                                          ▼        │
│  ┌──────────────┐                          ┌──────────────┐ │
│  │ Store texture│                          │request_redraw│ │
│  │ in holder    │                          │ for window   │ │
│  └──────────────┘                          └──────┬───────┘ │
│                                                   │         │
│                                                   ▼         │
│                                            ┌──────────────┐ │
│                                            │ Redraw       │ │
│                                            │ Requested    │ │
│                                            │ → render()   │ │
│                                            └──────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

**Key changes:**

1. **UserEvent enum** for cross-context signaling:

   ```rust
   #[derive(Debug, Clone)]
   pub enum UserEvent {
       FrameReady(WindowId),
   }
   ```

2. **EventLoopProxy** stored in RenderHandler:

   ```rust
   pub struct OsrRenderHandler {
       // ... existing fields ...
       proxy: Arc<EventLoopProxy<UserEvent>>,
       window_id: WindowId,
   }
   ```

3. **Signal on paint** - when CEF finishes painting, notify the event loop:

   ```rust
   // In on_accelerated_paint, after storing texture:
   let _ = self.handler.proxy.send_event(
       UserEvent::FrameReady(self.handler.window_id)
   );
   ```

4. **Handle user events** - request redraw only when signaled:

   ```rust
   fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
       match event {
           UserEvent::FrameReady(window_id) => {
               if let Some(instance) = self.instances.get(&window_id) {
                   instance.state.get_window().request_redraw();
               }
           }
       }
   }
   ```

5. **Disable external_begin_frame** - CEF now drives its own frame timing:

   ```rust
   let window_info = WindowInfo {
       external_begin_frame_enabled: false as _,  // Was: accelerated_osr as _
       // ...
   };
   ```

6. **Simplify RedrawRequested** - just render, no continuous loop:

   ```rust
   WindowEvent::RedrawRequested => {
       // Render only when requested (triggered by CEF frame events)
       instance.state.render(&instance.texture_holder);
       // No more: request_redraw() or send_external_begin_frame()
   }
   ```

**Additional fix:** Added `no_sandbox: true` to Settings to resolve helper
process sandbox issues on macOS.

## Files Modified

| File                                     | Lines Changed | Purpose                                        |
| ---------------------------------------- | ------------- | ---------------------------------------------- |
| `cef/src/osr_texture_import/iosurface.rs`| ~66           | Metal API type fix, IOSurface validation       |
| `examples/osr/src/main.rs`               | ~650          | Input, multi-browser, event-driven rendering   |
| `examples/osr/src/webrender.rs`          | ~100          | Per-browser textures, context menu, signaling  |

## Files NOT Modified

- `cef-rs/sys/` - Low-level CEF C API bindings
- `cef-rs/cef/src/` - Core library (except iosurface.rs)
- `cef-rs/examples/cefsimple/` - Other examples
- Build system, CI, documentation

## Running the Validation Example

```bash
cd cef-rs
cargo build -p cef-osr
cargo run -p bundle-cef-app -- cef-osr -o cef-osr.app
./cef-osr.app/Contents/MacOS/cef-osr
```

The example opens two browser windows (github.com and google.com) to validate
multi-browser support. Test by:

- Interacting with each window independently (typing, clicking, scrolling)
- Closing one window and verifying others continue working
- Resizing windows

## Known Issues

### Fullscreen Broken

Fullscreen mode crashes due to a winit issue with NSApplication event handling.
This is deferred to WezTerm integration, which uses its own windowing system.

### Clippy Warnings

The objc crate generates `unexpected_cfgs` warnings for `cargo-clippy` feature
checks. Suppressed with `#![allow(unexpected_cfgs)]` in iosurface.rs.

### macOS: Multiple Browsers Fail When Launched from Terminal

**Status:** Investigating

**Problem:** On macOS, CEF only supports one browser instance when the app is
launched directly from terminal (`./binary`). Subsequent browsers fail silently
(blank screen, no errors). However, when launched via `open app.app`
(LaunchServices), multiple browsers work correctly.

**Affected:**

- cef-rs OSR example: Second browser window doesn't load when run from terminal
- WezTerm + CEF: Can only open one browser; reopening or opening in another pane
  fails

**Symptoms:**

- First browser loads and renders correctly
- Second browser (simultaneous or sequential) shows blank screen
- No error messages - silent failure
- `on_paint` callback either never fires or fires after browser is already
  closed

**Reproduction:**

```bash
# FAILS - only first browser works
./cef-osr.app/Contents/MacOS/cef-osr

# WORKS - both browsers work
open cef-osr.app
```

**Root Cause (Hypothesis):** When launched via `open`, macOS LaunchServices:

- Registers the app as a foreground GUI application
- Properly connects the app to WindowServer
- Sets up NSApplication with correct activation policy

When launched directly from terminal, CEF's message pump or browser management
may not function correctly because the process isn't properly registered as a
GUI app.

**Fix Status:**

- **cef-rs OSR example:** FIXED (commit `b4f4bbab5`). Adding
  `NSApp().setActivationPolicy_(NSApplicationActivationPolicyRegular)` before
  CEF initialization allows multiple browsers to work from terminal.
- **WezTerm + CEF:** NOT YET FIXED. The same fix was attempted but produced
  inconsistent results (debug builds can't load any browsers, release builds
  load only one). WezTerm's more complex initialization may require additional
  work. Deferred for now.

**Workaround:** Use `open WezTerm.app` for development and testing. This works
reliably with multiple browsers.

## Next Steps

These modifications validate that cef-rs is ready for WezTerm integration. The
patterns established here (per-browser texture storage, input routing, context
menu suppression) should transfer directly to the BrowserPane implementation in
ts2.

See [termsurf2.md](termsurf2.md) for the integration roadmap.

---

## Appendix: Historical Approaches

### Swift Integration (Abandoned)

We attempted to integrate CEF directly with Swift but hit fundamental issues
with struct marshalling. CEF's C API wrapper validates struct layouts, and
Swift's class memory model doesn't produce the expected layouts.

**The Core Problem**

When passing a `cef_app_t` struct to `cef_initialize()`, CEF's validation
failed:

```
[FATAL:cef/libcef_dll/ctocpp/app_ctocpp.cc:118] CefApp_0_CToCpp called with invalid version -1
```

CEF reads `base.size` from the struct pointer and validates it matches the
expected size (80 bytes for `cef_app_t`). Swift's memory layout for classes
doesn't match what CEF expects.

**Approaches Tried**

1. **Direct struct allocation** - Failed validation
2. **CEF.swift marshaller pattern** - Embedded C struct as first property of
   Swift class at offset 16. Still failed with modern Swift.
3. **Global @convention(c) functions** - Avoided closure capture issues but
   didn't fix validation.

**Why Rust Works**

Rust structs have `#[repr(C)]` which guarantees C-compatible memory layout.
cef-rs uses this to create CEF structs that pass validation. This is why we
moved to cef-rs instead of continuing to fight Swift's memory model.

**References**

- [CEF Forum: CefApp_0_CToCpp invalid version](https://magpcss.org/ceforum/viewtopic.php?f=6&t=19114)
- [CEF.swift](https://github.com/aspect-apps/aspect/tree/main/aspect-platform/aspect-platform-cef/aspect-platform-cef-swift) -
  Historical Swift bindings (circa 2016, no longer maintained)

### Zig Integration (Superseded)

We originally planned to integrate CEF directly into Ghostty's Zig codebase. Zig
has predictable C-compatible memory layouts and Ghostty already calls
Metal/Objective-C from Zig.

This approach was superseded by WezTerm + cef-rs because:

- WezTerm is already pure Rust (single language)
- cef-rs already handles all CEF struct marshalling
- Both WezTerm and cef-rs use wgpu for GPU rendering

See the appendix in [termsurf2.md](termsurf2.md) for more details on the
superseded Zig approach.
