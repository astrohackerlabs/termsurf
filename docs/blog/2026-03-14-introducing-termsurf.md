+++
title = "Introducing TermSurf"
author = "Ryan X. Charles"
date = "2026-03-14"
+++

Developers live in terminals. We write code, run builds, manage servers, tail
logs, and navigate filesystems without ever reaching for a mouse. The terminal
is fast, composable, and keyboard-driven. It is the natural habitat of focused
work.

But then we need a browser.

Maybe it is `localhost:3000` to check a UI change. Maybe it is documentation, a
dashboard, or a pull request. Whatever it is, we alt-tab out of the terminal,
lose our place, and break the flow we spent minutes building. The browser is a
separate world with its own window manager, its own keybindings, and its own
attention demands.

TermSurf eliminates that context switch.

## What TermSurf is

TermSurf is a protocol for embedding web browsers inside terminal emulators.
Type `web localhost:3000` and the page renders right there, in your terminal
pane, alongside your code and your shell. No new window. No alt-tab. No
context switch.

It is not a text-mode browser. It is a real browser engine — full CSS, full
JavaScript, full GPU rendering — displayed as an overlay inside the terminal
window. You get the same rendering you would see in Chrome or Safari, but
inside the tool you already live in.

## How it works

TermSurf is a network of interchangeable components — terminals, browser
engines, and TUIs — all speaking the same protobuf protocol over Unix sockets.

```
┌─────────┐  ┌─────────┐  ┌─────────┐
│  TUI 1  │  │  TUI 2  │  │  TUI N  │    N TUIs (e.g., `web`)
└────┬────┘  └────┬────┘  └────┬────┘
     │            │            │
     └────────────┼────────────┘
                  │  Unix socket
           ┌──────┴──────┐
           │     GUI     │                1 GUI (terminal emulator)
           │ (Wezboard)  │
           └──┬───┬───┬──┘
              │   │   │
              │   │   │  Unix sockets
              │   │   │
     ┌────────┘   │   └──────┐
     │            │          │
┌────┴────┐ ┌─────┴───┐ ┌────┴────┐
│ Roamium │ │ Surfari │ │ Roamium │    M engines (one per profile)
│ profile │ │ profile │ │ profile │
│   "A"   │ │   "B"   │ │   "C"   │
└─────────┘ └─────────┘ └─────────┘
```

There are three kinds of components:

**The GUI** is a terminal emulator that implements the TermSurf protocol. It
listens on a Unix socket, accepts connections from TUIs and browser engines,
and renders browser content as overlays at pixel coordinates. The current GUI
is Wezboard, a fork of WezTerm.

**The TUI** is a terminal application that provides browser chrome — a URL bar,
navigation, modes — inside a terminal pane. The current TUI is `web`. It
connects to the GUI over the socket and sends protocol messages to control the
browser.

**The browser engine** runs as a separate process, one per profile. Each engine
process connects to the GUI and renders web content into a GPU surface that the
GUI composites into the terminal window. On macOS, this uses CALayerHost for
zero-copy rendering — the browser's GPU output appears directly in the terminal
window without any pixel copying.

The protocol (`termsurf.proto`) defines 30+ message types covering tab
lifecycle, navigation, input forwarding, GPU compositing, state
synchronization, and request/reply pairs. All messages are length-prefixed
protobuf over Unix domain sockets.

## Not locked to one browser

Every browser engine runs as a separate process speaking the same protocol.
This means TermSurf is not tied to any single engine:

| Engine   | Binary   | Status     |
| -------- | -------- | ---------- |
| Chromium | Roamium  | Working    |
| WebKit   | Surfari  | Planned    |
| Gecko    | Waterwolf| Planned    |
| Ladybird | Girlbat  | Planned    |

Each engine follows the same pattern: a C shared library wrapping the engine's
embedding API, linked by a Rust binary that handles Unix socket IPC, protobuf
parsing, and process lifecycle. The Rust binary is roughly 400 lines and is
almost entirely reusable across engines.

You could have one pane running Roamium (Chromium), another running Surfari
(WebKit), and a third running Girlbat (Ladybird) — all in the same terminal
window, all speaking the same protobuf messages.

## Not locked to one terminal

Any terminal emulator that implements the TermSurf protocol can host browser
overlays. The current GUI is Wezboard, a WezTerm fork. But the protocol is
designed so that forks of Ghostty, Kitty, Alacritty, and iTerm2 could all
serve as TermSurf GUIs.

The protocol is the product. Individual apps are implementations.

## What works today

Wezboard and Roamium are functional on macOS. Here is what you can do right
now:

- Open any URL in a terminal pane with `web <url>`.
- Split panes — the browser overlay repositions and resizes immediately.
- Switch tabs — overlays hide and restore correctly.
- Open DevTools for any browser pane.
- Navigate with keyboard-driven modes (vim-style).
- Use multiple browser profiles with isolated cookies and storage.
- GPU-accelerated rendering with no pixel copying (CALayerHost compositing).

The experience is surprisingly natural. You split a terminal, type
`web localhost:3000`, and your app appears next to your editor. You resize the
split and the browser follows. You switch to another tab to run tests, and when
you switch back, the browser is exactly where you left it.

## What is next

TermSurf is early. The protocol works, the rendering works, and the core
workflow is solid. But there is a long list ahead:

- WebKit and Gecko engine integrations.
- Linux and Windows support.
- Forks of more terminal emulators.
- Bookmarks, history, downloads, and other standard browser features.
- A richer TUI with tab bar, status line, and search.

The most important work is on the protocol itself. Every feature starts as a
protobuf message. The protocol is versioned, extensible, and designed to grow.

## Get involved

TermSurf is open source. The code, the protocol, the issues, and the
experiments are all public.

- [GitHub](https://github.com/termsurf/termsurf)
- [Website](https://termsurf.com)

If you have ever wished you could see a web page without leaving your terminal,
TermSurf is for you.
