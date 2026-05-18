+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

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

## Experiment 3: `web devtools` auto-targeting

### Hypothesis

If `web devtools` (bare keyword, no `://`) auto-targets the most recently
focused browser tab, then the user can open DevTools without knowing any tab ID.
This is the overwhelmingly common case — one browser tab open, user wants to
inspect it.

### Why the GUI resolves auto-targeting, not Chromium

Chromium profile servers are per-profile. Each server only knows about its own
tabs. But `web devtools` (bare, no `--profile`) needs to work across profiles —
it should target whatever the user was just looking at, regardless of which
profile it belongs to.

The GUI is the only component that sees all panes across all profiles and tracks
which one is focused. It already maintains `focused_pane` (the pane_id of the
currently focused pane) in `xpc.zig`. The GUI is the correct place to resolve
"most recent browser tab" into a concrete `(profile, tab_id)` pair.

The resolution happens entirely in the GUI. Chromium always receives an explicit
`inspected_tab_id > 0` — it never needs auto-targeting logic.

### Plan

Changes to TUI, GUI, and Chromium (minor). No new XPC actions.

#### 1. Chromium: Include `tab_id` in `tab_ready` reply

The `tab_ready` XPC message currently only sends `pane_id`. Add `tab_id` so the
GUI can store it per-pane:

```cpp
// In CreateTab, after assigning tab_id:
xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
xpc_dictionary_set_string(msg, "action", "tab_ready");
xpc_dictionary_set_string(msg, "pane_id", pane_id.c_str());
xpc_dictionary_set_int64(msg, "tab_id", tab->tab_id);
xpc_connection_send_message(tab_conn, msg);
```

For DevTools tabs (`tab_id == 0`), the field is 0 — the GUI uses this to
distinguish browser panes from DevTools panes.

#### 2. GUI: Store `tab_id` per-pane and track last focused browser pane

**Add `tab_id` to `Pane` struct:**

```zig
tab_id: i64 = 0, // From tab_ready. 0 = DevTools or not yet assigned.
```

**Update `handleTabReady`** to store it:

```zig
fn handleTabReady(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const tab_id = xpc_dictionary_get_int64(msg, "tab_id");
    if (panes.get(pane_id)) |p| {
        p.tab_id = tab_id;
    }
    log.info("tab_ready pane={s} tab_id={d}", .{ pane_id, tab_id });
}
```

**Track `last_focused_browser_pane`** — a separate variable from `focused_pane`.
Updated in the focus handler whenever a non-DevTools pane
(`inspected_tab_id == 0`) receives focus:

```zig
var last_focused_browser_pane: ?[]const u8 = null;

// In handleFocusChanged, when focused == true:
if (p.inspected_tab_id == 0) {
    last_focused_browser_pane = pane_id;
}
```

This is separate from `focused_pane` because if the user focuses a DevTools pane
and then runs `web devtools` from a third pane, we still want to target the
browser tab, not the DevTools tab.

**Resolve auto-targeting in `handleSetDevtoolsOverlay`:**

When `inspected_tab_id == 0`, look up `last_focused_browser_pane` and use its
`tab_id` and `server`:

```zig
if (p.inspected_tab_id == 0) {
    // Auto-target: resolve from last focused browser pane.
    const target_pane_id = last_focused_browser_pane orelse {
        log.err("devtools auto-target: no browser pane has been focused", .{});
        return;
    };
    const target = panes.get(target_pane_id) orelse {
        log.err("devtools auto-target: pane {s} not found", .{target_pane_id});
        return;
    };
    p.inspected_tab_id = target.tab_id;
    // Use the target's server (profile), not the --profile argument.
    p.server = target.server;
    if (target.server) |s| s.pane_count += 1;
}
```

This overrides both the `inspected_tab_id` and the `server` (profile) on the
DevTools pane. The `--profile` flag is ignored when auto-targeting — the
DevTools always connects to the same profile server as the inspected tab.

