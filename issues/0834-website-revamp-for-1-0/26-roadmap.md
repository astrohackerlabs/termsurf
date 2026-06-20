# Experiment 26: Roadmap (Phase 4)

## Description

A Phase-4 experiment adding the **Roadmap** page — the consolidated,
clearly-marked list of planned-but-unshipped work (scope decision 6: "the site
includes a roadmap for planned-but-unshipped features … with explicit 'planned'
treatment so nothing reads as shipped"). The planned items are already
established and verified across the site (About's Planned section,
Architecture's engine table, the home page); this page gathers them into one
destination with unambiguous "not in 1.0" framing and **no invented timelines**.

## Key decisions

1. **New page `roadmap.mdx`, `section: "TermSurf"`, `order: 1`.** Route
   `/docs/roadmap`. The IA's "TermSurf" group is **already** in `SECTION_ORDER`
   (after Protocol, before Help) and intended to hold the Roadmap, so the page
   becomes that group's first occupant with **no `docs-nav.ts` change** and
   lands at an appropriately late nav position. (Later experiments may fold How
   TermSurf Works / Web TUI / Architecture / Protocol into the same TermSurf
   group per the IA; this experiment only adds the Roadmap page.)
2. **Content = the established planned set, grouped and clearly "planned".**
   Four groups, each explicitly not-in-1.0:
   - **Browser features** — bookmarks, user-facing **multi-tab browsing**,
     history, downloads, PDF viewing. (Frame tabs as the user-facing browsing
     UI, not raw tab primitives — the engine/protocol already expose tab
     primitives, so "tabs" here means the multi-tab browsing experience,
     avoiding a false contradiction with the protocol/Roamium pages — review
     point.)
   - **Browser engines** — WebKit (Surfari) **planned**; Gecko (Waterwolf) and
     Ladybird (Girlbat) **researched** (preserve the status nuance per Exp 19);
     link Architecture's engine table rather than restating statuses.
   - **Front-ends** — Kitty, Alacritty, iTerm2 (in addition to Ghostboard).
   - **Platforms** — Linux and Windows; later iOS and Android.
3. **Accuracy — planned, no timelines, no overclaim.** Everything on the page is
   framed as planned/under consideration; **no** dates or version commitments;
   the engine planned-vs-researched distinction is kept (consistent with
   About/Architecture). Nothing here is presented as shipped. macOS-only remains
   the 1.0 reality (scope decision 5). Consistent with the home/About framing
   (Exp 17/19).
4. **No duplication beyond intent.** About has a brief Planned summary; this is
   the dedicated, fuller roadmap. The page cross-links **About** (for what
   _does_ ship) and **Architecture** (engine table) instead of restating engine
   internals or the shipped inventory.
5. **Design system, zero JS.** Plain MDX → `prose-termsurf`; grouped lists;
   semantic tokens only; links only to **built** pages (`/docs/about`,
   `/docs/architecture`).

## Changes

Files in `website/`:

1. **`src/content/docs/roadmap.mdx`** — new page in the "TermSurf" group (the
   four planned groups + cross-links). Appears under a new **TermSurf** sidebar
   group heading (after Protocol) and in the generated `/docs` index
   automatically via `getDocsNav()`.

No other files change: schema, `docs-nav.ts` ("TermSurf" already in
`SECTION_ORDER`), generated references, and the fork are untouched. Page count
**82 → 83**, and a new "TermSurf" section heading appears in the nav.

## Verification

1. **Builds + placed correctly.** `bun run build` emits `/docs/roadmap`; total
   pages **83**. The sidebar + generated `/docs` index show a **TermSurf** group
   (after **Protocol**, per `SECTION_ORDER`) containing the Roadmap page.
   `bunx astro check` 0 errors.
2. **Accuracy (clearly planned).** Built `/docs/roadmap`: every item is under a
   planned/not-in-1.0 framing; **no** dates/version numbers as commitments; the
   engine nuance (WebKit planned; Gecko/Ladybird researched) is preserved;
   nothing is presented as shipped; no non-macOS-as-shipped claim. Cross-check
   the planned set against About/Architecture (consistency, no contradiction).
3. **No duplication.** The page links About (shipped) and Architecture (engine
   table) rather than restating the shipped inventory or engine internals.
4. **Design system, zero JS, links resolve.** `prose-termsurf`; no hardcoded
   hex; no `<astro-island>` beyond the inherited Pagefind search; dead-link
   crawl over `/docs/roadmap` = 0 broken.
5. **a11y.** Exactly one `<h1>` ("Roadmap"), ordered `<h2>`s (no skipped
   levels); descriptive link text.
6. **No regressions.** `gen:references --check` + `import:vt --check` exit 0;
   the new TermSurf group/entry is the only nav addition;
   search/`/`/`/welcome`/other pages unchanged.

A full pass adds the consolidated roadmap, satisfying scope decision 6 and
seeding the IA's TermSurf group. Next Phase-4 candidates: Browser Engines
(Roamium rework + engine roadmap detail) and the protocol refresh.

## Design Review

Independent `adversarial-reviewer`. **Verdict: APPROVE** (no Required findings).
The reviewer verified the placement mechanism (`"TermSurf"` is in
`SECTION_ORDER` at rank 6 after Protocol/before Help; no page currently uses
that section, so `roadmap.mdx` becomes its first occupant with no `docs-nav.ts`
edit; group sorts after Protocol; route `/docs/roadmap`; the `/docs` index
renders the new "TermSurf" `<h2>` automatically), the page count (82 → 83), and
full consistency with `about.mdx` Planned + `architecture.mdx`'s engine table +
root `CLAUDE.md` (engine planned-vs-researched nuance, frontends, platforms,
browser features — nothing shipped listed as planned, no contradiction). The
no-timeline/no-overclaim framing is endorsed. Two **Optional** notes:

1. **(folded in)** Frame "tabs" as user-facing **multi-tab browsing**, since the
   protocol/Roamium already expose tab primitives — so the item doesn't read as
   contradicting those pages. Decision 2 updated.
2. **(deferred follow-up)** About's brief Planned summary duplicates the roadmap
   set (drift risk); adding an About→Roadmap backlink is a sensible later tweak,
   not required here.
