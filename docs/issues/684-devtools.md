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
6. **Synthetic URL in `ShellTabObserver`** — DevTools tabs store
   `inspected_tab_id` in their `TabState`. The `ShellTabObserver` checks this
   field in `DidFinishNavigation()` and sends `devtools://N` instead of the
   internal HTTP URL, so the TUI always displays the correct synthetic URI

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
| `shell_tab_observer.h`                | Observer with XPC URL/title/cursor     |
| `shell_tab_observer.cc:99-128`        | `DidFinishNavigation()` — URL updates  |
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

#### 0. Create Chromium branch and build

Create `146.0.7650.0-issue-684` from `146.0.7650.0-issue-680` (the most recent
branch with all TermSurf modifications). Add it to the Branches table in
`docs/chromium.md`.

```bash
cd chromium/src
git checkout 146.0.7650.0-issue-680
git checkout -b 146.0.7650.0-issue-684
```

Build after each change with:

```bash
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default chromium_profile_server
```

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

### Result: FAILURE

The DevTools pane times out — no browser content ever appears. The overlay never
renders because the Chromium profile server never sends back a `ca_context_id`
for the DevTools tab.

Two issues were identified and fixed during the experiment, but the core problem
persists:

1. **GURL parsing (fixed).** `GURL("devtools://1")` doesn't recognize
   `devtools://` as a standard scheme, so `url.SchemeIs("devtools")` returned
   false and the DevTools detection was silently skipped. Fixed by parsing the
   raw `url.spec()` string instead. However, the same GURL issue means
   `Shell::CreateNewWindow(browser_context, url, ...)` receives an invalid GURL
   for the initial creation — the Shell may be created in a broken state before
   we override the URL to the DevTools frontend.

2. **Deeper problem: the Shell is created with the invalid `devtools://1` URL.**
   `CreateTab` constructs the Shell via
   `Shell::CreateNewWindow(browser_context, load_url, ...)` where `load_url` is
   correctly set to the DevTools frontend HTTP URL. But `Shell::CreateNewWindow`
   internally calls `WebContents::Create()` and then `LoadURL()`. If the Shell's
   internal state or the WebContents' initial navigation is tainted by the
   invalid GURL, the DevTools frontend may never load properly — the renderer
   process may fail to start, the compositor may never produce frames, and no
   `ca_context_id` is ever generated.

The fundamental issue is that the `devtools://` URL hijack approach tries to
shoehorn a special-purpose tab into the generic `create_tab` path, but GURL's
handling of non-standard schemes creates problems at multiple layers. A
dedicated `create_devtools_tab` XPC action that never passes `devtools://`
through GURL would avoid this entirely — the profile server would receive the
inspected tab ID as a separate integer field, not encoded in a URL.

## Experiment 2: Dedicated `create_devtools_tab` XPC action

### Hypothesis

A dedicated `create_devtools_tab` XPC action — separate from `create_tab` — will
avoid the GURL parsing problems from Experiment 1. The inspected tab ID is
passed as an integer field, not encoded in a URL. The profile server constructs
the DevTools frontend URL internally and never sees `devtools://`.

### Why this fixes Experiment 1

Experiment 1 failed because `devtools://1` was passed through GURL at multiple
layers (TUI `normalize_url`, GUI XPC forwarding, Chromium
`Shell::CreateNewWindow`). Even after fixing the profile server's detection, the
URL still flows through other code that may reject or mangle non-standard
schemes.

This experiment avoids GURL entirely for the `devtools://` scheme. The TUI
detects `devtools://` as a string, extracts the integer tab ID, and sends a
different XPC action with the tab ID as a separate field. The profile server
receives a normal integer, looks up the tab, and loads the DevTools frontend URL
(which is a standard `http://127.0.0.1:...` URL that GURL handles fine).

### Plan

Changes across all three components: TUI, GUI, and Chromium profile server.

#### 0. Chromium branch

Continue on `146.0.7650.0-issue-684`. Revert the Experiment 1 `devtools://`
detection code in `CreateTab` (restore it to its pre-experiment state), then add
the new `CreateDevToolsTab` method.