When `inspected_tab_id > 0` (explicit `devtools://3`), the existing behavior is
unchanged — the TUI's `--profile` flag determines the server.

#### 3. TUI: Detect bare `devtools` keyword

In `tui/src/main.rs`, add a check for bare `devtools` (exact match):

```rust
let inspected_tab_id: i64 = if raw_url.starts_with("devtools://") {
    raw_url["devtools://".len()..].parse::<i64>().unwrap_or(0)
} else if raw_url == "devtools" {
    0  // Auto-target: GUI resolves to most recent browser tab.
} else {
    -1 // Not a DevTools request.
};
let is_devtools = inspected_tab_id >= 0;
```

Change the sentinel from `0` (not DevTools) to `-1` (not DevTools), so that `0`
means "auto-target". The `is_devtools` flag controls whether to send
`set_devtools_overlay` vs `set_overlay`.

The URL bar displays `devtools` (no `://N`) until the profile server sends back
`devtools://N` via `url_changed`, at which point it updates to show the resolved
tab ID.

#### 4. Chromium: No auto-targeting logic

`CreateDevToolsTab` always receives `inspected_tab_id > 0` (resolved by the
GUI). No changes to the tab lookup. The existing code handles it correctly.

### Test

1. Open TermSurf, navigate to a page: `web example.com`
2. Open a Ghostty split pane
3. Type `web devtools` (bare, no `://`, no number)
4. **Verify:** DevTools opens and inspects the `example.com` tab
5. **Verify URL bar:** Initially shows `devtools`, then updates to
   `devtools://1` after the profile server resolves the tab
6. **Verify explicit still works:** `web devtools://1` works as before
7. **Verify cross-profile:** Open `web --profile work google.com`, focus it,
   then open a split and type `web devtools` — should inspect the `work` profile
   tab, not the `default` profile tab

### Result: FAILURE

Both `web devtools` and `web devtools://0` hang indefinitely — the TUI shows a
spinner that never resolves into a DevTools pane.

### What went wrong

The root cause is a `p.browsing` gate in `handlePaneFocusChanged`
(`xpc.zig:850`):

```zig
if (is_focused) {
    // Only focus if the web TUI is in browse mode.
    if (p.browsing) {
        sendFocusChanged(pane_id, true);
    }
}
```

`sendFocusChanged` is the only function that updates
`last_focused_browser_pane`. But it is only called when gaining focus **if
`p.browsing` is true** — meaning the user must be in Browse mode (having pressed
Enter in the TUI). The TUI starts in Control mode
(`let mut mode = Mode::Control`), so `p.browsing` is false by default.

This means:

1. **Opening `web google.com` does not track the pane.** The TUI starts in
   Control mode, so `set_overlay` is sent with `browsing = false`. The pane is
   created with `p.browsing = false`. The initial `sendFocusChanged` call at
   pane creation is gated by `if (p.browsing)` — never called.
   `last_focused_browser_pane` stays null.

2. **Switching away from the pane and back does not track it.** macOS fires
   `paneFocusChanged(true)` when the user returns to the pane. But
   `handlePaneFocusChanged` checks `p.browsing` — it's still false (Control
   mode), so `sendFocusChanged` is never called. `last_focused_browser_pane`
   stays null.

3. **`web devtools` fails because the tracker is null.** The GUI receives
   `set_devtools_overlay` with `inspected_tab_id = 0`, creates a new Pane, hits
   the auto-target block, checks `last_focused_browser_pane`, finds null, cleans
   up the pane, and returns. Nothing is sent to Chromium. The TUI spins forever.

The `p.browsing` gate exists for a good reason: Chromium keyboard/mouse
forwarding should only activate in Browse mode. But `last_focused_browser_pane`
was bolted onto `sendFocusChanged`, inheriting a gate that doesn't apply to it.
"This pane has a browser tab" is true whether the user is in Control mode or
Browse mode.

**Secondary issues** (would have surfaced even if the browsing gate were fixed):

