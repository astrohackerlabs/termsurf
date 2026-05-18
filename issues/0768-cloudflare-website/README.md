+++
status = "open"
opened = "2026-03-29"
+++

# Issue 768: Migrate termsurf.com from Fly.io to Cloudflare Pages

## Goal

Move termsurf.com hosting from Fly.io (Docker + SSR) to Cloudflare Pages
(static), matching the deployment pattern used by ryanxcharles.com and
astrohacker.com.

## Background

### Current setup (Fly.io)

The TermSurf website (`website/`) uses TanStack Start — a React meta-framework
with SSR, server functions, and file-based routing. It deploys as a Docker
container to Fly.io:

- **Framework:** TanStack Start (React 19 + Vite 7)
- **Rendering:** Server-side rendering with `createServerFn` for runtime data
- **Deploy:** `fly deploy` via multi-stage Dockerfile (Bun build, Bun serve)
- **Region:** iad (single region, auto-stop machines)
- **Blog:** Markdown in top-level `blog/`, content loaded at runtime via server
  function reading files from disk

The SSR server function (`src/server/blog.ts`) reads markdown files from
`../../blog/` at request time. This is the only server-side behavior — all other
data (blog metadata, commit history) is pre-generated as JSON at build time.

### Target setup (Cloudflare Pages)

Both ryanxcharles.com and astrohacker.com follow the same pattern:

- **Framework:** Astro 6 with `output: "static"`
- **Styling:** Tailwind CSS v4 via `@tailwindcss/vite`
- **Build output:** `dist/` (static HTML/CSS/JS)
- **Deploy:** `wrangler pages deploy dist`
- **Config:** No `wrangler.toml` — project name cached locally by wrangler
- **DNS:** Managed in Cloudflare dashboard
- **www redirect:** `public/_redirects` file or Cloudflare dashboard rule

Deploy command from package.json:

```
"deploy": "bun run build && wrangler pages deploy dist"
```

### Why migrate

- **Cost:** Cloudflare Pages is free for static sites. Fly.io bills for machine
  time.
- **Performance:** Cloudflare's CDN serves from 300+ edge locations. Fly.io
  serves from one region (iad).
- **Simplicity:** No Docker, no server process, no health checks. Just static
  files on a CDN.
- **Consistency:** Match the deploy pattern of the other sites.

## Analysis

### What needs to change

The website must become fully static. The one piece of server-side behavior is
blog post content loading — the server function reads markdown from disk at
request time. This must move to build time.

**Two approaches:**

#### Approach A: Convert to Astro

Replace TanStack Start with Astro, matching ryanxcharles.com and astrohacker.com
exactly. Astro's content collections handle markdown natively. The existing
React components can run as Astro islands (`client:load`).

**Pros:**

- Identical stack to the other two sites.
- Astro's content collections are purpose-built for markdown blogs.
- Simpler mental model — one framework across all sites.
- Smaller output (Astro ships zero JS by default, only hydrates islands).

**Cons:**

- Requires rewriting routes, layouts, and the root document.
- TanStack Router's file-based routing doesn't map 1:1 to Astro's.
- The Three.js welcome page needs to be ported as a client island.

#### Approach B: Keep TanStack Start, add static adapter

TanStack Start supports static site generation via `@tanstack/static-adapter`.
The server function for blog content can be replaced with build-time data
loading (read all markdown at build, write rendered HTML to JSON).

**Pros:**

- Minimal code changes — keep existing routes, components, and layouts.
- No framework migration.

**Cons:**

- TanStack Start's static mode is less mature than Astro's.
- Diverges from the other sites' stack.
- Still ships the full React runtime to every page.

### Recommendation

This issue does not prescribe an approach. Experiment 1 should determine which
is more practical.

### Deployment changes (either approach)

Regardless of framework choice:

1. **Add `wrangler` as a dev dependency.**
2. **Add deploy script:**
   `"deploy": "bun run build && wrangler pages deploy dist"`
