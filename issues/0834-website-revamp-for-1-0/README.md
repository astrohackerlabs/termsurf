+++
status = "open"
opened = "2026-06-20"
+++

# Issue 834: Complete Website Revamp for 1.0

## Goal

Completely redo the TermSurf website (`website/`) for the 1.0 release. The new
site must (1) document every feature TermSurf currently ships — with an emphasis
on the `web` TUI and the basic UX of how TermSurf works — and (2) provide
comprehensive terminal documentation that mirrors Ghostty's documentation site,
since TermSurf's frontend (Ghostboard) is a Ghostty fork, plus the
terminal-level features TermSurf has added on top (e.g. the split pane border
feature).

The end state is a documentation site that gives a TermSurf user everything a
Ghostty user gets from `ghostty.org/docs`, **plus** TermSurf-specific
documentation for the browser-in-terminal protocol, the `web` TUI, browser
engines (Roamium), and Ghostboard's additions.

## Background

### Precursor

Issue 833 (closed) was a narrow content-accuracy refresh: it corrected stale
1.0.0 install paths, the app bundle name, and the Homebrew trusted-tap flow on
the existing site. This issue is **not** a content patch — it is a complete
revamp of structure, coverage, design, and tooling. Issue 833's corrected
content is the accurate baseline to carry forward.

### Current website state (what we are starting from)

Audited at the start of this issue:

- **Stack:** Astro 6 static site (`output: "static"`), TypeScript strict, Bun,
  React 19 islands (only the Three.js `/welcome` scene), Tailwind CSS v4 via the
  Vite plugin. Deployed to **Cloudflare Pages** via `wrangler pages deploy dist`
  (the `scripts/deploy.sh` Fly.io path is stale and references non-existent
  `build:data`/`fly.toml`/`Dockerfile`).
- **Content authoring:** All content is hand-authored directly in `.astro`
  files. **No markdown, no MDX, no content collections.** Code-block styling is
  re-declared per page (`const cb = "..."`).
- **Navigation:** The docs sidebar is a hardcoded array in
  `src/components/DocPage.astro` — adding a page requires editing component
  code. Header has only GitHub + Docs.
- **Pages today:** `/`, `/welcome`, `/docs`, `/docs/getting-started`,
  `/docs/architecture`, `/docs/components/{webtui,roamium}`,
  `/docs/protocol/{overview,messages}`, `/docs/reference/configuration` (~10
  pages total).
- **Design:** Tokyo Night palette via semantic CSS variables in
  `src/styles/style.css` (light/dark via `prefers-color-scheme`), Space Grotesk
  headings + JetBrains Mono. `.prose-termsurf` class for doc typography. This is
  a usable foundation, not a documented design system.

### What TermSurf 1.0 actually ships (coverage target)

Inventory taken at the start of this issue (shipped unless noted). This is the
TermSurf-specific surface the site must cover:

- **Core UX flow:** `web <url>` in a terminal pane → `web` TUI connects to the
  GUI over a Unix socket → GUI launches/reuses a Chromium (Roamium) process for
  the profile → browser renders as a zero-copy GPU overlay (CALayerHost) → TUI
  draws chrome (URL bar, border, status bar). The basic UX (no alt+tab, modal
  navigation) is the headline story and the documentation's center of gravity.
- **`web` TUI modes (vim-inspired):** Control, Browse, Edit (Normal/Insert/
  Visual submodes), Command.
- **Keybindings:** documented in `docs/keybindings.md` (Esc/Enter mode switches,
  `i`/`A`/`I`/`n`/`v`/`V` URL editing, `:` command mode, `Cmd+C` copy URL, `q`
  quit, `Cmd+[`/`Cmd+]`/`Cmd+R` browser nav).
- **Commands:** `:quit`/`:q`, `:dark [on|off|s]`, `:devtools [dir]`,
  `:viewport height <rows>`, `:viewport reset`.
- **CLI:** `web [URL] [options]`; subcommands `url`, `last`, `status`, `file`;
  options `-p/--profile`, `--incognito`, `-b/--browser`, `--primary-screen`;
  smart URL resolution (`web google.com`, `web :3000`, `web ./file.html`,
  `web devtools`).
- **Profiles:** isolated cookies/storage/sessions per profile; incognito.
- **Browser features shipped:** tab lifecycle, DevTools-in-split, dark-mode
  forwarding, JS dialogs (alert/confirm/prompt/beforeunload), HTTP auth
  (Basic/Digest/NTLM/Negotiate), cursor/target-URL/title/loading state,
  renderer-crash recovery.
