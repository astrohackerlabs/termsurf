# Experiment 18: Docs landing / section-index template (Phase 2)

## Description

The last Phase-2 deliverable: the **docs-landing / section-index page
template**. The current `/docs` page (`src/pages/docs/index.astro`) is
**hand-authored and stale** — it lists only "Components" (webtui, roamium) and
"Protocol" (overview, messages) plus a one-line Quick Start, and it predates the
issue-834 IA. It omits the whole **Configuration** section (the guide + the two
generated references), **Terminal API** (the 64-page VT reference), and the
**Getting Started / Architecture** top-level pages — and it hardcodes its links,
so it drifts every time content changes.

This experiment rebuilds `/docs` as a **generated section index**: it derives
its content from the same `getDocsNav()` source the sidebar uses, so it mirrors
the real content tree exactly and **cannot drift** — every section and page that
exists appears, in IA order, and nothing unbuilt is linked. This is the
section-index _template_ (Phase 2); the deep per-section content is Phases 3–4.

## Key decisions

1. **Generate from `getDocsNav()`, don't hardcode.** Reuse the nav model already
   built for the sidebar (`src/lib/docs-nav.ts` → `getDocsNav()`), which returns
   ordered groups
   `{ section, items: [{href,label}], subgroups: [{subsection, items}] }`. The
   landing renders an intro, then each group as a section heading with its page
   links (and nested subsection links for the VT group). Because both the
   sidebar and the landing read the same source, they stay in lockstep — adding
   a doc page later automatically lists it here too. **No hardcoded page
   links**, so no drift and no risk of linking an unbuilt page.
2. **Keep the `DocPage` shell.** `/docs` stays inside `DocPage.astro` (so it
   keeps the generated sidebar, search, the Exp-15 a11y baseline, and
   `prose-termsurf` styling). Only the article body changes — from the stale
   hardcoded lists to the generated index.
