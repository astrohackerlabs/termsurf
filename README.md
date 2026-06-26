# TermSurf

TermSurf embeds a real web browser inside a terminal emulator. Run `web`, open a
URL, and the page appears in a terminal pane alongside shells, editors, and
other terminal workflows.

TermSurf is also a protocol: TUIs, terminal frontends, and browser engine
processes communicate over Unix sockets with protobuf messages. The current
client includes:

- `ghostboard/` — the primary terminal frontend, based on Ghostty.
- `webtui/` — the `web` TUI and browser controls.
- `roamium/` — the Chromium-backed browser engine process.
- `surfari/` — the WebKit-backed browser engine process.
- `proto/` — the TermSurf wire protocol.
- `chromium/` and `webkit/` — patch archives and workspace instructions for the
  engine integrations.

This public repository contains the open source client code needed to build the
terminal/browser experience. TermSurf Cloud, private planning, internal issue
records, and release orchestration live outside this repository.

## Install

The Homebrew cask currently targets Apple silicon macOS and installs
`TermSurf.app`, the `web` CLI, Roamium with Chromium runtime resources, and
Surfari with WebKit runtime resources:

```bash
brew tap termsurf/termsurf
brew trust termsurf/termsurf
brew install --cask termsurf
```

To upgrade:

```bash
brew update && brew upgrade --cask termsurf
```

## Build

Development builds require Xcode, Zig, Rust, Chromium's `depot_tools`, and the
WebKit build tooling described in `webkit/README.md`. Chromium and WebKit are
large; plan for significant disk space and long first builds.

```bash
brew install zig
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
git clone https://chromium.googlesource.com/chromium/tools/depot_tools.git \
  chromium/depot_tools
```

Fetch the Chromium and WebKit checkouts described by `chromium/README.md` and
`webkit/README.md`, apply the current patch archives, then build the client
components:

```bash
./scripts/build.sh chromium
./scripts/build.sh roamium
./scripts/build.sh webkit
./scripts/build.sh surfari
./scripts/build.sh webtui
./scripts/build.sh ghostboard
```

For a release-style local build:

```bash
./scripts/build.sh all --release
```

The Ghostboard app bundle is written to:

```text
ghostboard/macos/build/Release/TermSurf.app
```

## Run

During development, launch Ghostboard from the source tree:

```bash
cd ghostboard
zig build run
```

Inside that terminal, run the debug `web` binary and point it at the locally
built Roamium engine:

```bash
../target/debug/web \
  --browser ../chromium/src/out/Default/roamium \
  https://example.com
```

## License

See `LICENSE`, `NOTICE`, and `TRADEMARKS.md`.