#### 1. Chromium: Add `create_devtools_tab` XPC handler

In `shell_browser_main_parts.cc`, add a new XPC action in the control connection
event handler (alongside `create_tab`, `resize`, etc.):

```cpp
} else if (action && std::string_view(action) == "create_devtools_tab") {
  const char* pane_id_str = xpc_dictionary_get_string(event, "pane_id");
  std::string pane_id(pane_id_str ? pane_id_str : "");
  int inspected_tab_id = (int)xpc_dictionary_get_int64(event, "inspected_tab_id");
  int pw = (int)xpc_dictionary_get_uint64(event, "pixel_width");
  int ph = (int)xpc_dictionary_get_uint64(event, "pixel_height");
  bool dark = xpc_dictionary_get_bool(event, "dark");
  content::GetUIThreadTaskRunner({})->PostTask(
      FROM_HERE,
      base::BindOnce(&ShellBrowserMainParts::CreateDevToolsTab,
                     base::Unretained(self), pane_id,
                     inspected_tab_id, pw, ph, dark));
}
```

#### 2. Chromium: Add `CreateDevToolsTab` method

New method on `ShellBrowserMainParts`. Largely copied from `CreateTab` but with
key differences:

- Receives `inspected_tab_id` (integer) instead of a URL
- Looks up the inspected tab's `WebContents` by tab ID
- Constructs the DevTools frontend URL internally:
  `http://127.0.0.1:{port}/devtools/devtools_app.html?targetType=tab`
- Creates the Shell with this HTTP URL (standard GURL, no issues)
- Creates `ShellDevToolsFrontend(shell, inspected_contents)` for bindings
- Does NOT assign an auto-incrementing tab ID (DevTools tabs get `tab_id=0`)
- Stores `inspected_tab_id` in the `TabState` (see below)
- Passes `inspected_tab_id` to the `ShellTabObserver` for URL override

The compositor, CALayerParams callback, cursor callback, XPC tab connection, and
`tab_ready` message are identical to `CreateTab`.

**`TabState` addition:**

```cpp
struct TabState {
  int tab_id = 0;          // Auto-incrementing, 1-based. 0 = DevTools tab.
  int inspected_tab_id = 0; // Nonzero = this is a DevTools tab inspecting this ID.
  // ... existing fields ...
};
```

In `CreateDevToolsTab`, after creating the tab:

```cpp
tab->inspected_tab_id = inspected_tab_id;
tab->tab_observer->SetInspectedTabId(inspected_tab_id);
```

#### 3. Chromium: URL override in `ShellTabObserver`

The `ShellTabObserver` sends `url_changed` XPC messages whenever Chromium
navigates (in `DidFinishNavigation`). For DevTools tabs, the real URL is the
internal DevTools frontend HTTP URL
(`http://127.0.0.1:{port}/devtools/devtools_app.html?targetType=tab`), which is
meaningless to the user — it contains no pane ID or tab ID and cannot be used as
a navigable address.

The fix: `ShellTabObserver` gets an `inspected_tab_id_` field. When nonzero,
`DidFinishNavigation` sends `devtools://N` (where N is the inspected tab ID)
instead of the real URL. This ensures the TUI always displays the correct
synthetic URI.

**Header (`shell_tab_observer.h`):**

```cpp
void SetInspectedTabId(int id);

// In private:
int inspected_tab_id_ = 0;  // Nonzero = DevTools tab; send synthetic URL.
```

**Implementation (`shell_tab_observer.cc`):**

```cpp
void ShellTabObserver::SetInspectedTabId(int id) {
  inspected_tab_id_ = id;
}
```

**In `DidFinishNavigation`**, replace the URL extraction:

```cpp
// For DevTools tabs, send the synthetic devtools://N URL instead of the
// internal HTTP URL (Issue 684).
std::string url;
if (inspected_tab_id_ > 0) {
  url = base::StringPrintf("devtools://%d", inspected_tab_id_);
} else {
  url = navigation_handle->GetURL().spec();
}
```