- **Ghostboard terminal additions over Ghostty:** the split **pane border**
  feature — `split-border-width`, `focused-split-border-color`,
  `unfocused-split-border-color`, `unfocused-split-saturation` (in
  `ghostboard/src/config/Config.zig`); page title in viewport border;
  active-pane indicator; zero-copy Metal compositing.
- **Architecture:** multi-process (one engine process per profile),
  Unix-socket + protobuf IPC (`termsurf.proto`), GPU compositing.
- **Planned / not 1.0 (document as roadmap, not as shipped):** bookmarks (issue
  100), tabs (issue 310), history, downloads, PDF (issue 792); other engines
  (Surfari/WebKit, Waterwolf/Gecko, Girlbat/Ladybird); other frontends
  (Kitty/Alacritty/iTerm2).

### Ghostty docs we must mirror (coverage target)

Ghostboard is a Ghostty fork, so the site must reach parity with Ghostty's docs
(`ghostty.org/docs`) for terminal behavior, then layer TermSurf on top.
Ghostty's top-level sections:

1. **About** — what it is, philosophy, platform support.
2. **Install** — binary/packages, build from source, packaging, release notes.
3. **Configuration** — overview (file location, `key = value` syntax, includes,
   conditionals) + an **auto-generated option reference** + **keybindings**
   (overview, trigger sequences, **auto-generated action reference**).
4. **Linux** — Linux notes, systemd & D-Bus.
5. **Features** — color theme, shell integration, SSH, AppleScript (macOS).
6. **Terminal API (VT)** — overview + sequence reference + external protocols +
   Concepts / Control / ESC / CSI / OSC sequence groups (dozens of pages).
7. **Help** — terminfo, GTK/macOS platform notes, synchronized output.
8. **Financial Support.**

**Key fact (corrected after research — see scope decisions).** Two distinct
mechanisms, not one:

- **Config + keybind references _are_ machine-generated from source.** Ghostty
  generates `docs/config/reference.mdx` from `Config.zig` and
  `docs/config/keybind/reference.mdx` from `Binding.zig` via its `mdgen` /
  `helpgen` build pipeline, then copies them into the website with a
  `sync-webdata` step. TermSurf's Ghostboard fork already carries that machinery
  (`ghostboard/src/helpgen.zig`, `src/build/mdgen/mdgen.zig`,
  `src/build/GhosttyDocs.zig`) and the generated artifacts
  (`ghostboard/zig-out/share/ghostty/doc/ghostty.5.md`, `ghostty.1.md`). So
  TermSurf _can_ generate these from its own fork, reflecting TermSurf's
  divergences (config path `~/.config/termsurf/config`, pane border options,
  etc.).
- **The VT sequence reference is NOT generated from source — anywhere.**
  Ghostty's per-sequence VT pages are **hand-authored MDX** in its website repo
  (`ghostty-org/website`, ~64 files under `docs/vt/**`). The Zig terminal source
  carries only ad-hoc inline comments (`// CUU - Cursor Up`), no structured,
  extractable catalog; `sync-webdata` never touches VT. Ghostty's own
  `docs/vt/reference.mdx` states it is a hand-curated work-in-progress that does
  not cover every supported sequence. The Ghostty website (including the VT MDX)
  is **MIT-licensed**, so the content is reusable with attribution.

## Scope decisions (settled with the user before drafting experiments)

These were decided up front because they shape the phases:

