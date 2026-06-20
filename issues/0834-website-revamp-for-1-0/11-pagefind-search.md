# Experiment 11: Docs search with Pagefind (Phase 1)

## Description

A Phase-1 deliverable: client-side search over the docs. The site now has ~70
doc pages (migrated guides, generated config/keybind references, the 64-page VT
reference) — search is the missing way to navigate them.

**Pagefind** is the right fit for a static Astro site: it indexes the **built
HTML** in `dist/` after the build (no server, no SaaS), ships a small
self-hosted index + UI, and is the de-facto standard for Astro/Starlight docs
search. The binary is installable and runnable here (verified:
`pagefind 1.5.2`).

### How it works (and the one JS exception)

Pagefind runs **after** `astro build`, crawls `dist/`, and writes a search index
plus its UI assets into `dist/pagefind/`. A small search component loads
`/pagefind/pagefind-ui.js` on docs pages. Search is inherently client-side, so
this is the **one** place the site ships JavaScript — scoped to docs pages only;
the marketing home and `/welcome` stay as they are.

**Dev behavior (corrected per review):** the index/UI assets exist only in
`dist/` after a build, so in `astro dev` the script 404s. To avoid a console
`ReferenceError`, the init **guards on the global**
(`if (typeof PagefindUI !== "undefined") new PagefindUI(...)`), so dev shows an
inert search box (no crash) and `build`/`preview` show working search.

## Key decisions

1. **Index only doc content.** Add `data-pagefind-body` to the `prose-termsurf`
   `<article>` in `DocPage.astro` so Pagefind indexes the doc prose, not the
   nav, header, or footer. This keeps results clean (no sidebar/boilerplate
   hits) and excludes non-doc pages (home/welcome) from the index automatically.
2. **Build pipeline.** Change `package.json` `build` to
   `astro build && pagefind --site dist` so every build (and therefore
   `bun run deploy`, which calls `bun run build`) produces the search index.
   `astro check` and the generators are unaffected (separate commands).
3. **Search UI in the docs sidebar.** A `Search.astro` component renders a
   Pagefind UI mount point + loads `/pagefind/pagefind-ui.css` (plain `<link>`)
   and `/pagefind/pagefind-ui.js`, then initializes `new PagefindUI(...)`.
   Placed at the top of the `DocPage` sidebar. **Both scripts MUST use
   `is:inline`** (review finding): the site currently ships zero `<script>`
   tags, and Astro by default processes/bundles `<script>` through Vite — but
   `pagefind-ui.js` exists only in `dist/pagefind/` post-build (not in
   `src`/`public`/the module graph), so a processed tag would fail to resolve.
   `is:inline` makes Astro emit the tags verbatim.
   - **Mobile note (review):** the `DocPage` sidebar is `hidden … md:block`, so
     placing search there means **no search below the `md` breakpoint** for now.
     Accepted for this experiment; responsive nav/search is a Phase-2 (design)
     concern, recorded as a known limitation.
4. **Tokyo Night theming.** Map Pagefind UI CSS variables
   (`--pagefind-ui-primary`, `--pagefind-ui-background`, `--pagefind-ui-border`,
   `--pagefind-ui-font`, etc.) to the site's semantic variables so the search
   box matches the brand.
5. **`.gitignore` / build artifact.** `dist/pagefind/` is build output (under
   the already-gitignored `dist/`); nothing new is committed beyond source.
   Pagefind is a `devDependency` (already added).

## Changes

Files in `website/`:

1. **`package.json`** — `build` → `astro build && pagefind --site dist`
   (`pagefind` already in devDependencies).
2. **`src/components/Search.astro`** (new) — Pagefind UI mount + script/style
   includes; Tokyo Night variable mapping (scoped styles).
3. **`src/components/DocPage.astro`** — add `data-pagefind-body` to the
   `<article>`; render `<Search />` at the top of the sidebar `<nav>`/aside.
4. **`src/styles/style.css`** — Pagefind UI CSS-variable overrides (or scoped in
   `Search.astro`).

No content, fork, or schema changes; the `[...slug]` route and nav are untouched
except the `data-pagefind-body` attribute + the search component.

## Verification

Run from `website/`.

1. **Index builds.** `bun run build` runs Astro then Pagefind; `dist/pagefind/`
   exists with the index and `pagefind-ui.js` + `pagefind-ui.css`.
   - **Pass:** `dist/pagefind/` populated. **Fail:** missing/empty.
2. **Doc content indexed, boilerplate excluded.** The Pagefind index contains
   doc terms (spot-check a distinctive token, e.g. a VT sequence name like
   `DECSTBM` or `bell-features`), and does **not** index nav/footer chrome.
   `data-pagefind-body` is present on doc `<article>`s and absent on `/` and
   `/welcome` (so they're not indexed).
   - **Pass:** doc terms found, home/welcome absent from index. **Fail:**
     otherwise.
3. **Search UI present on docs pages.** Built doc pages include the Pagefind UI
   mount + `/pagefind/pagefind-ui.js`; `/` and `/welcome` do **not** load it
   (stay zero-JS).
   - **Pass:** UI on docs only. **Fail:** UI missing, or JS leaking to home.
4. **Build + check clean.** `bun run build` succeeds; `astro check` 0 errors;
   page count unchanged (search is a component on existing pages); existing
   importer/reference `--check`s still pass.
   - **Pass:** all hold. **Fail:** any error.
5. **No regressions.** Existing doc pages, `/`, `/welcome` render; nav
   unchanged.

A full pass adds working docs search — a Phase-1 deliverable. Remaining Phase 1:
versioning posture and the full IA/sitemap; then Phases 2–4.

## Design Review

Independent `adversarial-reviewer`. **Verdict: APPROVE WITH CHANGES.** It
**empirically** confirmed the load-bearing assumptions by running pagefind on a
fresh build: `pagefind --site dist` (1.5.2) emits `pagefind-ui.js` + `.css` **by
default** (no `@pagefind/default-ui` needed); adding `data-pagefind-body` to the
`<article>` correctly scopes indexing and drops `/`+`/welcome` (0 `<article>`s);
result titles resolve from the in-article `<h1>` on all 74 doc pages; and deploy
is safe (a **local** `bun run build` runs pagefind where the binary is
installed, then uploads `dist/` — no Cloudflare git-build, so no build-image
binary risk). Two **Required** fixes, folded in:

1. **Script loading must be `is:inline`** — the site ships zero `<script>`
   today; Astro would try to bundle a processed tag, but `pagefind-ui.js` is
   dist-only, so it must be emitted verbatim. (Decision 3.)
2. **Dev degradation was misstated** — in dev the script 404s and a bare init
   throws; the init now **guards on `typeof PagefindUI`**. (Description.)

Optional/nit folded in: the `md:block` sidebar means no search on mobile for now
(recorded as a Phase-2 concern); `astro check` is invoked as `bunx astro check`.
