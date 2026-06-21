# WebKit Hosting Context Proof

This is the Issue 756 Experiment 2 harness. It proves whether a WebKit-owning
process can export a Core Animation/WebKit render context and display that
surface in a separate host process.

The harness builds one Objective-C binary with two modes:

- `--owner` creates a `WKWebView`, loads deterministic local HTML, exports the
  owner layer through a private Core Animation remote context, and launches the
  host process.
- `--host <context-id>` creates a separate Cocoa window, creates a `CALayerHost`
  for the exported context ID, and displays the hosted surface without creating
  a `WKWebView`.

This is intentionally not Surfari and not `libtermsurf_webkit`. It is a narrow
compositor proof.

## Build

From the TermSurf repo root:

```bash
surfari-proofs/hosting-context/build.sh
```

## Run

```bash
surfari-proofs/hosting-context/build/WebKitHostingProof --owner
```

The owner process writes a log to stdout and launches the host process. The host
prints its own log to stdout when run directly with `--host`.

## Stress Run

Experiment 3 uses stress mode:

```bash
surfari-proofs/hosting-context/build/WebKitHostingProof --owner --stress
```

Stress mode keeps the same two-process architecture, then runs deterministic
owner resize, host resize, navigation, host hide/show, and clean termination
steps.
