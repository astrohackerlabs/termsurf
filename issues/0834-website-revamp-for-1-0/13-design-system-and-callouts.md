# Experiment 13: Design system foundation + callout primitive (Phase 2)

## Description

The first Phase-2 (design) experiment. Scope decision 4 is **keep & refine Tokyo
Night** — Phase 2 _systematizes_ the existing look into a documented design
system, it does not reinvent it. This experiment does two things:

1. **Document the design system** — formalize the already-implemented Tokyo
   Night tokens, type scale, spacing, and the component inventory (so Phases 3–4
   reuse primitives instead of inventing them, and the system is an explicit
   artifact).
2. **Fix a real gap with a new primitive — callouts.** 9 doc pages (VT + config)
   use GitHub-style alerts — `[!NOTE]` (×10) and `[!WARNING]` (×1) in the actual
   content — but they currently render as **literal `[!NOTE]` text** in a plain
   blockquote (no remark plugin handles them) — they look broken. Add the
   standard `remark-github-blockquote-alert` plugin (installed) and style its
   output in Tokyo Night. (All five kinds — note/tip/important/warning/caution —
   are styled for future use, but only NOTE and WARNING are exercised by the
   current build, so verification validates those two.)

## Key decisions

1. **Callouts via `remark-github-blockquote-alert`** (v2.1.0, exports
   `remarkAlert`). Wire it into `astro.config.mjs` `markdown.remarkPlugins`. It
   transforms `> [!NOTE]`-style blockquotes into
   `<div class="markdown-alert markdown-alert-note">` with a
   `.markdown-alert-title` (icon + label). Applies to `.md` and `.mdx`. **No
   content-source edits** — the 9 pages keep their `[!NOTE]` markup (so the
   VERIFIED/generated pages don't drift); only render-time behavior + styling
   change.
2. **Tokyo Night callout styling** in `src/styles/style.css`: map each alert
   kind to a semantic accent — Note→`--color-primary` (blue),
   Tip→`--color-success` (green), Important→`--color-secondary` (purple),
   Warning→`--color-warning`, Caution→`--color-caution` — as a left-border +
   tinted box, consistent with `.prose-termsurf` and the
   `.code-block`/disclosure styling. **Warning/Caution need new semantic
   tokens** (review finding): add `--color-warning` (amber) and
   `--color-caution` (red) to the `@theme` block in **both** the light and dark
   palettes — no hardcoded hex in the alert rules. The plugin's inline icon
   `<svg>`/`.octicon` has no `fill`, so set `fill: currentColor` on the
   `.markdown-alert-title` icon (else it renders black on the dark background —
   review finding); the title color carries the per-kind accent so the icon
   inherits it.
3. **Design-system documentation** in `website/CLAUDE.md` (a new "Design system"
   section): the color tokens (the `@theme` CSS variables, light + dark),
   typography (Space Grotesk headings / JetBrains Mono mono + the
   `.prose-termsurf` size scale), spacing conventions, and the **component
   inventory** — `prose-termsurf` elements, code blocks, tables, generated
   reference layout, nav sidebar + `<details>` disclosures, Pagefind search,
   `VTSequence`, footnotes, and the new callouts. Explicitly **no version
   switcher** (per Exp 12).
4. **Scope for this experiment.** Tokens + inventory documentation + the callout
   primitive only. Page templates (home/article/reference/section-index),
   responsive/mobile nav, the a11y baseline, and the home/marketing-page
   treatment are **later Phase-2 experiments** — this one establishes the
   documented token/primitive foundation they build on.

## Changes

Files in `website/`:

1. **`astro.config.mjs`** — add `remarkAlert` to `markdown.remarkPlugins` (MDX
   inherits these via `@astrojs/mdx`'s default `extendMarkdownConfig: true` —
   verified; add a one-line comment recording that `mdx()` must stay
   option-free, or re-list the plugin, so MDX alerts don't silently stop
   transforming).
2. **`src/styles/style.css`** — `.markdown-alert*` Tokyo Night styling
   (container, title, per-kind accent), scoped under `.prose-termsurf` where
   appropriate.
3. **`website/CLAUDE.md`** — the "Design system" section (tokens, type, spacing,
   component inventory).

No content/source-page edits; no fork/schema/nav changes. `package.json` already
has the dev dependency.

## Verification

1. **Callouts render styled.** The 9 pages' `[!NOTE]`/`[!WARNING]`/etc. render
   as `.markdown-alert` boxes with a title and accent — **no literal `[!NOTE]`**
   text remains in built HTML. Spot-check `vt/osc/1x` (NOTE), `vt/osc/52`
   (NOTE + WARNING), `vt/concepts/colors` (NOTE).
   - **Pass:** styled alerts, zero literal `[!…]`. **Fail:** literal markers
     remain or unstyled.
2. **Tokyo Night, zero new JS.** Alert boxes use the semantic color variables
   (no hardcoded colors); the plugin is build-time remark (no client JS added).
3. **Build + checks clean.** `bun run build` 76 pages; `astro check` 0 errors;
   `gen:references --check` + `import:vt --check` exit 0; dead-link crawl clean.
4. **Design system documented.** `website/CLAUDE.md` has a complete Design
   system section (tokens, type scale, spacing, component inventory incl.
   callouts; no version switcher).
5. **No regressions.** Existing pages, `/`, `/welcome`, search, nav unchanged;
   normal (non-alert) blockquotes still render via `.prose-termsurf blockquote`.

A full pass establishes the documented design-system foundation and fixes the
broken callouts. Later Phase-2 experiments: page templates, responsive/mobile
nav + a11y, and the home/marketing treatment.

## Design Review

Independent `adversarial-reviewer`. **Verdict: APPROVE WITH CHANGES.** Verified
the load-bearing question — `@astrojs/mdx` 6.0.3 defaults
`extendMarkdownConfig: true` and `mdx()` is called option-free, so
`markdown.remarkPlugins` **applies to both `.md` and `.mdx`** (8 of 9 alert
files are `.mdx` under `vt/`, so this is the case that mattered). Also
confirmed: the plugin emits inline `<svg>` (zero JS), only dep
`unist-util-visit`; alerts become `<div class="markdown-alert …">` so they
escape `.prose-termsurf blockquote` (plain blockquotes keep it); no
`.markdown-alert`/`.octicon` collision; render- time only, so
`gen:references`/`import:vt` `--check` are unaffected. Two **Required** fixes,
folded in:

1. Warning/Caution have no existing token → add `--color-warning` (amber) and
   `--color-caution` (red) to the `@theme` light + dark blocks; no hardcoded hex
   in the alert rules (per the "no hardcoded colors" rule).
2. The plugin's icon `<svg>` has no `fill` → set `fill: currentColor` on the
   alert-title icon so it takes the accent instead of rendering black on dark.

Optional/nit folded in: only NOTE + WARNING are actually used (TIP/IMPORTANT/
CAUTION styled for future, not build-validated); add an `astro.config.mjs`
comment recording the MDX-inheritance dependency on `mdx()` staying option-free.

## Result

**Result:** Pass

The callout primitive works and the design system is documented; all five
verification criteria pass.

### What was built

- `astro.config.mjs` — `remarkAlert` added to `markdown.remarkPlugins` (with the
  MDX-inheritance comment).
- `src/styles/style.css` — new `--color-warning`/`--color-caution` tokens (light
  - dark) and `.markdown-alert*` Tokyo Night styling (per-kind accent via
    `--alert-accent`, icon `fill: currentColor`).
- `website/CLAUDE.md` — a "Design system" section (color tokens, typography +
  size scale, spacing, component inventory incl. callouts; no version switcher).

### Verification results

1. **Callouts render styled** — built `vt/osc/1x` (note), `vt/osc/52` (note +
   warning), `vt/concepts/colors` (note) emit `.markdown-alert-note` /
   `.markdown-alert-warning` divs; **zero** literal `[!NOTE]`/`[!WARNING]` text
   remains. **Pass.**
2. **Tokyo Night, zero new JS** — alerts use the semantic tokens (no hardcoded
   hex in the rules); the icon `fill: currentColor` ships in the built CSS;
   `remarkAlert` is build-time (no client JS). **Pass.**
3. **Build + checks clean** — `bun run build` 76 pages; `astro check` 0 errors;
   `gen:references --check` + `import:vt --check` exit 0; dead-link crawl = 0.
   **Pass.**
4. **Design system documented** — `website/CLAUDE.md` Design system section is
   complete (tokens, type scale, spacing, inventory, no switcher). **Pass.**
5. **No regressions** — `/`, `/welcome`, search, nav unchanged; plain
   blockquotes (e.g. the VT index attribution note, which isn't an `[!…]` alert)
   still render via `.prose-termsurf blockquote`. **Pass.**

## Conclusion

Phase 2 has its documented design-system foundation, and a real rendering bug
(literal `[!NOTE]` across 9 pages) is fixed with a properly themed callout
primitive. The token set gained `--color-warning`/`--color-caution`. Remaining
Phase-2 experiments: page templates (home/article/reference/section-index),
responsive/mobile nav + an accessibility baseline, and the home/marketing-page
treatment. Then Phases 3–4 build the content into the IA.
