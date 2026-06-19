# Ghostty app build/run/automate harness (Issue 802, Exp 2 + 3)

Tooling to build, run, and UI-automate the **real, unmodified** Ghostty macOS
app (vendored at `vendor/ghostty/macos`, commit `2c62d18`, v1.3.2-dev) as the
libroastty conformance oracle. Full findings:
`issues/0802-…/02-ghostty-app-baseline.md` (the toolchain investigation) and
`…/03-macos-only-build.md` (the working build).

## The toolchain situation (resolved)

- Ghostty 1.3.2-dev hard-requires **zig 0.15.2** (its `requireZig` enforces an
  exact major.minor; the system 0.16.0 fails to compile `build.zig`).
  `setup-zig.sh` pins 0.15.2 under `vendor/toolchains/` (gitignored).
- zig 0.15.2 **cannot link this machine's Xcode 26.4 SDK**
  (`__availability_version_check`) — a too-new point release — but **can** link
  the **CommandLineTools 26.0** SDK.
- The macOS app needs only the **macOS** slice of `GhosttyKit`. The full
  xcframework also builds an **iOS** slice, which needs an iOS SDK zig 0.15.2
  can't link (only iOS 26.4 is present; CLT has none).

## Build the app (approach 1 — macOS-only, no Xcode change)

```bash
scripts/ghostty-app/build-macos-app.sh [Debug|ReleaseLocal]
# -> vendor/ghostty/macos/build/<config>/Ghostty.app
```

What it does (see the script): pin zig 0.15.2 → apply
`macos-only-xcframework.patch` (gate the iOS slice on `.universal`) → build the
macOS lib + Metal shaders under `DEVELOPER_DIR=CommandLineTools` with Xcode's
`metal` on `PATH` → `xcodebuild -create-xcframework` under Xcode →
`macos/build.nu` builds the Swift app under Xcode. The **app is unaltered**; the
only change is the build-only `build.zig` patch.

## Run + lifecycle (start → drive → stop)

```bash
PID=$(scripts/ghostty-app/start-app.sh)       # launch + wait for window; prints PID
APP="$PWD/vendor/ghostty/macos/build/Debug/Ghostty.app"
osascript -e "tell application \"$APP\" to activate"
screencapture -x shot.png                     # (Screen Recording granted to the app)
scripts/ghostty-app/stop-app.sh               # MANDATORY when done — see below
```

**Always stop what you start.** `stop-app.sh` SIGKILLs the debug-build PID (and
the byte probe) — scoped to `vendor/ghostty/macos/build/…`, so it never touches
an installed Ghostty or any other app. **Do not** `osascript … to quit`:
graceful quit pops an "are you sure?" dialog that needs the user. End every
experiment with `stop-app.sh` — leave nothing dangling (see the issue README's
Process-hygiene rule).

## Window-isolated screenshot (Exp 4)

Capture **just** an app's window — independent of Space, occlusion, or which
Space the agent's terminal occupies — via `screencapture -l<id>` with the window
id resolved by `winid.swift`:

```bash
scripts/ghostty-app/screenshot.sh Ghostty [out-name]   # -> prints a PNG path (outside the repo)
scripts/ghostty-app/screenshot.sh --list Ghostty       # list candidate windows
# target may be an owner name (substring), a bundle id, or a pid
```

**Screenshots are never committed** (see the issue's Screenshots policy): output
goes to `${TERMSURF_SHOT_DIR:-$HOME/.cache/termsurf/shots}`. Verified: a Ghostty
window captured at `1600×1264 px` (= `800×632 pt` × 2 Retina) — the window crop,
not the full display — with live terminal content, while another app was
fullscreen on another Space.

## Input injection (Exp 5)

Drive the app from the agent. Keyboard via `osascript` System Events, mouse via
`inject.swift` (CGEvent), with a raw-mode PTY byte logger (`byteprobe.py`) as
the precise oracle. `input-matrix.sh <stage>` drives grouped tests.

```bash
swift scripts/ghostty-app/inject.swift move|click|drag|scroll  ...   # CGEvent mouse
python3 scripts/ghostty-app/byteprobe.py /tmp/ghostty-exp5/bytes.log [mouse,focus,paste]
scripts/ghostty-app/input-matrix.sh bootstrap|probe-start|keyA-...|...
```

**Operational facts (learned in Exp 5 — reuse these):**

- **activate-first**: `osascript … to activate` the target before injecting —
  keyboard reaches only the frontmost app, mouse only the active Space. Oracles
  (window-id screenshot, byte-log file, `pbpaste`) survive the Space switch.
- **Warmup key**: the first keystroke after each `activate` is dropped (focus
  settling); send a throwaway key first.
- **Byte log**: never truncate it while `byteprobe.py` holds it open (writes
  land at the old fd offset → a hole of `00` bytes); read by line offset. The
  probe runs raw with ISIG off so Ctrl-C/D/Z arrive as bytes.
- **Bootstrap to bash**: Ghostty's default shell is nushell — inject
  `exec bash --norc --noprofile` first so POSIX driver commands work.
- **Known input failures** (see Exp 5): F11 (macOS-swallowed), Ctrl-K/Ctrl-L
  (app-consumed), dead-key compose, synthetic double-click word-select. Scroll +
  SGR mouse reporting work.
