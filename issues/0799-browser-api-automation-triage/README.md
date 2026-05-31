+++
status = "open"
opened = "2026-05-30"
+++

# Issue 799: Browser API Automation Triage

## Goal

Identify the fundamental missing browser API and web-feature surfaces in
TermSurf, decide which ones can be solved with automated verification, and then
solve the automatable subset in this issue.

Items that cannot be solved or verified automatically must be explicitly
deferred, with the reason recorded. Items that are not automatable today but can
be made automatable through upfront work, instrumentation, fixtures, or a
one-off manual baseline should stay in scope if that upfront work is practical.

## Background

TermSurf's Chromium embedder is built from Chromium's Content API rather than
full Chrome. That gives TermSurf the control it needs for the terminal browser
model, but it also means TermSurf does not automatically inherit every
browser-side service, delegate, prompt, Mojo binder, permission path, download
handler, and product feature that Chrome wires up.

This gap has appeared in several places:

- [Issue 616](../0616-web-features/README.md) inventoried missing browser
  features such as JavaScript dialogs, downloads, file uploads, page zoom, HTTP
  Basic Auth, crash recovery, camera/mic permissions, console capture, session
  isolation, bookmarking, and other browser affordances.
- [Issue 655](../0655-substack-blank/README.md) found a more fundamental failure
  mode: Substack called the Badging API, Roamium had no
  `blink.mojom.BadgeService` binder, and Chromium killed the renderer for a bad
  Mojo message. A no-op `BadgeService` binder fixed that specific crash, but the
  issue concluded that TermSurf needs a systematic audit of missing Mojo
  interfaces.
- `TODO.md` still records this as a future issue: missing Mojo binders are a
  "ticking time bomb" because a renderer can crash whenever page JavaScript
  calls an unhandled browser API.
- Recent PDF work added several narrow browser API, extension, resource, stream,
  and input surfaces using the Electron-style pattern: implement only the
  embedder pieces TermSurf actually needs, then verify them automatically.

The next step is to turn these scattered findings into one disciplined browser
API effort. The issue must not become an unbounded promise to implement all of
Chrome. It should first classify each missing API or feature by whether it can
be implemented and verified automatically, then proceed only on the subset that
meets that bar.

## Scope

The initial source list includes:

- the missing web-feature inventory in Issue 616;
- the missing Mojo binder finding from Issue 655;
- the current `TODO.md` web-feature and future-issue entries;
- any related browser API gaps discovered during Issues 750, 780, 792-798, and
  recent Chromium embedder work;
- current source-code evidence in Roamium, Wezboard, webtui, and
  `libtermsurf_chromium` for what is already implemented.

Candidate surfaces include, but are not limited to:

- JavaScript dialogs: `alert`, `confirm`, and `prompt`;
- downloads and download lifecycle reporting;
- file uploads and file picker behavior;
- page zoom;
- HTTP Basic Auth;
- renderer crash recovery and user-visible crash UX;
- camera and microphone permissions;
- console capture;
- session isolation and incognito behavior;
- bookmarks and persisted browser data features;
- TermSurf-specific JavaScript API hooks;
- hide/show webview behavior;
- multi-webview stacking;
- native and web drag-and-drop, including file uploads;
- systematic missing Mojo binder coverage;
- Notifications and Push API browser services;
- Geolocation;
- WebAuthn and Credential Management;
- Payment Request;
- Web Share;
- File System Access and related storage/quota prompts;
- Web Bluetooth, USB, HID, Serial, and MIDI;
- screen capture and media-capture permissions;
- Permissions API plumbing;
- service-worker-adjacent browser services that require browser-process
  delegates or binders.

Already solved items should be recorded as solved and excluded from the active
implementation scope. Examples likely include `target="_blank"` handling,
clipboard, DevTools, dynamic page titles, URL normalization, PDF-specific
browser API surfaces, and the `BadgeService` stub.

## Automation Boundary

This issue is explicitly automation-first.

For each candidate API or feature, the first experiment must classify it into
one of these buckets:

1. **Automatable now** — can be implemented and verified with automated tests,
   fixtures, protocol injection, screenshots, log assertions, DevTools protocol,
   fake-GUI harnesses, or local test servers.
2. **Automatable after setup** — cannot be verified today, but practical upfront
   work can make it automatable. Examples include adding a local fixture,
   writing a probe harness, adding debug instrumentation, adding a fake browser
   API caller page, or doing one one-off manual baseline to calibrate the
   automation.
3. **Deferred: not automatable enough** — requires ongoing manual judgment,
   native UI interaction that cannot be safely contained, platform permissions
   that cannot be reliably automated, fast human interaction, external accounts,
   non-deterministic third-party services, or broad product decisions that are
   outside this issue.
4. **Already solved** — covered by prior issues and verified enough to exclude,
   though regression tests may still be useful.

Only buckets 1 and 2 are in implementation scope for this issue. Bucket 3 must
be deferred explicitly. Bucket 4 must be documented so the issue does not
rediscover old work.

## Principles

- Do not implement a broad Chrome product stack just because a missing API
  exists. Follow the Electron-style TermSurf pattern: narrow embedder-owned
  plumbing, scoped to the specific feature being fixed.
- Do not silently suppress crashes without recording the API or binder that
  caused them.
- Do not claim an API is fixed unless the verification can prove the intended
  behavior, not merely prove that the renderer survived.
- Prefer local deterministic fixtures over third-party sites.
- Prefer automated protocol, DevTools, screenshot, and log-based verification
  over manual testing.
- If a one-off manual test is needed to make later work automatable, record the
  exact manual baseline and why it is sufficient.
- Keep non-automatable work out of this issue rather than letting the scope
  expand indefinitely.

## Expected First Experiment

The first experiment should be a triage experiment, not an implementation
experiment.

It should gather the known missing APIs and features, audit current source and
issue history, then produce a table with:

- candidate API or feature;
- source of the requirement;
- current implementation status;
- likely implementation layer;
- automation classification;
- proposed automated verification method;
- whether the item is in scope for this issue;
- follow-up issue needed, if deferred.

The result of that experiment determines the implementation order. No
implementation experiments should be designed until the triage table is complete
and reviewed.

## Experiments

- [Experiment 1: Inventory and classify missing browser APIs](01-inventory-and-classify.md)
  — **Pass**
- [Experiment 2: Build the browser API no-crash audit harness](02-browser-api-no-crash-harness.md)
  — **Pass**
- [Experiment 3: Add PaymentRequest default-deny binder](03-payment-request-default-deny.md)
  — **Pass**
- [Experiment 4: Enable contained generic downloads](04-contained-downloads.md)
  — **Pass**
- [Experiment 5: Add protocol-mediated JavaScript dialogs](05-javascript-dialogs.md)
  — **Pass**
- [Experiment 6: Add automated page zoom](06-page-zoom.md) — **Pass**
- [Experiment 7: Add protocol console capture](07-console-capture.md) — **Pass**
- [Experiment 8: Add protocol HTTP Basic Auth](08-http-basic-auth.md) — **Pass**
- [Experiment 9: Add renderer crash recovery UX](09-renderer-crash-recovery.md)
  — **Pass**
- [Experiment 10: Add explicit default-deny permissions](10-default-deny-permissions.md)
  — **Designed**
