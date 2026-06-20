# TermSurf Website

The TermSurf website at termsurf.com. Built with Astro (static output) and
deployed to Cloudflare Pages.

## Build Commands

| Command              | Purpose                              |
| -------------------- | ------------------------------------ |
| `bun run dev`        | Start Astro dev server               |
| `bun run build`      | Build static site to `dist/`         |
| `bun run build:icons`| Process icons from `raw-icons/`      |
| `bun run gen:references`| Regenerate the config + keybind reference pages |
| `bun run deploy`     | Build + deploy to Cloudflare Pages   |

## Stack

- **Framework:** Astro 6 with `output: "static"`
- **Integrations:** `@astrojs/react` (for Three.js welcome page)
- **Styling:** Tailwind CSS v4 via `@tailwindcss/vite`
- **Fonts:** Space Grotesk (headings) + JetBrains Mono (mono) via Google Fonts
- **Hosting:** Cloudflare Pages via `wrangler pages deploy dist`
- **Package manager:** Bun

## Information architecture & versioning (issue 834)

**Target sitemap** (sidebar section order in `src/lib/docs-nav.ts`
`SECTION_ORDER`): ungrouped (Getting Started, About), then Install,
Configuration (overview + generated Config Options + Keybind Actions + planned
keybindings overview), Features, Terminal API (the VT reference), TermSurf (How
TermSurf Works, Web TUI, Architecture, Protocol, Browser Engines, Roadmap),
Help, Sponsor. Ghostty-parity sections are
macOS-accurate (no Linux/GTK). Sections marked elsewhere as "Phase 3/4" are not
built yet; `Components`/`Protocol` are transitional and fold into the `TermSurf`
group when its landing page exists.

