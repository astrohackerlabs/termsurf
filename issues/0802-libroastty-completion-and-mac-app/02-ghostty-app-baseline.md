+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.result]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"
+++

# Experiment 2: Baseline & feasibility — build, run, and automate the real Ghostty app

## Description

Before porting anything, de-risk the entire conformance strategy by proving — on
the **real, unmodified** Ghostty macOS app (vendored at `vendor/ghostty/macos`,
version **1.3.2-dev**) — that in _this_ environment we can:

1. **build** the app from source,
2. **run** it (signing/permissions),
3. **automate** it programmatically (drive input + capture screenshots), and
4. capture a **golden baseline** (reference screenshots + behavior) that the
   `roastty`-backed app will later be diff-tested against.

The whole Issue-802 plan is a bet that the app can be built, run, and
UI-automated; this experiment settles that bet cheaply against a known-good
binary, and produces a **reusable build + run + automate harness** that Phase D
(UI tests for the roastty app) inherits. A negative result here is just as
valuable — it tells us to adjust the approach (or what permission you must
grant) before sinking work into the port.

This experiment changes **no roastty source.** It builds vendored Ghostty and
produces a harness + baseline artifacts + a documented findings record.

## Environment (already confirmed)

- App project present: `vendor/ghostty/macos/Ghostty.xcodeproj` (+ entitlements,
  `Ghostty-Info.plist`, `Ghostty.sdef` AppleScript dictionary,
  `Ghostty.xctestplan`).
- Toolchain present: `zig` (0.16.0 installed), `xcodebuild`, `osascript`,
  `screencapture`, Xcode at `/Applications/Xcode.app`.
- **Real GUI session** (not headless SSH): `SSH_TTY` unset,
  `TERM_PROGRAM=Wezboard` — so there is a window server to drive.

## Known risks to resolve (the point of the spike)

- **Zig version.** `minimum_zig_version = 0.15.2` is a **floor, not a pin**, and
  the installed zig is `0.16.0`. Zig has breaking changes between minors, so
  0.16.0 may not build a 0.15.x-era dev tag — but since 0.15.2 is only a
  minimum, the spike should **determine and record the exact zig that builds
  1.3.2-dev**: try the installed 0.16.0 once, and if it fails install/select a
  working 0.15.x (pinned download / `zvm` / `asdf`) and pin the precise version.
  **The required zig version is a hard input — do not "upgrade" ghostty to fit a
  newer zig.**
- **Network / dependency fetch.** `build.zig.zon` pulls many dependencies from
  `deps.files.ghostty.org` (some non-lazy), so the first `zig build` needs
  network egress to populate the zig cache. A fetch failure must be triaged as a
  _network_ blocker, not a toolchain one.
- **Build flow.** Per `vendor/ghostty/macos/AGENTS.md`: do **not** `zig build`
  the app directly — run `zig build -Demit-macos-app=false` to produce
  `GhosttyKit.xcframework` (which the Xcode project consumes; the project runs
  no zig build phase), then `macos/build.nu` to build the `.app`. Follow that
  doc as the authoritative build guide. (`nu` 0.113.0 is already installed.)
- **Automation permissions.** Driving + screenshotting a GUI app from the
  agent's shell inherits the controlling terminal's (Wezboard's) TCC grants. It
  will likely require a **one-time manual grant** of **Accessibility** and
  **Screen Recording** (and possibly Automation/AppleEvents) to Wezboard in
  System Settings. If so, document the exact grant needed as the remediation —
  that is a successful finding, not a failure.
- **Signing.** Build a **local/debug** configuration
  (`GhosttyDebug.entitlements` / `GhosttyReleaseLocal.entitlements`) —
  ad-hoc/local signing, no distribution cert.
- **Build duration.** A clean zig + Xcode build may exceed the 15-min
  bounded-run cap. Builds run as **tracked background tasks** (Central-time
  stamped) with a generous timeout, since they are one-off builds, not flaky
  test loops; only the short automation steps use the bounded runner if at all.

## Changes / Deliverables

No roastty code changes. The experiment produces:

- **A reusable harness** under `scripts/ghostty-app/` (or similar), with small,
  documented steps:
  - `build.sh` — select zig 0.15.x, build GhosttyKit
    (`zig build -Demit-macos-app=false`), then `macos/build.nu`; emits the
    `.app` path.
  - `run.sh` — launch the built `.app` (and quit it cleanly).
  - `automate.sh` — drive the app (send keystrokes via `osascript`/AppleEvents
    or Accessibility; type a deterministic command), and `screencapture` the
    window.
  - `screenshot.sh` — capture a named PNG of the app's window. (Exact tool
    choice — `osascript` System Events vs an XCUITest target vs `cliclick` — is
    decided during implementation based on what actually works; the harness
    wraps whatever does.)
