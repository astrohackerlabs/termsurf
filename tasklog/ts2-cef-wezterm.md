# CEF + WezTerm Integration

This document explains how CEF (Chromium Embedded Framework) is integrated into our
WezTerm fork for TermSurf 2.0.

## Overview

**What:** TermSurf 2.0 embeds a full Chromium browser inside WezTerm, enabling
terminal panes and browser panes in the same window.

**Why:**
- Cross-platform (Linux, Windows, macOS) vs TermSurf 1.x's macOS-only WKWebView
- Full Chromium API (DevTools, extensions, proper cookie handling)
- Single language (Rust) vs Zig + Swift + Objective-C
- Both WezTerm and cef-rs use wgpu for GPU rendering

**Current status:** CEF loads and initializes inside WezTerm. No browser panes yet -
this is the foundation for that work.

```
[CEF] Framework loaded
[CEF] Initialized successfully
```

## Architecture

```
┌─────────────────────────────────────────┐
│           Rust Application              │
│   (TermSurf-specific UI, integration)   │
├─────────────────────────────────────────┤
│     WezTerm Core        │   CEF (cef-rs)│
│  Terminal emulation     │   Browser     │
│     wgpu rendering      │   OSR mode    │
├─────────────────────────────────────────┤
│              wgpu (unified)             │
├─────────────────────────────────────────┤
│   Metal │ Vulkan │ DX12 │ OpenGL       │
└─────────────────────────────────────────┘
```

CEF runs in Off-Screen Rendering (OSR) mode, rendering to GPU textures that can be
composited with terminal content:

- **macOS:** IOSurface → Metal → wgpu
- **Linux:** DMA-BUF → Vulkan → wgpu
- **Windows:** D3D11 → wgpu

## Key Components

| Component | Location | Purpose |
|-----------|----------|---------|
| `wezterm-gui` | `wezterm-gui/` | Main application, CEF init/shutdown |
| `wezterm-cef-helper` | `wezterm-gui/src/bin/` | CEF subprocess handler |
| `build-debug.sh` | `scripts/` | Build debug bundle with CEF |
| `build-release.sh` | `scripts/` | Build release bundle with CEF |
| `cef-rs` | `../cef-rs/` | Rust bindings for CEF |

## WezTerm Integration Code

### CEF Initialization (`wezterm-gui/src/main.rs`)

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
fn init_cef() -> Result<(), String> {
    use cef::{args::Args, execute_process, initialize, library_loader, App, Settings};

    // Load CEF framework from bundle
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let loader = library_loader::LibraryLoader::new(&exe, false);
    if !loader.load() {
        return Err("Failed to load CEF framework".into());
    }

    // Handle CEF subprocesses (renderer, GPU, etc.)
    let args = Args::new();
    let ret = execute_process(Some(args.as_main_args()), None::<&mut App>, std::ptr::null_mut());
    if ret >= 0 {
        std::process::exit(ret);  // This is a subprocess, exit
    }

    // Create our App with BrowserProcessHandler for message pump integration
    let mut app = cef_integration::create_app();

    // ... execute_process for subprocesses ...

    // Compute path to helper binary (required for CEF to find helpers)
    let helper_path = exe.parent().unwrap().parent().unwrap()
        .join("Frameworks/WezTerm Helper.app/Contents/MacOS/WezTerm Helper");

    let settings = Settings {
        windowless_rendering_enabled: 1,  // OSR mode
        external_message_pump: 1,         // We control the event loop
        no_sandbox: 1,                    // Required on macOS
        browser_subprocess_path: CefString::from(helper_path.to_string_lossy().as_ref()),
        ..Default::default()
    };

    if initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut()) != 1 {
        return Err("CEF initialize failed".into());
    }

    Ok(())
}
```

Called in `main()` after `notify_on_panic()`, before `run()`.

### CEF Shutdown

```rust
// CEF must shut down BEFORE WezTerm's GUI infrastructure.
// CEF shutdown triggers callbacks that require the GUI thread to still be active.
#[cfg(all(target_os = "macos", feature = "cef"))]
cef::shutdown();
```

Called in `main()` BEFORE `Mux::shutdown()` and `frontend::shutdown()`. Order matters -
CEF's shutdown triggers callbacks that expect the GUI thread to still be active.

### CEF Integration Module (`wezterm-gui/src/cef_integration.rs`)

CEF requires a message pump to process its internal work queue. When
`external_message_pump: 1` is set, CEF calls our `on_schedule_message_pump_work`
callback whenever it needs work processed:

```rust
wrap_browser_process_handler! {
    struct WezTermBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            schedule_cef_work(delay_ms);
        }
    }
}
```

We use `CFRunLoopTimer` to schedule `do_message_loop_work()` calls on the main
thread, ensuring CEF's work is processed at the exact times it requests.

### Helper Binary (`wezterm-gui/src/bin/wezterm-cef-helper.rs`)

CEF spawns subprocesses for rendering, GPU, plugins, etc. Each subprocess runs
this helper binary:

```rust
fn main() {
    let args = Args::new();

    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), true);
        assert!(loader.load());
        loader
    };

    execute_process(Some(args.as_main_args()), None::<&mut App>, std::ptr::null_mut());
}
```

### Cargo.toml Changes

```toml
[features]
cef = ["dep:cef"]