1. **VT / Terminal API reference: reuse Ghostty's MIT MDX as a base, then
   extend.** Auto-generation from source was the original intent but research
   proved it impossible — the descriptive prose does not exist in Ghostty's (or
   our fork's) source code (see the corrected Key fact above). Instead, import
   Ghostty's hand-authored VT MDX (`docs/vt/**`, MIT-licensed) into TermSurf's
   Astro pipeline with attribution as the starting point, then fill gaps and
   TermSurf-ify over time toward a more complete reference. This is the heaviest
   content deliverable and gets its own phase emphasis. (Rejected alternatives:
   building source-generation infra in the fork — very large, even upstream
   hasn't done it; overview-and-link-out — drops the mirror goal.)
2. **Config + keybind references are auto-generated from source; VT is not.**
   Build a generation pipeline so the **config-option reference** and
   **keybind-action reference** are produced from the fork's source / generated
   man pages (`Config.zig` → `ghostty.5.md`, the keybind action set → `actions`)
   at build time — the fork already has this machinery. The **VT reference is
   the exception**: it is sourced per decision 1 (reuse + extend hand-authored
   MDX), not generated. Hand-written config/keybind reference pages are
   explicitly out — they drift from source.
3. **Keep Astro; improve it, don't re-platform.** The architecture phase works
   within Astro. The work is restructuring (markdown/MDX + content collections,
   generated navigation, a reference-generation build step, a documented design
   system, search) — not switching frameworks. Evaluating an Astro-native docs
   layer (e.g. Starlight) vs. extending the current bespoke setup is a
   legitimate architecture-phase question, but the answer must remain
   Astro-based.
4. **Visual identity: keep and refine Tokyo Night.** The existing Tokyo Night
   palette (Space Grotesk headings + JetBrains Mono) is the established brand.
   Phase 2 systematizes it into documented design tokens and components — it
   does not reinvent the look or explore new palettes/logo treatments.
5. **Platform docs: macOS-only and accurate.** TermSurf 1.0 ships macOS-only
   (Ghostboard is a macOS Ghostty fork). Document only what TermSurf actually
   supports. Ghostty's Linux / systemd / D-Bus / GTK pages are omitted or
   clearly deferred — do not document platform behavior users cannot use. This
   trims literal Ghostty parity in favor of an accurate site.
6. **Roadmap: yes, clearly marked.** The site includes a roadmap for
   planned-but-unshipped features (bookmarks, tabs, history, downloads, PDF,
   additional engines/frontends), with explicit "planned" treatment so nothing
   reads as shipped. Phase 4 owns the roadmap presentation.

## Phased approach

The user wants this delivered in phases. Each phase is a coherent body of work;
experiments are designed, reviewed, implemented, and concluded **one at a time**
within a phase (never listed upfront — the result of each experiment informs the
next, per the project process). The phases below define the order and the
deliverable of each, not a pre-enumerated experiment list.

### Phase 1 — Architecture

Decide the content and build architecture, within Astro. Deliverables:

- Content model: markdown/MDX + Astro content collections (or Starlight),
  replacing hand-authored `.astro` doc pages.
- **Reference auto-generation pipeline (config + keybind only):** a build step
  that turns the Ghostboard fork's source / generated man pages (`ghostty.5.md`,
  `Config.zig`, keybind action set) into the config-option and keybind-action
  reference pages. Define where the fork artifacts come from at build time and
  how they stay in sync. The **VT reference is out of this pipeline** — it is
  reused-and-extended hand-authored MDX per scope decision 1, not generated.
- **VT content import pipeline:** decide how Ghostty's MIT-licensed VT MDX is
  imported into the Astro content model with attribution (a base to extend), and
  how TermSurf's own additions/edits layer on top without being overwritten by
  re-imports.
- Navigation generated from the content tree (no more hardcoded sidebar).
- Search, versioning posture (1.0 now; how future versions are handled), and
  URL/redirect strategy from the current pages.
- Deploy story cleaned up (Cloudflare Pages as the single source of truth; fix
  or remove the stale Fly.io path in `scripts/deploy.sh`).
- A complete information architecture / sitemap for the new site (Ghostty-parity
  sections + TermSurf sections), used by all later phases.

### Phase 2 — Design

Define the visual design system for the revamped site. Deliverables:

- Documented design tokens (extend the existing Tokyo Night semantic variables),
  typography scale, spacing, and component inventory (code blocks, callouts,
  tables, nav, sidebar, search, version switcher, reference-page layout).
- Page templates: landing/home, docs article, auto-generated reference page,
  section index.
- Responsive + light/dark behavior, accessibility baseline.
- The home/marketing page treatment for 1.0 (the headline UX story).

### Phase 3 — Ghostty-parity terminal docs & pages

Build out the terminal documentation to mirror Ghostty, populated from the
TermSurf fork. Deliverables:

- About, Install (binary/Homebrew, build from source, packaging, release notes),
  Configuration (overview + **generated** option reference + keybindings
  overview/triggers + **generated** action reference), Features (theme, shell
  integration, SSH, AppleScript — as applicable to TermSurf's macOS-only
  support), **Terminal API / VT** (overview + **reused-and-extended** sequence
  reference from Ghostty's MIT MDX, per scope decision 1), Help (terminfo, macOS
  platform notes, synchronized output), and Financial Support equivalents. Per
  scope decision 5, Ghostty's Linux / systemd / D-Bus / GTK pages are omitted or
  clearly deferred.
