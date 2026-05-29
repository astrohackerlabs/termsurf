# Experiment 22: Trace Wrapper Body Delivery

## Description

Experiment 21 proved that the intercepted PDF navigation now commits as an HTML
document and opens an HTML parser:

```text
[issue-792-exp21] document-commit url=http://127.0.0.1:9787/bitcoin.pdf mime_type=text/html document_class=html is_for_external_handler=0 child_count=0
[issue-792-exp21] document-parser-open url=http://127.0.0.1:9787/bitcoin.pdf mime_type=text/html parser=html
```

It also proved that the generated wrapper payload is non-empty and contains the
expected declarative shadow-root iframe:

```text
[issue-792-exp19] wrapper-payload ... bytes=536 has_template=1 has_iframe=1 has_about_blank=1 has_internal_id=1 has_pdf_extension_url=1
```

The first missing transition is between `document-parser-open` and
`declarative-shadow-root`. That narrows the problem to body delivery and parser
ingestion: the HTML parser exists, but the wrapper bytes are not reaching the
HTML tree builder path that would see `<template shadowrootmode="closed">`.

Experiment 22 is diagnostic-only. It must not change the URL loader throttle,
MIME type, wrapper payload, data pipe contents, parser scheduling, shadow-root
behavior, iframe creation, stream manager behavior, or extension startup.

This experiment must receive Claude design review before implementation. After
implementation and result recording, Claude must review the completed output
before any next experiment is designed.

## Changes

1. Create the Chromium implementation branch.

   Start from the accepted Experiment 21 branch:

   ```bash
   git -C chromium/src checkout 148.0.7778.97-issue-792-exp21
   git -C chromium/src checkout -b 148.0.7778.97-issue-792-exp22
   ```

   Add the branch to `chromium/README.md` only after the branch builds and the
   result is accepted.

2. Preserve Experiment 19-21 diagnostics.

   Keep the existing logs that prove:

   ```text
   components-resource-pak loaded=1
   wrapper-payload bytes=536 has_iframe=1
   document-commit mime_type=text/html document_class=html
   document-parser-open parser=html
   ```

3. Instrument body-loader startup.

   In `third_party/blink/renderer/core/loader/document_loader.cc`, add a narrow
   `[issue-792-exp22]` log around every `body_loader_->StartLoadingBody(this)`
   call that could apply to the wrapper navigation.

   Required log:

   ```text
   [issue-792-exp22] body-loader-start url=<url> mime_type=<mime> reason=<commit|preload|mhtml|resume> has_body_loader=<0|1> is_static_data=<0|1> loading_empty=<0|1> is_main_frame=<0|1>
   ```

   Gate this log with the same wrapper-document predicate used by Experiment 21:
   PDF MIME, external handler, the PDF extension ID, or the local `bitcoin.pdf`
   fixture URL. The fixture-specific URL gate is acceptable here because this is
   temporary diagnostic code.

   If a call site's semantics do not cleanly map to
   `commit|preload|mhtml|resume`, use a more specific label based on the
   surrounding code, such as `mhtml-archive`, `restart`, or
   `post-commit-preload`. The label is only for differentiating call sites in
   the log.

4. Instrument body data arrival.

   In `DocumentLoader::BodyDataReceivedImpl(...)`, log whether bytes arrive from
   the navigation body loader before buffering or commit.

   Required log:

   ```text
   [issue-792-exp22] body-data-received url=<url> encoded_size=<n> parser_blocked=<n> in_commit_data=<0|1> has_template=<0|1> has_iframe=<0|1> has_shadowrootmode=<0|1> has_internal_id=<0|1> sample=<short escaped prefix>
   ```

   The marker checks should inspect only the current chunk. The `sample` must be
   short, escaped, and capped at 64 bytes so logs stay readable.

5. Instrument buffering and commit.

   In `DocumentLoader::ProcessDataBuffer(...)` and
   `DocumentLoader::CommitData(...)`, log whether incoming bytes are buffered,
   committed, or skipped because the parser/frame is unavailable.

   Required logs:

   ```text
   [issue-792-exp22] process-data-buffer url=<url> has_data=<0|1> parser_blocked=<n> in_commit_data=<0|1> buffered_encoded=<n> buffered_decoded=<n> action=<buffer|commit|drain>
   [issue-792-exp22] commit-data url=<url> encoded_size=<n> has_frame=<0|1> document_parsing=<0|1> has_parser=<0|1> action=<append|skip>
   ```

   If `commit-data ... action=skip` appears, the log must make the skipped
   condition obvious from the fields.

