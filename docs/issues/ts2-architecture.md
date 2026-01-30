# TermSurf Architecture

This document explains the architectural decisions behind TermSurf, including
the evolution from 1.x to 2.0.

## Project Evolution

### TermSurf 1.x (Stable, macOS-only)

TermSurf 1.x is built on **Ghostty + WKWebView**:
- **Terminal:** Ghostty (Zig core + Swift macOS app)
- **Browser:** Apple's WKWebView (WebKit)
- **Platform:** macOS only

This architecture works and is stable, but has fundamental limitations that led
to the 2.0 redesign.

### TermSurf 2.0 (In Development, Cross-platform)

TermSurf 2.0 is built on **WezTerm + cef-rs**:
- **Terminal:** WezTerm (Rust)
- **Browser:** CEF via cef-rs (Chromium)
- **Platforms:** macOS, Linux, Windows

See [termsurf2-wezterm-analysis.md](termsurf2-wezterm-analysis.md) for the full
architecture analysis.

## Why We're Moving to 2.0

### WKWebView Limitations (1.x)

WKWebView is lightweight and native, but has critical limitations for a
"terminal that's also a real browser":

1. **Incomplete API** - WKWebView lacks:
   - Proper visited link styling (requires private API workarounds)
   - Full cookie/storage control
   - Extension support
   - Chrome DevTools (only Safari Web Inspector)
   - Robust download handling
   - Many other browser features standard in Chromium

2. **macOS-only** - No path to Linux/Windows without a complete rewrite.
   WebKitGTK exists for Linux but has different APIs and quirks.

3. **Apple controls the roadmap** - We can't add features Apple doesn't expose.

### CEF Advantages (2.0)

CEF (Chromium Embedded Framework) via cef-rs provides:

1. **Complete browser** - Full Chromium with all standard browser features
2. **Cross-platform** - Same code works on macOS, Linux, and Windows
3. **Chrome DevTools** - Full debugging capabilities
4. **Consistent behavior** - Same rendering engine everywhere

### WezTerm Advantages (2.0)

WezTerm provides:

1. **Single language** - Pure Rust vs Zig + Swift + Objective-C
2. **Cross-platform** - Already works on macOS, Linux, Windows
3. **wgpu rendering** - Same GPU abstraction as cef-rs, enabling clean compositor
4. **Active community** - Well-maintained, feature-rich terminal

## Architecture Comparison

| Aspect | 1.x (Ghostty + WKWebView) | 2.0 (WezTerm + cef-rs) |
|--------|---------------------------|------------------------|
| Languages | Zig + Swift + Objective-C | Rust |
| Platforms | macOS only | macOS, Linux, Windows |
| Browser API | Limited (WKWebView) | Complete (Chromium) |
| DevTools | Safari Web Inspector | Chrome DevTools |
| GPU | Metal only | wgpu (Metal/Vulkan/DX12) |
| Binary size | ~20MB | ~150MB (includes CEF) |

---

# TermSurf 1.x Architecture

The remainder of this document describes the 1.x architecture in detail.

## Requirements

TermSurf has two primary requirements:

1. **Browser as a pane**: Display web content in terminal panes, not as separate windows
2. **CLI-first**: Invoke browser via command line (`web open`), not GUI

## Stack

```
┌─────────────────────────────────────────┐
│           Swift UI Layer                │  termsurf-macos/
│   (WebViewOverlay, ControlBar, etc.)    │
├─────────────────────────────────────────┤
│           WKWebView (WebKit)            │  Apple's WebKit framework
├─────────────────────────────────────────┤
│         libghostty (Zig)                │  ts1/src/
│   Terminal emulation, GPU rendering     │
├─────────────────────────────────────────┤
│      Metal (macOS GPU)                  │
└─────────────────────────────────────────┘
```

## Browser Integration (WKWebView)

For 1.x, we use Apple's native WKWebView:

**Why WKWebView for 1.x?**
- **Zero dependencies**: Built into macOS, no additional frameworks
- **Native Swift integration**: Seamless API, no C marshalling
- **Profile isolation**: `WKWebsiteDataStore(forIdentifier:)` on macOS 14+
- **Console capture**: JS injection for console.log/error interception
- **DevTools**: Safari Web Inspector available

**Trade-offs accepted**:
- WebKit only (not Chromium)
- No Chrome DevTools (Safari Web Inspector instead)
- Console capture requires JS injection (not native callback)
- Limited browser API (see "Why We're Moving to 2.0" above)

## CLI-App Communication

### The Problem

TermSurf needs a way for CLI tools (`web open`, etc.) to communicate with
the running TermSurf app to control browser panes.

### Solution: Unix Domain Sockets

We use Unix domain sockets for CLI-to-app communication:

```
┌─────────────────────────────────────────────────────────────┐
│ TermSurf App                                                │
│                                                             │
│  ┌──────────────┐         ┌─────────────────┐              │
│  │ SocketServer │◄────────│ CommandHandler  │              │
│  └──────┬───────┘         └────────┬────────┘              │
│         │                          │                        │
│  ┌──────▼───────────────────────────────────────────────┐  │
│  │ Terminal Pane (shell with env vars)                  │  │
│  │   TERMSURF_SOCKET=/tmp/termsurf-12345.sock           │  │
│  │   TERMSURF_PANE_ID=pane-abc-123                      │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              ▲
                              │ Unix Socket (JSON)
                    ┌─────────┴─────────┐
                    │ web CLI           │
                    └───────────────────┘
```

