# Issue 644: Simplified C++ Profile Server

## Goal

Replace the Content Shell fork with a minimal, purpose-built C++ profile server.
The current `chromium_profile_server` carries ~100 Content Shell files we never
modify. Strip it down to only what TermSurf needs: a thin executable that
creates BrowserContexts, manages WebContents, handles XPC, and streams CAContext
IDs back to the GUI. No Shell windows, no DevTools frontend, no Content Shell
boilerplate.

## Background

### The Content Shell problem

The current Chromium Profile Server (`chromium_profile_server`) is a fork of
Content Shell — Chromium's reference embedder. Content Shell is designed to be a
complete minimal browser with its own window, toolbar, DevTools, and test
infrastructure. TermSurf uses none of that. We subclass a few Content Shell
classes, override path resolution, and add XPC handling. But the build target
pulls in ~100 files of Content Shell code: `ShellBrowserMainParts`,
`ShellContentBrowserClient`, `ShellWebContentsViewDelegate`,
`ShellDevToolsFrontend`, `ShellJavaScriptDialog`, and dozens more.

This creates three problems:

1. **Upgrade friction.** Every Chromium version upgrade risks merge conflicts in
   Content Shell files we don't own. The more Content Shell code we depend on,
   the more conflicts we face.

2. **Complexity.** Understanding what our server actually does requires
   separating our ~1,050 lines from Content Shell's thousands. New contributors
   see 100+ files and can't tell which ones matter.

3. **Unnecessary code.** Content Shell creates Shell windows, handles DevTools,
   manages JavaScript dialogs, and implements test-specific behaviors. None of
   this is relevant to a headless profile server that streams CAContext IDs over
   XPC.

### What Issues 642–643 taught us

Issues 642–643 attempted to solve this by rewriting the server in Zig. The
Zig-to-Chromium bridge works (dlopen, C API shim, WebContents creation,
CAContext IDs), but XPC integration never worked end-to-end across 7
experiments. The failure pattern: standalone Chromium works, but the full GUI →
XPC → server → GUI pipeline doesn't.

The lesson isn't that Zig is wrong — it's that the rewrite was too ambitious.
Changing the language AND the build system AND the deployment AND the XPC
implementation all at once made failures hard to diagnose. A simpler approach:
keep C++, keep the working build system, but strip out Content Shell.

### What we actually need

The profile server needs exactly these capabilities:

- **ContentMain entry point** — initialize Chromium
- **BrowserContext** — create isolated browser profiles with persistent storage
- **WebContents** — create headless web pages, navigate, resize
- **Compositor** — persistent compositor for stable CAContext IDs
- **XPC** — connect to the GUI gateway, receive commands, send back events
- **Input forwarding** — route mouse, keyboard, scroll events to WebContents
- **Observation** — URL, title, loading state, cursor changes → XPC messages

Content Shell provides all of this, but buried under layers of Shell-specific
abstractions. A simplified server implements these directly against the Content
API.

## Approach

Create a new directory `chromium/src/content/termsurf_browser/` with a minimal
Content API embedder. Start from scratch — not by forking Content Shell, but by
implementing only the required Content API interfaces. Use the existing
`chromium_profile_server` as a reference for what works, but don't copy its
Content Shell dependencies.

The key Content API classes to implement:

- `ContentMainDelegate` — app initialization, creates the browser client
- `ContentBrowserClient` — creates the BrowserContext, configures the browser
- `BrowserMainParts` — lifecycle hooks (pre-main-message-loop, post-startup)
- `BrowserContext` — profile storage, cookie/cache path configuration
- `WebContentsDelegate` — handles navigation, title changes, new windows
- `WebContentsObserver` — observes loading state, URL changes

Everything else — Shell windows, DevTools frontend, JavaScript dialogs, test
infrastructure — is omitted.

## Experiments

### Experiment 1: Restore the Working C++ Profile Server

Before changing anything, get back to a known-good state. Issues 642–643 left
behind uncommitted Zig code in the main repo and switched the Chromium fork to
branches with the `zig_profile_server` target. The existing C++ profile server
(`chromium_profile_server`) still works — we just need to point at the right
branch and clean up.

