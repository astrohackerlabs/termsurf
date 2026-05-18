+++
status = "closed"
opened = "2026-03-04"
closed = "2026-03-04"
+++

# Issue 706: Fix DevTools crash in Plusium

## Goal

Make Chrome DevTools work in Plusium. Currently, opening DevTools crashes
Plusium with a SEGV inside `ShellDevToolsBindings::AttachInternal()`. The same
DevTools code path works in the Chromium Profile Server. This issue debugs and
fixes the crash so DevTools works identically in Plusium.

## Background

### What works

Plusium is a standalone C++ binary (`content/plusium/plusium_main.cc`) that
wraps Chromium's Content API through `libtermsurf_content`, a C library that
subclasses `ContentMainDelegate`, `ContentBrowserClient`, `BrowserMainParts`,
etc. Regular browser tabs work end-to-end: the TUI sends
`web google.com --browser plusium`, the GUI spawns Plusium, Chromium renders the
page, CALayerHost composites it into the terminal pane at 60fps. Navigation,
input forwarding, URL sync, title sync, loading state — all functional.

The IPC routing for DevTools is also correct (Issue 705 Experiment 10). The
GUI's `QueryDevtoolsReply` returns the browser and profile of the inspected tab,
the TUI passes them through, and `handleSetDevtoolsOverlay` routes
`CreateDevtoolsTab` to the correct Plusium process via `tab_to_pane` lookup.

### The crash

When DevTools is requested, the full chain works up to the point where
`ShellDevToolsBindings` tries to attach to the inspected tab:

1. TUI sends `set_devtools_overlay` with `inspected_tab_id=1`
2. GUI looks up tab 1 in `tab_to_pane`, finds the Plusium server, sends
   `CreateDevtoolsTab`
3. Plusium receives the message, `FindByTabId(1)` succeeds
4. `TsBrowserMainParts::CreateDevToolsTab` creates a Shell with the DevTools
   URL, does all compositor/observer setup, then creates
   `new ShellDevToolsFrontend(shell, inspected_wc)`
5. The DevTools frontend page loads, `PrimaryMainDocumentElementAvailable`
   fires, calling `devtools_bindings_->Attach()`
6. `ShellDevToolsBindings::AttachInternal()` calls
   `DevToolsAgentHost::GetOrCreateForTab(inspected_contents_)`
7. **SEGV** inside `WebContentsDevToolsAgentHost::InnerAttach(WebContents*)` at
   a corrupted address (`a7fe5f1dbd82658f`)