This also suppresses any internal DevTools sub-navigations (panel switches,
resource loads) from leaking through — the TUI always sees `devtools://N`.

#### 4. GUI: Forward `create_devtools_tab` XPC action

In `gui/src/apprt/xpc.zig`, add handling for `create_devtools_tab` from the TUI.
This is identical to the existing `set_overlay` → `create_tab` flow, but sends a
`create_devtools_tab` action to the Chromium profile server with:

- `pane_id` — the DevTools pane's ID
- `inspected_tab_id` — the integer tab ID to inspect
- `pixel_width`, `pixel_height` — pane dimensions
- `dark` — color scheme

This requires the GUI to know the tab ID for a given pane. The profile server
already sends `tab_ready` — add a `tab_id` field to this message. The GUI stores
it per-pane.

#### 5. TUI: Detect `devtools://` and send different XPC action

In `tui/src/main.rs`, detect when the URL starts with `devtools://`:

- Extract the integer tab ID from the URL string (e.g. `devtools://3` → `3`)
- Send a `create_devtools_tab` XPC message to the GUI instead of the normal
  `set_overlay` flow
- Include `inspected_tab_id` as a separate integer field

In `tui/src/xpc.rs`, add a `send_create_devtools_tab` method.

### Test

1. Open TermSurf, navigate to a page: `web example.com`
2. Open a Ghostty split pane
3. Type `web devtools://1` — the TUI detects the scheme, sends
   `create_devtools_tab` with `inspected_tab_id=1`
4. **Verify rendering:** DevTools frontend renders in the new pane
5. **Verify interaction:** Click Elements panel, expand DOM nodes, type in
   Console
6. **Verify hover highlighting:** Hover over an element in the Elements panel —
   the corresponding element on the inspected page should highlight
7. **Verify keyboard/mouse:** All DevTools panels accept input

### Result: SUCCESS

DevTools renders in a terminal pane via `web devtools://1`. The dedicated
`create_devtools_tab` XPC action bypasses GURL entirely — the inspected tab ID
flows as a plain integer through all three components (TUI → GUI → Chromium),
and the profile server constructs the DevTools frontend URL internally using a
standard `http://127.0.0.1:...` URL that GURL handles without issue.

The `ShellTabObserver` sends a synthetic `devtools://N` URL back through the XPC
chain, so the TUI displays the correct URI in the URL bar instead of the
internal HTTP address.

### What worked

1. **Dedicated XPC action.** Separating `create_devtools_tab` from `create_tab`
   was the key insight. The inspected tab ID never touches GURL — it's an
   integer field in the XPC message, looked up server-side.

2. **ShellDevToolsFrontend reuse.** The existing `ShellDevToolsFrontend` class
   works perfectly without a native window. It observes the DevTools
   WebContents, waits for `PrimaryMainDocumentElementAvailable`, and calls
   `Attach()` to connect the Mojo protocol pipes. No modifications to the class
   were needed beyond making the constructor public (done in Experiment 1).

3. **Synthetic URL in ShellTabObserver.** The `inspected_tab_id_` field on the
   observer cleanly overrides `DidFinishNavigation` to send `devtools://N`
   instead of the internal HTTP URL. Internal DevTools sub-navigations (panel
   switches, resource loads) never leak to the TUI.

4. **Full infrastructure reuse.** The DevTools tab uses the same persistent
   compositor, CALayerParams callback, cursor callback, XPC tab connection, and
   `tab_ready` flow as regular browser tabs. `CreateDevToolsTab` is a copy of
   `CreateTab` with DevTools-specific logic — no refactoring of shared code was
   needed.

### What remains

- Tab ID display in the viewport border (`[avatar][profileName]/[tabId]`)
- `web devtools` (no tab ID) auto-targeting the most recent tab
- Tab ID in the `tab_ready` XPC reply for the GUI/TUI to store
- Lifecycle handling (inspected tab closes → DevTools tab behavior)
- Keyboard shortcut (Cmd+I) to open DevTools from a browser pane
- URL bar DevTools (typing `devtools://1` in an existing pane's URL bar)
