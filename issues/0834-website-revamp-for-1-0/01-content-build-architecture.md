# Experiment 1: Content model & generated navigation (Phase 1, keystone)

## Description

Phase 1 (Architecture) has several deliverables: the content model, a
config/keybind reference-generation pipeline, a VT MDX import pipeline,
generated navigation, search, versioning posture, deploy cleanup, and the full
information architecture. This experiment establishes the **keystone the rest
sit on**: the content model and generated navigation. Everything else in Phase 1
plugs into this — generated references and imported VT MDX become content
entries; nav, search, and the sitemap all read the content tree.

**What this experiment decides and proves:**

1. **Framework choice, within Astro** (scope decision 3 — Astro, not a
   re-platform): extend the existing bespoke Astro site with **content
   collections + MDX + a generated docs route**, rather than adopting Astro
   Starlight.
2. **Content model:** a typed `docs` content collection with a frontmatter
   schema that drives page metadata and navigation order/grouping.
3. **Generated navigation:** the docs sidebar is generated from the collection,
   retiring the hardcoded array in `src/components/DocPage.astro`.
4. **Brand preservation** (scope decision 4 — keep & refine Tokyo Night): the
   migrated pages render through the existing `prose-termsurf` styling and Tokyo
   Night theme with visual parity, and the custom `/` and `/welcome` pages keep
   working unchanged.

**Explicitly deferred to later Phase 1 experiments** (to keep this one focused
and testable): the config/keybind reference-generation pipeline, the VT MDX
import pipeline, client-side search (Pagefind), the versioning posture, the
deploy/`scripts/deploy.sh` cleanup, and the full sitemap/IA authoring. This
experiment delivers the substrate they all depend on.

### Why extend bespoke Astro instead of Starlight

This is the load-bearing decision, so the rationale is recorded for the design
review to challenge. The two real options:

- **(A) Astro Starlight** — a docs framework. Gives generated sidebar, built-in
  Pagefind search, MDX, content collections, i18n/versioning patterns, and
  accessibility "for free." Costs: it is opinionated about layout and ships its
  own design system, so matching the established Tokyo Night brand (scope
  decision 4) means overriding its CSS variables and component slots and
  fighting its defaults; integrating Tailwind v4 with Starlight needs
  verification (`@astrojs/starlight-tailwind` historically targeted Tailwind
  v3); the custom marketing home and the Three.js `/welcome` page live outside
  Starlight and must be themed separately to stay consistent; and Starlight's
  compatibility with Astro 6 + Tailwind v4 is an unverified risk surface.
- **(B) Extend the bespoke Astro site** — add `@astrojs/mdx` + content
  collections, generate the sidebar from the collection, and build the docs
  route ourselves (search/versioning come in later experiments). Costs: we build
  nav generation, and later search and versioning, ourselves — all well-trodden.
  Benefits: full design control so the existing Tokyo Night brand and Tailwind
  v4 setup are preserved exactly; the existing custom `/` and `/welcome` pages
  already fit the same layout; no framework restyling fight; no new
  cross-version compatibility risk beyond core Astro features.

**Decision: (B).** The site's defining constraints here are "keep and refine the
existing Tokyo Night identity" (scope decision 4) and an existing Tailwind v4 +
custom-page investment. Starlight's free features come with a restyling and
compatibility tax that substantially offsets them under those constraints, while
the work (B) adds — generated nav now, search and versioning later — is modest
and standard. If the thin-slice proof below fails (e.g., a blocking limitation
in core content collections), the result will record it and the next experiment
can revisit Starlight; that is the experiment method working as intended.

## Changes

A thin vertical slice that stands up the architecture end-to-end against real
content. Files in `website/`:

1. **`package.json`** — add the official `@astrojs/mdx` integration to
   dependencies.
2. **`astro.config.mjs`** — register `mdx()` **after** `react()` in the
   integrations array (`integrations: [react(), mdx()]`), so MDX inherits the
   JSX renderer config, per Astro's guidance. Keep `output: "static"` and
   `trailingSlash: "never"` so existing URLs are unchanged. Also set
   `markdown: { syntaxHighlight: false }` — see the code-block decision in step
   4; this disables Astro's default Shiki highlighter so fenced code blocks fall
   back to plain `.prose-termsurf pre` styling instead of Shiki's inline
   per-token styles (which would override the Tokyo Night CSS variables and
   break brand parity).
