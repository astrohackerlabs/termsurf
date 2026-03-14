# TermSurf Website

The TermSurf website at termsurf.com. Built with TanStack Start, React, and
Tailwind CSS v4. TanStack Start provides SSR, server functions, and file-based
routing via the Vite plugin.

## Build Commands

| Command | Purpose |
| --- | --- |
| `bun run dev` | Start dev server |
| `bun run build` | TanStack Start production build (client + server) |
| `bun run build:blog` | Generate blog.json + feeds from blog-posts/ |
| `bun run build:commits` | Generate commits.json from git history |
| `bun run build:data` | Run all data generators (commits + blog) |

Always run `build:data` before `build` to ensure data files are up to date.

## TanStack Start

### File-based routing

Routes live in `src/routes/`. The TanStack Start Vite plugin generates
`src/routeTree.gen.ts` automatically. Never edit the generated file.

### Naming conventions

| Pattern | Example | URL |
| --- | --- | --- |
| Index | `routes/index.tsx` | `/` |
| Static | `routes/about.tsx` | `/about` |
| Dynamic param | `routes/blog/$slug.tsx` | `/blog/:slug` |
| Layout (with URL) | `routes/blog.tsx` | Wraps all `/blog/*` children |
| Pathless layout | `routes/_app/dashboard.tsx` | `/dashboard` (layout without URL segment) |

### Server functions

Server-only code uses `createServerFn` from `@tanstack/react-start`. Server
functions run on the server during SSR and via RPC on client-side navigation.
Place server functions in `src/server/`.

```tsx
import { createServerFn } from "@tanstack/react-start";

export const getData = createServerFn({ method: "GET" })
  .inputValidator((id: string) => id)
  .handler(async ({ data: id }) => {
    // Server-only code (filesystem, database, etc.)
  });
```

Call server functions from route loaders:

```tsx
export const Route = createFileRoute("/example/$id")({
  loader: ({ params: { id } }) => getData({ data: id }),
  component: Example,
});
```

### Data loading

Static data (commits) comes from JSON files generated at build time
(`data/commits.json`). Import these directly. Blog post content is loaded at
runtime via a server function (`src/server/blog.ts`) that reads markdown from
`blog-posts/`.

### Route params

Dynamic segments use `$` in filenames. Access with `Route.useLoaderData()` or
`Route.useParams()`:

```tsx
<Link to="/blog/$slug" params={{ slug: post.slug }}>Read</Link>
```

### Root route

`src/routes/__root.tsx` renders the full HTML document (`<html>`, `<head>`,
`<body>`) with `HeadContent` and `Scripts` from `@tanstack/react-router`.
TanStack Start manages SSR — there is no separate server file.

## Blog

### Writing posts

Blog posts are markdown files in `blog-posts/`.

**Filename format:** `YYYY-MM-DD-slug.md`

**Front matter:** TOML between `+++` delimiters.

```toml
+++
title = "Post Title"
author = "Author Name"
date = "YYYY-MM-DD"
+++
```

### Build pipeline

`scripts/build-blog.ts` reads all `.md` files from `blog-posts/`,
parses TOML front matter, and produces:

- `website/data/blog.json` — typed metadata only (content loaded at runtime)
- `website/public/blog/feed.json` — JSON Feed
- `website/public/blog/feed.atom.xml` — Atom feed
- `website/public/blog/feed.rss.xml` — RSS feed

Posts are sorted newest-first. Run `bun run build:blog` after adding or editing
posts.

### Types

`src/blog.ts` defines `BlogPost` and `BlogData` interfaces. Import these when
working with blog data.

### Markdown rendering

Uses `react-markdown` with the unified plugin chain:

- `remark-smartypants` — smart typography (curly quotes, em-dashes)
- `remark-gfm` — GitHub-flavored markdown (tables, strikethrough)
- `remark-math` — math syntax (`$...$` inline, `$$...$$` display)
- `rehype-katex` — render math to HTML
- `rehype-highlight` — syntax highlighting via highlight.js/lowlight

## Styling

### Theme

Tokyo Night Day (light) is the default. Tokyo Night (dark) activates via
`prefers-color-scheme: dark`. Both are defined in `src/globals.css` using CSS
custom properties that the `@theme` block and a `@media` override control.

**Do not hardcode colors.** Use the semantic variables: `text-primary`,
`bg-background`, `text-muted`, `border-border`, etc. These automatically
switch between light and dark mode.

### Aesthetic

Cypherpunk / brutalist terminal aesthetic:

- **Monospace everything.** Body text, headings, nav — all use `--font-mono`.
- **ASCII borders.** Use box-drawing characters (`────`, `┌─`, `└─`) instead
  of CSS borders for section dividers.
- **No rounded corners.** No `rounded` classes, no border-radius.
- **No shadows.** No drop shadows or box shadows.
- **No smooth transitions** on interactive elements.
- **Bracket links.** Navigation uses `[label]` style. Active page: `>[label]`.
- **Dense layout.** Tight spacing, less padding.
- **Scanline overlay.** Subtle CRT scanlines via `body::after` in globals.css.

### Voice

We write like Wired Issue 1.02. Breathless. Electric. Like we just plugged into
something bigger than ourselves and we're trying to explain it before the
connection drops. The 'net is a living thing and we're inside it. Every sentence
should make the reader want to open a terminal.

- **Write like a dispatch from cyberspace.** Not a product page. Not a pitch
  deck. A dispatch.
- **The 'net is alive.** It hums. It sprawls. It doesn't have "users" — it has
  citizens, rebels, explorers. TermSurf doesn't "help you browse" — it jacks
  you into the web from the command line.
- **Declare, don't sell.** "The web runs in your terminal" — not "TermSurf
  provides an integrated browsing experience."
- **Verbs that hack.** "Fork." "Hack." "Override." "Intercept." "Jack in."
  Never "leverage," "utilize," "onboard," or "streamline."
- **Name the metal.** Chromium, WebKit, Gecko, Unix sockets, protobuf,
  CALayerHost. The reader wants to know what's under the hood. Tell them. They
  can handle it.
- **First person plural.** *We* wrote this. *We* ship the code. *You* run it.
  This isn't a corporation talking. It's a mailing list.
- **Short sentences.** Staccato. Let them hit. Information wants to be free and
  it doesn't want to wait for your subordinate clause.
- **Proper Case for titles and labels.** Nav links, page titles, headings — all
  Proper Case. UPPERCASE IS FOR SHOUTING.
- **Bracket links like it's Usenet.** `[fork the source]` `[read the protocol]`
  — not "Get Started" buttons with drop shadows.
- **No adjectives that sell.** No "powerful." No "innovative." No "seamless." If
  the thing is good, the reader will feel it in their chest.
- **Address the reader like they have root.** "Your terminal." "Your keyboard."
  Not "end users." Not "customers." You're an operator. Act like one.

### Welcome page

The `/welcome` route is a standalone Three.js 3D experience. It has its own
styling and is excluded from the shared layout in `__root.tsx`. **Do not modify
the welcome page** when changing site-wide styles.
