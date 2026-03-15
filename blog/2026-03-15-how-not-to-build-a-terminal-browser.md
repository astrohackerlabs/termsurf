+++
title = "How Not to Build a Terminal Browser"
author = "Max Commits"
date = "2026-03-15"
+++

You want to jack the web into your terminal. Full Chromium. GPU-rendered. No
alt-tab, no context switch. Type `web localhost:3000` and the page is _there_,
right next to your shell. Sounds simple. It is not simple. We have 250+ issue
documents that prove it.

Many of them end with **Result: Fail**.

This is a field report from the wreckage.

## CEF: Close, But Not Close Enough

We wired up CEF — the Chromium Embedded Framework. Render pages off-screen, pipe
the pixels into a terminal pane, ship it. Two generations ran on this circuit.
ts2 embedded CEF in-process. ts3 split it out over XPC.

We pushed it to ~50fps. But the CPU was screaming. 26 experiments across Issues
325–350. We hacked the frame callback. Profiled the compositor. Overrode the
paint scheduler. Could we have gotten CEF to 60fps? Maybe. But every experiment
was a fight against an API that was not designed for this.

The real question was not "can we make CEF work?" It was "why are we fighting
CEF's API when we could fork Chromium directly?" A fork gives total control. No
API limitations. No off-screen rendering pipeline. Direct access to the
compositor, the GPU layers, the process model. Everything.

We dropped CEF and forked Chromium.

## Two Profiles, Two Frames Per Second

Dropped down to raw Chromium internals. The Content API. No wrapper, no
framework — bare metal. ts4 proved the concept. One profile, one process, 60fps.
Clean signal.

Then we spawned a second profile in the same process.

2fps. Both panes. The whole system seized.

We thought it was a bug. Fourteen issues. We intercepted Electron's throttling
patches — all 147. Could not build: they depend on the entire Electron tree,
Node.js and 85 sub-patches included. Applied just the three bypass patches. They
compiled. Zero effect. The code paths they target — `Hide()`, `WasOccluded()` —
never fire in our layout. Dead wires.

Issue 621 cracked it open. The contention is on Blink's main thread. Two
`BrowserContext` instances in one process fight over the JavaScript scheduler.
CSS animations? 60fps. A single `requestAnimationFrame` loop? 2fps. A trivial
30-line rAF loop is enough to trigger it.

Not a bug. A constraint. Baked into Chromium's architecture. Two JS runtimes in
one process will never hit 60fps simultaneously.

The only fix: one process per profile. Fork the process. Isolate the scheduler.
That is the law now.

## Four Systems, Four Coordinate Spaces

Every browser overlay needs pixel coordinates. The window manager has coordinates.
The split tree has coordinates. The TUI protocol has coordinates. The GPU
compositor has coordinates. They all disagree.

Issue 727 — placing a second webview — took seven experiments. Experiment 2
doubled the y-offset by mixing window-level and pane-level signals. Experiment 3
rendered the overlay beyond the visible window because `contentsScale` defaulted
to 1.0 on Retina — should be 2.0, every pixel doubled. Experiment 5 fixed the
scale but forgot the URL bar offset. Experiment 6: off by half a cell height.
Border widths.

Seven attempts to place a rectangle. The formula is four terms from four systems
that do not share a coordinate space. Every positioning feature starts wrong.

Issue 749: overlays flashed on the wrong side of a split pane. The CALayerHost
was forged before the render pass knew where to place it. First fix made it
worse — the overlay appeared at 0,0. Second fix deferred creation to the render
pass, where the coordinates already exist. Born in place. No flash.

## The Patch That Fell Through the Wire

Issue 639: `target="_blank"` links open nothing. Chromium spawns an orphaned
window that no one can see. We overrode `IsWebContentsCreationOverridden`,
intercepted the creation, posted a deferred navigation back to the source tab.
Three methods in `shell.cc`. Signal restored.

Issue 708: refactored the Chromium fork. Renamed directories. The Issue 639
commits did not carry forward. The override vanished. Links broke again.
Silently. No crash, no error — just nothing happening when you click.

Issue 750: re-applied the same three methods to the new paths. Same patch, weeks
later. If the test suite does not cover a behavior, a refactor can erase it and
you will not know until someone clicks a link and the wire is dead.

## The Map Inside the Graveyard

The failures are not random. They cluster. And the clusters draw a map.

**Sometimes the right move is to go deeper.** CEF could probably hit 60fps with
enough effort. But every hour fighting CEF's API was an hour not spent building
with total control. Chromium's 2fps multi-profile contention was a harder wall —
a constraint forged into the engine. You cannot tune past it. You have to rewire
the architecture.

**Coordinates fail because four systems refuse to speak the same language.** The
window, the split tree, the TUI, the compositor — each measures in its own
units. Every overlay feature requires multiple experiments because the formula
crosses four borders.

**Cross-process is a minefield.** IPC ordering. Re-entrancy. A `u32` where macOS
expects a `u64` crashes the scroll handler at runtime. A RefCell borrow held
during an event dispatch panics when CEF's message loop re-enters. These are not
deep problems. They are a thousand small wires that must all connect.

**Refactoring severs working circuits.** Rename a directory, lose a fix. No test,
no signal. The patch falls through.

## What the Wreckage Built

Every dead circuit pointed somewhere.

CEF's API limitations pushed us to fork Chromium directly. Chromium's scheduler
contention forced one-process-per-profile. We used XPC for IPC early on because
we thought we needed it for shared GPU textures — but CALayerHost replaced that
approach. CALayerHost just needs a layer ID, which is an integer. You can send an
integer over any socket. XPC is macOS-only. Unix domain sockets work everywhere.
We dropped XPC the same week we shipped CALayerHost.
Ghostboard ran on Ghostty — Linux and macOS only. We forked WezTerm instead:
Windows support on day one, and Rust's ecosystem gives us ratatui and edtui off
the shelf.

TermSurf today is a protocol. 30+ message types over Unix domain sockets. Any
terminal. Any browser engine. Any TUI. One process per profile. CALayerHost for
zero-copy GPU compositing. The architecture is clean because we tried every wrong
architecture first.

250+ issues. The graveyard is large. But every headstone has an arrow on it, and
they all point forward.
