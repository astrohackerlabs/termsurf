+++
status = "closed"
opened = "2026-03-15"
closed = "2026-03-15"
+++

# Issue 753: Blog post — everything that didn't work

## Goal

Write a blog post documenting all the failed approaches, dead ends, and
abandoned architectures across TermSurf's development history. The post should
be honest, entertaining, and useful to other developers building similar
projects.

## Background

TermSurf has gone through 5+ generations (ts1–ts5, Ghostboard, Wezboard) and
750+ issue documents. Many of those issues contain failed experiments —
sometimes multiple failures per issue before finding the right approach. This is
valuable material that rarely gets written up publicly.

### Why this matters

Most open source projects only show the final result. The failures are
invisible. But the failures are where the real learning happens — they reveal
the constraints of the tools, the assumptions that were wrong, and the
architectural decisions that only become obvious in hindsight.

### Research results

Three agents reviewed all 750+ issue documents. Here is the full catalog of
failures, organized by theme.

### The big architectural dead ends

**CEF off-screen rendering (ts2/ts3) — 26 experiments, never broke 31fps.** The
single biggest dead end. CEF's off-screen rendering caps at ~31fps on macOS.
Issues 325–350 tried everything and couldn't overcome it. This killed both ts2
(in-process CEF) and ts3 (out-of-process CEF via XPC) and forced the pivot to
Chromium Content API.

**Multi-profile in one Chromium process — 2fps with any JavaScript.** Issues
407–421 proved that two `BrowserContext` instances in one process drop to 2fps
whenever both run JavaScript (even a trivial `requestAnimationFrame` loop). CSS
animations work at 60fps — it's specifically Blink main thread scheduling
contention. This forced the one-process-per-profile architecture that defines
TermSurf today.

**Electron's patch set — can't be applied in isolation.** Issue 409 tried
applying all 147 Electron patches to fix the 2fps problem. They require the
entire Electron build system (Node.js, 85 sub-patches,
`is_electron_build=true`). Issue 410 applied just the 3 throttling patches —
they applied cleanly but had zero effect because the targeted code paths
(Hide/WasOccluded) are never called in TermSurf's layout.

**Swift integration with CEF — memory layout mismatch.** Swift classes don't
produce C-compatible struct layouts. CEF validates `base.size` from the struct
pointer; Swift doesn't match. Fatal error:
`CefApp_0_CToCpp called with invalid version -1`. Rust's `#[repr(C)]` solved it.

**Ghostboard archived, Wezboard replaced it.** Ghostboard (Ghostty fork)
completed full browser integration, but Ghostty only runs on Linux and macOS.
WezTerm already runs on Windows, making it the better foundation to prove
TermSurf works cross-platform. WezTerm is also Rust, which has a much larger
ecosystem than Zig — packages like ratatui and edtui are available off the shelf
instead of needing to be written from scratch.

### Failed experiments by theme

#### Rendering and compositing

- **CALayerHost positioning** (Issue 625) — Multiple failed experiments with
  mismatched positioning, CALayer tree conflicts, and CAContext ID propagation
  issues before it worked.
- **FrameSinkVideoCapturer** — Replaced by CALayerHost for zero-copy GPU
  compositing.
- **Second webview positioning** (Issue 727) — 6 partially-failed experiments
  before getting the coordinate math right. contentsScale defaulting to 1.0 on
  Retina (should be 2.0), double-counting offsets, forgetting border widths.
- **Overlay flash on wrong pane** (Issue 749 Exp 1) — Removing
  `update_ca_layer_frame()` made the flash worse (0,0 instead of wrong pane).
  Fixed by deferring creation to the render pass.

#### IPC and protocol

- **XPC gateway re-entrancy** (Issue 653) — Debug and release builds interfered
  via shared Mach service. Partial fix with separate service names.
- **Protocol message ordering** (Issues 728–729) — Messages arriving out of
  order caused pane layout desync.
- **DevTools standalone routing** (Issue 705) — Three failed experiments.
  DevTools requires the XPC gateway architecture; standalone binaries can't
  multiplex messages.
- **Direct binary launches** (Issue 705 Exp 2) — Skipping the C++ wrapper loses
  IPC message framing.

#### Input handling

- **Mouse event forwarding degradation** (Issue 346) — 125Hz mouse movement
  dropped framerate to 12fps due to task queue contention.
- **Scroll phase types** (Issue 731 Exp 1) — Runtime panic because `phase` was
  `u32` but NSEventPhase returns `u64`.
- **Key event forwarding** (Issue 726 Exp 4) — WezTerm's abstract `KeyCode` enum
  doesn't map to Chromium's platform-specific key codes.
- **Scroll event forwarding** (Issue 726 Exp 5) — WezTerm strips scroll phase
  info from NSEvents; Chromium's scroll state machine needs it.
- **Click suppression** (Issue 726 Exp 6) — WezTerm's event dispatch has no
  "consumed" return value.
- **Copy/paste via key synthesis** (Issue 206 Exp 1) — RefCell borrow panic from
  re-entrant macOS callbacks through CEF's message loop.

#### Browser features

