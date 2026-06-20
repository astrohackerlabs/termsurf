# Experiment 8: VT OSC subsection — TermSurf rebrand + fork verification (Phase 1)

## Description

The fourth and heaviest VT verification pass: the **OSC** subsection (15 pages,
**10 with product/behavior claims** — the design review found `osc/8` also has
behavioral claims). Same method as Concepts/Control — verify each claim against
the Ghostboard fork **by tracing to the consumer, not just the parser**, rebrand
confirmed claims, correct/soften genuinely stale ones, then move all 15 OSC
pages to the importer's `VERIFIED` set. The 5 claim-free OSC pages (`4`, `5`,
`104`, `105`, `7` — `7` was the Exp-3 slice) are verified-by-absence like
CSI/ESC.

> **Critical lesson from the design review (verify the consumer, not the
> parser):** the first draft assumed `osc/1x`'s "only 10-12" and `osc/11x`'s
> "only 110-112" were stale because the OSC parser _routes_ 10-19/110-119 to the
> color parser. But the **consumer** applies only foreground/background/cursor
> (10-12 / 110-112) and explicitly no-ops 13-19 / 113-119
> (`termio/stream_handler.zig:1254-1386`,
> `terminal/stream_terminal.zig:614-647`, all logging "not implemented"). So
> those page claims are **accurate** — and, worse, **Experiment 5
> over-claimed**: `concepts/colors.mdx` was changed to "ten dynamic colors, OSC
> 10-19", which is **false**. This experiment also **corrects that Exp-5 error**
> (see decision below). Routing/parsing ≠ application.

### Claim inventory (10 pages) and verification plan

| Page     | Claim(s)                                                                                                           | Verify against                                                                                                                            | Expected disposition                                                                                                      |
| -------- | ------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `0`      | "does not support changing the window icon" (so OSC 0 sets title only)                                             | `osc.zig` OSC 0 → `change_window_title`; OSC 1 (icon) handling                                                                            | verify → rebrand or correct                                                                                               |
| `1`      | "do not implement this sequence" + a **Yes/No compat table** (Ghostty = No)                                        | `osc.zig` OSC 1 `change_window_icon`                                                                                                      | verify; **set the TermSurf cell from the fork**, keep other-terminal columns                                              |
| `1x`     | "only supports `n` between 10 and 12; others ignored"                                                              | **ACCURATE** — consumer `stream_handler.zig:1254-1268` applies only 10/11/12, no-ops 13-19 ("not implemented")                            | **keep** the caveat; rebrand Ghostty→TermSurf only (do **not** widen)                                                     |
| `11x`    | "only supports `n` between 110 and 112; others ignored"                                                            | **ACCURATE** — same reset path, `stream_handler.zig:1291-1331` resets only 110/111/112                                                    | **keep** the caveat; rebrand only                                                                                         |
| `2`      | "always unconditionally expects `t` to be UTF-8"                                                                   | `stream.zig:1977` UTF-8 validated                                                                                                         | **confirmed** → rebrand                                                                                                   |
| `8`      | "only recognized parameter is `id`"; "refuses a `file://` URI without hostname"                                    | OSC 8 hyperlink parser                                                                                                                    | verify → rebrand/soften (design review flagged osc/8 is NOT claim-free)                                                   |
| `9`      | "parser silently converts invalid ConEmu → OSC 9"                                                                  | `osc/parsers/osc9.zig:186` `break :conemu` fallback                                                                                       | **confirmed** → rebrand                                                                                                   |
| `22`     | "uses CSS's list of cursor shapes" + a **version compat table** (Ghostty = `1.0.0`)                                | `terminal/mouse.zig:31-69` cursor-shape names; OSC-22 "Yes" justified by `stream_handler.zig:1057`; `1.0.0` cell is **Ghostty's** version | rebrand prose (confirmed); **version cell** → TermSurf only with a fork-verified value, else Yes/version-neutral (rule 1) |
| `52`     | "only recognizes `c`,`p`,`s`"; "defaults to … clipboard"; "only one clipboard per sequence"                        | c/p/s + alias in consumer `stream_handler.zig:1066-1071`; default-to-`c` + one-clipboard in `clipboard_operation.zig:27,40-45`            | verify each → rebrand; reconcile "primary" → the fork defaults omitted `t` to the **standard** clipboard (`c`)            |
| `conemu` | "differentiates by…"; "only implements listed extensions"; "shown as a progress bar"; **"hardcoded ~15s timeout"** | `osc9.zig`; **the 15s timeout exists on macOS too** — `macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:46,219`                | **confirmed for macOS** → keep/rebrand the timeout (cite the macOS path); verify the other claims                         |

### Special-handling rule: version/compat tables (`osc/1`, `osc/22`)

Keep the other-terminal columns (factual comparisons), but the **Ghostty
column** states _Ghostty's_ values (`1.0.0`, `No`). Relabel that column
"TermSurf" only with a **fork-verified** value (a version where support is
confirmed, or Yes/No), never by copying Ghostty's number. If the TermSurf value
can't be verified, drop the version specificity (Yes/No) rather than assert
Ghostty's. (The OSC-1 "No" and OSC-22 "Yes" cells are fork-justified above; the
`1.0.0` _version_ is not TermSurf's and must not be copied.)

### Range claims (1x, 11x) are ACCURATE — keep, don't widen