3. **`src/content.config.ts`** (Astro 6 content layer) — define a `docs`
   collection using the `glob` loader with an explicit `base` so entry IDs (and
   therefore URLs) are correct:
   `glob({ base: "./src/content/docs", pattern: "**/*.{md,mdx}" })`. With this
   base, `entry.id` is the path relative to `src/content/docs` (e.g.
   `components/webtui`), which maps 1:1 to the doc URL. Zod `schema`:
   - `title: string` (required)
   - `description: string` (optional) — for `<head>` + section indexes
   - `section: string` (optional) — sidebar group heading (e.g. "Components",
     "Protocol", "Reference"); ungrouped entries render above the first group
   - `order: number` (optional, default a high value) — sort within a section
   - `draft: boolean` (optional, default false) — excluded from build and nav
4. **Code-block rendering decision (brand-parity critical).** The current pages
   do manual syntax highlighting with literal `<span class="text-primary">…`
   inside `<pre class={cb}>` where `cb` is a per-page Tailwind class string
   (`getting-started.astro`, `architecture.astro`). MDX has no `cb` constant in
   scope and Astro's default Shiki would override the theme. To preserve parity
   exactly:
   - Add a single shared `.code-block` rule to `src/styles/style.css` carrying
     the styling currently in the `cb` string (background, left border, padding,
     mono size, overflow), so code blocks no longer depend on a per-page
     constant or Tailwind utilities inside MDX.
   - Port code blocks into MDX as **raw `<pre class="code-block">` HTML** (MDX
     renders raw HTML), preserving the existing manual token `<span>`s. The
     highlight token colors are expressed via `.code-block` CSS classes (not
     Tailwind utilities) so they do not depend on Tailwind scanning `.mdx`.
   - This removes the per-page `cb` constant entirely and is the mechanism
     Verification step 3 asserts.
5. **`src/content/docs/**`** — migrate the current doc page content into the collection as `.mdx`,
   preserving existing URLs and section grouping:
   - `getting-started.mdx`, `architecture.mdx` (ungrouped/top)
   - `components/webtui.mdx`, `components/roamium.mdx` (section "Components")
   - `protocol/overview.mdx`, `protocol/messages.mdx` (section "Protocol")
   - `reference/configuration.mdx` (section "Reference")

   The `/docs` landing (`docs/index.astro`) is handled by the index route in
   step 8, not a collection entry. Content is ported faithfully (code blocks per
   step 4, the ASCII architecture diagram in `<pre>`, tables); this validates
   the model against real content. Phases 3–4 will rewrite/expand this content —
   the migration here proves the substrate, not the final copy.

6. **`src/lib/docs-nav.ts`** — a small helper that reads the `docs` collection
   via `getCollection("docs")`, filters drafts, sorts by
   `(section, order, title)`, and returns an ordered nav structure (groups with
   their entries, ungrouped entries first). Single source of truth for the
   sidebar and for later section-index/sitemap use.
7. **`src/pages/docs/[...slug].astro`** — a dynamic route with
   `getStaticPaths()` over the collection (**filtering `data.draft` here too**,
   not only in the nav, so draft entries emit no HTML), rendering each entry's
   MDX body inside the existing docs layout (sidebar + `prose-termsurf`
   article). Sets `<title>`/description from frontmatter. `params.slug` =
   `entry.id`, so URLs map 1:1 to the collection path and
   `/docs/getting-started`, `/docs/components/webtui`, etc. are unchanged.
8. **`src/components/DocsSidebar.astro`** (or fold into the layout) — renders
   the generated nav from `docs-nav.ts`, replacing the hardcoded `pages` array,
   with the same active-state styling DocPage currently uses.
9. **Retire `src/components/DocPage.astro`** and the migrated
   `src/pages/docs/**/*.astro` content pages once the route + sidebar render the
   collection, so the site has a single, consistent docs path (no half-migrated
   mix). **`src/pages/docs/index.astro` must drop its `import DocPage` and
   switch to the generated sidebar / `docs-nav.ts`** (otherwise the build breaks
   once `DocPage.astro` is removed); it stays as the docs landing.
10. **`src/pages/index.astro` link audit** — the home page hardcodes links to
    doc URLs (`/docs/getting-started`, `/docs/components/webtui`, …). Confirm
    every such link still resolves after migration; update any that shift. (URLs
    are designed to be unchanged, so this is a safety check, not a rename.)