4. **Half-created pane on failure.** The initial implementation created the Pane
   in the `panes` map before checking whether auto-targeting could succeed. When
   auto-targeting failed, the function returned early, leaving an orphaned pane
   with no server and no tab. The TUI waited forever for a `tab_ready` that
   would never arrive. This was partially fixed by adding `cleanupPane()`, but
   the fundamental timing problem remained.

5. **`tab_id` may be 0 at resolution time.** Even if `last_focused_browser_pane`
   were set, the target pane's `tab_id` is only populated when the `tab_ready`
   XPC message arrives from Chromium. There's a race: if the DevTools request
   arrives before `tab_ready`, `target.tab_id` is still 0 and auto-targeting
   fails.

6. **No error feedback to the TUI.** When auto-targeting fails, there is no
   mechanism to tell the TUI "this didn't work." The TUI has no timeout or
   fallback — it just spins.

### Ideas for next steps

- **Track last browser pane independently of Chromium focus.** Update
  `last_focused_browser_pane` in `handlePaneFocusChanged` regardless of
  `p.browsing`. The browsing gate should only control Chromium focus
  (`sendFocusChanged`), not the auto-target tracker. Alternatively, update it in
  `handleTabReady` — every `tab_ready` with `tab_id > 0` means a browser tab was
  just created, which is the most reliable signal.

- **Add a timeout or error response.** If auto-targeting fails, the GUI should
  send an error message back to the TUI via XPC so the TUI can display "No
  browser tab to inspect" instead of spinning forever.

- **Simplify: require explicit tab IDs for now.** `web devtools://1` already
  works (Experiment 2). Auto-targeting is a convenience, not a blocker. Ship
  explicit targeting first, add auto-targeting as a follow-up once the
  focus-tracking infrastructure is more robust.

## Experiment 4: `web last` diagnostic command

### Hypothesis

If `web last` queries the GUI for the most recently active browser pane and
prints the profile, pane ID, and tab ID, then we can verify whether the GUI's
tracking state is correct before attempting auto-targeting. This is a diagnostic
tool and a prerequisite for reliable `web devtools` auto-targeting.

### Why a diagnostic first

Experiment 3 failed because `last_focused_browser_pane` was never populated —
the `p.browsing` gate in `handlePaneFocusChanged` blocked the update. But we
couldn't see this at runtime because there was no way to inspect the GUI's
internal state. `web last` makes the state visible so we can verify the fix
before building on top of it.

### Plan

Changes to TUI and GUI only. No Chromium changes.

#### 1. GUI: Track `last_browser_pane` on tab creation, not focus

Replace the broken focus-based tracker with a creation-based one. In
`handleTabReady`, when `tab_id > 0` (browser tab, not DevTools), update a new
`last_browser_pane` variable:

```zig
var last_browser_pane: ?[]const u8 = null;

fn handleTabReady(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const tab_id = xpc_dictionary_get_int64(msg, "tab_id");

    if (panes.get(pane_id)) |p| {
        p.tab_id = tab_id;
        if (tab_id > 0) {
            last_browser_pane = p.pane_id_key; // heap-allocated, stable
        }
    }

    log.info("tab_ready pane={s} tab_id={d}", .{ pane_id, tab_id });
}
```

Also update it in `handlePaneFocusChanged` **without** the `p.browsing` gate —
any non-DevTools pane that gains focus is the last active browser pane
regardless of mode:

```zig
// In handlePaneFocusChanged, when is_focused == true:
if (p.inspected_tab_id == 0 and p.tab_id > 0) {
    last_browser_pane = p.pane_id_key;
}
```

Remove the old `last_focused_browser_pane` variable — it was never reliably
populated.

#### 2. GUI: Handle `query_last` request

Add a new synchronous request/reply handler (same pattern as `hello`):

