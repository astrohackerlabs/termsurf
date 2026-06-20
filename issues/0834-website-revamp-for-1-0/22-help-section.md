# Experiment 22: Help section (Phase 3)

## Description

A Phase-3 (Ghostty-parity) experiment adding the **Help** section. Ghostty's
Help section covers terminfo, platform notes, and synchronized output. TermSurf
has no Help page. This experiment adds one, scoped to the two solidly
**fork-verified** terminal topics plus a short macOS note:

- **Terminfo** — the fork sets `TERM = xterm-ghostty` (`Config.zig:3749`) and
  ships the `ghostty.terminfo` entry
  (`zig-out/share/terminfo/ghostty.terminfo`). The fork kept Ghostty's terminfo
  name — documented honestly, not renamed.
- **Synchronized output** — DEC private **mode 2026** is supported and
  _implemented_: `terminal/modes.zig:296` defines `synchronized_output = 2026`,
  `terminal/Parser.zig:794` parses `?2026$`, and the **renderer pauses rendering
  while the mode is set** (`renderer/generic.zig:1176-1178`, "synchronized
  output started, skipping render") — the proof the mode does real work, not
  just `stream_terminal.zig`'s mode-bit no-op. Reset on resize in
  `terminal/c/terminal.zig:567`.
- **macOS note** — brief; TermSurf 1.0 is macOS-only; link Getting Started for
  install locations (scope decision 5).

## Key decisions

1. **One page `help.mdx`, `section: "Help"`, `order: 1`.** Route `/docs/help`.
   `SECTION_ORDER` already lists "Help" (after Protocol/TermSurf, before
   Sponsor), so the new group slots correctly with no nav-data change.
2. **Terminfo subsection (verified).** State that TermSurf reports
   `TERM=xterm-ghostty` and ships that terminfo entry (inherited from Ghostty —
   the fork did not rename it). If a program over SSH or in an old environment
   doesn't recognize `xterm-ghostty`, use the SSH integration to install it on
   the remote host (link **Features → SSH integration**), or it falls back to
   `xterm-256color`. Cross-link the `term` / `shell-integration-features`
   options in the config reference rather than restating their full text.
3. **Synchronized output subsection (verified).** Explain that TermSurf supports
   the synchronized-output mode (DEC private mode **2026**), which full-screen
   TUIs use to batch a frame of updates so the screen repaints atomically (no
   flicker/tearing). State it plainly; this is a real, **implemented** mode —
   the renderer actually pauses rendering while it's set
   (`renderer/generic.zig:1176`, review-confirmed), not just a parsed mode bit —
   and it is not a config option.
4. **macOS note (brief).** One short paragraph: TermSurf 1.0 is macOS-only; for
   where the app, the `web` CLI, and Roamium are installed, link Getting
   Started. No Linux/GTK content (scope decision 5).
5. **Accuracy — verified, no invention.** Every technical claim traces to the
   fork (the two source facts above + the man page). The terminfo name is stated
   exactly as the fork uses it (`xterm-ghostty`) — this is the one place honesty
   requires naming "ghostty" in a present-tense fact, which is correct because
   that genuinely is the terminfo TermSurf ships. No invented modes/options.
6. **Design system, zero JS.** Plain MDX → `prose-termsurf`; semantic tokens
   only; links only to **built** pages (`/docs/features`,
   `/docs/reference/config`, `/docs/getting-started`).

## Changes

Files in `website/`:

1. **`src/content/docs/help.mdx`** — new page (frontmatter + Terminfo,
   Synchronized output, macOS note). Appears under a new "Help" sidebar group
   and in the generated `/docs` index automatically via `getDocsNav()`.

No other files change: schema, `docs-nav.ts` (Help already in `SECTION_ORDER`),
generated references, and the fork are untouched. Page count **79 → 80**, and a
new "Help" section heading appears in the nav (after Protocol).

## Verification

1. **Builds + placed correctly.** `bun run build` emits `/docs/help`; total
   pages **80**. The sidebar + generated `/docs` index show a **Help** group
   after **Protocol** (per `SECTION_ORDER`), containing the Help page.
   `bunx astro check` 0 errors.
2. **Accuracy (fork-verified).** Terminfo: `TERM=xterm-ghostty` matches
   `Config.zig:3749`; synchronized output: mode `2026` matches
   `terminal/modes.zig:296`. No invented modes/options; exact config-option text
   is linked, not restated. Spot-check both against the fork sources.
3. **macOS-accurate.** No Linux/GTK content; the macOS note links Getting
   Started.
4. **Design system, zero JS, links resolve.** `prose-termsurf`; no hardcoded
   hex; no `<astro-island>` beyond the inherited Pagefind search; dead-link
   crawl over `/docs/help` = 0 broken (every cross-link resolves).
5. **a11y.** Exactly one `<h1>` ("Help"), ordered `<h2>`s (no skipped levels);
   descriptive link text.
6. **No regressions.** `gen:references --check` + `import:vt --check` exit 0;
   the new "Help" group/entry is the only nav addition;
   search/`/`/`/welcome`/other pages unchanged.

A full pass adds the Help section at Ghostty parity (macOS-applicable,
fork-verified). The last Phase-3 candidate is **Sponsor** (Financial Support),
after which Phase 3 is complete and the issue moves to Phase 4
(TermSurf-specific docs).

## Design Review

Independent `adversarial-reviewer`. **Verdict: APPROVE** (no Required findings).
The reviewer independently confirmed every technical claim against the fork:
terminfo `term = "xterm-ghostty"` (`Config.zig:3749`/`:4519`) with the shipped
`ghostty.terminfo` entry whose name literally is `xterm-ghostty|ghostty|Ghostty`
(so stating it is a true present-tense fact, not an overclaim; the fork kept
upstream's terminfo name and there is no TermSurf-renamed one); the SSH
`ssh-env`/`ssh-terminfo` install + `xterm-256color` fallback (man page
3180–3204, opt-in via `shell-integration-features`); and mode 2026 defined
(`modes.zig:296`), parsed (`Parser.zig:794`), reset on resize
(`c/terminal.zig:567`). Placement (`Help` in `SECTION_ORDER` after TermSurf),
scope (only `help.mdx`), links (`features`/`config`/`getting-started` + the
`#term`/`#shell-integration-features` anchors all exist), and macOS framing all
confirmed. One **Optional** finding, folded in:

- The synchronized-output _implementation_ evidence is
  `renderer/generic.zig: 1176-1178` (the renderer pauses rendering while the
  mode is set), not `stream_terminal.zig:553` (an explicit no-op that only
  records the mode bit). The design's evidence trail now cites the renderer; the
  page claim ("supports synchronized output, repaints atomically") was already
  accurate.