If the highlight-token spans turn out to need Tailwind utilities inside MDX
after all, add an `@source` directive for `src/content/**` to `style.css` so
Tailwind v4 scans MDX — but step 4's CSS-class approach is designed to avoid
this dependency.

Out of scope for this experiment (own later experiments): reference generation,
VT import, Pagefind search, versioning, deploy-script cleanup, content rewrite.

## Verification

Run from `website/`.

1. **Build + type-check succeed:** `bun run build` completes with no errors and
   `astro check` reports no type errors; `dist/` contains static HTML for every
   migrated doc URL.
   - **Pass:** clean build, clean `astro check`, all doc pages emitted.
   - **Fail:** build/type error or any migrated page missing from `dist/`.
2. **URL parity:** every previously-existing doc URL still resolves with no
   trailing slash (`/docs/getting-started`, `/docs/architecture`,
   `/docs/components/webtui`, `/docs/components/roamium`,
   `/docs/protocol/overview`, `/docs/protocol/messages`,
   `/docs/reference/configuration`, `/docs`).
   - **Pass:** all resolve, unchanged paths.
   - **Fail:** any path 404s or changes shape.
3. **Brand parity, with the code-block mechanism asserted:** in `bun run dev`, a
   migrated page (e.g. `/docs/architecture`) renders with the Tokyo Night theme
   and `prose-termsurf` styling — headings, the ASCII diagram, and tables
   visually match the pre-migration page (compare against `git stash`/
   screenshots). **Specifically confirm code blocks render via the `.code-block`
   class with the manual token `<span>`s (per Changes step 4) — no
   `class="astro-code"` / inline Shiki `style="..."` in the emitted HTML.**
   - **Pass:** visually equivalent; code blocks use `.code-block`, no Shiki
     inline styles; no hardcoded colors introduced.
   - **Fail:** visible styling regression or Shiki-highlighted output present.
4. **Generated nav:** the sidebar is produced from the collection. Adding a new
   `.mdx` entry with frontmatter `section`/`order` makes it appear in the
   correct group and position **without editing any component code**; removing
   it removes it. (Verified with a temporary `draft`/throwaway entry, then
   reverted.)
   - **Pass:** nav reflects the collection automatically.
   - **Fail:** nav requires manual editing or ignores frontmatter ordering.
5. **Internal links resolve:** every internal `/docs/...` link in
   `src/pages/index.astro`, the header, and the migrated pages points at a URL
   that exists in the build (no 404s). A simple link check over `dist/` (or grep
   the hardcoded links against emitted paths) suffices.
   - **Pass:** all internal doc links resolve.
   - **Fail:** any internal link 404s.
6. **Custom pages intact:** `/` and `/welcome` build and render unchanged
   (Three.js scene still loads via `client:only="react"`).
   - **Pass:** both unchanged.
   - **Fail:** either regresses.

A full pass means the content substrate is in place and the remaining Phase 1
experiments (references, VT import, search, versioning) can build on it.

## Design Review

**Reviewer:** independent `adversarial-reviewer` agent (separate context,
read-only), at the design gate before implementation.

**Verdict:** APPROVE WITH CHANGES.

The reviewer confirmed the load-bearing decision is sound: extending bespoke
Astro over Starlight is well-justified against scope decisions 3–4; the scope is
a coherent single vertical slice; the deferral list (references, VT import,
search, versioning, deploy cleanup) is the right cut; `src/content.config.ts` is
the correct Astro 6 location; `trailingSlash: "never"` is already set so URL
parity is achievable; the `[...slug].astro` route does not collide with the
retained `/docs` `index.astro`; and adding `@astrojs/mdx` does not touch the
`.astro` home or the `.tsx` `/welcome` React island.

Required findings, all resolved in this design:

1. **Code-block rendering was unspecified — the top brand-parity risk.** Astro
   6.1.1 ships Shiki by default, which would emit `class="astro-code"` with
   inline per-token styles that override the Tokyo Night variables. Resolved:
   Changes step 4 now specifies disabling default syntax highlighting
   (`markdown: { syntaxHighlight: false }`), a shared `.code-block` CSS class
   replacing the per-page `cb` constant, and porting code blocks as raw
   `<pre class="code-block">` HTML with the manual token `<span>`s; Verification
   step 3 now asserts this mechanism explicitly.