**Versioning:** single-version, **no version switcher** for 1.0 (one current
version; Cloudflare keeps prior deploys for rollback). If multiple supported
versions ever exist: snapshot under a version prefix + add a switcher. (So
Phase 2's design system should **not** include a version-switcher component.)

## Docs content

Doc pages are MDX entries in the `docs` content collection
(`src/content/docs/**/*.mdx`), rendered by the dynamic route
`src/pages/docs/[...slug].astro`. The collection schema and loader live in
`src/content.config.ts`; the sidebar is generated from the collection by
`src/lib/docs-nav.ts` (no hardcoded nav). To add a doc page, create an `.mdx`
file under `src/content/docs/` with frontmatter — it appears in the build and
sidebar automatically.

Frontmatter: `title` (required), `navLabel` (optional shorter sidebar label),
`description`, `section` (sidebar group heading), `order` (sort within section),
`draft` (excludes from build + nav).

| Path | Source | Content |
|------|--------|---------|
| `/` | `src/pages/index.astro` | Homepage — hero, screenshot, description |
| `/docs` | `src/pages/docs/index.astro` | Docs landing |
| `/docs/getting-started` | `src/content/docs/getting-started.mdx` | Install + setup |
| `/docs/architecture` | `src/content/docs/architecture.mdx` | Multi-process design |
| `/docs/components/webtui` | `src/content/docs/components/webtui.mdx` | Web TUI |
| `/docs/components/roamium` | `src/content/docs/components/roamium.mdx` | Roamium engine |
| `/docs/protocol/overview` | `src/content/docs/protocol/overview.mdx` | Protocol design |
| `/docs/protocol/messages` | `src/content/docs/protocol/messages.mdx` | Message reference |
| `/docs/reference/configuration` | `src/content/docs/reference/configuration.mdx` | Config guide (hand-written) |
| `/docs/reference/config` | `src/content/docs/reference/config.md` | Config option reference (generated) |
| `/docs/reference/keybind-actions` | `src/content/docs/reference/keybind-actions.md` | Keybind action reference (generated) |
| `/welcome` | `src/pages/welcome.astro` | Three.js 3D experience |

## Generated reference pages

`src/content/docs/reference/config.md` and
`src/content/docs/reference/keybind-actions.md` are **generated, do not edit by
hand.** `scripts/gen-references.ts` parses the Ghostboard fork's
`zig-out/share/ghostty/doc/ghostty.5.md` (config man page) and writes both pages;
the output is committed so the Cloudflare build needs no fork checkout (Ghostty's
`sync-webdata` model). Regenerate with `bun run gen:references` whenever the
fork's config options or keybind actions change; `bun run gen:references --check`
fails if the committed pages are stale. The fork man-page path can be overridden
with `--in <path>` or `GHOSTTY_DOC`.

## Terminal API (VT) docs

The `/docs/vt/**` pages (64) are **adapted from Ghostty's MIT-licensed VT
documentation** (`ghostty-org/website`); see the repo `NOTICE`. They are
**generated, do not edit by hand** — `scripts/import-vt.ts` reads a Ghostty
website checkout (`--in <repo>/docs/vt` or `GHOSTTY_VT_DIR`) and writes all VT
pages with nested-nav frontmatter (`section: Terminal API`, `subsection`,
`order`, `navLabel`), adapted links/anchors (placeholders inlined, Ghostty
config links → `/docs/reference/config`, in-page fragments re-slugged to match
heading ids), and the safe `## Ghostty Status` → `## Implementation Status`
rename. `bun run import:vt` regenerates; `--check` flags drift.
`src/components/VTSequence.astro` is a static (zero-JS) port of Ghostty's
`VTSequence` component, provided to MDX via `components={{ VTSequence }}`.

**Interim voice:** product/behavior claims remain **upstream-attributed** (they
name Ghostty, which is true — TermSurf's Ghostboard inherits its VT engine),
behind the framing note on `/docs/vt`. The full **TermSurf rebrand + per-claim
fork verification** (against `ghostboard/src/**`, platform-aware) is done by
issue 834's Experiment 5+ (one per subsection), after which those pages are
hand-maintained, not regenerated.

## Components

| File | Purpose |
|------|---------|
| `src/layouts/Base.astro` | HTML shell, fonts, header, footer |
| `src/components/VTSequence.astro` | VT escape-sequence diagram (static port of Ghostty's component) |
| `src/components/Header.astro` | Logo + nav links |
| `src/components/Footer.astro` | Astrohacker branding + copyright |
| `src/components/DocPage.astro` | Docs layout shell (sidebar from `docs-nav.ts` + prose article) |
| `src/components/WelcomePage.tsx` | Three.js welcome scene (React island) |
| `src/pages/docs/[...slug].astro` | Renders `docs` collection entries |
| `src/lib/docs-nav.ts` | Generates the docs sidebar from the collection |

## Styling

### Theme

Tokyo Night color palette. Light mode is default, dark mode activates via
`prefers-color-scheme: dark`. Colors defined in `src/styles/style.css` using
CSS custom properties.

**Do not hardcode colors.** Use semantic variables: `text-primary`,
`bg-background`, `text-muted`, `border-border`, etc.

### Fonts

- `--font-heading`: Space Grotesk (headings)
- `--font-mono`: JetBrains Mono (code)

Loaded via Google Fonts in `Base.astro`.

### Docs Prose

Doc pages use `prose-termsurf` class for styled content (defined in
`style.css`). Headings, paragraphs, links, code blocks, tables, lists, and
blockquotes are all styled.

### Welcome Page

The `/welcome` route is a standalone Three.js 3D experience with its own
layout (no header/footer). Uses `client:only="react"`. Do not modify when
changing site-wide styles.

## Design system (issue 834, Phase 2)

Refine-don't-reinvent: the system formalizes the existing Tokyo Night look.

**Color tokens** (`@theme` in `style.css`, light default + `prefers-color-scheme:
dark` override — never hardcode hex, use these): `--color-background`,
`--color-background-dark`, `--color-background-highlight`, `--color-foreground`,
`--color-foreground-dark`, `--color-primary` (blue), `--color-secondary`
(purple), `--color-accent` (cyan), `--color-success` (green), `--color-warning`
(amber), `--color-caution` (red), `--color-muted`, `--color-border`. Tailwind
exposes them as `text-*`/`bg-*`/`border-*` utilities.

**Typography:** `--font-heading` Space Grotesk (headings), `--font-mono`
JetBrains Mono (code), loaded in `Base.astro`. The doc size scale is in
`.prose-termsurf` (body 0.875rem/1.7; h1 1.125rem; h2 1rem primary; h3
0.875rem; code 0.8125rem). Spacing: 1rem block rhythm, 0.75rem code padding,
2rem h2 top margin.

**Component inventory** (all Tokyo Night, mostly zero-JS):

- `prose-termsurf` — doc article typography (headings, p, links, lists, tables,
  blockquotes, hr).
- Code blocks — `bg-background-dark` + left border; hand-highlighted token
  `<span>`s (Shiki disabled).
- Generated reference layout — `# Title` + `## \`name\`` entries (config/keybind).
- Sidebar nav — generated from `docs-nav.ts`; VT subsections as zero-JS
  `<details>` disclosures (`.docs-nav-sub`).
- Search — Pagefind UI in the sidebar (`Search.astro`); the one JS feature,
  docs-pages only.
- `VTSequence.astro` — static escape-sequence diagram.
- Footnotes — `.prose-termsurf .footnotes`.
- Callouts — GitHub `[!NOTE]`-style alerts via `remark-github-blockquote-alert`
  → `.markdown-alert*`, accented per kind (note/tip/important/warning/caution).

**No version switcher** (single-version posture, Exp 12). Pending Phase-2 work:
page templates, responsive/mobile nav (sidebar is `md:block` — no search/nav
below `md` yet), and an accessibility baseline.