### Why Unix Sockets Over OSC Escape Sequences?

We considered OSC escape sequences (like iTerm2 and Kitty use) but chose Unix
domain sockets for these reasons:

| Aspect | OSC Escape Sequences | Unix Domain Sockets |
|--------|---------------------|---------------------|
| **libghostty changes** | Required (fork) | **None** |
| **Bidirectional** | No | **Yes** |
| **Protocol** | String parsing | **Structured JSON** |
| **Robustness** | Broken by pipes | **Always works** |
| **Blocking** | Not possible | **By default** |

**Key advantages:**

1. **No libghostty modification** - All code lives in `termsurf-macos/`
2. **Bidirectional** - CLI can receive responses and events
3. **Robust** - Works regardless of stdout redirection or piping
4. **Structured** - JSON protocol avoids escaping issues

### Protocol

```json
// Request (CLI → App)
{"id": "1", "action": "open", "paneId": "abc-123", "data": {"url": "https://..."}}

// Response (App → CLI)
{"id": "1", "status": "ok", "data": {"webviewId": "wv-456"}}

// Event (App → CLI, when webview closes)
{"id": "1", "event": "closed", "data": {"exitCode": 0}}
```

### Environment Variables

When TermSurf spawns a shell, it sets:
- `TERMSURF_SOCKET` - Path to the Unix domain socket
- `TERMSURF_PANE_ID` - Unique identifier for this pane

These are inherited by all child processes, allowing the CLI tool to discover
the socket path and identify which pane it's running in.

## SplitTree Architecture

Ghostty's macOS app uses a binary tree for pane layout:

```swift
// Ghostty's SplitTree (Sources/Features/Splits/SplitTree.swift)
indirect enum Node: Codable {
    case leaf(view: ViewType)  // ViewType = terminal surface
    case split(Split)
}

struct Split {
    let direction: Direction  // horizontal or vertical
    let ratio: Double
    let left: Node
    let right: Node
}
```

### TermSurf Extension

We extend this to support multiple pane types:

```swift
// TermSurf modification
enum PaneContent {
    case terminal(TerminalSurfaceView)
    case browser(WebViewOverlay)  // WKWebView-based
}

indirect enum Node: Codable {
    case leaf(pane: PaneContent)
    case split(Split)
}
```

This allows:
- Same SplitTree logic for layout
- Same focus navigation (ctrl+h/j/k/l)
- Terminal and browser panes are peers

## Console Bridging

When a browser pane is active, JavaScript console output appears in the terminal
where `web open` was run. See [console.md](console.md) for full details.

### Implementation

Console output flows through the socket connection to the blocking CLI:

1. **JavaScript injection** - Console methods are overridden to capture output
2. **Swift handler** - `WKScriptMessageHandler` receives messages
3. **Socket event** - Swift sends `{"event":"console","data":{"level":"log","message":"..."}}` to CLI
4. **CLI output** - CLI writes to stdout (log/info) or stderr (warn/error)

```
Browser console.log() → Swift → Socket → CLI stdout → Terminal
```

This approach avoids direct PTY access and leverages the existing socket infrastructure.

### JavaScript API

With the `--js-api` flag, pages can programmatically control the webview:

```javascript
window.termsurf.webviewId  // Unique webview ID
window.termsurf.exit(0)    // Close with exit code 0
window.termsurf.exit(1)    // Close with exit code 1
```

The exit code is passed through the socket to the CLI, which exits with that code.

## File Structure (1.x)

```
termsurf/
├── ts1/                          # TermSurf 1.x (Ghostty-based)
│   ├── src/                      # libghostty (Zig) - shared core
│   │   └── cli/web.zig           # CLI web command
│   ├── macos/                    # Original Ghostty macOS app
│   └── termsurf-macos/           # TermSurf macOS app
│       └── Sources/
│           ├── App/              # App delegate, main entry
│           ├── Features/
│           │   ├── Splits/       # SplitTree (extended for browser panes)
│           │   ├── Terminal/     # Terminal views
│           │   ├── Socket/       # Unix domain socket server
│           │   └── WebView/      # WKWebView implementation
│           └── Ghostty/          # Ghostty integration
├── ts2/                          # TermSurf 2.0 (WezTerm-based)
├── cef-rs/                       # CEF Rust bindings
└── docs/                         # Documentation
```

## Related Documentation

### TermSurf 1.x
- [console.md](console.md) - Console bridging and JS API
- [keybindings.md](keybindings.md) - Keyboard shortcuts
- [webview.md](webview.md) - WKWebView implementation details
- [libghostty.md](libghostty.md) - Changes to libghostty

### TermSurf 2.0
- [termsurf2-wezterm-analysis.md](termsurf2-wezterm-analysis.md) - Architecture analysis
- [cef-rs.md](cef-rs.md) - Our cef-rs modifications