```zig
fn handleQueryLast(msg: xpc_object_t) void {
    const profile_filter = str(xpc_dictionary_get_string(msg, "profile"));
    const reply = xpc_dictionary_create_reply(msg) orelse return;

    // If a profile filter is given, find the last browser pane for that profile.
    // Otherwise use the global last_browser_pane.
    var target_pane: ?*Pane = null;
    var target_pane_id: []const u8 = "";

    if (profile_filter.len > 0 and !std.mem.eql(u8, profile_filter, "(null)")) {
        // Walk panes to find the last one matching this profile with tab_id > 0.
        // For now, just check last_browser_pane and verify its profile matches.
        if (last_browser_pane) |lpid| {
            if (panes.get(lpid)) |p| {
                if (p.server) |s| {
                    if (std.mem.eql(u8, s.profile_key, profile_filter)) {
                        target_pane = p;
                        target_pane_id = lpid;
                    }
                }
            }
        }
    } else {
        if (last_browser_pane) |lpid| {
            if (panes.get(lpid)) |p| {
                target_pane = p;
                target_pane_id = lpid;
            }
        }
    }

    if (target_pane) |p| {
        xpc_dictionary_set_string(reply, "pane_id", target_pane_id.ptr);
        xpc_dictionary_set_int64(reply, "tab_id", p.tab_id);
        if (p.server) |s| {
            xpc_dictionary_set_string(reply, "profile", s.profile_key.ptr);
        }
    }
    // If no target found, reply has no fields — TUI checks for "pane_id".

    const conn = xpc_dictionary_get_remote_connection(msg);
    if (conn != null) {
        xpc_connection_send_message(conn, reply);
    }
}
```

Wire it into the dispatch:

```zig
} else if (std.mem.eql(u8, action_str, "query_last")) {
    handleQueryLast(msg);
}
```

#### 3. TUI: Add `last` subcommand

Add to the `Commands` enum:

```rust
#[derive(Subcommand)]
enum Commands {
    Url { url: String },
    /// Show the last active browser pane/tab
    Last,
}
```

In `main()`, handle it before entering the TUI event loop:

```rust
if let Some(Commands::Last) = cli.command {
    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
        match conn.send_query_last(pid, &profile) {
            Some((profile, pane_id, tab_id)) => {
                println!("profile: {}", profile);
                println!("pane_id: {}", pane_id);
                println!("tab_id:  {}", tab_id);
            }
            None => {
                println!("No active browser tab found.");
            }
        }
    } else {
        println!("Not running inside TermSurf.");
    }
    return Ok(());
}
```

#### 4. TUI XPC: Add `send_query_last`

Same synchronous request/reply pattern as `send_hello`:

```rust
pub fn send_query_last(
    &self,
    pane_id: &str,
    profile: &str,
) -> Option<(String, String, i64)> {
    // Send { action: "query_last", pane_id, profile }
    // Receive { pane_id, tab_id, profile } or empty dict
    // Return Some((profile, pane_id, tab_id)) or None
}
```

### Test

1. Open TermSurf, navigate to a page: `web google.com`
2. Open a Ghostty split pane
3. Type `web last`
4. **Expected output:**
   ```
   profile: default
   pane_id: XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX
   tab_id:  1
   ```
5. Type `web last` again after opening a second browser tab — should show tab_id
   2
6. Type `web --profile work last` with no work-profile tab open — should show
   "No active browser tab found."
7. If `web last` shows correct data, the tracker is working and `web devtools`
   auto-targeting can be built on top of it in the next experiment

### Result: SUCCESS

`web last` correctly reports the most recently active browser pane:

```
profile: default
pane_id: 1F208F7C-5B2F-4E24-92BA-9C00427E2D5D
tab_id:  1
```

### Conclusion

The experiment required three rounds of diagnostics to get working. Each round
revealed a deeper bug:

**Round 1 — initial implementation.** `web last` returned "No active browser tab
found." with `pane_count=1 has_last=false last_pane=(null)`. This told us the
GUI had one pane registered (created by `handleSetOverlay`) but
`last_browser_pane` was never set. The `handleTabReady` function should have set
it when `tab_id > 0`, so either `handleTabReady` never ran or `tab_id` was 0.

