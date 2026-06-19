# TermSurf Website

The TermSurf website at termsurf.com. Built with Astro (static output) and
deployed to Cloudflare Pages.

## Build Commands

| Command              | Purpose                              |
| -------------------- | ------------------------------------ |
| `bun run dev`        | Start Astro dev server               |
| `bun run build`      | Build static site to `dist/`         |
| `bun run build:icons`| Process icons from `raw-icons/`      |
| `bun run deploy`     | Build + deploy to Cloudflare Pages   |

## Stack

- **Framework:** Astro 6 with `output: "static"`
- **Integrations:** `@astrojs/react` (for Three.js welcome page)
- **Styling:** Tailwind CSS v4 via `@tailwindcss/vite`
- **Fonts:** Space Grotesk (headings) + JetBrains Mono (mono) via Google Fonts
- **Hosting:** Cloudflare Pages via `wrangler pages deploy dist`
- **Package manager:** Bun

## Pages

| Path | File | Content |
|------|------|---------|
| `/` | `src/pages/index.astro` | Homepage — hero, screenshot, description |
| `/docs` | `src/pages/docs/index.astro` | Docs landing |
| `/docs/getting-started` | `src/pages/docs/getting-started.astro` | Install + setup |
| `/docs/architecture` | `src/pages/docs/architecture.astro` | Multi-process design |
| `/docs/components/webtui` | `src/pages/docs/components/webtui.astro` | Web TUI |
| `/docs/components/roamium` | `src/pages/docs/components/roamium.astro` | Roamium engine |
| `/docs/protocol/overview` | `src/pages/docs/protocol/overview.astro` | Protocol design |
| `/docs/protocol/messages` | `src/pages/docs/protocol/messages.astro` | Message reference |
| `/docs/reference/configuration` | `src/pages/docs/reference/configuration.astro` | Config reference |
| `/welcome` | `src/pages/welcome.astro` | Three.js 3D experience |

## Components

| File | Purpose |
|------|---------|
| `src/layouts/Base.astro` | HTML shell, fonts, header, footer |
| `src/components/Header.astro` | Logo + nav links |
| `src/components/Footer.astro` | Astrohacker branding + copyright |
| `src/components/DocPage.astro` | Docs layout with sidebar nav |
| `src/components/WelcomePage.tsx` | Three.js welcome scene (React island) |

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
