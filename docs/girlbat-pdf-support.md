# Girlbat PDF Support

Girlbat uses Ladybird's built-in PDF path. Current Ladybird source routes
`application/pdf` and `text/pdf` documents through an internal
`resource://ladybird/pdfjs/web/viewer.html` page backed by pdf.js.

## Source Evidence

- `vendor/ladybird/Libraries/LibWeb/DOM/DocumentLoading.cpp`
  - `can_load_document_with_type()` accepts `application/pdf` and `text/pdf`.
  - `load_pdf_document()` loads `resource://ladybird/pdfjs/web/viewer.html` and
    dispatches the PDF response to the viewer.
- `vendor/ladybird/Libraries/LibWeb/Page/Page.h`
  - `Page::m_pdf_viewer_supported` currently defaults to `true`.
- `vendor/ladybird/Libraries/LibWeb/HTML/Navigator.cpp`
  - `navigator.pdfViewerEnabled` returns `window.page().pdf_viewer_supported()`.
- `vendor/ladybird/Libraries/LibWeb/HTML/MimeTypeArray.cpp` and
  `vendor/ladybird/Libraries/LibWeb/HTML/PluginArray.cpp`
  - PDF MIME types and plugin names are exposed only when the same
    `pdf_viewer_supported` flag is true.

## Runtime Assets

PDF navigation depends on runtime pdf.js resources. In particular,
`load_pdf_document()` calls `MUST(Core::Resource::load_from_uri(...))` for
`resource://ladybird/pdfjs/web/viewer.html`, so a missing runtime asset can
abort the WebContent process.

Girlbat's Cargo build stages pdf.js assets into the local runtime resource root
used by `target/debug/girlbat` and `target/release/girlbat`:

```text
target/Resources/ladybird/pdfjs/
```

The smoke test asks Girlbat for its resolved runtime root with
`--termsurf-resource-root-smoke` and verifies these files before navigating to a
PDF:

- `ladybird/pdfjs/web/viewer.html`
- `ladybird/pdfjs/web/viewer.mjs`
- `ladybird/pdfjs/web/viewer.css`
- `ladybird/pdfjs/web/pdfjs-ladybird-transport.mjs`
- `ladybird/pdfjs/build/pdf.mjs`
- `ladybird/pdfjs/build/pdf.worker.mjs`

## Smoke Coverage

Run:

```bash
scripts/test-girlbat-pdf-smoke.py
```

The smoke test proves:

- Girlbat reports `navigator.pdfViewerEnabled === true` through the
  `ConsoleMessage` protocol path.
- Girlbat exposes `application/pdf` and `text/pdf` through
  `navigator.mimeTypes`.
- Standard PDF plugin names are present when the Ladybird PDF flag is true.
- The runtime pdf.js assets are present in the resource root Girlbat reports.
- A local one-page PDF served as `application/pdf` is requested by Girlbat.
- No `RendererCrashed` event is observed during the PDF navigation window.
- Girlbat remains queryable after the PDF navigation attempt.

## Limitations

This is smoke evidence, not full PDF parity with Roamium or Surfari.

The current headless protocol smoke proves server access, no crash, and
post-navigation responsiveness. It does not prove visible PDF rendering in
Ghostboard, multi-page lazy rendering, selection, find-in-PDF, PDF download,
print, accessibility, zoom behavior, or Chrome/Safari-level PDF viewer parity.