**Round 2 — enhanced diagnostics.** Added `tab_ready_count` (how many times
`handleTabReady` was called) and `first_pane_tab_id` (the tab_id stored on the
first pane) to the reply. Result: `tab_ready_count=2 first_pane_tab_id=0`. This
was the critical finding: `handleTabReady` ran twice (once for the browser tab,
once for some other message), but the pane's `tab_id` was still 0. The handler
was dispatched correctly — the problem was upstream in Chromium.

**Round 3 — Chromium ordering bug.** Traced the Chromium `CreateTab` flow in
`shell_browser_main_parts.cc`. Found the root cause: `tab_ready` was sent at
line 485 with `tab->tab_id`, but `tab->tab_id` wasn't assigned until line 572
(`tab->tab_id = next_tab_id_++`). The message always sent 0. Fix: moved the
`tab_id` assignment before the `tab_ready` send. After rebuilding Chromium,
`web last` returned `tab_id: 1` correctly.

**Key insight.** The Experiment 3 failure was not caused by the `p.browsing`
gate alone. Even after fixing the tracker to use `handleTabReady` instead of
`handlePaneFocusChanged`, it still failed because the underlying Chromium
`tab_ready` message never contained a valid `tab_id`. Two independent bugs
compounded: the GUI tracked on the wrong event (focus instead of creation), AND
Chromium sent the ID before assigning it. Both had to be fixed.

**Changes made:**

- **Chromium** (`shell_browser_main_parts.cc`): Moved
  `tab->tab_id = next_tab_id_++` before the `tab_ready` XPC send in `CreateTab`.
  Removed the duplicate assignment that was 90 lines too late.
- **GUI** (`xpc.zig`): Added `tab_ready_count` diagnostic counter.
  `handleTabReady` logs whether the pane was found. `handleQueryLast` returns
  `tab_ready_count`, `first_pane_tab_id`, and `first_pane_id` in the failure
  reply.
- **TUI** (`xpc.rs`): `send_query_last` prints all diagnostic fields on failure.

The `last_browser_pane` tracker now works correctly. It updates on tab creation
(in `handleTabReady` when `tab_id > 0`) and on pane focus (in
`handlePaneFocusChanged` without the `p.browsing` gate, for non-DevTools panes
with `tab_id > 0`). This is the foundation for `web devtools` auto-targeting in
the next experiment.

## Experiment 5: Cleanup debug scaffolding

### Goal

Strip diagnostic artifacts added during Experiments 3–4 debugging. The core
features (auto-targeting, `web last`, `last_browser_pane` tracker) all work.
What remains is debug noise that shouldn't ship.

### Changes

#### 1. GUI (`xpc.zig`): Remove `tab_ready_count`

Delete the global variable (line 122), the increment in `handleTabReady` (line
618), and the `tab_ready_count` field in the `handleQueryLast` failure reply
(line 847). Also remove `found_pane` (line 621) and simplify the log back to:

```zig
log.info("tab_ready pane={s} tab_id={d}", .{ pane_id, tab_id });
```

#### 2. GUI (`xpc.zig`): Simplify `handleQueryLast` failure reply

The failure path (lines 842–867) dumps `pane_count`, `has_last`, `last_pane`,
`tab_ready_count`, `first_pane_tab_id`, and `first_pane_id`. Replace with a
minimal failure reply — just an empty dict (no `pane_id` key). The TUI already
checks for `pane_id` being null to detect failure. Remove lines 844–867 entirely
(keep the log line).

#### 3. TUI (`xpc.rs`): Simplify `send_query_last` failure path

The failure branch (lines 431–461) reads and prints six diagnostic fields.
Replace with a simple `return None` — no `eprintln!`. The `web last` command
already prints "No active browser tab found." in `main.rs`.

#### 4. TUI (`main.rs`): Rename `Last` → `Status`

Rename the subcommand from `Last` to `Status`. The command becomes `web status`.
It reports the current state of the most recent browser pane — "status" is more
descriptive than "last". Update the enum variant, the match arms, and the doc
comment.

