+++
status = "open"
opened = "2026-05-29"
+++

# Issue 792: Inline PDF support via a separable extensions browser system

## Goal

Render PDFs inline in Roamium — `web localhost/file.pdf` shows the document in
the overlay, not a blank white page or a download — with clean, contained code
that does not regress any existing functionality. We do this by staying on
**content_shell** and adding Chromium's **extensions + guest-view +
`PdfViewerStreamManager`** browser system as a **separable layer** on the
current embedder, modeled on how **Electron** wires that layer onto its own
content-embedder.

## Principles

Two principles govern every decision in this issue:

1. **We are using content_shell.** TermSurf stays on `content/shell`. We do
   **not** re-base or rewrite on app_shell (`extensions/shell`) — Issue 791
   settled that. The extensions/guest-view/`PdfViewerStreamManager` system is
   added as a **separable layer** on TermSurf's existing `Ts*` classes and
   per-tab CALayerHost window model.

2. **When in doubt, do it like Electron.** Electron is the primary reference,
   and it reinforces principle 1: Electron is itself a content-embedder (not
   app_shell) that bolts on its **own** extensions glue
   (`ElectronExtensionSystem`, `ElectronExtensionsBrowserClient`, its
   `streams_private` PDF stream handoff, its component-extension resource
   manager) and patches Chromium's PDF stream path to call that glue. That is
   exactly the shape this issue needs — embedder-owned extension wiring on a
   minimal content base, not Chrome's full browser stack. Chromium's
   `extensions/shell` (app_shell) is a useful Chromium-maintained
   cross-reference, but where the two disagree or Electron is clearer, **follow
   Electron**.

3. **If doing it like Electron is hard and takes a lot of work, do not complain.
   Do it.** This is a real port, not a hook — Issues 789/790 proved the cheap
   shortcuts do not work. The amount of work is not a reason to deviate. Put in
   the work, follow Electron, ship inline PDF.

4. **Every experiment gets Claude review before and after.** For every
   experiment design, ask Claude to review it, fix all real issues Claude finds,
   and do not implement until Claude agrees the design is good. After completing
   an experiment, ask Claude to review the implementation, verification, and
   recorded result; fix all real issues Claude finds, and do not proceed to the
   next experiment until Claude agrees the output is good.

## Background

### How we got here

Inline PDF has been pursued and parked across four now-closed issues. This issue
resumes the work with a settled architectural direction.

- [Issue 776: PDF files show blank white screen](../0776-pdf-not-loading/README.md)
  — investigation. Proved no single Chromium toggle fixes it; TermSurf needs an
  Electron-style embedder layer.
- [Issue 789: Electron-Style PDF Viewer Infrastructure](../0789-electron-style-pdf-viewer/README.md)
  — built the stream handoff, viewer shell, `chrome://resources` loading, and a
  `mimeHandlerPrivate` shim; the viewer reached `getStreamInfo()`.
- [Issue 790: PDF Viewer Mojo Bindings / OOPIF](../0790-pdf-viewer-mojo-bindings/README.md)
  — got Mojo JS bindings, OOPIF viewer mode, and the internal PDF plugin to
  instantiate; stopped at the `IsPdfRenderer()` process-model layer. **Decisive
  finding:** completing inline PDF requires adopting Chromium's canonical
  extensions + guest-view + `PdfViewerStreamManager` stack. Issue 790 then
  restored the app to the pre-PDF baseline (`148.0.7778.97-issue-784`) and
  deferred PDF pending a foundation decision. The PDF work is preserved as 11
  branches + `chromium/patches/issue-789/`.
- [Issue 791: Evaluate re-basing on app_shell](../0791-app-shell-foundation/README.md)
  — investigation. Audited the embedder's content/shell coupling and concluded:
  **do not re-base or rewrite on app_shell.** Coupling is shallow and
  FFI-insulated from roamium; all rendering/input/compositing lives on
  `content/public` + `ui::` (preserved trivially); app_shell's only real benefit
  (the extensions/guest-view wiring PDF needs) is **separable** and comes
  bundled with a single-window macOS model that conflicts with TermSurf's
  per-tab CALayerHost overlay architecture.

### The settled direction

From Issue 791's conclusion, the path to inline PDF is:

> Stay on content_shell and add the extensions browser system as a layer, using
> `extensions/shell` as the reference. A separate issue should pick that up when
> PDF work resumes.

This is that issue. Per the principles above, the primary reference for _how_ to
add that layer is **Electron** (a content-embedder with its own extensions
glue); `extensions/shell` is a secondary, Chromium-maintained cross-reference.

