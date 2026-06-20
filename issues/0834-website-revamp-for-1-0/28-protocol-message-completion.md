# Experiment 28: Protocol message reference completion (Phase 4)

## Description

The last Phase-4 content experiment: complete the **protocol message
reference**. `protocol/messages.mdx` opens "Complete reference for **every**
protobuf message in `termsurf.proto`," but it does not cover every message. The
proto has 40 messages (plus the `TermSurfMessage` wrapper the intro explains).
The page already documents 32 of them — including the handshake and query
**pairs** under combined "Request / Reply" headings
(`HelloRequest / HelloReply`, `QueryLast*`, `QueryDevtools*`, `QueryTabs*`). **8
messages are genuinely missing** (they back features already documented
elsewhere — the Web TUI's Auth/Dialog modes, console output, renderer-crash
recovery, GUI-active state, and the `TabInfo` nested type that `QueryTabsReply`
already references):

`HttpAuthRequest`, `HttpAuthReply`, `JavaScriptDialogRequest`,
`JavaScriptDialogReply`, `ConsoleMessage`, `RendererCrashed`, `SetGuiActive`,
`TabInfo`.

(An earlier draft of this design wrongly listed 16 missing — it missed that the
8 handshake/query messages are documented under **combined** `A / B` headings.
Corrected after design review.) Nothing currently on the page is invented/stale,
but one existing table is **incomplete**: `QueryDevtoolsRequest` omits its
`browser` field (proto has `pane_id`, `inspected_tab_id`, `profile`, `browser`).

## Key decisions

1. **Edit `protocol/messages.mdx` in place** (route `/docs/protocol/messages`,
   `section: Protocol`, `order: 2`). Add the 8 genuinely-missing messages and
   fix the one incomplete existing table; leave the other 32 entries as-is. No
   nav/section change.
2. **Add the 8 missing messages**, field tables transcribed from
   `proto/termsurf.proto` (verified extraction), grouped into new `<h2>`
   sections, matching the page's existing style (request/reply **pairs** use a
   single combined `A / B` heading with a Message/Field/Type table):
   - **HTTP authentication** — `HttpAuthRequest / HttpAuthReply`. Request:
     `tab_id`, `request_id`, `url`, `auth_scheme`, `challenger`, `realm`,
     `is_proxy`, `first_auth_attempt`, `is_primary_main_frame_navigation`,
     `is_navigation`. Reply: `tab_id`, `request_id`, `accepted`, `username`,
     `password`.
   - **JavaScript dialogs** — `JavaScriptDialogRequest / JavaScriptDialogReply`.
     Request: `tab_id`, `request_id`, `dialog_type`, `origin_url`, `message`,
     `default_prompt_text`. Reply: `tab_id`, `request_id`, `accepted`,
     `prompt_text`.
   - **Page events** — `ConsoleMessage` (`tab_id`, `level`, `message`, `line_no`
     int32, `source_id`), `RendererCrashed` (`tab_id`, `termination_status`,
     `termination_status_code` int32, `url`, `can_reload`).
   - **`SetGuiActive`** (`tab_id`, `active`, `reason`) — a GUI→Engine state
     update; add it under the **existing `State` section** (alongside
     `FocusChanged`/`SetColorScheme`), not a new heading (review point —
     consistency).
   - **`TabInfo`** — the nested type referenced by `QueryTabsReply.tabs` (`id`,
     `inspected_tab_id`, `pane_id`, `url`). Document it adjacent to the
     Request/Reply section so the `TabInfo[]` reference resolves.
3. **Fix the incomplete existing table.** Add the missing `browser` (string)
   Request field to `QueryDevtoolsRequest / QueryDevtoolsReply` so it matches
   the proto.
4. **Accuracy — transcribed, not invented.** Every added message and field
   (name + type) is taken from the proto; `line_no`/`termination_status_code`
   are `int32`. After this, the documented set = **40** messages = all of
   `proto/termsurf.proto` except the `TermSurfMessage` wrapper (which the intro
   explains), so the "every protobuf message" claim becomes true.
5. **Design system, zero JS.** Plain MDX → `prose-termsurf`; field tables in the
   existing combined-pair / single style; semantic tokens; no new links
   required. (`protocol/overview.mdx` is **not** changed — the review confirmed
   it has no stale "XPC"/message-count claim.)

## Changes

Files in `website/`:

1. **`src/content/docs/protocol/messages.mdx`** — add the 8 missing messages
   (grouped sections + `TabInfo`); add the missing `browser` Request field to
   the `QueryDevtools*` table.

No other files change: schema, `docs-nav.ts`, generated references,
`proto/termsurf.proto`, `protocol/overview.mdx`, and the fork are untouched.
Page count stays **83**.

## Verification

1. **Completeness (proto-verified, combined-heading aware).** A script extracts
   every `<h3>` heading in built `messages.mdx`, **splits combined `A / B`
   headings on `/`** and trims, and compares the resulting message set to the 40
   `proto/termsurf.proto` messages minus `TermSurfMessage`: **0** in
   proto-not-on-page and **0** on-page-not-in-proto, and **no duplicate**
   message name. (This fixes the naive check that previously mis-flagged the
   combined headings.)
2. **Accuracy.** The 8 added messages' fields/types match the proto (spot-check
   `HttpAuthRequest`, `JavaScriptDialogRequest`, `RendererCrashed`,
   `ConsoleMessage`, `TabInfo`); `QueryDevtoolsRequest` now includes `browser`.
   No invented field/message.
3. **Builds + checks.** `bun run build` 83 pages; `bunx astro check` 0 errors;
   `gen:references --check` + `import:vt --check` exit 0.
4. **Design system, zero JS, links resolve.** `prose-termsurf`; no hardcoded
   hex; no `<astro-island>` beyond the inherited Pagefind search; dead-link
   crawl over `/docs/protocol/messages` = 0 broken.