- Config + keybind reference pages are produced by the Phase 1 generation
  pipeline, reflecting TermSurf's actual fork (TermSurf config path, pane border
  options, etc.). The VT reference comes from the Phase 1 VT import pipeline,
  not source generation.

### Phase 4 — TermSurf-specific documentation

Document everything TermSurf adds on top of a terminal. Deliverables:

- The core UX story: how TermSurf works end-to-end (`web <url>` → overlay), the
  no-alt-tab pitch, modal navigation. Center of gravity of the site.
- The **`web` TUI**: modes, keybindings, commands, CLI/subcommands/flags, URL
  resolution, profiles/incognito, dark mode, DevTools splits, dialogs/auth,
  status/last.
- The **protocol** (`termsurf.proto`): overview + message reference, refreshed.
- **Browser engines**: Roamium (shipped) + the engine roadmap
  (Surfari/Waterwolf/Girlbat).
- **Ghostboard additions over Ghostty**: split pane borders and any other
  TermSurf-only terminal config/behavior, cross-linked from the Ghostty-parity
  config reference.
- **Architecture**: multi-process model, Unix-socket + protobuf IPC, GPU
  compositing — refreshed and expanded.
- **Roadmap**: bookmarks, tabs, history, downloads, PDF, additional
  engines/frontends — clearly marked as planned, not shipped.

## Experiments

### Phase 1 — Architecture

- [Experiment 1: Content model & generated navigation](01-content-build-architecture.md)
  — **Pass**
- [Experiment 2: Config & keybind reference generation](02-config-keybind-reference-generation.md)
  — **Pass**
- [Experiment 3: VT reference infrastructure + proof slice](03-vt-infrastructure-and-slice.md)
  — **Pass**
- [Experiment 4: VT bulk import + nested navigation](04-vt-bulk-import-nested-nav.md)
  — **Pass** (mechanical import + nested nav; TermSurf rebrand + fork
  verification decomposed into Experiment 5+)
- [Experiment 5: VT Concepts — TermSurf rebrand + fork verification](05-vt-concepts-rebrand-verify.md)
  — **Pass**
- [Experiment 6: VT Control — TermSurf rebrand + fork verification](06-vt-control-rebrand-verify.md)
  — **Pass**
- [Experiment 7: VT CSI + ESC — verify (no claims) + mark verified](07-vt-csi-esc-verify.md)
  — **Pass**
- [Experiment 8: VT OSC — TermSurf rebrand + fork verification](08-vt-osc-rebrand-verify.md)
  — **Pass**
- [Experiment 9: VT top-level pages — TermSurf rebrand + finalize](09-vt-toplevel-rebrand-verify.md)
  — **Pass** (completes scope decision 1 — entire VT reference fork-verified +
  TermSurf-branded)
- [Experiment 10: Deploy script cleanup (Cloudflare Pages)](10-deploy-script-cleanup.md)
  — **Pass**
- [Experiment 11: Docs search with Pagefind](11-pagefind-search.md) — **Pass**
- [Experiment 12: IA, sitemap & versioning posture](12-ia-sitemap-versioning.md)
  — **Pass** (closes Phase 1 — Architecture)

### Phase 2 — Design

- [Experiment 13: Design system foundation + callout primitive](13-design-system-and-callouts.md)
  — **Pass**
- [Experiment 14: Responsive mobile docs nav](14-responsive-mobile-nav.md) —
  **Pass**
- [Experiment 15: Accessibility baseline](15-accessibility-baseline.md) —
  **Pass**
- [Experiment 16: Tokyo Night contrast refinement](16-tokyo-night-contrast-refinement.md)
  — **Pass**
- [Experiment 17: Home / marketing page treatment](17-home-marketing-treatment.md)
  — **Pass**
- [Experiment 18: Docs landing / section-index template](18-docs-landing-section-index.md)
  — **Pass** (closes Phase 2 — Design)

### Phase 3 — Ghostty-parity terminal docs & pages

- [Experiment 19: About page](19-about-page.md) — **Designed**

## Process notes

- Experiments are created one at a time inside their phase, each with its own
  `NN-{slug}.md` file, design review, plan commit, implementation, result, and
  result commit before the next is designed.
- This issue is documentation/website work. It modifies `website/**` and may add
  a reference-generation build step that **reads** Ghostboard fork artifacts; it
  does not modify the Chromium fork, the protocol, the engines, or the release
  scripts beyond the website deploy path, unless an experiment surfaces a doc
  claim that cannot be made true without a separate code issue.
