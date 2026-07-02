# Girlbat Capabilities

Girlbat is the Ladybird-backed TermSurf engine. Its current state is a protocol
completion baseline for Issue 884, not a claim that Ladybird/Girlbat is
feature-equivalent to Roamium or Surfari.

This document is Issue 884's documented capability and limitation mechanism.
Experiment 34 does not add machine-readable runtime capability negotiation to
the TermSurf protocol. If WebTUI, Ghostboard, or release tooling later need
runtime negotiation, that should be a separate protocol/product issue.

## Current Proof

- The [Girlbat protobuf coverage matrix](girlbat-protobuf-coverage.md) has no
  `Missing` rows and is inventory-checked by
  `scripts/build-girlbat-protobuf-coverage.py --check`.
- Lifecycle, tab creation, tab close, navigation, query, resize, focus,
  visibility, color-scheme, input, callbacks, and shutdown paths have targeted
  protocol or headless smoke coverage. Many rows remain partial because visible
  runtime parity is not yet proven.
- Devtools creation is explicitly unsupported. Girlbat creates no phantom
  devtools tab and leaves existing tab state coherent.
- HTTP Basic Auth is explicitly unsupported. Girlbat does not synthesize auth
  requests without a Ladybird embedder challenge hook, and stale replies are
  deterministic no-ops.
- PDF has smoke evidence only. Girlbat verifies Ladybird pdf.js assets,
  `navigator.pdfViewerEnabled`, PDF MIME/plugin exposure, local
  `application/pdf` navigation access, no renderer crash during the smoke
  window, and post-navigation responsiveness. See
  [Girlbat PDF support](girlbat-pdf-support.md).
- Renderer crash, console messages, title changes, hover cursor/target URL,
  JavaScript dialogs, and basic input forwarding have targeted proof for the
  exercised slices.
- Visible rendering has structural proof through the IOSurface side channel,
  Ghostboard AppKit presentation path, and a normal local HTTP page launched
  through `web --browser girlbat` in Debug Ghostboard. See
  [Girlbat rendering strategy](girlbat-rendering-strategy.md) and
  [Girlbat render-surface transport](girlbat-render-surface-transport.md).

## Current Limits

Girlbat should not be presented as a drop-in replacement for Roamium or Surfari
yet.

- Most browser API families from
  [Issue 841](../issues/0841-browser-api-implementation-audit/01-cross-engine-browser-api-audit.md)
  are unproven for Girlbat. The probe roadmap is tracked in
  [Girlbat browser API probe plan](girlbat-browser-api-probe-plan.md).
- Headless protocol proofs do not prove Chrome/Safari-level web compatibility.
  They show that Girlbat receives, routes, replies to, or explicitly rejects
  TermSurf protocol messages coherently.
- Several web APIs are intentionally future product/security scope, including
  password-manager integration, WebAuthn/passkeys, media capture, geolocation,
  external protocol launches, Payment Request, native sharing, notifications,
  push, and hardware/device APIs.
- PDF support is not yet proven for visible rendering, multi-page lazy
  rendering, selection, find-in-PDF, save, print, accessibility, or zoom parity.
- Generic page printing, downloads, popups/new windows, file input, clipboard,
  storage/profile isolation, service workers, permissions-policy, browser
  identity, fullscreen, and pointer lock still need Girlbat-specific probes
  before support can be claimed.

## Operational Guidance

- Treat `Unsupported` protobuf rows as deterministic behavior, not hidden
  crashes or hangs.
- Treat `Partial` protobuf rows as usable experiment evidence only for the
  tested slice named in the row.
- Treat `source-evidence-only` browser API rows as research leads, not runtime
  proof.
- Keep release notes and UI language conservative until screenshot/readback
  evidence, continuous frame delivery, and browser API probes exist.