2. **Glob loader `base` was ambiguous, risking wrong entry IDs/URLs.** Resolved:
   Changes step 3 now specifies
   `glob({ base: "./src/content/docs", pattern: "**/*.{md,mdx}" })` so
   `entry.id` maps 1:1 to the doc URL.
3. **Integration order unspecified.** Resolved: Changes step 2 now specifies
   `integrations: [react(), mdx()]` (mdx after the JSX framework).

Optional findings, also addressed: draft filtering added to `getStaticPaths`
(step 7), not just nav; Tailwind-v4-scans-`.mdx` risk sidestepped by expressing
token colors as `.code-block` CSS rather than utilities inside MDX (with an
`@source` fallback noted); `astro check` and an internal-link check added to
Verification (steps 1, 5); and the `src/pages/docs/index.astro` `DocPage` import
removal called out explicitly (step 9) so retiring `DocPage.astro` cannot break
the build.

## Result

**Result:** Pass

The content substrate is in place: a typed `docs` content collection, MDX
authoring, a generated docs route, and generated navigation — with full URL and
brand parity. All six verification criteria pass.

### What was built (final state)

- `package.json` — added `@astrojs/mdx` and `@astrojs/check` (dev); bumped
  `astro` `^6.1.1` → `^6.4.8` (see deviation 1).
- `astro.config.mjs` — `integrations: [react(), mdx()]`,
  `markdown: { syntaxHighlight: false }`.
- `src/content.config.ts` — `docs` collection,
  `glob({ base: "./src/content/docs", pattern: "**/*.{md,mdx}" })`, schema
  `{ title, navLabel?, description?, section?, order=1000, draft=false }`.
- `src/styles/style.css` — added `@source "../content";` (see deviation 2).
- `src/content/docs/**` — 7 migrated pages: `getting-started.mdx`,
  `architecture.mdx`, `components/{webtui,roamium}.mdx`,
  `protocol/{overview,messages}.mdx`, `reference/configuration.mdx`.
- `src/lib/docs-nav.ts` — builds grouped, sorted, draft-filtered nav from the
  collection.
- `src/pages/docs/[...slug].astro` — dynamic route over the collection
  (draft-filtered in `getStaticPaths`), renders MDX body in the docs layout.
- `src/components/DocPage.astro` — refactored to consume `getDocsNav()` (see
  deviation 4).
- `src/layouts/Base.astro` — optional `description` →
  `<meta name="description">`.
- Retired: the 7 old `src/pages/docs/**/*.astro` content pages.
  `docs/index.astro` retained (now renders generated nav via the refactored
  `DocPage`).

### Deviations from the approved design (with rationale)

1. **Astro bumped 6.1.1 → 6.4.8 (required).** `@astrojs/mdx@6.0.3`'s peer
   dependency is `astro ^6.4.0`; 6.1.1 was too old. Bumped to the latest 6.x —
   still Astro 6, so scope decision 3 ("keep Astro") holds. Build and
   `astro check` are clean on 6.4.8.
2. **Code-block parity via `@source` + verbatim utilities, not a `.code-block`
   semantic class.** The design proposed rewriting code-block token colors into
   a `.code-block` CSS class to avoid depending on Tailwind scanning MDX. In
   practice the pages use Tailwind utilities beyond code tokens (table status
   colors `text-success`/`text-muted`, the diagram `<pre>`), so the cleanest
   faithful migration was the design's documented fallback: an explicit
   `@source "../content"` directive (deterministic, not the automatic-detection
   gamble) plus copying markup verbatim. `markdown.syntaxHighlight: false` is
   still set. Verified: zero `astro-code`/Shiki inline styles in any built doc
   page; token spans and utility classes render.
3. **Code-block frame uses the existing `.prose-termsurf pre` rule, no new
   `.code-block` class.** `.prose-termsurf pre` already styles code blocks
   (background, left border, padding) and is _more_ specific than a bare
   `.code-block` class would be, so adding `.code-block` would lose the
   specificity battle. Reusing the existing rule is simpler and achieves parity.
4. **`DocPage.astro` refactored to consume generated nav, not retired in favor
   of a separate `DocsSidebar`.** The experiment's goal is _generated
   navigation_; feeding `getDocsNav()` into the existing layout shell delivers
   that with less churn and leaves `docs/index.astro` working unchanged (it
   inherits the generated nav). No half-migrated mix remains.
5. **Added `navLabel` to the schema.** The original sidebar showed "Overview"
   for the page titled "Protocol Overview". `navLabel` preserves the short
   sidebar label while keeping the full page `<title>`. Falls back to `title`.

