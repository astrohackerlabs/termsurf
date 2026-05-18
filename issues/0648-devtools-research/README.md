+++
status = "closed"
opened = "2026-02-26"
closed = "2026-02-26"
+++

# Issue 648: DevTools Research

## Goal

Scope out our options for implementing Chrome DevTools in TermSurf. DevTools is
essential for web developers — inspecting DOM, debugging JavaScript, viewing
network requests, profiling performance. We need to understand how DevTools
works in our Chromium fork and decide the best way to expose it.

## How DevTools works in our fork

### The DevTools HTTP server

Every Chromium profile server already starts a DevTools HTTP server on an
ephemeral localhost port (`shell_devtools_manager_delegate.cc:116-155,167-177`):

```cpp
DevToolsAgentHost::StartRemoteDebuggingServer(
    CreateSocketFactory(), browser_context->GetPath(), base::FilePath());
```

The port is queryable via `ShellDevToolsManagerDelegate::GetHttpHandlerPort()`.
The DevTools frontend is bundled in the Chromium build and served at:

```
http://127.0.0.1:{port}/devtools/devtools_app.html?targetType=tab
```

This server is already running for every profile. We just don't expose it.

### Shell::ShowDevTools()

`shell.cc:411-418` opens DevTools by creating a new native Shell window:

```cpp
void Shell::ShowDevTools() {
    if (!devtools_frontend_) {
        auto* devtools_frontend = ShellDevToolsFrontend::Show(web_contents());
        devtools_frontend_ = devtools_frontend->GetWeakPtr();
    }
    devtools_frontend_->Activate();
}
```

### ShellDevToolsFrontend::Show()

`shell_devtools_frontend.cc:39-46` creates a new Shell window and loads the
frontend URL:

```cpp
Shell* shell = Shell::CreateNewWindow(
    inspected_contents->GetBrowserContext(), GURL(), nullptr, gfx::Size());
ShellDevToolsFrontend* devtools_frontend =
    new ShellDevToolsFrontend(shell, inspected_contents);
shell->LoadURL(GetFrontendURL());
```

The frontend connects to the inspected page via `ShellDevToolsBindings`, which
implements the Chrome DevTools Protocol (CDP) over an internal message pipe.

### How the bindings work

The DevTools frontend URL (`?targetType=tab`) carries no targeting information.
The connection between a DevTools window and its inspected page is set up
entirely in C++, not via the URL.

The flow when "Inspect" is triggered:

1. `Shell::ShowDevTools()` calls `ShellDevToolsFrontend::Show(web_contents())` —
   passing the inspected page's `WebContents*`.
2. `Show()` creates a new Shell window, then constructs
   `ShellDevToolsFrontend(shell, inspected_contents)`.
3. The constructor (`shell_devtools_frontend.cc:69-76`) creates
   `ShellDevToolsBindings` with two `WebContents` pointers:

```cpp
ShellDevToolsFrontend::ShellDevToolsFrontend(
    Shell* frontend_shell, WebContents* inspected_contents)
    : WebContentsObserver(frontend_shell->web_contents()),
      frontend_shell_(frontend_shell),
      devtools_bindings_(
          new ShellDevToolsBindings(frontend_shell->web_contents(),
                                    inspected_contents,
                                    this)) {}
```

4. When the DevTools HTML finishes loading,
   `PrimaryMainDocumentElementAvailable()` fires and calls
   `devtools_bindings_->Attach()`, which connects the DevTools agent for the
   inspected page to this specific frontend via Mojo IPC.

Two DevTools windows load the identical URL but inspect different pages because
each has its own `ShellDevToolsBindings` pointing to a different `WebContents`.

`ShellDevToolsBindings` (`shell_devtools_bindings.h:35-93`) implements
`DevToolsAgentHostClient`, receiving protocol messages from the inspected page's
`DevToolsAgentHost` and forwarding them to the frontend JavaScript via
`DevToolsFrontendHost`. This is the same in-process mechanism that release
Chrome uses — no HTTP, no WebSocket, just Mojo pipes.

**Implication for TermSurf:** When the user presses Cmd+I, the profile server
already knows which tab's `WebContents` to inspect. We create a second
`WebContents` (without a native window), load the DevTools frontend into it,
wire up `ShellDevToolsBindings(devtools_wc, inspected_wc, ...)`, and send its
CAContext ID to the GUI as a second overlay. The URL doesn't matter — the
targeting is entirely in the bindings.

### Keyboard shortcut infrastructure

`shell_browser_main_parts.cc:776-798` already intercepts Cmd+key shortcuts
before forwarding to the renderer:

```cpp
if (has_meta && type != "up") {
    bool handled = true;
    switch (windows_key_code) {
        case ui::VKEY_OEM_4: /* Cmd+[ — back */
        case ui::VKEY_OEM_6: /* Cmd+] — forward */
        case ui::VKEY_R:     /* Cmd+R — reload */
        default: handled = false;
    }
    if (handled) return;
}
```

Adding `VKEY_I` for Cmd+I follows the same pattern.