3. **First deploy:** `wrangler pages deploy dist` will prompt to create the
   project (name: `termsurf`).
4. **DNS:** In Cloudflare dashboard, add CNAME records for `termsurf.com` and
   `www.termsurf.com` pointing to the Pages deployment. Configure www redirect
   in the dashboard.
5. **Remove:** `fly.toml`, `Dockerfile`, `.dockerignore`, and `fly deploy`
   script.

## Experiments

### Experiment 1: Convert website from TanStack Start to Astro

Replace the TanStack Start framework with Astro (static output), matching the
pattern used by ryanxcharles.com and astrohacker.com. Then deploy to Cloudflare
Pages via wrangler.

#### Current inventory

**Routes (11):**

| Route               | URL              | Content                                                |
| ------------------- | ---------------- | ------------------------------------------------------ |
| `__root.tsx`        | (layout)         | HTML shell, Header/Footer, scanline overlay, dark mode |
| `index.tsx`         | `/`              | Hero, screenshot, latest blog post, commit log (10)    |
| `blog.tsx`          | `/blog` (layout) | Feed link declarations, outlet                         |
| `blog/index.tsx`    | `/blog/`         | Blog archive listing                                   |
| `blog/$slug.tsx`    | `/blog/:slug`    | Individual post (loads markdown via server function)   |
| `manifesto.tsx`     | `/manifesto`     | Static prose                                           |
| `commits.tsx`       | `/commits`       | Full commit list                                       |
| `docs.tsx`          | `/docs`          | Placeholder ("Coming soon")                            |
| `welcome.tsx`       | `/welcome`       | Three.js 3D scene (standalone, no layout)              |
| `test-media.tsx`    | `/test-media`    | Test page (camera/mic)                                 |
| `test-download.tsx` | `/test-download` | Test page (downloads)                                  |

**Components (4):**

| Component       | Type                                          | Notes                                  |
| --------------- | --------------------------------------------- | -------------------------------------- |
| `Header.tsx`    | React (uses `useRouterState` for active link) | Needs active-page detection in Astro   |
| `Footer.tsx`    | React (pure, no hooks)                        | Convert to `.astro`                    |
| `CommitLog.tsx` | React (`useState` for expand/collapse)        | Keep as React island (`client:load`)   |
| `Markdown.tsx`  | React (react-markdown + remark/rehype)        | Replace with Astro's built-in markdown |

**Server function (1):**

`src/server/blog.ts` — `getBlogPost()` reads markdown from `../../blog/` at
request time. This must move to build time. Astro's content collections or a
build script can handle this.

**Data files (2):**

- `data/blog.json` — blog metadata (generated by `scripts/build-blog.ts`)
- `data/commits.json` — commit history (generated by `scripts/build-commits.ts`)

Both are already generated at build time. No change needed.

**Static assets:**

- `public/favicon.ico`
- `public/images/` — screenshots, logos
- `public/blog/` — RSS/Atom/JSON feeds (generated by `scripts/build-blog.ts`)
- `public/helvetiker_bold.typeface.json` — Three.js font for welcome page

#### Changes

**1. New config files**

- `astro.config.mjs` — static output, `trailingSlash: "never"`, Tailwind vite
  plugin, React integration (for interactive islands).
- `tsconfig.json` — update to use Astro's TypeScript preset.
- `package.json` — replace TanStack Start deps with Astro + `@astrojs/react`.
  Add `wrangler` as dev dependency. Update scripts:
  - `"dev": "astro dev"`
  - `"build": "bun run build:data && astro build"`
  - `"deploy": "bun run build && wrangler pages deploy dist"`

**2. Layout: `src/layouts/Base.astro`**

Replace `__root.tsx`. The Astro layout renders the HTML shell:

- `<html lang="en" class="dark">`
- Charset, viewport, title (from prop)
- Import `globals.css`
- Scanline overlay div
- Slot for page content

**3. Header: `src/components/Header.astro`**

