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

## Components

| File | Purpose |
|------|---------|
| `src/layouts/Base.astro` | HTML shell, fonts, header, footer |
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
