# Experiment 12: Information architecture, sitemap & versioning posture (Phase 1)

## Description

The last two Phase-1 deliverables: a **complete information architecture /
sitemap** for the revamped site (the structure all of Phases 2–4 build into),
and the **versioning posture** for 1.0. This is primarily a **planning +
nav-ordering** experiment — it does not write the Ghostty-parity or
TermSurf-specific page _content_ (that's Phases 3–4); it fixes the **structure**
those pages will slot into, and makes the generated sidebar order ready for
them.

### Versioning posture (decided here, obvious default for 1.0)

**Single-version, no version switcher for 1.0.** TermSurf ships one current
version; the docs describe it. We do **not** add a Pagefind/Starlight-style
version dropdown or `/v1/` URL prefixes now — they add structure with no second
version to switch to, and Cloudflare Pages keeps prior deploys if a rollback is
ever needed. Rationale + the future path (if multiple supported versions ever
exist: snapshot under a version prefix + add a switcher) are documented so the
decision is explicit, not accidental. No code for versioning is added.

### Information architecture / sitemap (the keystone)

The target top-level structure mirrors Ghostty's docs (scope decision 5:
macOS-accurate, Linux/GTK omitted) and layers TermSurf's own sections, ordered
for the generated sidebar:

1. **(ungrouped, top)** — Getting Started; About _(new, Phase 3)_
2. **Install** _(new, Phase 3)_ — Homebrew/binary, build from source, release
   notes (packaging omitted — macOS cask only)
3. **Configuration** — overview _(rework of the current hand-written
   `reference/configuration`)_, Config Options _(generated, Exp 2)_, Keybind
   Actions _(generated, Exp 2)_
4. **Features** _(new, Phase 3)_ — color theme, shell integration, SSH,
   AppleScript (macOS-applicable only)
5. **Terminal API** — Overview, Reference, External +
   Concepts/Control/CSI/ESC/OSC _(done, Exps 3–9)_
6. **TermSurf** _(new TermSurf-specific group, Phase 4)_ — How TermSurf Works
   (the UX story), Web TUI _(have)_, Architecture _(have)_, Protocol overview +
   messages _(have)_, Browser Engines (Roamium + roadmap), Roadmap
7. **Help** _(new, Phase 3)_ — terminfo, macOS platform notes, synchronized
   output (GTK/Linux items omitted)
8. **Sponsor / Financial Support** _(new, Phase 3)_

This **reorganizes** the current ad-hoc sections (Components, Protocol,
Reference, Architecture) into the target groups. To keep this experiment bounded
and low-risk, the **URLs of existing pages do not move yet** — only the sidebar
**grouping/order** is set to the target via `docs-nav.ts`'s ordering maps;
re-homing pages to new URLs (with redirects) happens as each section's content
is built in Phases 3–4, against this sitemap. The full sitemap (every planned
page + its URL + source + phase) is recorded as the durable spec.

## Changes

1. **`issues/0834-website-revamp-for-1-0/12-ia-sitemap-versioning.md`** (this
   file) — the authoritative IA/sitemap spec + versioning decision (the
   deliverable).
2. **Regroup `Reference` → `Configuration` (the coherent path, per review).**
   Consolidate the three config pages into one `Configuration` group:
   - **`website/scripts/gen-references.ts`** — emit `section: Configuration`
     (instead of `Reference`) for `config.md` and `keybind-actions.md`; re-run
     `bun run gen:references` so the committed pages match (else
     `gen:references --check` would drift — review finding).
   - **`website/src/content/docs/reference/configuration.mdx`** —
     `section:   Reference` → `Configuration`. URLs are unchanged (they derive
     from `entry.id`/file path, not `section`). `Components`/`Protocol` are
     **not** regrouped into `TermSurf` yet — that group has no landing page, so
     folding `Web TUI`/`Overview`/`Messages` under a bare "TermSurf" heading
     would be confusing; deferred to Phase 4 (recorded).
3. **`website/src/lib/docs-nav.ts`** — set `SECTION_ORDER` to the target order
   **keeping explicit ranks for the still-present transitional sections** so
   they don't sort to the bottom as "unknown" (review finding):
   `["Install", "Configuration", "Features", "Terminal API", "Components", "Protocol", "TermSurf", "Help", "Sponsor"]`.
   With current pages, the rendered order is: ungrouped (Getting Started,
   Architecture) → Configuration → Terminal API → Components → Protocol.
   (`Components`/`Protocol` leave the list when they fold into `TermSurf` in
   Phase 4.)
4. **`website/CLAUDE.md`** — record the IA/sitemap + versioning posture as the
   canonical structure reference for later phases, and note Phase 2 should drop
   the "version switcher" from its component inventory (per the posture here).

The IA also notes a planned **keybindings overview/triggers** page under
Configuration (Ghostty-parity, Phase 3) so the sitemap is complete. No content
pages are written; no URLs change; no fork changes.

## Verification

1. **Sitemap complete & coherent.** The spec lists every planned section/page
   with URL, content source, and phase; existing pages and the done VT/reference
   work map into it without contradiction.
2. **Versioning posture documented.** The single-version decision + rationale +
   future path are recorded (in this file and `website/CLAUDE.md`).
3. **Nav ordering ready (concrete check).** `docs-nav.ts` `SECTION_ORDER`
   reflects the target order; the **built sidebar's section sequence** is
   exactly ungrouped → Configuration → Terminal API → Components → Protocol
   (asserted against the rendered nav), with no URL changes and no empty
   sections.
4. **Build clean.** `bun run build` succeeds; `astro check` 0 errors; the
   `gen:references`/`import:vt` `--check`s still pass; dead-link crawl clean.
5. **No regressions.** All existing pages, `/`, `/welcome`, and search still
   work.

A full pass closes Phase 1 (architecture): content model, generated references,
the complete VT reference, deploy cleanup, search, and now the IA/sitemap +
versioning posture. Phase 2 (design system) and Phases 3–4 (the Ghostty-parity
and TermSurf-specific page content) build into this structure.

## Design Review

Independent `adversarial-reviewer`. **Verdict: APPROVE WITH CHANGES.** Confirmed
the IA/sitemap is sound and complete against the issue goals (Ghostty-parity +
TermSurf sections, macOS-accurate), the versioning posture is well-justified,
and that regrouping `section` frontmatter does **not** move URLs
(`docs-nav.ts:35` derives `href` from `entry.id`). Two **Required** fixes,
folded in:

1. The generated `config.md`/`keybind-actions.md` hardcode `section: Reference`
   in `gen-references.ts`; renaming to `Configuration` requires updating the
   **generator** too (else `gen:references --check` drifts). Adopted the
   coherent path: rename in the generator + regenerate + the hand-written guide.
2. `SECTION_ORDER` must keep **explicit ranks** for still-present transitional
   sections (`Components`, `Protocol`) or they sort last as "unknown"
   (`docs-nav.ts:28-31,83-85`). Done.

Optional/nit folded in: a concrete rendered-nav-order assertion (Verification
3); a planned keybindings-overview page noted under Configuration; Phase 2 to
drop the version-switcher component (per the posture). Components/Protocol fold
into the TermSurf group is correctly **deferred** to Phase 4 (no landing page
yet).