- **target="\_blank" links** (Issue 639/750) — Fixed twice. First implementation
  lost during the Issue 708 refactoring.
- **Chromium bundle path override** (Issue 704 Exp 5) — Skipping
  `EnsureCorrectResolutionSettings()` broke other init paths.
- **Deferred view attachment** (Issue 411 Exp 1) — WebContents never appeared;
  `RenderFrameCreated` callback never fired.

### Recurring patterns in the failures

1. **Performance walls** — CEF 31fps ceiling, Chromium multi-profile 2fps. These
   aren't bugs to fix; they're architectural constraints that force redesigns.
2. **Coordinate math** — Every overlay positioning feature took multiple
   experiments to get the math right (scale factors, borders, split offsets).
3. **Cross-process complexity** — IPC ordering, re-entrancy, type mismatches
   across language boundaries.
4. **Lost work during refactoring** — The target="\_blank" fix being lost when
   file paths changed.

### Successful pivots that emerged from failures

- CEF → Chromium Content API (ts4+)
- In-process multi-profile → one-process-per-profile
- Swift/Zig CEF integration → Rust with `#[repr(C)]`
- FrameSinkVideoCapturer → CALayerHost (zero-copy rendering)
- Key event synthesis → direct frame methods (avoided re-entrancy)
- Ghostboard → Wezboard (cross-platform, Rust ecosystem)
- XPC for all IPC → Unix sockets + protobuf

## Experiments

### Experiment 1: Write the blog post

#### Description

Write a blog post titled "How Not to Build a Terminal Browser". The post uses
the cypherpunk/Wired 1990s voice established in the website CLAUDE.md.

#### Structure

The post has three acts:

**Act 1: The graveyard (~40%).** Open with the biggest, most dramatic failure —
the CEF framerate ceiling. 26 experiments. Never broke 31fps. Then cascade
through the other dead ends: multi-profile 2fps, Electron patches that had zero
effect, Swift structs that crash CEF. Each failure is a short, punchy paragraph.
Don't explain everything — name the metal, state the failure, move on. The
reader should feel the velocity of hitting walls.

**Act 2: The turn (~20%).** This is the reward. About 2/3 through, pivot from
"everything that broke" to "what the failures taught us." The failures aren't
random — they cluster around a few recurring constraints (performance walls,
coordinate math, cross-process complexity, lost work during refactoring). This
section names the patterns and shows how each dead end pointed toward the
architecture TermSurf has today. The reader gets the payoff: the failures were
the map.

**Act 3: What works now (~20%).** Brief. Show the current state — the
architecture that all those failures forced into existence.
One-process-per-profile. Unix sockets + protobuf. CALayerHost zero-copy
rendering. The protocol is the product. End with the list of successful pivots
as proof that the graveyard built the road.

Keep the post under 1500 words. Short paragraphs. Staccato sentences. Name the
metal.

#### Changes

**`blog/2026-03-15-how-not-to-build-a-terminal-browser.md`**

Create a new blog post with TOML front matter. Use the established markdown
format from the existing post.

#### Verification

```bash
cd website && bun run build:blog
```

Blog data should build with 2 posts. Visit `http://localhost:3000/blog` and
verify the new post appears and renders correctly.

**Result:** Pass

Blog post written, builds correctly, renders on the site.

#### Conclusion

"How Not to Build a Terminal Browser" covers the full catalog of failures in
cypherpunk voice. Three-act structure: the graveyard (CEF, multi-profile, Swift,
coordinates, lost patches), the map (patterns in the failures), and what the
wreckage built (the current architecture).

### Experiment 2: Rewrite "Introducing TermSurf" in cypherpunk voice

#### Description

The first blog post ("Introducing TermSurf") reads like a product page. It
explains correctly but it does not electrify. Rewrite it in the same cypherpunk
voice as "How Not to Build a Terminal Browser" — dispatches from cyberspace, not
a pitch deck. Change author to "Wez Longboard".

The content stays the same — what TermSurf is, how it works, the architecture
diagram, the engine table, what works today, what is next. But the language
changes. Declare, don't sell. Name the metal. Short sentences. Verbs that hack.

#### Changes

**`blog/2026-03-14-introducing-termsurf.md`**

Rewrite the post. Keep the TOML front matter format. Keep the architecture
diagram and engine table. Change author to "Wez Longboard". Rewrite all prose in
cypherpunk voice.

#### Verification

```bash
cd website && bun run build:blog
```

Blog data should build with 2 posts. Visit `http://localhost:3000/blog` and
verify both posts render correctly.

**Result:** Pass

Post rewritten in cypherpunk voice, renamed to "Terminal, Meet Browser", author
changed to Wez Longboard. Builds and renders correctly.

#### Conclusion

Both blog posts now share the same cypherpunk voice. "Terminal, Meet Browser"
introduces TermSurf as a dispatch from cyberspace, not a product page.

## Conclusion

Two blog posts written in cypherpunk voice: "Terminal, Meet Browser" (the
introduction) and "How Not to Build a Terminal Browser" (the failures catalog).
Both by Wez Longboard. Both build and render on the site.
