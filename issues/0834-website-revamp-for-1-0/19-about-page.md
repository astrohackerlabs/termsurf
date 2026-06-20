# Experiment 19: About page (Phase 3)

## Description

The first **Phase 3** (Ghostty-parity terminal docs & pages) experiment. Ghostty
opens its docs with an **About** section (what it is, philosophy, platform
support). TermSurf has no equivalent — the docs jump straight into Getting
Started / Architecture, with no page that answers "what _is_ this, what does it
run on, what's shipped vs planned." This experiment adds an accurate, macOS-only
**About** page as a top-level doc.

It establishes the Phase-3 content pattern: a hand-authored MDX page in the docs
collection, styled by `prose-termsurf`, every claim **verified against the
authoritative project source** (the root `CLAUDE.md` engine/platform/frontend
status, plus the issue's shipped inventory), with roadmap items clearly marked
"Planned" (scope decisions 5 and 6).

## Key decisions

1. **A top-level (ungrouped) MDX page, `about.mdx`.** Place it alongside
   `getting-started.mdx` / `architecture.mdx` (no `section`, so it joins the
   ungrouped lead group in the sidebar + the generated `/docs` "Overview").
   Route `/docs/about`. Frontmatter: `title: About`, a `description`, and
   **`order: 1.5`** so the nav reads **Getting Started (1) → About (1.5) →
   Architecture (2)** — matching the IA's "ungrouped (Getting Started, About)"
   without renumbering the existing pages (the schema's `order` is a number; 1.5
   sorts cleanly between them). No nav-data, schema, or component change.
2. **Content = what TermSurf actually is, macOS-accurate.** Sections:
   - **What it is** — a protocol for embedding real web browsers inside terminal
     emulators; `web <url>` opens a browser as a GPU overlay in a pane; no
     alt+tab.
   - **The protocol is the product** — terminals (GUIs), browser engines, and
     TUIs are separate processes speaking one protobuf/Unix-socket protocol;
     multi-process by necessity (one engine process per profile).
   - **What's shipped (1.0)** — macOS only; **Ghostboard** (a Ghostty fork) as
     the GUI, the **`web`** TUI, and **Roamium** (Chromium) as the engine.
   - **Relationship to Ghostty** — Ghostboard is a fork of Ghostty; TermSurf
     inherits its terminal + VT behavior and adds the browser-overlay protocol
     and terminal features (e.g. split pane borders). Credit Ghostty (consistent
     with the existing VT-docs attribution + repo `NOTICE`).
   - **Roadmap (clearly "Planned")** — other engines, **preserving the
     root-`CLAUDE.md` status nuance**: WebKit/Surfari is **Planned**, while
     Gecko/Waterwolf and Ladybird/Girlbat are **Researched** (investigated, not
     committed) — don't present a Researched engine as a firm commitment (review
     point; phrase as "planned" for WebKit and "researched / under
     consideration" for Gecko + Ladybird). Also: other front-ends (Kitty,
     Alacritty, iTerm2), other platforms (Linux, Windows; later iOS/Android),
     and the browser-feature roadmap (bookmarks, tabs, history, downloads, PDF).