3. **Ungrouped pages get an "Overview" heading.** `getDocsNav()` emits a leading
   group with **`section: null`** (`docs-nav.ts` uses
   `entry.data.section ?? null`) for ungrouped top-level pages (Getting Started,
   Architecture). On the landing, detect it with a **falsy check** (mirroring
   `DocPage`'s `{group.section && …}`, never `=== undefined`) and render that
   group under a plain "Overview" heading (the sidebar shows it unheaded, but a
   landing page reads better with every list under a heading). All other groups
   use their real section name.
4. **Design system, zero new JS.** Plain `prose-termsurf` markup (headings +
   link lists) using semantic tokens; no client JS, no new component. The intro
   paragraph is **rewritten** (not carried over): one short, accurate sentence
   framing TermSurf as a protocol for embedding browsers in terminals — phrased
   as the protocol's design (which is engine-agnostic by spec), not as a claim
   that multiple engines ship today, and making no WebKit/superlative/non-macOS
   claim (consistent with Exp 17). It links nowhere unbuilt.
5. **Accuracy.** The page mentions only what exists. No "Planned" sections are
   linked (they aren't in the collection yet, so `getDocsNav()` won't emit
   them). The intro makes no engine/platform overclaims (consistent with Exp
   17).

## Changes

Files in `website/`:

1. **`src/pages/docs/index.astro`** — replace the hand-authored Quick
   Start/Components/Protocol lists with a generated section index built from
   `getDocsNav()` (intro + per-group headings + link lists, including VT
   subsections). Still wrapped in `DocPage`.

No other files change: `docs-nav.ts`, `DocPage.astro`, the content collection,
schema, generated references, and VT pages are all untouched. No new components,
no fork changes.

## Verification

1. **Mirrors the sidebar.** The generated landing lists **exactly** the sections
   and pages `getDocsNav()` yields, in the same order — cross-check the built
   `/docs` article links against the sidebar `<nav>` on the same page (same
   hrefs and labels; VT subsections present). No section/page in the nav is
   missing from the landing, and the landing links nothing the nav doesn't.
2. **All links resolve.** Every link in the `/docs` article targets a built page
   — dead-link crawl over `/docs` = 0 broken (every `getDocsNav()` href is a
   real collection route by construction).
3. **Accuracy.** The landing links no unbuilt/"Planned" page; the intro makes no
   WebKit/multi-engine/superlative/non-macOS claim (greps clean, consistent with
   Exp 17).
4. **Design system, zero new JS.** Article uses `prose-termsurf` + semantic
   tokens; no hardcoded hex in `docs/index.astro`; built `/docs` adds no
   `<script>`/`astro-island` beyond the existing Pagefind search.
5. **Build + checks.** `bun run build` 76 pages (count unchanged — `/docs`
   already existed); `bunx astro check` 0 errors; `gen:references --check` +
   `import:vt --check` exit 0.
6. **No regressions.** Sidebar, search, other docs pages, `/`, `/welcome`
   unchanged; `/docs` still renders inside `DocPage` with the sidebar.

A full pass closes Phase 2: the docs landing is an accurate, drift-proof section
index generated from the content tree. After this, Phase 2 (Design) is complete
and the issue moves to Phase 3 (Ghostty-parity terminal docs) and Phase 4
(TermSurf-specific docs). (Logged follow-up: the `/welcome` on-black contrast
fix.)

## Design Review

Independent `adversarial-reviewer`. **Verdict: APPROVE** (no Required findings).
The reviewer confirmed the `getDocsNav()` return shape matches exactly
(`{section, items:[{href,label}], subgroups:[{subsection, items}]}`, async),
that `DocPage` consumes the same function so the landing genuinely mirrors the
sidebar (with the subgroup-drop failure mode guarded by verification 1), that
the ungrouped Getting Started/Architecture group leads the array, that every
emitted href is a built `[...slug]` route (drafts excluded, no "Planned" pages
exist), that scope is index.astro-only and zero-JS, and that 73 content + 3
static = 76 pages. Two **Optional** nits, folded in:

1. The ungrouped group's `section` is **`null`**, not `undefined` — decision 3
   now says so and mandates a falsy check (mirroring `DocPage`).
2. The intro should be explicitly rewritten rather than carrying forward the
   existing "any browser engine" line — decision 4 now specifies the rewritten,
   accurate intro.

## Result

**Result:** Pass

`/docs` is now a generated section index that mirrors the sidebar exactly; all
six criteria pass.

### What was built

`src/pages/docs/index.astro` — derives the landing from `await getDocsNav()`: a
rewritten one-sentence intro (protocol framing, no engine/platform overclaim),
then each nav group rendered as a section (`{group.section ?? "Overview"}` h2,
its `items` as a link list, and each VT `subgroups` entry as an h3 + link list),
all inside the existing `DocPage` shell. The hand-authored Quick
Start/Components/Protocol lists are gone.

### Verification results

1. **Mirrors the sidebar** — built `/docs`: the article and the sidebar `<nav>`
   each expose the **same 73** `/docs` hrefs (set difference empty both ways),
   no label mismatches; article headings are Overview → Configuration → Terminal
   API (Concepts/Control/CSI/ESC/OSC) → Components → Protocol, matching nav
   order. **Pass.**
2. **All links resolve** — dead-link crawl over the `/docs` article = 0 broken
   (every href is a real collection route). **Pass.**
3. **Accuracy** — greps of built `/docs` for
   webkit/only-browser/the-only/multiple-engines/linux/windows = empty; nothing
   unbuilt is linked. **Pass.**
4. **Design system, zero new JS** — no hardcoded hex in `docs/index.astro`;
   `prose-termsurf` + DocPage; `0` `astro-island` (only the inherited Pagefind
   search). **Pass.**
5. **Build + checks** — `bun run build` 76 pages; `bunx astro check` 0 errors /
   0 warnings / 3 pre-existing hints; `gen:references --check` +
   `import:vt --check` exit 0. **Pass.**
6. **No regressions** — diff is `docs/index.astro` only; sidebar, search, other
   docs, `/`, `/welcome` unchanged; `/docs` still renders inside `DocPage`.
   **Pass.**

## Conclusion

Phase 2 (Design) is complete. The docs landing is an accurate, drift-proof
section index generated from the content tree — it will list new sections/pages
automatically as Phases 3–4 add them, with no risk of stale or dead links. Phase
2 delivered: the documented design system + callouts (Exp 13), responsive mobile
nav (Exp 14), the accessibility baseline (Exp 15), AA contrast refinement (Exp
16), the home/marketing treatment (Exp 17), and this section-index template (Exp
18). The issue now moves to **Phase 3** (Ghostty-parity terminal docs) and
**Phase 4** (TermSurf-specific docs). Logged follow-up: the `/welcome` on-black
contrast fix (needs a welcome-page change).

## Completion Review

Independent `adversarial-reviewer` at the result gate. **Verdict: APPROVE** (no
Required/Optional/Nit findings). Against a fresh build the reviewer confirmed
the landing and sidebar each expose exactly **73** identical `/docs` hrefs
(empty set difference both ways, no label mismatches), that all 64 VT pages
appear under their subsection h3s (the `subgroups`-drop failure mode did not
occur), that the mirror is structural (both call `getDocsNav()`), 0 broken
links, accuracy greps empty (intro is the rewritten line, not the old "any
browser engine"), one `<h1>` with IA-ordered h2/h3 and no literal
"null"/"undefined", zero hardcoded hex / no new JS, 76 pages, `astro check` 0
errors, drift checks exit 0, and diff scoped to `docs/index.astro` only.