## Options

### Option A: Native window

Call `Shell::ShowDevTools()` as-is. DevTools opens in a regular macOS window
outside the terminal.

**Pros:**

- Zero new code. One line in the `HandleKeyEvent` switch.
- Full DevTools experience — resizable, dockable panels, all features work.
- Doesn't interfere with the overlay pipeline.

**Cons:**

- Breaks the "never leave the terminal" promise. DevTools floats as a separate
  window.
- No keyboard shortcut to return focus to the terminal. User must click.

### Option B: Second overlay

Create DevTools as a second CALayerHost overlay in the same terminal pane. Split
the viewport vertically (content top, DevTools bottom) or horizontally.

**Pros:**

- DevTools stays inside the terminal pane. True integration.
- Could share the existing overlay architecture.

**Cons:**

- Major new infrastructure: split layout management, input routing between two
  overlays, resize coordination.
- The GUI currently assumes one overlay per pane. Significant refactoring.
- DevTools panels (Elements, Console, Network, etc.) need full keyboard/mouse
  interaction — all of which must be forwarded separately to the DevTools
  WebContents.

### Option C: Separate terminal pane

Open DevTools in a new Ghostty split pane. The DevTools frontend is already
served via HTTP — just navigate a second `web` TUI to the DevTools URL.

**Pros:**

- Reuses all existing infrastructure: `web` TUI, overlay pipeline, XPC, input
  forwarding.
- Ghostty handles the split layout natively. No new layout management code.
- DevTools stays in the terminal. User can resize splits with Ghostty
  keybindings.
- Each pane is independent. No input routing complexity.

**Cons:**

- Requires a way to discover the DevTools HTTP port. The Chromium server knows
  it, but the TUI and GUI don't. Need an XPC message to query it.
- The DevTools URL includes a `?ws=` parameter targeting a specific page. Need
  to construct the full URL with the correct WebSocket debugger endpoint.
- Two `web` TUIs means two overlays, two Chromium profile server connections.
  The DevTools frontend runs in the same Chromium process (same profile server)
  but as a separate tab.

### Option D: Remote DevTools in external browser

Expose the DevTools HTTP server port and let the user open `chrome://inspect` or
`http://localhost:{port}` in their regular browser.

**Pros:**

- Zero GUI/TUI changes. Just document how to connect.
- Full DevTools experience in a real browser.
- Works today if the user passes `--remote-debugging-port=9222` to the Chromium
  server.

**Cons:**

- Requires leaving the terminal (defeats the purpose).
- Manual setup. Not discoverable.
- Useful as a fallback, not as the primary experience.

## Testing DevTools locally

Content Shell is already built and can be used to see how DevTools works with
our Chromium fork.

### Launch Content Shell with remote debugging

```bash
"/Users/ryan/dev/termsurf/chromium/src/out/Default/Content Shell.app/Contents/MacOS/Content Shell" \
  --remote-debugging-port=9222 \
  '--remote-allow-origins=*' \
  https://example.com
```

This opens a Content Shell window with a URL bar and the webpage.

### Open DevTools (in-process)

Right-click inside the Content Shell window and select "Inspect". This opens
DevTools in a new Content Shell window using `ShellDevToolsFrontend::Show()`,
which sets up `ShellDevToolsBindings` for a direct in-process connection. All
DevTools features work: element inspection, hover highlighting, DOM
manipulation, network panel, etc.

### Open DevTools (external browser)

Navigate to `http://127.0.0.1:9222` in an external browser. This lists
inspectable targets. Click one to open the DevTools frontend.

**Requires `--remote-allow-origins=*`** — without this flag, the DevTools server
rejects WebSocket connections from external origins and the frontend shows
"WebSocket disconnected" immediately.

### In-process vs out-of-process DevTools

When DevTools runs out-of-process (in an external browser), the element
inspection experience is degraded. The DevTools frontend renders a
**screenshot** of the page and overlays a reconstructed DOM layout on top of it.
Hovering over an element in the Elements panel highlights a region on this
screenshot — it cannot draw the blue/green overlay directly on the real page
because it's in a different process.

When DevTools runs in-process (Content Shell's "Inspect" or via
`ShellDevToolsBindings`), the DevTools agent communicates directly with the
renderer. Hovering over an element draws the highlight overlay directly on the
live page in real time. This is the native Chrome DevTools experience.

**This confirms that TermSurf must use in-process DevTools.** The second overlay
approach (Option B) is correct — DevTools needs to run inside the same Chromium
profile server process, connected via `ShellDevToolsBindings`, rendered as a
second CALayerHost overlay. Out-of-process DevTools (Option D) loses the element
hover highlighting and other features that require direct renderer access.

**Note:** The Chromium Profile Server (`Chromium Profile Server.app`) is
headless — it starts a server with no UI and idles until it receives XPC
messages. Use Content Shell for manual DevTools testing.

## Key files

