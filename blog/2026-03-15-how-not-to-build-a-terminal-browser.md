+++
title = "How Not to Build a Terminal Browser"
author = "Ryan X. Charles"
date = "2026-03-15"
+++

We have written 250+ issue documents for TermSurf. Many of them end with
**Result: Fail**. This is a tour of the graveyard.

## The 31fps Wall

We started with CEF — the Chromium Embedded Framework. The idea was simple:
render web pages off-screen, pipe the pixels into a terminal pane, done. We
built two generations around this. ts2 ran CEF in-process with WezTerm. ts3
moved CEF out-of-process and connected it over XPC.

Both hit the same wall. CEF's off-screen rendering caps at 31fps on macOS. We
ran 26 experiments across Issues 325–350. We tried every configuration. We
profiled the compositor. We hacked the frame callback. Nothing broke through.
31fps. That is the ceiling. It is a hard limit in CEF's off-screen pipeline and
no amount of tuning gets past it.

26 experiments. Same answer every time. CEF cannot do this.

## The 2fps Catastrophe

So we dropped CEF and went straight to the Chromium Content API. No wrapper, no
framework — raw Chromium internals. ts4 was the proof of concept. One browser
profile, one process, 60fps. It worked.

Then we added a second profile.

2fps. Both panes. We thought it was a bug. Issues 407–421, experiment after
experiment. We applied Electron's throttling patches — all 147 of them. Could
not even build: they require the entire Electron dependency tree, Node.js
included. We applied just the three throttling patches. They compiled. They had
zero effect. The code paths they target — `Hide()`, `WasOccluded()` — are never
called in our layout.

Issue 621 finally isolated it. The bottleneck is JavaScript execution on the
Blink main thread. Two `BrowserContext` instances sharing one process contend on
the JS scheduler. CSS animations? 60fps. `requestAnimationFrame`? 2fps. Even a
trivial 30-line rAF loop triggers it.

This is not a bug. It is an architectural constraint of Chromium. Two profiles in
one process will never render JavaScript at 60fps simultaneously. The only fix is
one process per profile.

## The Swift Memory Layout Crash

Before Rust, we tried Swift. Swift's class memory model does not produce
C-compatible struct layouts. CEF validates `base.size` from the raw struct
pointer on every callback. Swift structs do not match.

`[FATAL] CefApp_0_CToCpp called with invalid version -1`

One line. Fatal. No workaround. Rust's `#[repr(C)]` guarantees C layout. We
rewrote the bindings in Rust the same week.

## The Coordinate Math Gauntlet

Every time we position a browser overlay in a split pane, we get the math wrong
on the first try. Issue 727 — placing a second webview — took seven experiments.

Experiment 2: doubled the y-offset by mixing window-level and pane-level
coordinates. Experiment 3: overlay rendered beyond the visible window because
`contentsScale` defaulted to 1.0 on Retina (should be 2.0). Experiment 5: fixed
the scale but forgot the URL bar offset. Experiment 6: off by half a cell
height because pane borders were not accounted for.

Seven tries to place a rectangle at the right coordinates. The formula is not
complicated — it is `origin + border + pane_offset + cell_offset`, divided by
the scale factor. But every term in that formula comes from a different system
(the window, the split tree, the TUI protocol, the GPU layer) and they all use
different coordinate spaces.

Issue 749 was the same story. Browser overlays flashed on the wrong side of a
split pane because the CALayerHost was created before the render pass knew where
to put it. Two experiments. First one made it worse. Second one deferred
creation to the render pass, where the coordinates are already correct.

## The Fix You Write Twice

Issue 639: `target="_blank"` links silently fail because Chromium creates an
orphaned window. We overrode `IsWebContentsCreationOverridden`, posted a
deferred navigation, done. Three methods in `shell.cc`. Worked perfectly.

Issue 708: refactored the Chromium fork. Renamed directories. The Issue 639
commits were not carried forward. The fix vanished.

Issue 750: re-applied the same three methods to the new file paths. Same code,
same fix, six months later. The lesson is not about `target="_blank"`. The
lesson is that refactoring can silently erase working code, and if you do not
have a test that catches it, you will not notice until a user clicks a link and
nothing happens.

## What the Graveyard Taught Us

The failures are not random. They cluster.

**Performance walls are not bugs.** CEF's 31fps. Chromium's 2fps with
multi-profile JS. These are architectural constraints baked into the engines. You
cannot tune your way past them. You have to change the architecture. We changed
it three times.

**Coordinate math fails because the coordinates come from everywhere.** The
window manager, the split tree, the TUI, the GPU compositor — they all have
opinions about where things are, and they measure in different units. Every
overlay positioning feature requires multiple experiments because the formula
touches four systems that do not share a coordinate space.

**Cross-process is hard.** IPC ordering, re-entrancy, type mismatches across
language boundaries. A `u32` where macOS expects a `u64` crashes the scroll
handler. A RefCell borrow held during an event dispatch panics when CEF's
message loop re-enters. These are not deep problems — they are papercuts that
add up.

**Refactoring erases fixes.** If the test suite does not cover a behavior, a
rename can delete it. We lost `target="_blank"` handling for weeks.

## What the Failures Built

Every dead end pointed somewhere. CEF's framerate ceiling pushed us to Chromium's
Content API. Chromium's multi-profile contention forced one-process-per-profile.
Swift's struct layouts sent us to Rust. XPC's complexity led to Unix sockets and
protobuf. Ghostboard's macOS-only limitation led to Wezboard, where WezTerm
already runs on Windows and Rust's ecosystem gives us ratatui and edtui off the
shelf.

TermSurf today is a protocol. 30+ message types over Unix sockets. Any terminal,
any browser engine, any TUI. One process per profile. CALayerHost for zero-copy
GPU compositing. The architecture is clean because we tried every wrong
architecture first.

250+ issues. The graveyard is large. But every headstone has an arrow on it, and
they all point forward.
