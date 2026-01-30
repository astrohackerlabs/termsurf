# TermSurf

A terminal emulator with integrated browser panes.

## Project Structure

TermSurf is transitioning from version 1.x to 2.0:

| Version | Base | Browser | Platforms |
|---------|------|---------|-----------|
| **1.x** (`ts1/`) | Ghostty fork | WKWebView | macOS only |
| **2.0** (planned) | WezTerm fork | CEF via cef-rs | Linux, macOS, Windows |

```
termsurf/
├── ts1/           # TermSurf 1.x (Ghostty + WKWebView)
├── ts2/           # TermSurf 2.0 (WezTerm fork, in progress)
├── cef-rs/        # CEF Rust bindings (Chromium browser)
└── docs/issues/          # Documentation
```

## TermSurf 1.x

Current stable version. macOS terminal emulator with WKWebView browser panes.

```bash
cd ts1
./scripts/build-debug.sh --open
```

See `ts1/README.md` for details.

## TermSurf 2.0

Cross-platform rewrite using WezTerm + CEF. In progress.

### cef-rs Status

CEF (Chromium Embedded Framework) integration has been validated:

| Feature | Status |
|---------|--------|
| IOSurface texture import (macOS) | Working |
| Input handling (keyboard, mouse, scroll) | Working |
| Multiple browser instances | Working |
| Resize handling | Working |
| Context menu (right-click) | Suppressed (winit compatibility) |
| Fullscreen | Broken with winit (defer to WezTerm) |

```bash
# Build and run the OSR example
cd cef-rs
cargo build -p cef-osr
cargo run -p bundle-cef-app -- cef-osr -o cef-osr.app
./cef-osr.app/Contents/MacOS/cef-osr
```

### Next Steps

Fork WezTerm and add CEF browser panes using the validated cef-rs integration.

## Documentation

- `docs/issues/ts2-cef.md` - CEF integration details and validation results
- `docs/issues/ts2-wezterm-analysis.md` - WezTerm architecture analysis
- `AGENTS.md` - Development guide for coding agents

## License

See individual component licenses in `ts1/`, `ts2/`, and `cef-rs/`.