[target.'cfg(target_os = "macos")'.dependencies]
cef = { path = "../../cef-rs/cef", optional = true }

[[bin]]
name = "wezterm-cef-helper"
path = "src/bin/wezterm-cef-helper.rs"
required-features = ["cef"]
```

## Building & Running

### Prerequisites

1. Build cef-osr.app (provides CEF framework and helper bundle templates):
   ```bash
   cd ../cef-rs
   cargo build -p cef-osr
   cargo run -p bundle-cef-app -- cef-osr -o cef-osr.app
   ```

### Build with CEF

**Debug build:**
```bash
./scripts/build-debug.sh [--clean] [--open]
```

**Release build:**
```bash
./scripts/build-release.sh [--clean] [--open]
```

Both scripts:
1. Build `wezterm-gui` and `wezterm-cef-helper` with `--features cef`
2. Create bundle from `assets/macos/WezTerm.app` template
3. Copy CEF framework (~200MB) from cef-osr.app
4. Create 5 helper app bundles (Helper, GPU, Renderer, Plugin, Alerts)
5. Add `MallocNanoZone=0` to Info.plist (required for CEF on macOS)
6. Sign the bundle with ad-hoc signature

Flags:
- `--clean` - Clear build caches before building
- `--open` - Open the app after building

### Run

```bash
# Debug
./target/debug/WezTerm.app/Contents/MacOS/wezterm-gui

# Release
./target/release/WezTerm.app/Contents/MacOS/wezterm-gui
```

Expected output:
```
[CEF] Framework loaded
[0117/...WARNING:resource_util.cc:83] Please customize CefSettings.root_cache_path...
[CEF] Initialized successfully
```

## Bundle Structure

```
WezTerm.app/
├── Contents/
│   ├── Info.plist              # Includes LSEnvironment.MallocNanoZone=0
│   ├── MacOS/
│   │   └── wezterm-gui         # Main executable
│   └── Frameworks/
│       ├── Chromium Embedded Framework.framework/  # CEF (~200MB)
│       ├── WezTerm Helper.app/
│       │   └── Contents/MacOS/WezTerm Helper      # wezterm-cef-helper binary
│       ├── WezTerm Helper (GPU).app/
│       ├── WezTerm Helper (Renderer).app/
│       ├── WezTerm Helper (Plugin).app/
│       ├── WezTerm Helper (Alerts).app/
│       ├── libEGL.dylib        # ANGLE (OpenGL ES on Metal)
│       ├── libGLESv1_CM.dylib
│       └── libGLESv2.dylib
```

### Why 5 Helper Apps?

CEF uses a multi-process architecture. Each process type runs as a separate app:

| Helper | Purpose |
|--------|---------|
| Helper | General subprocess |
| Helper (GPU) | GPU compositing |
| Helper (Renderer) | Web page rendering |
| Helper (Plugin) | Browser plugins |
| Helper (Alerts) | System notifications |

All helpers use the same `wezterm-cef-helper` binary - only the bundle name differs.

## cef-rs Modifications

Our cef-rs fork includes fixes required for proper operation. Summary:

| Fix | Issue | Solution |
|-----|-------|----------|
| IOSurface texture import | Metal API type crash | Proper typed references instead of transmute |
| Purple flash | Uninitialized GPU memory | Clear to black before first CEF paint |
| Input handling | No keyboard/mouse | Added event translation to CEF format |
| Multi-browser | Global texture holder | Per-browser texture storage with HashMap |
| Context menu crash | winit NSApplication conflict | Suppress native context menu |
| Event-driven rendering | Continuous polling | Render only when CEF signals new frame |

See `ts2-cef.md` for detailed documentation of each fix.

## Known Issues

### Working
- CEF framework loads successfully
- CEF initializes with OSR settings
- Clean shutdown on app exit
- Helper processes spawn correctly (GPU, renderer, network, etc.)
- Message pump integration via `BrowserProcessHandler` callback

### Not Yet Implemented
- No browser pane creation
- No texture import into WezTerm's render pipeline
- No input routing to CEF browsers

### Platform Support
- **macOS:** Working (current focus)
- **Linux:** Not tested (should work with DMA-BUF path)
- **Windows:** Not tested (needs D3D11 texture sharing)

## Next Steps

1. **Create BrowserPane type** - New pane type that wraps a CEF browser
2. **Texture compositing** - Import CEF textures into wgpu render pipeline
3. **Input routing** - Send keyboard/mouse events to active browser pane
4. **CLI integration** - `web` command to open browser panes

## References

- [cef-mvp.md](cef-mvp.md) - Detailed execution log of the integration steps
- [cef.md](cef.md) - cef-rs modifications and validation
- [termsurf2.md](termsurf2.md) - Overall TermSurf 2.0 architecture
- [CEF Documentation](https://bitbucket.org/chromiumembedded/cef/wiki/Home)
- [cef-rs upstream](https://github.com/tauri-apps/cef-rs)
