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
current embedder, using `extensions/shell` (app_shell) as the reference
implementation rather than the base.

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
  (the extensions/guest-view wiring PDF needs) is **separable** and comes bundled
  with a single-window macOS model that conflicts with TermSurf's per-tab
  CALayerHost overlay architecture.

### The settled direction

From Issue 791's conclusion, the path to inline PDF is:

> Stay on content_shell and add the extensions browser system as a layer, using
> `extensions/shell` as the reference. A separate issue should pick that up when
> PDF work resumes.

This is that issue.

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
hosting. Those are exactly what the extensions browser system provides. app_shell
pre-wires them; content_shell does not. The 791 audit confirmed this layer is
separable from the window/shell layer, so it can be added to the current
embedder without re-basing.

## Architecture

### What "add the extensions browser system as a layer" means

On top of the Issue 784 `libtermsurf_chromium` (which is
`TsBrowserClient : content::ShellContentBrowserClient`, etc.), cherry-pick the
extensions integration that `extensions/shell` performs — but onto TermSurf's
existing `Ts*` classes and per-tab CALayerHost window model, **not** app_shell's
`AppWindow`/`DesktopController`:

- `ShellExtensionsBrowserClient` / `ShellExtensionsClient` equivalents —
  register the extensions system with the browser process.
- `ShellExtensionSystem` equivalent — load and run (component) extensions in the
  `ShellBrowserContext`.
- Extension URL-loader factories
  (`CreateExtensionNavigationURLLoaderFactory` and worker variants) — so
  `chrome-extension://` viewer resources load.
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
  decision). Use `extensions/shell` only as a source-level reference.
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

- How much of the `extensions/shell` browser wiring is the **minimum** needed to
  make `application/pdf` externally handled (the cheapest decisive spike)?
- Does standing up the extensions system on the existing per-tab window model
  introduce any conflict with the CALayerHost overlay path (the riskiest
  interaction)?
- Can the parked Issue 790 Exp 6 stream/extension code be lifted onto the 784
  baseline cleanly, or does it need rework against the now-present extensions
  system?

## Experiments

_None yet. The first experiment will be designed once the cheapest decisive
spike is scoped — standing up the minimal extensions browser system and
registering the PDF component extension so `application/pdf` becomes externally
handled. Each experiment is added here as its own `NN-{slug}.md` file when
created._