### Test

1. `cd gui && zig build` — compiles
2. `cd tui && cargo build` — compiles
3. `web google.com` in a pane, then `web status` in a split — prints profile,
   pane_id, tab_id
4. `web status` with no browser open — prints "No active browser tab found."
5. `web devtools` still works (auto-targeting unaffected)

### Result: FAILURE

After applying all four changes, both `web status` (the renamed `web last`) and
`web devtools` broke. `web status` reported "No active browser tab found."
instead of returning the active pane. `web devtools` timed out — it never
opened. Reverting to the pre-cleanup code restored both commands immediately.

### Conclusion

The cleanup commit made four changes simultaneously. All four compiled and
appeared logically sound — none seemed to alter control flow. Yet the result was
a total regression: both the diagnostic command and DevTools auto-targeting
stopped working. Reverting the commit fixed everything.

Because all four changes were applied together, the specific culprit is not yet
isolated. Hypotheses, in order of likelihood:

**Hypothesis A: Removing `tab_ready_count` or `found_pane` from
`handleTabReady`.** The `tab_ready_count += 1` line was a side-effecting
statement at the top of `handleTabReady`. Removing it changed the function's
compiled code layout. If the Zig compiler was making assumptions about side
effects or reordering the remaining logic differently without it, the
`panes.get(pane_id)` lookup or `last_browser_pane` assignment could have been
optimized away or reordered. Similarly,
`found_pane = panes.get(pane_id) != null` performed an extra hash lookup that
may have had a stabilizing effect. This is the most suspicious change because it
directly affects whether `last_browser_pane` gets set — and both
`web last`/`web status` AND `web devtools` depend on it.

**Hypothesis B: Removing diagnostic fields from `handleQueryLast` failure
reply.** The failure path previously populated `pane_count`, `has_last`,
`tab_ready_count`, `last_pane`, `first_pane_tab_id`, and `first_pane_id` into
the reply dictionary. With those removed, the reply is an empty dict. It's
possible that `xpc_dictionary_create_reply` returns a reply with some internal
structure, and the TUI's `xpc_dictionary_get_string(reply, "pane_id")` behaves
differently on a completely empty reply versus one with other keys set. This
could cause the success path to misparse the reply.

**Hypothesis C: Removing the TUI `eprintln!` diagnostic dump.** The TUI failure
path previously read six fields from the reply before returning `None`. With
those reads removed, the `xpc_release(reply)` happens immediately. If there's a
use-after-free or timing issue with the XPC reply object, the extra reads may
have acted as an accidental delay. Unlikely, but possible.

**Hypothesis D: Renaming `Last` → `Status`.** Clap derives the subcommand name
from the enum variant. `Last` becomes `last`, `Status` becomes `status`. If the
user typed `web last` after the rename, clap would not recognize it and could
fall through to the positional `url` argument, treating "last" as a URL. This
would explain `web status` failing if there's a separate issue, but doesn't
explain `web devtools` breaking — devtools doesn't depend on the subcommand
name.

**Next step.** Apply each change individually, rebuilding and testing after each
one.

## Experiment 6: Remove `tab_ready_count`

### Goal

Test whether removing the `tab_ready_count` diagnostic counter breaks `web last`
or `web devtools`. This is Experiment 5 Hypothesis A — isolated.

### Changes

#### GUI (`xpc.zig`): Remove `tab_ready_count` only

1. Delete the global variable declaration (`var tab_ready_count: i64 = 0;`).
2. Delete the increment (`tab_ready_count += 1;`) from `handleTabReady`.
3. Delete the `tab_ready_count` field from the `handleQueryLast` failure reply
   (`xpc_dictionary_set_int64(reply, "tab_ready_count", tab_ready_count);`).

No other changes. `found_pane`, diagnostic fields, TUI code, and subcommand
names stay exactly as they are.

### Test