Convert from React to Astro. Use `Astro.url.pathname` for active link detection
(replaces `useRouterState`). Pure markup, no client JS needed.

**4. Footer: `src/components/Footer.astro`**

Direct conversion from React to Astro. Trivial — it's already pure markup.

**5. CommitLog: keep as `src/components/CommitLog.tsx`**

This component uses `useState` for expand/collapse. Keep it as a React component
and render it as an Astro island with `client:load`. Remove the
`@tanstack/react-router` import (it doesn't use it).

**6. Markdown rendering**

Astro handles markdown natively. Blog posts can use Astro's built-in markdown
pipeline with the same remark/rehype plugins (smartypants, GFM, math, KaTeX,
highlight). No need for `react-markdown` or the `Markdown.tsx` component.

For blog posts loaded from `../../blog/`, use `build-blog.ts` to render markdown
to HTML at build time (add rendered HTML to `blog.json`). Then the blog post
page just injects the pre-rendered HTML.

**7. Pages**

Each TanStack route becomes an Astro page:

| TanStack route      | Astro page                    | Notes                                                       |
| ------------------- | ----------------------------- | ----------------------------------------------------------- |
| `index.tsx`         | `src/pages/index.astro`       | Import blog.json + commits.json, render CommitLog island    |
| `blog/index.tsx`    | `src/pages/blog/index.astro`  | Import blog.json, list posts                                |
| `blog/$slug.tsx`    | `src/pages/blog/[slug].astro` | `getStaticPaths()` from blog.json, render pre-built HTML    |
| `manifesto.tsx`     | `src/pages/manifesto.astro`   | Static prose, direct port                                   |
| `commits.tsx`       | `src/pages/commits.astro`     | Import commits.json, CommitLog island                       |
| `docs.tsx`          | `src/pages/docs.astro`        | Placeholder                                                 |
| `welcome.tsx`       | `src/pages/welcome.astro`     | Standalone layout, Three.js as `client:only="react"` island |
| `test-media.tsx`    | Drop                          | Test page, not needed in production                         |
| `test-download.tsx` | Drop                          | Test page, not needed in production                         |

**8. Blog build script update**

Update `scripts/build-blog.ts` to also render markdown content to HTML (using
unified/remark/rehype) and include it in `blog.json`. This eliminates the server
function — all blog content is available at build time.

**9. Deploy infrastructure**

- Remove `fly.toml`, `Dockerfile`, `.dockerignore`
- Update deploy script in package.json

**10. Globals and styling**

`src/globals.css` (or `src/styles/style.css`) carries over unchanged. Same Tokyo
Night theme, same scanline overlay, same `.prose-termsurf` styles. Tailwind v4
via `@tailwindcss/vite` — identical to the other sites.

#### Verification

1. **Build succeeds:**
   - `bun run build` completes without errors.
   - `dist/` contains static HTML for all pages.

2. **All pages render correctly:**
   - `/` — hero, screenshot, latest post link, commit log with expand/collapse.
   - `/blog` — post listing with dates, titles, authors.
   - `/blog/:slug` — rendered markdown with syntax highlighting, math, GFM.
   - `/manifesto` — full prose, correct styling.
   - `/commits` — full commit list with expand/collapse.
   - `/docs` — placeholder text.
   - `/welcome` — Three.js 3D scene loads and animates.

3. **Styling matches current site:**
   - Tokyo Night dark theme.
   - Monospace font, ASCII borders, scanline overlay.
   - `.prose-termsurf` styles on blog posts.
   - Active nav link uses `>[label]` syntax.

4. **Deploy to Cloudflare Pages:**
   - `wrangler pages deploy dist` succeeds.
   - Site is accessible at the Cloudflare Pages URL.
   - www redirect handled in Cloudflare dashboard.

5. **No regressions:**
   - RSS/Atom/JSON feeds still generated in `public/blog/`.
   - Commit log expand/collapse works (React island hydrates).
   - Internal links work (no 404s).
   - External links open in new tab.