- **Golden baseline artifacts** under
  `issues/0802-libroastty-completion-and-mac-app/baseline/` — a small set of
  reference PNGs: (a) a fresh window, (b) after typing a deterministic command
  (e.g. a fixed `printf` of ASCII + a color SGR line), (c) a basic Unicode/emoji
  line. Committed as the reference set (kept small).
- **A documented findings record** (this experiment's Result + a short
  `scripts/ghostty-app/README.md`): the exact zig version used, the build
  incantation that worked, the launch steps, the automation mechanism, and **the
  precise permissions that had to be granted** (with the System Settings path).

## Verification

Per the bounded-run convention for any test-suite-like steps (Central-stamped);
builds as tracked background tasks. Steps:

1. Read `vendor/ghostty/macos/AGENTS.md` (+ ghostty build docs) and follow the
   documented macOS build flow.
2. Obtain/select zig **0.15.x**; obtain `nu` if missing.
3. Build GhosttyKit + the `.app`; record the working commands.
4. Launch the `.app`; confirm it shows a working terminal (capture a
   screenshot).
5. Programmatically send a deterministic input and capture the resulting window
   — confirming automation works (or document the exact permission grant
   required).
6. Save the baseline PNGs and write the harness `README` + findings.

**Pass** = the real Ghostty 1.3.2-dev app **builds, launches, shows a working
terminal in a captured screenshot, and is driven + screenshotted
programmatically from this environment**, with the harness, baseline artifacts,
and required permissions all documented.

**Partial** = builds + launches + screenshots, but full input-automation is
blocked on a permission/tooling limitation that is documented with the exact
remediation (e.g. "grant Wezboard Accessibility + Screen Recording, then
re-run") — still a go decision for the approach.

**Fail** = cannot build or cannot run the real app (a toolchain/version blocker
that no reasonable step resolves) — a genuine finding that forces a plan change
before the port proceeds; document the blocker precisely.

**Scope caveat:** this spike proves automation in an **interactive GUI session
with TCC grants** (the agent in Wezboard). It does **not** by itself establish
_headless / CI_ automation (Issue-802 risk (c)). Phase D should therefore treat
"repeatable in this session" as the bar it inherits, and treat headless/CI runs
as a separate, later concern rather than an assumption.

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: APPROVED, no Required findings.** Independently verified
the load-bearing facts: `build.zig.zon` version `1.3.2-dev` /
`minimum_zig_version 0.15.2` vs installed zig `0.16.0`; the build flow matches
`macos/AGENTS.md` exactly (no direct `zig build` of the app;
`-Demit-macos-app=false` then `build.nu`; the xcodeproj consumes a prebuilt
`GhosttyKit.xcframework` and runs no zig phase); `nu` 0.113.0 / `osascript` /
`screencapture` / Xcode 26.4 present; GUI session confirmed; `build.nu` skips
`GhosttyUITests` "because it requires special permissions" (corroborating the
TCC caution); `vendor/ghostty` is git-ignored so the tracked harness/baseline
locations are correct. Findings adopted:

- **Optional — network/dep-fetch risk.** **Added:** the first `zig build`
  fetches many deps from `deps.files.ghostty.org`; a fetch failure is a network,
  not toolchain, blocker.
- **Optional — "0.15.x" was imprecise (floor, not pin).** **Fixed:** the spike
  now determines and records the _exact_ zig that builds 1.3.2-dev (try 0.16.0
  once, else a working 0.15.x).
- **Optional — headless/CI over-promise.** **Fixed:** added the scope caveat
  above; Phase D's wording softened from "CI-able" to repeatable-in-session.
- **Nits — `nu` already installed; `cliclick` absent (harness falls back to
  `osascript`/XCUITest).** Noted.

## Conclusion

_(to be written after the run)_
