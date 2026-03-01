# Issue 684: DevTools

Implement Chrome DevTools inside TermSurf. The user opens a Ghostty split pane
and types `web devtools://[tabId]` to inspect a browser tab. DevTools runs
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

### Tab IDs

Each profile server assigns an auto-incrementing integer ID to every browser tab
it creates. The ID is stable for the lifetime of the tab — it never changes and
is never reused during the server's lifetime. This gives users a human-readable
identifier for targeting DevTools instead of a UUID.

**DevTools tabs do not get tab IDs.** Only browser tabs (pages the user
navigates to) receive IDs. DevTools is an inspection tool, not a browsable tab.
This prevents recursive DevTools-for-DevTools, which would add complexity for no
benefit. Since DevTools tabs have no ID, they cannot be targeted by
`web devtools://[id]`.

Tab IDs are scoped to a profile server. Tab 3 in the `default` profile is
unrelated to tab 3 in the `work` profile. The profile is selected by the
existing `--profile` flag on the `web` CLI, not embedded in the URL. This avoids
redundancy and conflict — there is one way to select a profile (`--profile`) and
one way to select a tab (the integer ID).

### TUI Display

Both browser tabs and their DevTools tabs show the same
`[avatar][profileName]/[tabId]` label in the viewport border, using the browser
tab's ID. This makes it easy to visually pair them on screen — both panes show
`default/1`, confirming they're linked. The DevTools pane is distinguished by
its URL bar showing `devtools://[tabId]` (e.g. `devtools://1`) instead of a page
URL.

### URL Scheme

```
web devtools                      # inspect most recent tab (default profile)
web devtools --profile work       # inspect most recent tab (work profile)
web devtools://3                  # inspect tab 3 (default profile)
web devtools://3 --profile work   # inspect tab 3 (work profile)
```

The `devtools` keyword (no `://`) auto-targets the most recently active tab in
the profile. This is the common case — the user has one browser tab open and
wants to inspect it. The profile server tracks which tab was last active.

The `devtools://[tabId]` form targets a specific tab by its integer ID. The user
reads the ID from the viewport border (`default/3`) and types `devtools://3`.

The profile is always controlled by `--profile`, consistent with how `web`
already works for regular URLs:

```
web google.com                    # default profile
web google.com --profile work     # work profile
```

### Chromium Profile Server Changes Needed

The profile server's `CreateTab` method (~200 lines) handles: XPC parsing →
Shell/WebContents creation → persistent compositor setup → ShellTabObserver +
XPC connection → CALayerParams/cursor callbacks → state storage.

A `CreateDevToolsTab` reuses ~90% of this. The new pieces:

1. **Auto-incrementing tab ID** — the server maintains a counter, assigns an
   integer ID to each tab on creation, and includes it in the `tab_ready` XPC
   reply so the TUI can display it
2. **New XPC action `create_devtools_tab`** — receives `inspected_tab_id` (the
   integer tab ID to debug, or omitted for most recent) + `devtools_pane_id` +
   dimensions + dark
3. **Look up inspected WebContents** — walk `tabs_` to find the tab matching
   `inspected_tab_id` (or the most recently active tab if omitted)
4. **Create ShellDevToolsBindings** — one new line after creating the DevTools
   WebContents: `new ShellDevToolsBindings(devtools_wc, inspected_wc, delegate)`
5. **Load DevTools frontend URL** — instead of the user's URL, load
   `http://127.0.0.1:{port}/devtools/devtools_app.html?targetType=tab` using
   `ShellDevToolsManagerDelegate::GetHttpHandlerPort()`

Everything else — compositor, CALayerParams callback, cursor callback, XPC tab
connection, `tab_ready` message — is identical to `CreateTab`. Estimated new
C++: ~40–60 lines.

### Architecture

```
┌──────────────────────────────────────────────────────┐
│ Ghostty (terminal emulator)                          │
│                                                      │
│  ┌────────────────────┐  ┌────────────────────────┐  │
│  │ foo.com            │  │ devtools://1           │  │
│  │          default/1 │  │              default/1 │  │
│  │ [webpage]          │  │ [DevTools frontend]    │  │
│  └─────────┬──────────┘  └─────────────┬──────────┘  │
│            │                           │             │
└────────────┼───────────────────────────┼─────────────┘
             │ XPC                       │ XPC
             ▼                           ▼
┌──────────────────────────────────────────────────────┐
│ Chromium Profile Server (default)                    │
│                                                      │
│  Tab 1: foo.com (WebContents A)                      │
│  Tab 2: DevTools (WebContents B)                     │
│         ↕ ShellDevToolsBindings ↕                    │
│         inspects Tab 1                               │
└──────────────────────────────────────────────────────┘
```

### Flow

1. User opens a Ghostty split pane
2. User types `web devtools` or `web devtools://3`
3. TUI recognizes `devtools` keyword, extracts optional tab ID
4. TUI sends `create_devtools_tab` XPC to GUI with `inspected_tab_id` (or
   omitted for most recent)
5. GUI forwards to profile server
6. Profile server finds inspected tab's WebContents (by ID or most recent),
   creates DevTools WebContents (no tab ID assigned), wires up
   `ShellDevToolsBindings`, sends back CAContext ID in `tab_ready`