3. **Accuracy — verified against project source.** Every present-tense claim is
   checked against root `CLAUDE.md`: engine table (Chromium **Done**; WebKit
   **Planned**; Gecko/Ladybird **Researched**), platform ("will work on macOS,
   Linux, Windows … iOS and Android later" → only macOS ships now), frontends
   (Ghostboard primary; Kitty/Alacritty/iTerm2 planned). **No** non-macOS
   platform stated as working, **no** non-Chromium engine stated as shipped,
   **no** unsubstantiated superlative (consistent with Exp 17). Roadmap strictly
   under a "Planned" heading.
4. **Design system, zero JS.** Plain MDX → `prose-termsurf` (the doc-article
   template); semantic tokens only; cross-links only to **built** pages
   (`/docs/getting-started`, `/docs/architecture`, `/docs/protocol/overview`,
   `/docs/components/webtui`, `/docs/components/roamium`,
   `/docs/reference/configuration`). No links to unbuilt Phase-3/4 pages.

## Changes

Files in `website/`:

1. **`src/content/docs/about.mdx`** — new top-level doc (frontmatter + the
   sections above). It appears in the sidebar (ungrouped lead group) and the
   generated `/docs` index automatically via `getDocsNav()`.

No other files change: schema, `docs-nav.ts`, `DocPage`, other content, and the
fork are untouched. Page count goes **76 → 77**.

## Verification

1. **Builds + placed correctly.** `bun run build` emits `/docs/about`; total
   pages **77**. The sidebar ungrouped lead group and the `/docs` "Overview"
   list show **Getting Started → About → Architecture** in that order (order
   1/1.5/2). `bunx astro check` 0 errors.
2. **Accuracy (verified).** Built `/docs/about`: no claim that
   Linux/Windows/iOS/Android currently works (those appear only under
   "Planned"); no non-Chromium engine presented as shipped
   (WebKit/Gecko/Ladybird only under the roadmap, with WebKit "planned" vs
   Gecko/Ladybird "researched"); no **uniqueness/superlative** claim ("the
   only", "first-ever", "best", "only browser") — note the factual platform
   statement "macOS only" is **exempt** (it's accurate, not a superlative; the
   grep targets uniqueness claims, not the word "only"); the shipped components
   are exactly Ghostboard + `web` + Roamium; Ghostty is credited. Cross-check
   each present-tense sentence against root `CLAUDE.md`.
3. **Roadmap labeled.** Every planned engine/frontend/platform/feature sits
   under an explicit "Planned" heading — nothing planned reads as shipped (scope
   decision 6).
4. **Design system, zero JS, links resolve.** Renders via `prose-termsurf`; no
   hardcoded hex; no `<astro-island>` beyond the inherited Pagefind search;
   every internal link targets a built page (dead-link crawl over `/docs/about`
   = 0 broken).
5. **a11y.** Exactly one `<h1>` ("About"), ordered `<h2>`s (no skipped levels);
   any link text is descriptive.
6. **No duplication / contradiction.** About **summarizes and links** rather
   than re-documenting — it does not restate Architecture's
   multi-process/IPC/GPU detail (it links there) and does not repeat Getting
   Started's install steps (it links there); nothing on About contradicts those
   pages.
7. **No regressions.** `gen:references --check` + `import:vt --check` exit 0;
   the only intended additions elsewhere are About's new sidebar entry and
   `/docs` index line (no _unintended_ changes to other entries); search, other
   pages, `/`, `/welcome` unchanged.

A full pass adds the foundational About page and sets the Phase-3 pattern
(accurate, fork-verified, macOS-only, roadmap-marked). Next Phase-3 candidates:
a dedicated Install section, Features (macOS-applicable), Help, and Sponsor.

## Design Review

Independent `adversarial-reviewer`. **Verdict: APPROVE** (no Required findings).
The reviewer verified every planned fact against the authoritative root
`CLAUDE.md`: only Chromium/Roamium is "Done"; WebKit Planned, Gecko/Ladybird
Researched; macOS-only (Linux/Windows/iOS/Android future); Ghostboard is the GUI
and a Ghostty fork; Kitty/Alacritty/iTerm2 are planned frontends; the `NOTICE`
makes crediting Ghostty appropriate. Mechanism confirmed: the schema's `order`
is `z.number()` (1.5 valid) and `docs-nav.ts` `within()` sorts numeric order
then title, so About sorts Getting Started (1) → About (1.5) → Architecture (2)
in the ungrouped lead group; only those two pages are ungrouped today (no
stray-section risk); all six cross-links resolve; page count 76 → 77. Four
findings (all Optional/Nit), folded in:

1. **Preserve the engine status nuance** — WebKit is _Planned_ but
   Gecko/Ladybird are _Researched_; the roadmap now keeps that distinction
   rather than flattening all three to "Planned."
2. **Scope the superlative check** — exempt the factual "macOS only" platform
   statement; the grep targets uniqueness claims ("the only", "first-ever",
   "best", "only browser"), not the bare word "only."
3. **No-duplication guard** — added verification 6: About summarizes-and-links,
   never re-documents Architecture's multi-process/IPC/GPU or Getting Started's
   install steps, and contradicts neither.
4. **(Nit)** Verification "no regressions" reworded so it doesn't contradict the
   intended new sidebar/`/docs` entry.

## Result

**Result:** Pass

`/docs/about` is added as an accurate, macOS-only foundational page; all
criteria pass.

### What was built

`src/content/docs/about.mdx` (`order: 1.5`) — raw-HTML MDX matching the existing
doc style, in `prose-termsurf`: an intro (browser-in-terminal, no alt+tab);
**The protocol is the product** (engine-agnostic multi-process, one engine per
profile, links to Architecture); **What ships in 1.0** (macOS; Ghostboard /
`web` / Roamium with links to Getting Started, the config guide, and the
protocol overview); **Relationship to Ghostty** (fork, inherited VT, the
`NOTICE` attribution, links to the VT reference); and a **Planned** section that
keeps the status nuance (WebKit _planned_; Gecko/Ladybird _researched_;
Kitty/Alacritty/ iTerm2 frontends; Linux/Windows then iOS/Android;
bookmarks/tabs/history/ downloads/PDF).

### Verification results

1. **Builds + placed** — `bun run build` 77 pages; `/docs/about` emitted; both
   the sidebar lead group and the `/docs` "Overview" list read **Getting Started
   → About → Architecture** (order 1 / 1.5 / 2); `astro check` 0 errors.
   **Pass.**
2. **Accuracy (verified)** — built `/docs/about` has no uniqueness superlative
   ("the only"/"only browser"/"first-ever"/"best" all absent; factual "macOS"
   present); WebKit, Gecko, Ladybird, Linux, and Windows appear **only** inside
   the Planned section (none before it), so nothing non-macOS/non-Chromium reads
   as shipped; shipped components are exactly Ghostboard + `web` + Roamium;
   Ghostty credited. Each present-tense claim matches root `CLAUDE.md`.
   **Pass.**
3. **Roadmap labeled** — all planned engines/frontends/platforms/features sit
   under the "Planned" `<h2>`, with the planned-vs-researched distinction kept.
   **Pass.**
4. **Design system, zero JS, links resolve** — `prose-termsurf`; no hardcoded
   hex; 0 `astro-island`; dead-link crawl over `/docs/about` = 0 broken (all six
   cross-links resolve). **Pass.**
5. **a11y** — one `<h1>` ("About") → four ordered `<h2>`s, no skipped levels;
   link text is descriptive. **Pass.**
6. **No duplication** — About summarizes the protocol/architecture in two
   paragraphs and links to Architecture for detail; install is a link to Getting
   Started, not repeated; no contradictions. **Pass.**
7. **No regressions** — `gen:references --check` + `import:vt --check` exit 0;
   only `about.mdx` added (untracked, nothing else changed); search/other
   pages/`/`/`/welcome` unaffected. **Pass.**

## Conclusion

Phase 3 has its foundational About page and its content pattern: accurate,
fork-/source-verified, macOS-only, roadmap clearly marked, generated nav/index
picking it up automatically. Next Phase-3 candidates: a dedicated Install
section, Features (macOS-applicable: theme, shell integration, SSH, AppleScript
— each fork-verified), Help (terminfo, macOS notes, synchronized output), and
Sponsor.

## Completion Review

Independent `adversarial-reviewer` at the result gate. **Verdict: APPROVE** (no
findings). Against a fresh 77-page build the reviewer read every sentence and
confirmed all 13 future engine/frontend/platform terms (WebKit, Surfari, Gecko,
Waterwolf, Ladybird, Girlbat, Kitty, Alacritty, iTerm2, Linux, Windows, iOS,
Android) occur **strictly after** the "Planned" heading; the
planned-vs-researched nuance matches the `CLAUDE.md` table; the shipped stack is
exactly Ghostboard + `web` + Roamium with Ghostty credited; no uniqueness
superlative; the one claim not in `CLAUDE.md` ("split pane borders" as a
TermSurf addition) is corroborated by shipped closed issues (0823, 0786, 0787,
0785, 0777). Also confirmed: nav order Getting Started → About → Architecture,
all 7 internal links resolve, one `<h1>` + four ordered `<h2>`s, no hardcoded
hex, 0 `astro-island`, no Architecture/Getting-Started duplication,
`astro check` 0 errors, drift checks exit 0, and only `about.mdx` added.