1. `cd gui && zig build` — compiles
2. Rebuild and launch with `build-debug.sh --open`
3. `web google.com` in a pane
4. `web last` in a split — should print profile, pane_id, tab_id
5. `web devtools` in a split — should open DevTools

### Result: SUCCESS

Removing `tab_ready_count` does not break anything. `web last` and
`web devtools` both work. This rules out Experiment 5 Hypothesis A — the counter
was not the culprit.

## Experiment 7: Remove `found_pane` from `handleTabReady`

### Goal

Test whether removing the `found_pane` debug variable breaks `web last` or
`web devtools`. This isolates the second part of Experiment 5 Hypothesis A.

### Changes

#### GUI (`xpc.zig`): Remove `found_pane` only

1. Delete `const found_pane = panes.get(pane_id) != null;` from
   `handleTabReady`.
2. Simplify the log to `log.info("tab_ready pane={s} tab_id={d}", ...)` — remove
   `found={}` from the format string.

No other changes.

### Test

1. `cd gui && zig build` — compiles
2. Rebuild and launch with `build-debug.sh --open`
3. `web google.com` in a pane
4. `web last` in a split — should print profile, pane_id, tab_id
5. `web devtools` in a split — should open DevTools

### Result: SUCCESS

Removing `found_pane` does not break anything. The extra hash lookup was purely
diagnostic.

## Experiment 8: Simplify `handleQueryLast` failure reply

### Goal

Test whether removing the diagnostic fields from the `handleQueryLast` failure
reply breaks `web last` or `web devtools`. This is Experiment 5 Hypothesis B —
isolated.

### Changes

#### GUI (`xpc.zig`): Strip diagnostic fields from failure path

In `handleQueryLast`, the `else` branch (no matching pane) currently populates
`pane_count`, `has_last`, `last_pane`, `first_pane_tab_id`, and `first_pane_id`
into the reply. Remove all of these — keep only the log line. The reply will be
an empty dict (no `pane_id` key), which the TUI already interprets as failure.

No other changes.

### Test

1. `cd gui && zig build` — compiles
2. Rebuild and launch with `build-debug.sh --open`
3. `web google.com` in a pane
4. `web last` in a split — should print profile, pane_id, tab_id
5. `web devtools` in a split — should open DevTools

### Result: SUCCESS

Removing all diagnostic fields from the failure reply does not break anything.
An empty reply dict is sufficient — the TUI correctly interprets a missing
`pane_id` key as failure. Hypothesis B ruled out.

## Experiment 9: Simplify TUI `send_query_last` failure path

### Goal

Test whether removing the diagnostic `eprintln!` from `send_query_last` breaks
`web last` or `web devtools`. This is Experiment 5 Hypothesis C — isolated.

### Changes

#### TUI (`xpc.rs`): Remove diagnostic dump on failure

In `send_query_last`, the failure branch (when `pane_id` is null in the reply)
currently reads `pane_count`, `has_last`, `last_pane`, `tab_ready_count`,
`first_pane_tab_id`, and `first_pane_id` from the reply and prints them via
`eprintln!`. Replace the entire block with just
`xpc_release(reply); return None;`.

No other changes.

### Test

1. `cd tui && cargo build` — compiles
2. Rebuild and launch with `build-debug.sh --open`
3. `web google.com` in a pane
4. `web last` in a split — should print profile, pane_id, tab_id
5. `web devtools` in a split — should open DevTools

### Result: SUCCESS

Removing the diagnostic `eprintln!` does not break anything. Hypothesis C ruled
out.

## Experiment 10: Rename `Last` → `Status`

### Goal

Test whether renaming the `Last` subcommand to `Status` breaks `web devtools`.
This is Experiment 5 Hypothesis D — the only remaining suspect. Experiments 6–9
each passed individually, so if the Experiment 5 regression was caused by a
single change, this must be it.

### Changes

#### TUI (`main.rs`): Rename `Last` → `Status`

1. Change `Last,` to `Status,` in the `Commands` enum.
2. Update the doc comment from "Show the last active browser pane/tab" to "Show
   the active browser pane status".