The crash signature is `SEGV_ACCERR` — accessing memory at a clearly invalid
pointer. This happened in both Experiment 10 (which used `Show()`) and
Experiment 12 (which matched Profile Server's manual pattern). The crash point
moved slightly between experiments but the root cause is the same: the inspected
`WebContents*` pointer is corrupted or stale by the time the DevTools bindings
try to attach.

### Why Profile Server works

The Chromium Profile Server (`content/chromium_profile_server/`) is a monolithic
C++ binary that owns everything directly. Its `CreateDevToolsTab` receives
`inspected_tab_id` as an integer, looks up `WebContents*` from its own `tabs_`
vector, and creates `new ShellDevToolsFrontend(shell, inspected_contents)` at
the end. The `WebContents*` never crosses a library boundary.

Profile Server also has **forked copies** of several DevTools files:

- `shell_devtools_frontend.h` — constructor moved from private to public
- `shell_devtools_frontend.cc`
- `shell_devtools_bindings.h`
- `shell_devtools_bindings.cc`
- `shell_devtools_manager_delegate.h`
- `shell_devtools_manager_delegate.cc`

Plusium uses the stock `content/shell/browser/` versions of these files (with
only the constructor visibility change from Issue 705 Experiment 12). If Profile
Server's forked DevTools files contain fixes or workarounds beyond the
constructor visibility change, Plusium would be missing them.

### Architecture difference

Plusium's `CreateDevToolsTab` receives `void* inspected` — a raw pointer to
`WebContents` that was cast to `void*` at the C API boundary in
`libtermsurf_content.cc`, passed through the IPC dispatch in `plusium_main.cc`,
and cast back to `WebContents*` inside `TsBrowserMainParts::CreateDevToolsTab`.
The pointer itself is valid at the time of the call (the function successfully
looks up the inspected tab's ID from `tabs_`). But by the time
`PrimaryMainDocumentElementAvailable` fires and `AttachInternal()` runs, the
stored pointer appears corrupted.

### Key files

- `content/libtermsurf_content/ts_browser_main_parts.cc` — `CreateDevToolsTab()`
  (Plusium's implementation)
- `content/chromium_profile_server/browser/shell_browser_main_parts.cc` —
  `CreateDevToolsTab()` (Profile Server's working implementation)
- `content/shell/browser/shell_devtools_frontend.h` — constructor (now public)
- `content/shell/browser/shell_devtools_frontend.cc` — `Show()`,
  `PrimaryMainDocumentElementAvailable()`
- `content/shell/browser/shell_devtools_bindings.cc` — `AttachInternal()`, where
  the crash happens
- `content/chromium_profile_server/browser/shell_devtools_bindings.cc` — Profile
  Server's forked version (may differ from stock)
- `content/plusium/plusium_main.cc` — Plusium binary, IPC dispatch
- `content/libtermsurf_content/libtermsurf_content.cc` — C API exports

### Chromium branch

`146.0.7650.0-issue-705` (forked from `146.0.7650.0-issue-704`)

## Ideas for experiments

1. **Debug the pointer.** Add `LOG(INFO)` in `ShellDevToolsBindings` constructor
   and `AttachInternal()` to print the inspected `WebContents*` address at both
   points. If they differ, the pointer is being corrupted in storage. If they
   match, the object itself was destroyed or relocated between construction and
   attach.

2. **Diff Profile Server's forked DevTools files against stock.** Profile Server
   has forked copies of `shell_devtools_bindings.cc`,
   `shell_devtools_frontend.cc`, and `shell_devtools_manager_delegate.cc`. Diff
   them against the stock `content/shell/browser/` versions. Any differences
   beyond the constructor visibility change could explain why Profile Server
   works and Plusium doesn't.

3. **Look up by tab_id instead of pointer.** Change Plusium's C API to accept
   `inspected_tab_id` (an integer) instead of `void* inspected` for
   `CreateDevToolsTab`. Look up the `WebContents*` internally from
   `TsBrowserMainParts::tabs_` — the same way Profile Server does. This
   eliminates the pointer ever crossing the C API boundary.

4. **Component build investigation.** The build uses
   `is_component_build = true`. `DevToolsAgentHost::GetOrCreateForTab()` lives
   in `libcontent.dylib`, while `ShellDevToolsBindings::AttachInternal()` lives
   in the `plusium` binary. Test whether the pointer is valid when
   `AttachInternal()` is called but gets corrupted crossing the dylib boundary
   into `libcontent.dylib`.

5. **Use Profile Server's forked DevTools files.** If the diff (idea 2) reveals
   significant differences, link Plusium against Profile Server's forked
   DevTools files instead of the stock ones. This is the most direct path to
   matching Profile Server's behavior, though it increases coupling.

## Experiments

### Experiment 1: Diff Profile Server's forked DevTools files against stock

Compared all six forked DevTools files in
`content/chromium_profile_server/browser/` against their stock equivalents in
`content/shell/browser/`.

#### Results

**All six files are functionally identical.** The only differences are:

**`shell_devtools_bindings.h`** — Header guard name only.

**`shell_devtools_bindings.cc`** — Include paths only.

**`shell_devtools_frontend.h`** — Header guard, include path, and an Issue 684
comment on the constructor. No code change.

**`shell_devtools_frontend.cc`** — Include paths only.

**`shell_devtools_manager_delegate.h`** — Header guard and include path only.

**`shell_devtools_manager_delegate.cc`** — Include paths, plus two cosmetic
constants: Android socket name (`chromium_profile_server_devtools_remote` vs
`content_shell_devtools_remote`) and discovery page resource ID
(`IDR_CONTENT_CHROMIUM_PROFILE_SERVER_DEVTOOLS_DISCOVERY_PAGE` vs
`IDR_CONTENT_SHELL_DEVTOOLS_DISCOVERY_PAGE`).

No logic changes. No workarounds. No crash fixes. The `AttachInternal()` code
path is byte-for-byte identical between Profile Server's fork and stock.

#### What this eliminates

- **Idea #5 is dead.** Using Profile Server's forked files would change nothing
  — they're the same code with different include paths.
- The crash is NOT caused by Plusium using stock DevTools files.

#### What this tells us

The difference must be in how the two binaries call into the DevTools code, not
in the DevTools code itself. The remaining hypotheses are:

1. **The `void*` pointer boundary** (idea #3) — Profile Server never casts
   `WebContents*` to `void*` and back. Plusium does, through the C API.
2. **The component build** (idea #4) — Profile Server links DevTools code
   statically from its own fork. Plusium links stock DevTools from
   `content_shell_lib`, which is a shared library in component builds.
3. **The pointer itself** (idea #1) — we still don't know if the pointer is
   corrupted in storage or if the object was destroyed.

### Experiment 2: Pass tab_id instead of void\* for DevTools

`plusium_main.cc` does `FindByTabId(inspected_tab_id)` to get a `void* handle`,
passes it through the C API, where `CreateDevToolsTab` casts it back to
`WebContents*` and searches `tabs_` again to find the tab ID. A pointless
round-trip through `void*`. Pass the integer directly and look up `WebContents*`
inside `TsBrowserMainParts::CreateDevToolsTab` — the same way Profile Server
does.

#### What to change

**`libtermsurf_content.h`** — Change `ts_web_contents_t inspected` to
`int inspected_tab_id` on `ts_create_devtools_web_contents`.

**`libtermsurf_content.cc`** — Pass `inspected_tab_id` through.

**`ts_browser_main_parts.h`** — Change `void* inspected` to
`int inspected_tab_id`.

**`ts_browser_main_parts.cc`** — Remove the `void*` cast. Look up `WebContents*`
from `tabs_` by `inspected_tab_id`, matching Profile Server's pattern.

**`plusium_main.cc`** — Pass `m.inspected_tab_id()` directly instead of
`inspected->handle`. Remove the `FindByTabId` + handle lookup.

#### Verification

1. `autoninja -C out/Default plusium` — compiles clean.
2. `web google.com --browser plusium`, then `d` — DevTools opens without crash.
3. Hover over elements in DevTools — highlights on inspected page.

#### Result: Success — DevTools opens without crash

Passing `int inspected_tab_id` instead of `void* inspected` through the C API
fixed the crash. DevTools opens in Plusium without a SEGV.

The root cause was the `void*` round-trip. `plusium_main.cc` looked up the
inspected tab by ID, extracted its `void* handle` (a `WebContents*` cast), and
passed that pointer through the C API into `CreateDevToolsTab`, which cast it
back to `WebContents*`. By the time `ShellDevToolsBindings::AttachInternal()`
ran (asynchronously, after the DevTools DOM loaded), the pointer was corrupted.

The fix eliminates the pointer crossing entirely. `plusium_main.cc` passes the
integer tab ID, and `CreateDevToolsTab` looks up `WebContents*` from its own
`tabs_` vector — the same pattern Profile Server uses. The `WebContents*` stays
inside `TsBrowserMainParts` and never crosses the C boundary.

Five files changed, ~10 lines each. The simplest experiment in the issue solved
a crash that persisted through two prior issues and four experiments.

### Experiment 3: Audit remaining void\* usage in the C API

Now that passing `void*` across the C boundary caused the DevTools crash, audit
the entire C API (`libtermsurf_content.h`) for other `void*` usage.

#### Results

Three categories of `void*` in the API:

**`ts_web_contents_t` (alias for `void*`)** — 15 functions. Every tab operation
passes a `WebContents*` disguised as `void*`: create, destroy, navigate, mouse,
scroll, keyboard, focus, color scheme, resize. All 6 callbacks also pass `wc`
back to identify which tab fired. Plusium's `plusium_main.cc` stores these
handles in `TabEntry` structs and passes them back on every call.

**`ts_browser_context_t` (alias for `void*`)** — 4 functions. Profile
create/destroy and both tab creation functions take a `BrowserContext*` as
`void*`.

**`void* user_data`** — 8 functions. Standard C callback pattern where the
caller owns the data and casts it back. Not a concern.

#### Risk assessment

The DevTools crash was uniquely dangerous because `ShellDevToolsBindings` stored
the `void*`-derived pointer and used it asynchronously — after the DevTools DOM
loaded. The other functions use their handles synchronously (forward mouse
event, load URL, etc.), so the pointer is used immediately and is less likely to
go stale.

However, all `ts_web_contents_t` and `ts_browser_context_t` handles carry the
same fundamental risk. They could all be replaced with integer ID lookups —
`tab_id` for tabs, a `context_id` for profiles — matching the pattern that fixed
DevTools. This would make the C API boundary fully integer-based, with no C++
pointers ever crossing it.

## Conclusion

The DevTools crash in Plusium was caused by passing a `WebContents*` pointer
through the C API boundary as `void*`. The pointer was stored by
`ShellDevToolsBindings` and dereferenced asynchronously when the DevTools
frontend DOM loaded. By that time, the pointer was corrupted.

The fix was simple: pass `int inspected_tab_id` instead of `void* inspected` and
look up `WebContents*` internally from `TsBrowserMainParts::tabs_` — the same
pattern the Chromium Profile Server uses. Five files changed, ~10 lines each.

Three experiments:

1. **Diff Profile Server's DevTools files against stock** — all six files are
   functionally identical. Eliminated the hypothesis that Profile Server had
   special DevTools fixes.
2. **Pass tab_id instead of void\*** — fixed the crash. The `void*` round-trip
   was the root cause.
3. **Audit remaining void\* usage** — 15 functions still use `ts_web_contents_t`
   (`void*`) and 4 use `ts_browser_context_t` (`void*`). All are synchronous and
   low-risk. Added safety comments to every function in `libtermsurf_content.h`
   documenting the sync-only rule and referencing this issue as a warning.