#### Clean up the main repo

Delete all Zig profile server code from Issues 642–643:

**Delete the `browser/` directory entirely.** This was created for the Zig
profile server and is no longer needed. Committed files (`browser/build.zig`,
`browser/src/main.zig`) and uncommitted files (`browser/build.zig.zon`,
`browser/macos/Info.plist`, `browser/macos/PkgInfo`) all go.

**Restore `gui/src/apprt/xpc.zig`.** The uncommitted change points the server
path at `Zig Profile Server.app`. Revert it to the committed version, which
points at `Chromium Profile Server.app`:

```
"{s}/dev/termsurf/chromium/src/out/Default/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server"
```

#### Create the Chromium branch

The last branch with a working C++ profile server is `146.0.7650.0-issue-639`
(open new-tab links in same tab). The `issue-642` and `issue-643` branches have
the `zig_profile_server` target, not `chromium_profile_server`.

Create `146.0.7650.0-issue-644` from `146.0.7650.0-issue-639`. Add it to
`docs/chromium.md`.

#### Build and verify

```bash
cd chromium/src
git checkout 146.0.7650.0-issue-644
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default chromium_profile_server

cd ../../gui && zig build
open zig-out/TermSurf.app
```

Type `web google.com` in a terminal pane. Expected: web page renders, mouse
clicks work, keyboard input works, URL bar updates, page title syncs. All
features that were working before Issues 642–643 should work again.

#### Pass criteria

The C++ profile server works end-to-end with all previously-working features:
web rendering, mouse input, keyboard input, resize, navigation, URL sync, page
title sync.

#### Result: Pass

The C++ profile server works end-to-end. Web rendering, mouse input, keyboard
input, resize, navigation, URL sync, and page title sync all function correctly.
We are back to a known-good baseline.

### Experiment 2: Research the Existing Profile Server

Before building anything new, understand what we have. The current
`chromium_profile_server` is a fork of Content Shell with TermSurf-specific
modifications layered on top. This experiment maps out what Content Shell
provides, what TermSurf actually uses, and what can be dropped.

#### Questions to answer

1. **What TermSurf files exist?** List every file in
   `content/chromium_profile_server/` that we wrote or modified. For each file,
   summarize what it does in one sentence.

2. **What Content Shell files do we depend on?** Trace the `#include` and
   subclass chains from our files into `content/shell/`. For each Content Shell
   file we touch, document why — what base class or function do we use from it?

3. **What Content Shell files are pulled in transitively?** The BUILD.gn target
   depends on Content Shell sources. Many of those sources pull in more Content
   Shell code. List the full set of Content Shell files that end up in the
   build, grouped by category (browser, renderer, DevTools, test infra, UI,
   etc.).

4. **What Content API interfaces do we actually implement?** List the pure
   Content API classes (from `content/public/`) that our server needs:
   `ContentMainDelegate`, `ContentBrowserClient`, `BrowserMainParts`,
   `BrowserContext`, `WebContentsDelegate`, `WebContentsObserver`, etc. For
   each, note whether we implement it directly or inherit it through a Content
   Shell subclass.

5. **What Content Shell functionality do we rely on?** Some Content Shell code
   may do things we actually need — like setting up the network stack, creating
   the GPU process, or configuring the compositor. Identify any Content Shell
   logic that would need to be replicated in a from-scratch implementation.

6. **Is simplification feasible?** Given the answers above, is it realistic to
   implement a standalone Content API embedder that replaces Content Shell? What
   are the risks — are there Content Shell behaviors we depend on that would be
   hard to replicate?

#### Process

Read the source code in `chromium/src/content/chromium_profile_server/` and
trace its dependencies into `content/shell/` and `content/public/`. Use the
BUILD.gn files to understand what gets compiled. Read the Content Shell source
files we subclass to understand what behavior we inherit.

#### Pass criteria

A written analysis answering all six questions above, with enough detail to
design Experiment 3 (the simplified implementation). The analysis should make it
clear exactly which Content API interfaces to implement and what Content Shell
behavior (if any) needs to be replicated.
