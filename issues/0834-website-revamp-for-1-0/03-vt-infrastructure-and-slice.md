# Experiment 3: VT reference infrastructure + proof slice (Phase 1)

## Description

Scope decision 1: the VT / Terminal API reference is **reused from Ghostty's
MIT-licensed VT MDX as a base, then extended** (auto-generation from source is
impossible — the prose lives only in Ghostty's website repo). This experiment
stands up the **infrastructure** to host that content and proves it with a small
**slice**; the full 64-file import and nested navigation follow in Experiment 4.

Splitting here mirrors how Experiment 1 was scoped (substrate + thin slice). The
bulk import is mechanical once the infrastructure exists; the risk is all in the
infrastructure, so that is what this experiment validates.

### Source material (researched, available locally)

Ghostty's website repo (`ghostty-org/website`, **MIT**, "Copyright (c) 2024
Ghostty") contains **64 hand-authored VT MDX files** under `docs/vt`:
`index.mdx`, `reference.mdx`, `external.mdx`, and `concepts/` (4), `control/`
(5), `csi/` (29), `esc/` (8), `osc/` (15). Every sequence page uses one MDX
component, `<VTSequence sequence={[...]} unimplemented? />` (65 usages); no
other custom components appear. Pages also contain `## Validation` sections with
fenced bash code blocks and a number of placeholder `(#TODO)` links.

`VTSequence` (Ghostty's `src/components/vt-sequence/index.tsx`) is a **purely
presentational** React component: a `parseSequence` helper expands
`CSI`→`ESC [`, `OSC`→`ESC ]`, trailing `ST`→`ESC \`, treats `Pn` as a named
parameter, maps special names (`BEL`,`BS`,`TAB`,`LF`,`CR`,`ESC`,`...`) to hex,
and UTF-8-encodes the rest to hex; it renders an `<ol>` of cells (hex over
value) with an optional "Unimplemented" banner. It has no state or
interactivity.

### Key infrastructure decisions

1. **Port `VTSequence` as a static `.astro` component, not a React island.** It
   is non-interactive, so a server-rendered `.astro` component ships zero JS and
   avoids hydrating React per page. Only the pure `parseSequence` logic ports
   (into the component frontmatter); the React-only scaffolding — `useMemo`,
   `classNames`, the `keyCounts`/`key` dedup, `lucide-react` — is **dropped or
   swapped** (no `useMemo`/`key` in Astro; `lucide-react` → the installed
   `lucide-astro`; conditional classes inline). Styling maps Ghostty's CSS
   variables (`--gray-3`, `--atom-one-red`, `--jetbrains-mono`) onto TermSurf's
   Tokyo Night semantic variables (`--color-border`, an error tone,
   `--font-mono`).
2. **Provide the component to MDX via the render `components` prop.** The
   `[...slug].astro` route renders `<Content components={{ VTSequence }} />`, so
   imported VT MDX resolves `<VTSequence>` to our Astro port with no per-file
   import. (VT files stay `.mdx` — MDX-authored and MDX-safe; verified all 64
   use only `title`+`description` frontmatter, no `import`s, and no component
   other than `<VTSequence>`.) A referenced-but-unprovided component throws at
   build, so a wiring regression fails the build rather than shipping silently.
3. **Link adaptation (broadened after review — the slice must ship zero dead
   internal links).** Imported pages contain three kinds of problem links, all
   handled by the importer (by hand for the slice; recorded as the bulk rule for
   Experiment 4):
   - `[text](#TODO)` (Ghostty placeholders) → plain `text`.
   - Absolute internal links to pages **not present** on TermSurf's site — e.g.
     `bel.mdx`'s `/docs/config/reference#bell-features`, and (within the slice)
     `index.mdx`'s `/docs/vt/reference` and `/docs/vt/concepts/sequences` which
     are not yet imported → remap to the correct TermSurf URL when an equivalent
     exists (Ghostty's `/docs/config/reference` → TermSurf's
     `/docs/reference/config`, dropping unverified anchors), otherwise → plain
     `text`.
   - Relative `#anchor` links to other pages → same rule.
   - Reference-style link definitions (`[label]: /docs/vt/…`, used by some
     `osc/` pages) → same remap-or-inline rule (recorded for the Experiment 4
     bulk importer). For this experiment's slice, prefer OSC pages **without**
     cross-links to keep the slice clean. Principle: **no internal `/docs/...`
     or cross-page `#` link may point at a target the site does not build.**
     Verification crawls the built HTML — **head and body** — to enforce this
     (not merely the absence of `#TODO`).
4. **Content adaptation for Ghostty-specific prose (new, after review).** MIT
   permits verbatim reuse, but shipping Ghostty product claims on TermSurf's 1.0
   site is inaccurate (e.g. `bel.mdx` has a `## Ghostty Status` section and
   "implemented in Ghostty"; `index.mdx` says "applications that run in
   Ghostty"). Policy for imported prose: rename `## Ghostty Status` →
   `## Implementation Status`, and rewrite product references to the TermSurf
   terminal where they assert product behavior ("…in Ghostty" → "…in TermSurf").
   TermSurf's terminal (Ghostboard) is a Ghostty fork that inherits Ghostty's VT
   engine, so the status claims carry over. "Ghostty" is retained only where it
   names upstream Ghostty as the source/project (the attribution). This applies
   to the `description` frontmatter too (it renders into `<meta>`), so imported
   descriptions are adapted, not just body prose.

   **Recorded bulk rule for Experiment 4** (the other ~60 files contain shapes
   beyond the slice's): version-numbered compatibility tables with a
   `Ghostty | 1.0.0` column state _Ghostty's_ release version and are factually
   wrong for TermSurf — these need the column relabeled/removed, not a word
   swap; first-person "limitation on our end" and "Ghostty does not support…"
   phrasings also need rewriting. The slice avoids these by file selection;
   Experiment 4 must handle them explicitly.

5. **Attribution (MIT).** Add a repository `NOTICE` crediting Ghostty under MIT
   for the imported VT documentation (retaining the copyright + permission
   notice), and a short attribution line on the VT section index page.
6. **Navigation.** The current `docs-nav.ts` groups by a single `section`
   string. 64 VT pages across 5 sub-categories need **nested** nav, which is a
   larger change bundled with the deferred IA/section-ordering work — so this
   experiment puts the slice under a single flat `section: "Terminal API"` and
   **defers nested VT nav to Experiment 4** (with the bulk import). Recorded as
   a known limitation, not silently shipped.
7. **Platform scope.** VT sequences are terminal-protocol behavior, platform
   agnostic, so scope decision 5 (macOS-only) imposes no trimming here.
8. **Footnotes.** Some VT pages use GFM footnotes (`[^1]`, e.g. `bel.mdx`),
   which Astro renders into a footnotes section that `prose-termsurf` may not
   style. This experiment adds minimal footnote styling (or confirms the default
   is acceptable) and verifies footnotes render legibly.

## Changes

Files in `website/` unless noted:

1. **`src/components/VTSequence.astro`** (new) — static port of Ghostty's
   `VTSequence`: the `parseSequence` logic verbatim in the component script, the
   `<ol>`/cell markup, an optional `unimplemented` banner (using an existing
   `lucide-astro` icon, e.g. `OctagonAlert`/`TriangleAlert`), and scoped styles
   using Tokyo Night variables. Props: `sequence: string | string[]`,
   `unimplemented?: boolean`.
2. **`src/pages/docs/[...slug].astro`** — pass `components={{ VTSequence }}` to
   `<Content />` so MDX pages can use `<VTSequence>`.
3. **`src/content/docs/vt/index.mdx`** (imported + adapted) — the VT overview
   page, frontmatter `title: Terminal API`, `navLabel: Terminal API`,
   `section: Terminal API`, `order: 1`, an **adapted `description`** (no "in
   Ghostty"), plus a short MIT attribution line crediting Ghostty.
4. **A small proof slice of sequence pages** (imported + adapted, ~3 files
   spanning the component and categories), e.g.
   `src/content/docs/vt/csi/cup.mdx` (uses `<VTSequence>`),
   `src/content/docs/vt/control/bel.mdx` (footnotes + a `## Ghostty Status`
   section to exercise decisions 3, 4, 8), and one `osc/` page — each with
   `section: Terminal API` and an `order`, and all links + Ghostty-specific
   prose adapted per decisions 3–4. URLs: `/docs/vt`, `/docs/vt/csi/cup`, etc.
5. **`NOTICE`** (repo root, new) — MIT attribution for the imported Ghostty VT
   documentation (retains the Ghostty copyright and MIT permission notice).
6. **`src/styles/style.css`** — minimal `.prose-termsurf` footnote styling if
   the default is unstyled (decision 8).
7. **`website/CLAUDE.md`** — document the VT content origin (imported from
   Ghostty under MIT), the `VTSequence` component, the link/prose adaptation
   rules, and that bulk import + nested nav are pending (Experiment 4).

No Ghostboard fork changes; no content-schema change (VT uses the existing
fields). The Astro change is limited to passing the components map.

## Verification

Run from `website/`.

1. **`VTSequence` renders correctly.** On `/docs/vt/csi/cup`, the
   `<VTSequence sequence={["CSI","Py",";","Px","H"]} />` renders the expanded
   sequence: `ESC` (0x1B) `[` (0x5B) param `y` `;` (0x3B) param `x` `H` (0x48),
   matching Ghostty's `parseSequence` semantics (CSI expanded to ESC [;
   parameters shown as bare names; literals shown with hex).
   - **Pass:** diagram cells match the expected hex/value sequence.
   - **Fail:** wrong expansion, missing cells, or hydration/runtime error.
2. **Slice builds + renders + nav.** `bun run build` succeeds and `astro check`
   reports 0 errors; `/docs/vt` and the slice sequence pages emit and appear
   under a "Terminal API" sidebar section.
   - **Pass:** all hold. **Fail:** build/check error or missing page.
3. **Zero dead internal links (enforces decision 3).** Crawl the built HTML of
   the VT slice pages: every internal `href` (`/docs/...` or in-page `#anchor`)
   resolves to a page the build emitted or an id present on the page; **no**
   `#TODO`, no `/docs/vt/reference`, `/docs/vt/concepts/sequences`, or
   `/docs/config/reference` link remains.
   - **Pass:** all internal links resolve. **Fail:** any dead internal link.
4. **No Ghostty product claims (enforces decision 4).** The built slice pages
   contain no "in Ghostty"/"Ghostty Status" product text; the only "Ghostty"
   reference is the upstream attribution.
   - **Pass:** product prose adapted to TermSurf; attribution retained.
   - **Fail:** any unadapted Ghostty product claim.
5. **`VTSequence` renders + ships no client JS.** Built VT pages contain the
   sequence diagram markup and **no `<astro-island>`** / no client JS at all
   (zero-JS is structurally guaranteed by the `.astro` choice; this asserts it).
   - **Pass:** diagram present, no island/JS. **Fail:** island present or markup
     missing.
6. **Footnotes render (enforces decision 8).** `bel.mdx`'s footnote renders into
   a legible footnotes section (styled, not raw).
   - **Pass:** footnotes render and are styled. **Fail:** unstyled/broken.
7. **Attribution present.** `NOTICE` exists with the Ghostty MIT notice; the VT
   index page shows an attribution line.
   - **Pass:** both present. **Fail:** missing attribution.
8. **No regressions.** The 11 existing doc pages (8 original + 2 generated
   references + landing), `/`, and `/welcome` still build at their URLs.
   - **Pass:** unchanged. **Fail:** any regression.

A full pass means the VT hosting infrastructure (component, MDX wiring,
attribution) works end-to-end, leaving Experiment 4 to bulk-import the remaining
~60 files and build the nested Terminal API navigation.

## Design Review

**Pass 1 — CHANGES REQUIRED** (independent `adversarial-reviewer`, verifying
against the real Ghostty sources). The reviewer confirmed the infrastructure is
sound: `lucide-astro` is installed; `<Content components={{ VTSequence }} />` is
the correct Astro 6 MDX idiom and a missing component throws at build; all 64 VT
files use only `title`+`description` frontmatter (schema-accepted), no
`import`s, and only `<VTSequence>`; `parseSequence` is pure and ports cleanly;
the flat "Terminal API" section sorts last with no collision; MIT + NOTICE is
the correct obligation. Two **blocking** findings, both fixed in the revised
design:

1. **Dead internal links.** The slice files ship absolute internal cross-links
   to pages not on TermSurf's site (`index.mdx` → `/docs/vt/reference`,
   `/docs/vt/concepts/sequences`; `bel.mdx` →
   `/docs/config/reference#bell-features`), and verification only checked
   `#TODO`. Resolved: decision 3 broadened to a general link-adaptation rule
   (remap to the correct TermSurf URL or plain text), and verification step 3
   now **crawls built HTML for any dead internal link**.
2. **Undefined adaptation of Ghostty-specific prose.** `bel.mdx` has a
   `## Ghostty Status` section / "implemented in Ghostty"; shipping that on
   TermSurf is inaccurate. Resolved: new decision 4 defines the content
   adaptation policy (`Ghostty Status` → `Implementation Status`; product "…in
   Ghostty" → "…in TermSurf"; keep "Ghostty" only for upstream attribution),
   with verification step 4 asserting no Ghostty product claims ship.

Optional findings also addressed: the zero-JS check was vacuous for an `.astro`
component → strengthened to assert no `<astro-island>`/client JS (step 5); GFM
footnotes in slice files → decision 8 + step 6 add/verify footnote styling; and
the React-only scaffolding is now explicitly noted as dropped/swapped in
decision 1.

**Pass 2 — APPROVE.** A fresh reviewer verified both blockers genuinely resolved
against the real sources (the `cup.mdx`/`index.mdx`/`bel.mdx` links and prose
are covered, and the dead-link crawl + "only-attribution-Ghostty" invariants
backstop any incompleteness), and confirmed the `parseSequence` port, MDX
wiring, schema, and section-sort claims. Non-blocking refinements folded in
afterward: decision 3 names **reference-style link definitions**
(`[label]: url`) and says the crawl covers **head and body**, with the slice
preferring cross-link-free OSC pages; decision 4 adapts the **`description`
frontmatter** (it renders into `<meta>`) and records the **bulk rule for
Experiment 4** (Ghostty version-numbered compatibility tables — an accuracy
issue a word-swap can't fix —, "Ghostty does not support…", first-person
phrasings); change item 3 adapts the description.
