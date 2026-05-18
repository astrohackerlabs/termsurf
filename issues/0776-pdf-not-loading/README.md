+++
status = "open"
opened = "2026-04-11"
+++

# Issue 776: PDF files show blank white screen instead of rendering

## Goal

Opening a local PDF file via `web file.pdf` should render the PDF content, not a blank white screen.

## Background

When a user runs `web file.pdf` (or any local PDF path), the browser pane opens but displays a blank white screen instead of rendering the PDF. The navigation appears to work (no error is shown), but no PDF content is visible.

Chromium normally has a built-in PDF viewer (PDFium) that renders PDFs inline. The blank screen suggests either:

1. The PDF viewer plugin is not enabled or not included in the Chromium build/configuration used by Roamium.
2. The `file://` URL scheme is not being handled correctly for PDF MIME types.
3. The PDF viewer requires UI chrome (toolbar, scroll handling) that isn't available in the embedded context.
4. Content Security Policy or sandboxing restrictions are blocking the PDF plugin from loading.

## Analysis

Possible areas to investigate:

- **Chromium PDF plugin registration** — Check whether `libtermsurf_chromium` enables the PDF viewer plugin (`chrome_pdf`) during browser initialization. Headless or minimal embeddings often skip plugin registration.
- **MIME type handling** — Verify that `file://` URLs with `.pdf` extension are being served with `application/pdf` MIME type and routed to the PDF viewer.
- **Plugin process sandboxing** — The PDF viewer runs in a utility process; sandbox restrictions in the TermSurf build may prevent it from launching.
- **Alternative approach** — If enabling the built-in viewer is complex, consider whether PDF.js (Mozilla's JavaScript PDF renderer) could work as a fallback.