### Why the extensions system is the gating prerequisite

Chromium's inline PDF is not a plugin toggle — it is an OOPIF flow that rides on
the extensions/guest-view infrastructure:

```
PdfNavigationThrottle
  → intercepts the application/pdf response, claims the stream
  → PdfViewerStreamManager  (browser-side stream registry, keyed by frame)
  → the PDF extension (component extension) loads the viewer in an OOPIF
  → guest-view / MimeHandlerView hosts the viewer frame
  → the internal PDF plugin renders, talking Mojo to the browser
```

Issue 790 reached the point where the internal plugin instantiated, then hit the
`pdf::IsPdfRenderer()` process-model gate (`--pdf-renderer` switch) and the
absence of `PdfViewerStreamManager` / a registered PDF extension / guest-view
hosting. Those are exactly what the extensions browser system provides.
app_shell pre-wires them; content_shell does not. The 791 audit confirmed this
layer is separable from the window/shell layer, so it can be added to the
current embedder without re-basing.

## Architecture

### What "add the extensions browser system as a layer" means

On top of the Issue 784 `libtermsurf_chromium` (which is
`TsBrowserClient : content::ShellContentBrowserClient`, etc.), add the
extensions integration the way **Electron** does — embedder-owned glue on a
content base — onto TermSurf's existing `Ts*` classes and per-tab CALayerHost
window model, **not** app_shell's `AppWindow`/`DesktopController`:

- `ShellExtensionsBrowserClient` / `ShellExtensionsClient` equivalents —
  register the extensions system with the browser process.
- `ShellExtensionSystem` equivalent — load and run (component) extensions in the
  `ShellBrowserContext`.
- Extension URL-loader factories (`CreateExtensionNavigationURLLoaderFactory`
  and worker variants) — so `chrome-extension://` viewer resources load.
- `guest_view` / `MimeHandlerView` wiring — so the PDF viewer can be hosted as a
  guest frame.
- The **PDF component extension** registration — so `application/pdf` becomes
  externally handled and the `PdfNavigationThrottle` → `PdfViewerStreamManager`
  flow engages.
- The `--pdf-renderer` process-model pieces from the parked Issue 790 work.

The parked **Issue 790 Experiment 6** branch is the closest prior art for the
stream/extension portion and should be mined rather than rebuilt from scratch.

### Constraints / non-goals

- **No regressions.** Every Issue 715–789 feature (CALayerHost compositing, the
  Unix-socket/protobuf protocol, input forwarding, DevTools, dark mode, popups,
  multi-profile, the badge stub) must keep working. The baseline to protect is
  `148.0.7778.97-issue-784`.
- **Stay on content_shell.** Do not re-base/rewrite on app_shell (Issue 791
  decision). Model the extensions layer on **Electron**; use `extensions/shell`
  only as a secondary source-level cross-reference.
- **Contained code.** The extensions layer should be added as cleanly separable
  `Ts*`/`ts_*` additions, not smeared across the embedder.
- **Chromium branch discipline.** Every Chromium-modifying experiment forks the
  most relevant recent branch to `148.0.7778.97-issue-792` (or a per-experiment
  variant), is added to `chromium/README.md`, and is archived to
  `chromium/patches/`.
- **Chromium-engine only.** This is Roamium/Chromium-specific; the protocol, GUI
  (wezboard), TUI (webtui), and future engines (Surfari/Gecko/Ladybird) are
  unaffected.

### Open questions the experiments must resolve

- How much of Electron's extensions glue (cross-referenced against
  `extensions/shell`) is the **minimum** needed to make `application/pdf`
  externally handled (the cheapest decisive spike)?
- Does standing up the extensions system on the existing per-tab window model
  introduce any conflict with the CALayerHost overlay path (the riskiest
  interaction)?
- Can the parked Issue 790 Exp 6 stream/extension code be lifted onto the 784
  baseline cleanly, or does it need rework against the now-present extensions
  system?

## Experiments

- [Experiment 1: Map the Electron PDF extension layer](01-map-electron-pdf-extension-layer.md)
  — **Pass**
- [Experiment 2: Stand up the extension foundation](02-stand-up-extension-foundation.md)
  — **Pass**
- [Experiment 3: Register the PDF component extension](03-register-pdf-component-extension.md)
  — **Pass**
- [Experiment 4: Load PDF viewer resource bytes](04-load-pdf-resource-pack.md) —
  **Pass**
- [Experiment 5: Serve PDF extension resources](05-serve-pdf-extension-resources.md)
  — **Designed**