5. **a11y.** One `<h1>` ("Messages"), ordered `<h2>`/`<h3>` (no skipped levels).
6. **No regressions.** Routes/nav unchanged; the existing entries are unchanged
   except the `QueryDevtools*` `browser` fix;
   sidebar/search/`/`/`/welcome`/other pages unchanged.

A full pass makes the protocol message reference genuinely complete (all 40
messages), closing the last Phase-4 content gap. After this, Phase 4 — and the
issue's documentation coverage — is complete, and the issue can move to its
Conclusion (the deferred Sponsor page remains pending a real funding channel).

## Design Review

**Pass 1 — REJECT.** The first draft claimed 16 missing messages, but 8 of those
(`HelloRequest/Reply`, the three `Query*` pairs) are already documented under
**combined** `A / B` headings the naive `<h3>` regex missed; implementing it
would have duplicated content. The reviewer also flagged that the verification's
heading diff would itself mis-flag the combined headings, and that the existing
`QueryDevtoolsRequest` table omits its `browser` field. **Resolved:** the design
now adds the genuinely-missing **8** (`HttpAuthRequest/Reply`,
`JavaScriptDialog{Request,Reply}`, `ConsoleMessage`, `RendererCrashed`,
`SetGuiActive`, `TabInfo`), fixes the `QueryDevtools*` `browser` field, and the
verification splits combined `A / B` headings + asserts no duplicates.

**Pass 2 — APPROVE.** A fresh reviewer independently confirmed: proto-minus-
wrapper = 40; the page documents 32 unique (combined headings split, zero
duplicates); proto − documented = exactly the 8; all field names/types match the
proto (both `int32` cases verified); the `QueryDevtoolsRequest` `browser` gap is
real and the fix correct; 32 + 8 = 40 makes the "every protobuf message" claim
true; and the revised diff method is sound. One **Optional**, folded in:
`SetGuiActive` is documented under the existing **State** section (with
`FocusChanged`/`SetColorScheme`) rather than a new heading.

## Result

**Result:** Pass

The protocol message reference is now complete (all 40 messages); all criteria
pass.

### What was built

`src/content/docs/protocol/messages.mdx` — added the 8 missing messages:
`SetGuiActive` (under the existing **State** section); `ConsoleMessage` +
`RendererCrashed` (appended to **Engine Events**); `TabInfo` and two new `<h2>`
sections — **HTTP Authentication** (`HttpAuthRequest / HttpAuthReply`) and
**JavaScript Dialogs** (`JavaScriptDialogRequest / JavaScriptDialogReply`) —
with field tables transcribed from `proto/termsurf.proto`. Also fixed the
existing `QueryDevtoolsRequest` table to include its `browser` Request field.

### Verification results

1. **Completeness (proto-verified)** — the built page's `<h3>` set (combined
   `A / B` headings split, deduped) = **40** unique messages = the 40
   `proto/termsurf.proto` messages minus the `TermSurfMessage` wrapper: 0 in
   proto-not-on-page, 0 on-page-not-in-proto, 0 duplicate headings. The intro's
   "every protobuf message" claim now holds. **Pass.**
2. **Accuracy** — the 8 added messages' fields/types are transcribed from the
   proto (incl. `int32` `line_no`/`termination_status_code`); the
   `QueryDevtoolsRequest` `browser` field is now present. No invented field.
   **Pass.**
3. **Builds + checks** — `bun run build` 83 pages; `bunx astro check` 0 errors;
   `gen:references --check` + `import:vt --check` exit 0. **Pass.**
4. **Design system, zero JS, links resolve** — `prose-termsurf`; no hardcoded
   hex; 0 `astro-island`; dead-link crawl over `/docs/protocol/messages` = 0
   broken. **Pass.**
5. **a11y** — one `<h1>` ("Messages"); ordered `<h2>`/`<h3>` (no skipped
   levels). **Pass.**
6. **No regressions** — only `messages.mdx` changed; the existing entries are
   unchanged except the `QueryDevtools*` `browser` fix; routes/nav/search/`/`/
   `/welcome` unchanged. **Pass.**

## Conclusion

The protocol message reference is genuinely complete — all 40 `termsurf.proto`
messages documented (the 8 added: HTTP auth, JS dialogs, console, renderer
crash, GUI-active, TabInfo), plus an accuracy fix to `QueryDevtoolsRequest`.
This closes the last Phase-4 content gap. **Phase 4 is complete**, and with it
the issue's documentation coverage — every shipped surface (the `web` TUI, the
UX story, the protocol, Roamium, Ghostboard's pane borders, the architecture) is
documented and source-verified, with a clearly-marked roadmap. The issue can now
move to its Conclusion; the only outstanding item is the deferred Sponsor page,
which awaits a real funding channel from the user.

## Completion Review

Independent `adversarial-reviewer` at the result gate. **Verdict: APPROVE** (no
findings). Against a fresh 83-page build the reviewer confirmed: splitting the 4
combined `A / B` headings yields exactly **40 unique** messages = proto minus
`TermSurfMessage` (0 in proto-not-on-page, 0 on-page-not-in-proto, 0
duplicates); all 8 added messages' fields/types match the proto (both `int32`
cases, `HttpAuthRequest`'s 10 fields, no invented field); the
`QueryDevtoolsRequest` `browser` field is added; `git diff` is purely additive
(99 insertions, 0 deletions) so the existing 32 are untouched; one `<h1>`,
ordered headings, no hex, 0 `astro-island`, dead-link crawl 0 broken;
`astro check` 0 errors; drift checks exit 0; scope only `messages.mdx`; and the
README Exp 28 line is a clean single `— **Pass**`.