7. DevTools renders as a normal CALayerHost overlay in the new pane
8. TUI displays `devtools://1` in the URL bar and `default/1` in the viewport
   border (matching the inspected tab's label)
9. User inspects elements, debugs JS, views network — full DevTools experience

### Open Questions

1. **Lifecycle.** What happens when the inspected page closes? The DevTools pane
   should show a "target closed" state or close automatically. What happens when
   the DevTools pane closes? The ShellDevToolsBindings must be cleaned up.

2. **Keyboard shortcut.** Should Cmd+I in the browser pane automatically open a
   Ghostty split with `web devtools`? This requires the GUI to spawn a new
   terminal pane programmatically — possible but a new capability.

3. **Tab ID in `tab_ready`.** The profile server must include the integer tab ID
   in the `tab_ready` XPC reply for browser tabs so the TUI can display it in
   the viewport border. DevTools tabs omit this field (or send 0) since they
   don't have tab IDs.

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

## Experiment 1: DevTools via `devtools://` URL hijack

### Hypothesis

If the profile server intercepts `devtools://N` URLs in the existing
`create_tab` path, creates a DevTools WebContents with `ShellDevToolsBindings`
pointing at tab N, and renders it as a CALayerHost overlay, then:

1. The DevTools frontend will render and be interactive in a terminal pane
2. Hover highlighting will draw on the live inspected page (not a screenshot)
3. All standard DevTools features (Elements, Console, Network) will work

This proves the core architectural bet with zero TUI or GUI changes.

### Why this experiment first

The riskiest assumption is whether `ShellDevToolsBindings` + CALayerHost work
together. Everything else in the DevTools plan — tab IDs, URL parsing, viewport
borders, dedicated XPC actions — is straightforward plumbing. If DevTools can't
render through CALayerHost, or if hover highlighting doesn't work in-process
without a native window, the entire approach needs rethinking. This experiment
answers that question before investing in any plumbing.

### Plan

**Chromium profile server only.** No TUI or GUI changes. The user types
`web devtools://1` and it flows through the existing `create_tab` path.

#### 1. Add auto-incrementing tab ID to `TabState`

In `shell_browser_main_parts.h`, add a `tab_id` field to `TabState` and a
`next_tab_id_` counter to `ShellBrowserMainParts`:

```cpp
struct TabState {
  int tab_id = 0;  // auto-incrementing, 1-based
  // ... existing fields ...
};

int next_tab_id_ = 1;
```

In `CreateTab`, assign `tab->tab_id = next_tab_id_++;` before storing the tab.

#### 2. Detect `devtools://` URLs in `CreateTab`

At the top of `CreateTab`, before creating the Shell, check if the URL starts
with `devtools://`:

```cpp
void ShellBrowserMainParts::CreateTab(const GURL& url, ...) {
  // Check for devtools:// URL.
  bool is_devtools = url.scheme() == "devtools";
  int inspected_tab_id = 0;
  WebContents* inspected_contents = nullptr;

  if (is_devtools) {
    // Parse tab ID from host (devtools://3 → host is "3").
    base::StringToInt(url.host(), &inspected_tab_id);
    // Find inspected tab.
    for (auto& t : tabs_) {
      if (t->tab_id == inspected_tab_id) {
        inspected_contents = t->shell->web_contents();
        break;
      }
    }
    if (!inspected_contents) {
      LOG(ERROR) << "DevTools: tab " << inspected_tab_id << " not found";
      return;
    }
  }
```

#### 3. Load DevTools frontend URL instead of user URL

Replace the URL loaded into the Shell:

```cpp
GURL load_url = is_devtools
    ? GURL(base::StringPrintf(
          "http://127.0.0.1:%d/devtools/devtools_app.html?targetType=tab",
          ShellDevToolsManagerDelegate::GetHttpHandlerPort()))
    : url;
shell->LoadURL(load_url);
```

#### 4. Create `ShellDevToolsBindings`

After the Shell and WebContents are created, if this is a DevTools tab, wire up
the bindings. `ShellDevToolsFrontend::Show()` normally does this, but it creates
a native Shell window. We skip that and create the bindings directly:

```cpp
if (is_devtools) {
  // ShellDevToolsFrontend manages the bindings lifecycle and observes
  // the DevTools WebContents for PrimaryMainDocumentElementAvailable
  // (which triggers Attach).
  auto* devtools_frontend = new ShellDevToolsFrontend(
      shell, inspected_contents);
  // Store reference for cleanup.
  tab->devtools_frontend = devtools_frontend;
}
```

Note: `ShellDevToolsFrontend` already observes the WebContents via
`WebContentsObserver` and calls `Attach()` when the DOM loads. We reuse it as-is
— it doesn't actually need a native window, just a Shell with a WebContents.

#### 5. Skip tab ID for DevTools tabs

DevTools tabs don't get an auto-incrementing ID:

```cpp
if (!is_devtools) {
  tab->tab_id = next_tab_id_++;
}
// tab_id stays 0 for DevTools tabs.
```

### Test

1. Open TermSurf, navigate to a page: `web example.com`
2. Open a Ghostty split pane
3. Type `web devtools://1` — this should open DevTools for tab 1
4. **Verify rendering:** DevTools frontend renders in the new pane
5. **Verify interaction:** Click Elements panel, expand DOM nodes, type in
   Console
6. **Verify hover highlighting:** Hover over an element in the Elements panel —
   the corresponding element on the inspected page (in the other pane) should
   highlight with blue/green overlays in real time
7. **Verify keyboard/mouse:** All DevTools panels accept input — Network tab
   shows requests, Console accepts JavaScript, Sources shows files

### What success looks like

DevTools renders in a terminal pane like any other webpage, fully interactive,
with live hover highlighting on the inspected page. No native windows, no
external browser, no screenshots.

### What failure looks like

- DevTools frontend loads but can't connect to the inspected page (bindings
  broken without native Shell)
- Hover highlighting doesn't appear on the inspected page (CALayerHost rendering
  limitation)
- DevTools crashes or shows blank content (compositor/GPU process issue with two
  WebContents in one profile server)
