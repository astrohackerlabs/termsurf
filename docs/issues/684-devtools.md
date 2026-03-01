# Issue 684: DevTools

Implement Chrome DevTools inside TermSurf. The user opens a Ghostty split pane
and types `web devtools://[pane-id]` to inspect a browser tab. DevTools runs
in-process via `ShellDevToolsBindings` for full element inspection, hover
highlighting, and live DOM manipulation.

## Background

### Previous Research (Issue 648)

Issue 648 analyzed five options for DevTools integration:

- **Option A (native window)** — zero code, but breaks the "never leave the
  terminal" promise.
- **Option B (second overlay)** — DevTools inside the same pane. Requires
  multi-overlay support, input routing between two overlays, and major GUI
  refactoring.
- **Option C (separate pane via HTTP)** — DevTools in a Ghostty split, connected
  via the DevTools HTTP server. Out-of-process: loses hover highlighting.
- **Option D (remote browser)** — open `chrome://inspect` externally. Useful as
  a fallback, not a primary experience.
- **Option E (separate pane, in-process)** — DevTools in a Ghostty split,
  connected in-process via `ShellDevToolsBindings`. Full DevTools experience,
  reuses all existing infrastructure.

Issue 648 concluded Option E is the right approach.

### Why In-Process Matters

Out-of-process DevTools (HTTP/WebSocket) renders a **screenshot** of the page
and overlays a reconstructed DOM layout. Hovering over an element highlights a
region on the screenshot — it cannot draw on the live page because it's in a
different process.

In-process DevTools uses `ShellDevToolsBindings` to connect the frontend
directly to the inspected page's renderer via Mojo. Hover highlighting draws on
the live page in real time. This is the native Chrome DevTools experience.

### Why Separate Pane, Not Same TUI

Putting DevTools inside the same TUI (as a web-split) would require:

- Multi-overlay support in the GUI (Surface.zig assumes one overlay per pane,
  Metal.zig assumes one CALayerHost per surface, xpc.zig routes by pane_id
  assuming one browser per pane)
- Split layout management inside the TUI
- Input routing between two overlays in one pane
- Dual resize coordination

Using a separate Ghostty pane reuses 100% of existing infrastructure: each pane
has its own `web` TUI, its own overlay, its own XPC connection. Ghostty handles
split layout natively. The user can put DevTools on the right, bottom, or in a
separate tab.

### How DevTools Works in Chromium

The DevTools frontend is a web app bundled in the Chromium build, served
locally:

```
http://127.0.0.1:{port}/devtools/devtools_app.html?targetType=tab
```

The connection between a DevTools frontend and its inspected page is set up
entirely in C++, not via the URL. Two DevTools windows loading the identical URL
inspect different pages because each has its own `ShellDevToolsBindings`
pointing to a different `WebContents`.

**ShellDevToolsBindings** (`shell_devtools_bindings.cc:171`) takes two
`WebContents*` pointers:

```cpp
ShellDevToolsBindings(devtools_contents, inspected_contents, delegate)
```

When the frontend DOM loads, `PrimaryMainDocumentElementAvailable()` fires and
calls `Attach()`, which creates a `DevToolsAgentHost` for the inspected
WebContents and connects the Mojo protocol pipes. No Shell window required, no
native window required — just two WebContents pointers and a delegate with a
`Close()` method.

### Chromium Profile Server Changes Needed

The profile server's `CreateTab` method (~200 lines) handles: XPC parsing →
Shell/WebContents creation → persistent compositor setup → ShellTabObserver +
XPC connection → CALayerParams/cursor callbacks → state storage.

A `CreateDevToolsTab` reuses ~90% of this. The only new pieces:

1. **New XPC action `create_devtools_tab`** — receives `inspected_pane_id` (the
   tab to debug) + `devtools_pane_id` (the DevTools pane) + dimensions + dark
2. **Look up inspected WebContents** — walk `tabs_` to find the tab matching
   `inspected_pane_id`
3. **Create ShellDevToolsBindings** — one new line after creating the DevTools
   WebContents: `new ShellDevToolsBindings(devtools_wc, inspected_wc, delegate)`
4. **Load DevTools frontend URL** — instead of the user's URL, load
   `http://127.0.0.1:{port}/devtools/devtools_app.html?targetType=tab` using
   `ShellDevToolsManagerDelegate::GetHttpHandlerPort()`

Everything else — compositor, CALayerParams callback, cursor callback, XPC tab
connection, `tab_ready` message — is identical to `CreateTab`. Estimated new
C++: ~30–50 lines.

### Architecture

```
┌─────────────────────────────────────────────┐
│ Ghostty (terminal emulator)                 │
│                                             │
│  ┌──────────────┐  ┌─────────────────────┐  │
│  │ Pane 1       │  │ Pane 2              │  │
│  │ web foo.com  │  │ web devtools://P1   │  │
│  │              │  │                     │  │
│  │ [webpage]    │  │ [DevTools frontend] │  │
│  └──────┬───────┘  └──────────┬──────────┘  │
│         │                     │             │
└─────────┼─────────────────────┼─────────────┘
          │ XPC                 │ XPC
          ▼                     ▼
┌─────────────────────────────────────────────┐
│ Chromium Profile Server                     │
│                                             │
│  Tab 1: foo.com (WebContents A)             │
│  Tab 2: DevTools (WebContents B)            │
│         ↕ ShellDevToolsBindings ↕           │
│         inspects WebContents A              │
└─────────────────────────────────────────────┘
```

### Flow

1. User opens a Ghostty split pane
2. User types `web devtools://[pane-id]`
3. TUI recognizes `devtools://` scheme, extracts target pane ID
4. TUI sends `create_devtools_tab` XPC to GUI (instead of `create_tab`)
5. GUI forwards to profile server with `inspected_pane_id` + `devtools_pane_id`
6. Profile server finds inspected tab's WebContents, creates DevTools
   WebContents, wires up `ShellDevToolsBindings`, sends back CAContext ID
7. DevTools renders as a normal CALayerHost overlay in the new pane
8. User inspects elements, debugs JS, views network — full DevTools experience

### Open Questions

1. **Pane ID discoverability.** How does the user find the pane ID of the page
   they want to inspect? Options: display in the TUI status bar, copy to
   clipboard via keybinding, or auto-detect the most recent browser pane.

2. **Lifecycle.** What happens when the inspected page closes? The DevTools pane
   should show a "target closed" state or close automatically. What happens when
   the DevTools pane closes? The ShellDevToolsBindings must be cleaned up.

3. **Keyboard shortcut.** Should Cmd+I in the browser pane automatically open a
   Ghostty split with `web devtools://[current-pane-id]`? This requires the GUI
   to spawn a new terminal pane programmatically — possible but a new
   capability.

### Key Source Files

| File                                  | Purpose                                |
| ------------------------------------- | -------------------------------------- |
| `shell_browser_main_parts.cc:216-228` | `create_tab` XPC handler               |
| `shell_browser_main_parts.cc:352-562` | `CreateTab()` method                   |
| `shell_devtools_frontend.cc:39-47`    | `Show()` — creates DevTools Shell      |
| `shell_devtools_bindings.cc:171-182`  | Bindings constructor (two WebContents) |
| `shell_devtools_bindings.cc:216-238`  | `Attach()` — connects DevTools agent   |
| `shell_devtools_manager_delegate.cc`  | HTTP server setup, port query          |
| `shell.cc:411-418`                    | `ShowDevTools()` / `CloseDevTools()`   |

All Chromium paths relative to
`chromium/src/content/chromium_profile_server/browser/` (profile server) or
`chromium/src/content/shell/browser/` (upstream shell).