| File                                                      | Purpose                              |
| --------------------------------------------------------- | ------------------------------------ |
| `chromium/.../shell_browser_main_parts.cc:761-876`        | `HandleKeyEvent()` — Cmd+key switch  |
| `chromium/.../shell_devtools_frontend.h`                  | DevTools window creation             |
| `chromium/.../shell_devtools_frontend.cc:39-46`           | `Show()` — creates Shell + loads URL |
| `chromium/.../shell_devtools_manager_delegate.cc:116-177` | HTTP server setup, port query        |
| `chromium/.../shell.cc:411-418`                           | `ShowDevTools()` / `CloseDevTools()` |
| `chromium/.../shell_devtools_bindings.h`                  | CDP bindings (frontend ↔ inspected)  |
| `gui/src/apprt/xpc.zig:1051-1108`                         | `sendKeyEvent()` — key forwarding    |
| `gui/src/Surface.zig:2740-2747`                           | Ctrl+Esc interception                |

## Option E: `devtools://` protocol in a separate pane

Instead of cramming two overlays into one pane, use the terminal's native pane
support. The user opens a Ghostty split and types:

```
web devtools://[pane-id]
```

The `web` TUI recognizes the `devtools://` scheme, connects to the same profile
server via XPC, and the profile server creates a DevTools WebContents with
`ShellDevToolsBindings` pointing at the inspected tab. The DevTools WebContents
renders as a normal CALayerHost overlay in the new pane — identical to how a
regular webpage renders today.

**Pros:**

- **Zero new layout code.** No split viewports, no dock positions, no resize
  coordination. Ghostty handles pane splits natively.
- **Zero new input routing.** Each pane has its own `web` TUI, its own overlay,
  its own XPC connection. No ambiguity about which overlay receives input.
- **Full in-process DevTools.** The DevTools WebContents runs inside the same
  Chromium profile server as the inspected page. `ShellDevToolsBindings`
  provides the direct Mojo connection. Hover highlighting works on the live
  page.
- **Reuses everything.** Same `web` TUI, same overlay pipeline, same XPC
  protocol, same CALayerHost compositing. The only new code is URL scheme
  recognition and a new XPC action.
- **Flexible layout.** The user can put DevTools on the right, bottom, or any
  Ghostty split configuration. They can resize it with Ghostty keybindings. They
  can even move it to a different tab or window.

**Cons:**

- Each pane needs an ID that the user can reference. The TUI must display or
  make discoverable the pane ID of the browser it's inspecting.
- Two `web` TUIs share one profile server — the server must support a
  `create_devtools_tab` action that creates a DevTools WebContents for an
  existing tab instead of a new browsing tab.

### How it would work

1. **Pane ID assignment.** Each `web` TUI already has a `TERMSURF_PANE_ID` from
   the environment. This ID is sent to the profile server via `set_overlay`. The
   browser pane displays its ID somewhere accessible (status bar, or a
   keybinding to copy it).

2. **`web devtools://[pane-id]`** — the TUI parses the URL scheme:
   - Recognizes `devtools://` as a special scheme.
   - Sends a new XPC action (`create_devtools_tab`) to the profile server with
     the target pane ID.
   - The profile server looks up the target tab's `WebContents`, creates a new
     `WebContents` for the DevTools frontend, wires up `ShellDevToolsBindings`,
     and returns a CAContext ID.
   - The DevTools overlay renders in the new pane like any other page.

3. **Profile server changes.** New `create_devtools_tab` XPC action:
   - Receives: `pane_id` (the DevTools pane) + `target_pane_id` (the page to
     inspect).
   - Creates a `WebContents` via `WebContents::Create()` (no native window).
   - Loads the DevTools frontend URL from the built-in HTTP server.
   - Creates `ShellDevToolsBindings(devtools_wc, target_wc, ...)`.
   - Attaches a `TabObserver` and sends the CAContext ID back to the GUI.

4. **Lifecycle.** Closing the DevTools pane closes the `web devtools://` TUI,
   which drops the XPC connection, which cleans up the DevTools WebContents and
   bindings. Closing the inspected page should also close or disconnect the
   DevTools pane gracefully.

## Conclusion

Option E (`devtools://` protocol in a separate pane) is the best approach. It
reuses the terminal's native pane management instead of reinventing split
viewports inside the TUI. The profile server already supports multiple tabs per
process — a DevTools tab is just another tab with `ShellDevToolsBindings`
instead of a URL navigation.

Open questions before implementation:

- **Pane ID discoverability.** How does the user find the pane ID of the page
  they want to inspect? Options: display in the status bar, copy to clipboard
  via keybinding, or auto-open DevTools for the most recent browser pane.
- **Shortcut UX.** Should there be a keybinding (Cmd+I) that automatically opens
  a Ghostty split and runs `web devtools://[current-pane-id]`? This would
  require the GUI to spawn a new terminal pane programmatically.
- **Lifecycle edge cases.** What happens when the inspected page navigates? When
  it closes? When the profile server restarts?

Shelving this issue to think through the UX details. The architecture is clear —
`devtools://` protocol, same profile server, `ShellDevToolsBindings`, separate
pane.