3. Change `Commands::Last` to `Commands::Status` in both match arms.

No other changes. The command becomes `web status` instead of `web last`.

### Test

1. `cd tui && cargo build` — compiles
2. Rebuild and launch with `build-debug.sh --open`
3. `web google.com` in a pane
4. `web status` in a split — should print profile, pane_id, tab_id
5. `web devtools` in a split — should open DevTools

### Result: SUCCESS (not applied)

The rename compiles and works — both `web status` and `web devtools` function
correctly. Hypothesis D ruled out: the rename was not the Experiment 5 culprit.

However, `last` is the better name. This command reports the last active browser
pane — it's not a general-purpose status command. "Last" is more precise and
descriptive. The rename was stashed and will not be applied.

### Experiment 5 retrospective

All four changes passed individually (Experiments 6–10), yet they broke when
applied together in Experiment 5. This means the regression was caused by an
interaction between two or more changes, not by any single one. The exact
interaction remains unidentified. Since all debug scaffolding has now been
removed incrementally (Experiments 6–9) without issue, the cleanup is complete
and the mystery is moot.

## Conclusion

Issue 684 delivered two new TUI commands and the underlying GUI infrastructure
to support them:

### What works

1. **`web devtools`** — auto-targets the most recently active browser tab and
   opens Chrome DevTools in a split pane. Uses in-process
   `ShellDevToolsBindings` for full element inspection, hover highlighting, and
   live DOM manipulation.
2. **`web devtools://N`** — explicit tab targeting. Works for any tab ID.
3. **`web last`** — diagnostic command that prints the profile, pane ID, and tab
   ID of the most recently active browser pane.
4. **`web last --profile <name>`** — profile-filtered variant.
5. **`last_browser_pane` tracker** — GUI tracks the most recently active browser
   pane via two paths: `handleTabReady` (on tab creation when `tab_id > 0`) and
   `handlePaneFocusChanged` (on focus gain, without the `p.browsing` gate, for
   non-DevTools panes with `tab_id > 0`).

### What doesn't work (multi-profile)

The tracker uses a single global variable (`last_browser_pane`). This breaks
with multiple profiles:

1. **`web last` fails entirely with multiple profiles open.** Open a browser
   with the default profile, then open another with the "work" profile.
   `web last` (no filter) returns "No active browser tab found." instead of the
   work profile's pane info. The root cause needs investigation — the global
   should point to the most recent pane regardless of profile.
2. **`web last --profile default` fails when "work" was opened last.** The
   profile filter only checks `last_browser_pane`. If that pane belongs to
   "work", the filter rejects it and returns nothing. It does not search other
   panes.
3. **`web last --profile work` works** because the global happens to point to
   the work pane (most recently created).
4. **`web devtools` auto-targeting has the same limitation.** It uses the same
   `last_browser_pane` global, so it can only target the single most recent
   browser pane. With multiple profiles, it may target the wrong one or fail.

The fix requires per-profile tracking — either a map of profile →
last-browser-pane, or iterating all panes to find the most recent one matching
the requested profile. This is deferred to the next issue.

### Chromium bug fixed

Experiment 4 uncovered a Chromium ordering bug in `CreateTab`
(`shell_browser_main_parts.cc`): the `tab_ready` XPC message was sent before
`tab->tab_id` was assigned, so `tab_id` was always 0. Fixed by moving the
assignment before the send.

### Changes across all experiments

- **Chromium** (`shell_browser_main_parts.cc`): `tab_id` assignment ordering fix
  in `CreateTab`.
- **GUI** (`xpc.zig`): `last_browser_pane` global, `handleTabReady` tracker,
  `handlePaneFocusChanged` tracker (without `p.browsing` gate),
  `handleQueryLast` request/reply handler, `cleanupPane` clears tracker.
- **TUI** (`main.rs`): `Last` subcommand, `send_query_last` XPC call.
- **TUI** (`xpc.rs`): `send_query_last` implementation.