The design review (tracing the consumer, not the parser) confirmed the fork
applies only OSC 10-12 / 110-112 and explicitly no-ops 13-19 / 113-119. So both
"only 10-12" / "only 110-112" caveats are **fork-accurate** — keep them, rebrand
Ghostty→TermSurf only.

### Correct the Experiment 5 over-claim in `colors.mdx`

This pass revealed that Exp 5's `concepts/colors.mdx` edit — "TermSurf … 10
**dynamic colors** … OSCs 10 to 19" — is **false**: the fork applies only
foreground/background/cursor (OSC 10-12); the `Dynamic` enum's 13-19 slots are
defined but **not implemented** (consumer no-ops them). `colors.mdx` is
corrected here back to the fork-accurate statement — three dynamic colors
(foreground, background, cursor), set with OSC 10-12 — matching `osc/1x`. (The
Exp-5 _experiment file_ is a historical record and is not edited; only the live
`colors.mdx` content is corrected, and this correction is documented here.)

## Changes

Files in `website/`:

1. **`src/content/docs/vt/osc/{0,1,2,8,9,1x,11x,22,52,conemu}.mdx`** (10 pages)
   — per-claim fork-verified rebrand/correction/softening per the inventory; the
   5 claim-free OSC pages (`4,5,104,105,7`) unchanged.
2. **`src/content/docs/vt/concepts/colors.mdx`** — **correct the Exp-5
   over-claim**: dynamic colors back to three (foreground/background/cursor),
   OSC 10-12 (the fork applies only those; 13-19 are defined-but-unimplemented).
3. **`scripts/import-vt.ts`** — add the 15 `osc/*` rel-paths to `VERIFIED`
   (bringing it to 61; only the 3 top-level pages remain mechanical).
   `colors.mdx` is already in `VERIFIED`.

No fork source changes; no nav/schema changes.

## Verification

1. **Every claim fork-cited.** The Result records, per claim, the fork file:line
   and disposition (confirmed/corrected/softened) — verified by tracing the
   **consumer**, not just the parser. Special: the `osc/1`+`osc/22` table cells
   reflect the fork (no copied Ghostty version); the `conemu` timeout cites the
   macOS path; `osc/1x`/`11x` keep their "only 10-12 / 110-112" caveats.
2. **No unverified TermSurf assertion.** Built OSC pages assert no
   TermSurf-specific product fact that isn't fork-confirmed; no
   `Ghostty | 1.0.0` cell relabeled TermSurf with Ghostty's number.
3. **Cross-page consistency.** `colors.mdx` and `osc/1x` now agree: three
   dynamic colors (fg/bg/cursor), OSC 10-12.
4. **Importer coherence.** `import:vt --check` exits 0 (skips 61 verified); a
   re-run leaves OSC pages intact.
5. **Builds + links.** `bun run build` 76 pages; `astro check` 0 errors; OSC
   pages render; no dead links/fragments.
6. **No regressions.** Other pages unchanged.

A full pass leaves OSC fork-verified and TermSurf-branded **and** fixes the
Exp-5 dynamic-colors error; only the 3 top-level VT pages (Exp 9) then remain
before the VT framing note can drop.

## Design Review

**Pass 1 — REJECT.** An independent `adversarial-reviewer`, tracing the fork
_consumers_ (not just the parser), found three gate-blocking disposition errors
in the first draft, all corrected above:

1. `osc/1x` "only 10-12" and `osc/11x` "only 110-112" are **fork-accurate**
   (`stream_handler.zig:1254-1386` applies only 10-12/110-112, no-ops the rest)
   — the draft wrongly called them stale and would have asserted a false
   "supports 10-19". Now: keep + rebrand only.
2. This also exposed that **Exp 5's `colors.mdx` "ten dynamic colors, OSC 10-19"
   is false** — corrected here to three (fg/bg/cursor), OSC 10-12.
3. The `conemu` "~15s timeout" is **not** GTK-only — macOS has it
   (`SurfaceView_AppKit.swift:46`), so it's fork-confirmed; the draft's
   "soften/remove" would have deleted a true claim. Now: keep + rebrand.

Confirmed-correct dispositions (reviewer): osc/0 icon→title alias
(`stream.zig:1986-1989`), osc/1 "No" (`stream.zig:1988`), osc/2 UTF-8
(`stream.zig:1977`), osc/9 ConEmu→OSC9 (`osc9.zig:186`), osc/22 CSS cursor
shapes (`mouse.zig:31`) + OSC-22 "Yes" (`stream_handler.zig:1057`); the
version-table rule is rigorous enough. Optional findings folded in: `osc/8` has
behavioral claims (added to the inventory); `osc/52`'s c/p/s/alias lives in the
consumer `stream_handler.zig:1066` and "primary" maps to the standard (`c`)
clipboard.

**Pass 2 — APPROVE WITH CHANGES.** A fresh reviewer re-verified all three
corrections against the fork (consumer no-ops 13-19/113-119 at
`stream_handler.zig:1255-1380`; `color.zig:372-382` `Dynamic` 10-19; macOS 15s
timer at `SurfaceView_AppKit.swift:46`), confirmed osc/8's claims are
fork-backed (`hyperlink.zig:40`), the osc/52 cites, and the 10+5=15 page split,
with no new errors. One trivial fix (applied): the inventory header said "(9
pages)" → now "(10 pages)". Approved to implement.