None of these change the architecture the design established or the deferral
list; they are implementation refinements, each lower-risk than the written
approach.

### Verification results

1. **Build + type-check** — `bun run build` builds 10 pages (8 docs + `/` +
   `/welcome`); `astro check` reports 0 errors (only pre-existing `THREE.Clock`
   deprecation notices in `WelcomePage.tsx`, unrelated). **Pass.**
2. **URL parity** — all 7 doc URLs plus `/docs` emit at their original paths
   (directory format, no trailing slash), unchanged. **Pass.**
3. **Brand parity + code-block mechanism** — code blocks render with the Tokyo
   Night utility classes and manual token `<span>`s; grep confirms **zero**
   `astro-code`/inline Shiki styles across `dist/docs/`. **Pass.**
4. **Generated nav** — sidebar order and section grouping are identical to the
   old hardcoded nav (Getting Started, Architecture, [Components] Web TUI,
   Roamium, [Protocol] Overview, Messages, [Reference] Configuration). A
   temporary non-draft entry appeared automatically with no component edits; a
   temporary `draft: true` entry was excluded from both nav and the build; both
   reverted. **Pass.**
5. **Internal links resolve** — all `/docs/...` links in `index.astro` and
   `docs/index.astro` map to built files. **Pass.**
6. **Custom pages intact** — `/` and `/welcome` build unchanged; the Three.js
   island still loads via `client:only="react"`. **Pass.**

## Conclusion

Extending bespoke Astro with content collections + MDX + a generated route works
cleanly and preserves the Tokyo Night brand exactly — the Starlight
restyling/compatibility tax was correctly avoided. The substrate is ready for
the rest of Phase 1. The natural next experiments, in rough order: (2) the
config + keybind reference auto-generation pipeline from the Ghostboard fork;
(3) the VT MDX import pipeline (reuse + extend Ghostty's MIT MDX); then search
(Pagefind), versioning posture, the full IA/sitemap, and the deploy/`deploy.sh`
cleanup. The `@source`-based Tailwind scanning and the
`markdown.syntaxHighlight: false` choice are now baseline facts those
experiments inherit.

## Completion Review

**Reviewer:** independent `adversarial-reviewer` agent (separate context,
read-only), at the result gate before the result commit.

**Verdict:** APPROVED — no Required findings.

The reviewer independently re-ran `bun run build` (10 pages, correct directory
paths under `trailingSlash: "never"`) and `astro check` (0 errors), confirmed
**zero** `astro-code`/Shiki artifacts in `dist/docs/`, diffed all 7 migrated MDX
files against the deleted `.astro` originals (bodies byte-identical apart from
the intended `class={cb}` → verbatim Tailwind string substitution; entity counts
match; no JSX hazards), confirmed the generated nav reproduces the original
order/grouping with working active state and draft exclusion, confirmed no
leftover hardcoded `pages` array, and confirmed the plan commit was separate and
contained no result. All five deviations were judged sound and lower-risk than
the written design.

Findings addressed after the review:

- **(Optional) `z` deprecation hint.** `astro check` emitted new
  `'z' is deprecated` hints from re-exporting `z` via `astro:content`. Fixed:
  `src/content.config.ts` now imports `z` from `zod` directly; `astro check`
  drops to 2 hints (only the pre-existing `THREE.Clock` notices). The
  Verification text above is corrected to reflect this.
- **(Nit) Stale `website/CLAUDE.md`.** Its Pages/Components tables still listed
  the deleted `.astro` doc pages. Updated to document the `docs` content
  collection, the `[...slug].astro` route, `docs-nav.ts`, and the frontmatter
  schema.

Findings deferred (with rationale):

- **(Optional) Section ordering relies on alphabetical `localeCompare`.** Output
  is correct today (Components < Protocol < Reference is alphabetical), but a
  future section like "Advanced" would mis-sort. Section ordering is properly an
  information-architecture concern, so it is deferred to the IA/sitemap
  experiment, which will add an explicit section-order mechanism before
  authoring the full Ghostty-parity tree. Tracked here so it is not lost.
- **(Nit) Changes step 10 wording** ("home hardcodes links to doc URLs")
  slightly overstated — `index.astro` links only `/docs`. The verification
  conclusion was unaffected; recorded here rather than rewriting the committed
  plan narrative.