6. Instrument parser append and decoded append.

   In `third_party/blink/renderer/core/html/parser/html_document_parser.cc`, add
   narrow logs in:
   - `HTMLDocumentParser::AppendBytes(...)`;
   - `HTMLDocumentParser::Append(...)`;
   - `HTMLDocumentParser::FinishAppend()`;
   - optionally `HTMLDocumentParser::PumpTokenizer()` if the first three logs
     show bytes arrive but no tokens are built.

   Required logs:

   ```text
   [issue-792-exp22] parser-append-bytes url=<url> size=<n> stopped=<0|1> needs_decoder=<0|1> has_template=<0|1> has_iframe=<0|1> has_shadowrootmode=<0|1> sample=<short escaped prefix>
   [issue-792-exp22] parser-append-string url=<url> length=<n> stopped=<0|1> prefetch_only=<0|1> has_template=<0|1> has_iframe=<0|1> has_shadowrootmode=<0|1> sample=<short escaped prefix>
   [issue-792-exp22] parser-finish-append url=<url> should_pump_now=<0|1> is_preloading=<0|1> in_pump_session=<0|1>
   [issue-792-exp22] parser-pump url=<url> result=<0|1> have_seen_eof=<0|1> stopped=<0|1>
   ```

   Use the document URL to gate these logs to the wrapper document. Reuse the
   Experiment 21 wrapper-document predicate if it is convenient to share;
   otherwise replicate it locally in `html_document_parser.cc` or use the
   equivalent document URL checks. Do not log arbitrary page bodies.

7. Build and archive only after the result is accepted.

   Build with:

   ```bash
   cd chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

   If the experiment passes or produces a coherent diagnostic branch, commit the
   Chromium branch and regenerate:

   ```bash
   rm -rf ../../chromium/patches/issue-792/
   git format-patch 148.0.7778.97..HEAD -o ../../chromium/patches/issue-792/
   ```

## Verification

1. Build `libtermsurf_chromium` with `autoninja`.

2. Run the fake-GUI PDF smoke test against the local bitcoin PDF fixture:

   ```bash
   LOG_DIR="logs/issue-792-exp22-pdf-$(date +%Y%m%d-%H%M%S)"
   scripts/test-issue-792-fake-gui.py \
     http://127.0.0.1:9787/bitcoin.pdf \
     --serve-bitcoin-pdf \
     --log-dir "$LOG_DIR" \
     --seconds 18
   ```

3. Inspect `roamium.stderr` and classify the first missing body-delivery gate:
   - If `body-loader-start` is absent after `document-parser-open`, the wrapper
     document never starts loading its body; the next experiment should fix the
     `StartLoadingBody` call path or the preloading branch.
   - If `body-loader-start` appears but `body-data-received` is absent, the
     swapped data pipe is not delivering bytes to Blink; the next experiment
     should inspect the browser-side `InterceptResponse` / dummy loader
     completion ordering.
   - If `body-data-received` appears with `encoded_size=0`, the body loader is
     finishing without wrapper bytes; the next experiment should inspect data
     pipe construction and `OnComplete` timing.
   - If body chunks arrive and contain the wrapper markers but
     `commit-data ... action=skip` appears, the parser/frame state is invalid
     when bytes arrive; the next experiment should fix that ordering.
   - If `commit-data ... action=append` appears but `parser-append-bytes` is
     absent, `BodyData::AppendToParser(...)` is not reaching the HTML parser.
   - If `parser-append-bytes` appears but `parser-append-string` is absent, the
     decoder is not producing decoded HTML text.
   - If `parser-append-string` includes wrapper markers but
     `declarative-shadow-root` remains absent, the next experiment should
     instrument tokenizer/tree-builder token creation.
   - If `declarative-shadow-root` appears, Experiment 22 has advanced the chain;
     inspect whether `frame-owner-inserted`, `load-or-redirect-subframe`, and
     child `pvs-finish` appear.

4. Run the normal HTML smoke test:

   ```bash
   LOG_DIR="logs/issue-792-exp22-html-$(date +%Y%m%d-%H%M%S)"
   scripts/test-issue-792-fake-gui.py \
     http://localhost:9616/index.html \
     --log-dir "$LOG_DIR" \
     --seconds 8
   ```

   The HTML control must not emit `[issue-792-exp22]` body or parser logs.

5. Record the result in this file.

   The result must include:
   - the exact PDF and HTML log directories;
   - whether the wrapper body loader starts;
   - whether body bytes arrive;
   - whether those bytes contain the wrapper markers;
   - whether bytes are committed to the parser;
   - whether the HTML parser receives encoded and decoded data;
   - the first missing transition;
   - the concrete next experiment implied by that transition.

## Result

Not run yet.

## Conclusion

Pending implementation.
