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
