+++
status = "closed"
opened = "2026-03-14"
closed = "2026-03-14"
+++

# Issue 751: Move blog posts to top level

## Goal

Move blog posts from `website/blog-posts/` to a top-level `blog/` directory and
update the website build to use the repo root as the Docker build context. This
proves that the website can include markdown files from anywhere in the repo,
paving the way for top-level `docs/` integration later.

## Background

### Why move blog posts?

The website currently lives in `website/` with blog posts at
`website/blog-posts/`. The Dockerfile builds with `website/` as the context, so
the website can only access files inside that directory.

We want to eventually serve documentation from a top-level `docs/` directory
(like WezTerm's approach). To do that, the Docker build context must be the repo
root. Moving blog posts to the top level first is a smaller, safer change that
proves the pattern works before we tackle docs.

### The earthbucks-com pattern

The earthbucks-com project solves the same problem: a website subdirectory that
needs files from sibling directories. The technique:

1. The deploy script `cd`s to the repo root before running `fly deploy`
2. The Dockerfile uses `COPY` to pull in specific directories from the wider
   context
3. A root-level `.dockerignore` excludes heavy directories (`node_modules`,
   build artifacts)

### What needs to change

1. Move `website/blog-posts/` to `blog/` at the repo root
2. Create a root-level `.dockerignore` that excludes `chromium/`, `vendor/`,
   `wezboard/`, `roamium/`, `webtui/`, and other heavy directories — only allow
   `website/` and `blog/` into the context
3. Update the Dockerfile to build from the repo root context:
   - Copy `website/` for the app
   - Copy `blog/` for blog posts
   - Adjust working directories and paths
4. Update `website/package.json` deploy script:
   `"deploy": "cd .. && fly deploy --config website/fly.toml --dockerfile website/Dockerfile"`
5. Update `website/scripts/build-blog.ts` to read from `../blog/` (or `blog/`
   relative to repo root during Docker build)
6. Update `website/src/server/blog.ts` to read from the correct path at runtime
7. Update `website/vite.config.ts` blog markdown dev middleware to serve from
   the new location
8. Update `website/CLAUDE.md` to reflect the new blog post location

## Experiments

### Experiment 1: Move blog posts and widen Docker context

#### Description

Move blog posts to a top-level `blog/` directory, switch the Docker build
context to the repo root, and update all paths. This is a single coordinated
change — every path reference must update together or the build breaks.

#### Changes

**Move the directory:**

```bash
git mv website/blog-posts blog
```

**Create `/.dockerignore` (repo root):**

Allowlist approach — ignore everything, then explicitly allow what the website
build needs:

```
*
!website/
!blog/
website/node_modules
website/dist
website/.output
website/.tanstack
```

**Update `website/Dockerfile`:**

The build context is now the repo root. The Dockerfile copies `website/` and
`blog/` separately, then builds from `website/`.

```dockerfile
FROM oven/bun:1 AS base
WORKDIR /app

# Install dependencies
FROM base AS deps
COPY website/package.json website/bun.lock ./website/
WORKDIR /app/website
RUN bun install --frozen-lockfile

# Build
FROM base AS build
COPY --from=deps /app/website/node_modules ./website/node_modules
COPY website/ ./website/
COPY blog/ ./blog/
WORKDIR /app/website
RUN bun run build

# Production
FROM base AS production
WORKDIR /app
COPY --from=build /app/website/dist ./website/dist
COPY --from=build /app/website/node_modules ./website/node_modules
COPY --from=build /app/website/package.json ./website/package.json
COPY --from=build /app/blog ./blog
WORKDIR /app/website

ENV NODE_ENV=production
ENV PORT=3000
EXPOSE 3000

CMD ["bun", "dist/server/server.js"]
```

**Update `website/package.json`:**

Change the deploy script:

```json
"deploy": "cd .. && fly deploy --config website/fly.toml --dockerfile website/Dockerfile"
```

**Update `website/scripts/build-blog.ts`:**

Change `DOCS_DIR` (line 13) from `../blog-posts` to `../../blog`:

```typescript
const DOCS_DIR = path.resolve(import.meta.dir, "../../blog");
```

**Update `website/src/server/blog.ts`:**

Change `BLOG_DIR` (line 6) from `blog-posts` to `../blog`:

```typescript
const BLOG_DIR = path.resolve(process.cwd(), "../blog");
```

**Update `website/vite.config.ts`:**

Change `blogDir` (line 9) from `blog-posts` to `../blog`:

```typescript
const blogDir = path.resolve(__dirname, "../blog");
```

**Update `website/CLAUDE.md`:**

Change references from `blog-posts/` to `blog/` (at the repo root). Update the
blog writing instructions to reflect the new location.

#### Verification

1. Local dev server:

```bash
cd website && bun run build:blog && bun run dev
```

Visit `http://localhost:3000/blog` — blog listing should work. Click into the
post — content should render.

2. Production build:

```bash
cd website && bun run build:data && bun run build
```

Build should succeed with no errors.

3. Docker build (from repo root):

```bash
docker build -f website/Dockerfile -t termsurf-test .
```

Image should build successfully.

4. Deploy:

```bash
cd website && bun run deploy
```

Verify `https://termsurf.com/blog` works in production.

**Result:** Pass

Blog data build, production build, and Docker build all succeed. Blog posts are
read from the top-level `blog/` directory. The `.dockerignore` allowlist keeps
the context small (only `website/` and `blog/` are sent to Docker).

#### Conclusion

The earthbucks-com pattern works for TermSurf. The deploy script `cd`s to the
repo root, the Dockerfile copies in `website/` and `blog/` separately, and a
root `.dockerignore` excludes everything else. The same pattern can now be
extended to include a top-level `docs/` directory.

## Conclusion

Blog posts moved from `website/blog-posts/` to a top-level `blog/` directory.
The Docker build context is now the repo root, gated by an allowlist
`.dockerignore`. This proves the pattern for including any top-level directory
in the website build — docs integration can follow the same approach.
