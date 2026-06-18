# Experiment 7: Prove Copy-Current-URL

## Description

Issue 816 still needs Ghostboard-specific proof for copy-current-URL behavior.
The historical Issue 334 requirement was: Cmd+C in browser control mode copies
the current webview URL to the system clipboard and briefly shows user feedback,
while Browse mode still lets browser content handle Cmd+C.

Current webtui has URL editor clipboard support through `UrlClipboard`, but the
audit did not prove a dedicated copy-current-URL workflow under Ghostboard. This
experiment will first prove the current behavior, then implement the smallest
owner-specific fix if the behavior is missing.

## Changes

Planned investigation:

- Inspect the current copy/clipboard paths in:
  - `webtui/src/main.rs`;
  - `docs/keybindings.md`;
  - Ghostboard AppKit/menu key handling for Cmd+C;
  - Issue 334 historical copy-URL behavior;
  - Issue 810 clipboard/copy-current-URL findings.
- Determine whether Cmd+C in Control mode reaches webtui as a terminal key
  event, is handled by Ghostboard/Ghostty menu copy behavior, or is intercepted
  before either path can copy the URL.
- Identify whether the right owner is webtui command/key handling, Ghostboard
  macOS menu/key assignment handling, or both.

Planned harness changes:

- Add a `copy-current-url-smoke` scenario to
  `scripts/ghostboard-geometry-matrix.sh`.
- Serve or navigate to a local fixture URL with a unique query marker.
- Clear the macOS clipboard to a sentinel value before each copy attempt.
- Drive the app into webtui Control mode and trigger the copy-current-URL
  workflow.
- Read the macOS clipboard with `pbpaste` and assert it equals the current URL.
- Capture webtui state trace, app log, Roamium trace, and clipboard before/after
  evidence.

Planned behavior targets:

- Primary target: Cmd+C in webtui Control mode copies the current URL to the
  system clipboard.
- Secondary target: the TUI records a test-only copy-current-URL trace event and
  exposes brief feedback evidence so the harness can distinguish a deliberate
  URL copy from an accidental terminal selection copy.
- Browse mode must not be hijacked by the copy-current-URL behavior. If tested
  in this experiment, Cmd+C in Browse mode should remain browser-owned.

Planned fix policy:

- If Cmd+C reaches webtui in Control mode, implement the copy-current-URL
  behavior in webtui and update keybinding documentation.
- If Cmd+C is intercepted by Ghostboard/Ghostty menu handling before webtui,
  implement the narrow Ghostboard-owned menu/key-assignment path that copies the
  active webtui URL only when the browser pane is in Control mode. Do not change
  ordinary terminal selection copy.
- If a reliable Cmd+C path cannot be implemented without broader app keybinding
  work, add a webtui command such as `:copy-url` only as a diagnostic fallback
  and record Cmd+C as unresolved. Do not count the fallback command as full pass
  for the historical requirement.

Planned issue-doc changes:

- Record the current behavior, owner, implemented path, clipboard evidence, and
  whether Browse mode remains browser-owned.
- If a keybinding is added or changed, update `docs/keybindings.md`.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0816-ghostboard-browser-state-interruptions/README.md issues/0816-ghostboard-browser-state-interruptions/07-prove-copy-current-url.md`.
2. If Rust files change, `cargo fmt -- <changed-rust-files>`.
3. If Zig files change, `zig fmt <changed-zig-files>`.
4. If keybindings change, update `docs/keybindings.md` and run prettier on it.

Static/build checks:

1. `prettier --check --prose-wrap always --print-width 80 issues/0816-ghostboard-browser-state-interruptions/README.md issues/0816-ghostboard-browser-state-interruptions/07-prove-copy-current-url.md`.
2. `bash -n scripts/ghostboard-geometry-matrix.sh`.
3. `cargo check -p webtui` if webtui changes.
4. `cargo build -p webtui` if webtui changes.
5. If Ghostboard Zig or non-`macos/` Ghostboard files change, run
   `cd ghostboard && zig build -Demit-macos-app=false`.
6. If Ghostboard app files change or a Ghostboard rebuild is needed, run
   `cd ghostboard && macos/build.nu --configuration Debug --action build`.
7. `shellcheck scripts/ghostboard-geometry-matrix.sh` if available.
8. `git diff --check`.

Design gate:

- This experiment file is plan-only until a fresh-context design review approves
  it.
- Record design review findings and fixes in this file.
- Commit the approved experiment plan before implementation begins.

Completion gate:

- After implementation and verification, record `## Result` and `## Conclusion`
  in this file.
- Update the Issue 816 README experiment status from `Designed` to the final
  result.
- Request a fresh-context completion review, fix all real findings, and record
  the final completion-review verdict in this file.
- Commit the reviewed experiment result separately before designing or
  implementing the next experiment.

Runtime checks:

1. `scripts/ghostboard-geometry-matrix.sh copy-current-url-smoke`.
2. Confirm the local fixture URL with unique query marker is loaded.
3. Confirm the clipboard is set to a sentinel value before copying.
4. Confirm Control-mode Cmd+C triggers copy-current-URL behavior.
5. Confirm the clipboard equals the current URL after copying, including the
   query marker.
6. Confirm trace/feedback evidence identifies the URL-copy action.
7. Confirm ordinary Browse-mode browser copy behavior is not broken, or record
   Browse-mode coverage as a required follow-up if it is too broad for this
   experiment.

Pass criteria:

- Cmd+C in Control mode copies the current URL under debug Ghostboard.
- Clipboard evidence exactly matches the current URL and cannot pass from a
  stale sentinel or terminal selection.
- The harness includes durable assertions for key/menu path, URL-copy trace or
  feedback, and clipboard contents.
- Any keybinding or owner change is documented.

Partial criteria:

- A fallback command can copy the URL, but Cmd+C remains intercepted or
  unresolved.
- The owner is proven, but the correct fix requires broader Ghostboard menu or
  keybinding work.
- Control-mode copy works, but Browse-mode non-interference remains unproven.

Fail criteria:

- The harness cannot distinguish current URL copy from terminal selection copy.
- The clipboard cannot be read or written reliably under the VM.
- The implementation counts a diagnostic fallback command as full historical
  Cmd+C parity.

## Design Review

Fresh-context adversarial design review by Codex subagent `Plato`:

- **Verdict:** Approved.
- **Findings:** None.
- **Reviewer notes:** The reviewer confirmed the README links Experiment 7 as
  `Designed`, required sections are present, scope matches the Issue 816
  copy-current-URL gap, Cmd+C menu interception is explicitly handled,
  verification includes clipboard sentinel, exact URL, trace/feedback, and
  fallback-not-full-pass criteria, and hygiene plus design/completion gates and
  keybinding documentation requirements are present.

## Result

**Result:** Pass

Implemented a Ghostboard-owned copy-current-URL path with a webtui fallback:

- `webtui/src/main.rs` now handles Cmd+C in Control mode if the key reaches the
  TUI, writes the current URL to the clipboard, emits a test-only
  `copy_current_url` state trace, and shows short `url copied` feedback.
- `docs/keybindings.md` documents Cmd+C in webtui Control mode as copy current
  URL.
- `ghostboard/src/apprt/termsurf.zig` now exposes a focused-pane
  `copyCurrentUrl` query that returns the stored current URL only when the
  TermSurf pane is focused, not in Browse mode, and has a URL.
- `ghostboard/src/main_c.zig` and `ghostboard/include/ghostty.h` expose that
  query to the macOS app as `termsurf_copy_current_url`.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  copies the TermSurf URL to `NSPasteboard.general` from:
  - the Cmd+C key-equivalent path, before Ghostty's normal copy binding, only
    when there is no terminal selection; and
  - the normal menu copy action as a fallback after `copy_to_clipboard` reports
    that it did not copy a terminal selection.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift` and
  `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView.swift` now expose a
  short-lived `URL copied` overlay for the proven Ghostboard-owned path.
- `scripts/ghostboard-geometry-matrix.sh` now has a `copy-current-url-smoke`
  scenario that serves a local unique URL, sets the clipboard to a sentinel,
  injects Cmd+C, verifies Ghostboard selected the current URL, verifies AppKit
  copied it in Control mode, verifies feedback, asserts `pbpaste` exactly equals
  the current fixture URL, enters Browse mode, injects Cmd+C again, and verifies
  the TermSurf copy-current-URL path stays inactive with the clipboard sentinel
  unchanged.

The first runtime attempt failed after proving Cmd+C does not invoke the AppKit
menu `copy(_:)` path in this scenario. Ghostboard's `performKeyEquivalent`
recognized Cmd+C as the normal `copy_to_clipboard` binding and sent it directly
through `keyDown`, so the initial menu-fallback implementation was never called:

- Failed run:
  `logs/ghostboard-geometry-copy-current-url-smoke-harness-20260618-000827.log`
- Evidence:
  `logs/ghostboard-geometry-copy-current-url-smoke-app-20260618-000827.log`
  records `perform_key_equivalent_binding`, `key_down`, and `copy_to_clipboard`,
  but no `CopyCurrentUrl`.

After adding the guarded key-equivalent path, the runtime smoke passed once:

- Passing run: `scripts/ghostboard-geometry-matrix.sh copy-current-url-smoke`
- Timestamp: `20260618-001035`
- Harness log:
  `logs/ghostboard-geometry-copy-current-url-smoke-harness-20260618-001035.log`
- App log:
  `logs/ghostboard-geometry-copy-current-url-smoke-app-20260618-001035.log`
- Webtui state trace:
  `logs/ghostboard-geometry-copy-current-url-smoke-webtui-20260618-001035.log`
- Roamium trace:
  `logs/ghostboard-geometry-copy-current-url-smoke-roamium-20260618-001035.log`
- Clipboard target: `http://127.0.0.1:50156/copy-20260618-001035.html`

The first completion review found two real gaps: the proven Ghostboard path did
not show user feedback, and Browse-mode non-interference was guarded in code but
not verified by the harness. Both were fixed, and the enhanced runtime smoke
passed:

- Passing run: `scripts/ghostboard-geometry-matrix.sh copy-current-url-smoke`
- Timestamp: `20260618-001624`
- Harness log:
  `logs/ghostboard-geometry-copy-current-url-smoke-harness-20260618-001624.log`
- App log:
  `logs/ghostboard-geometry-copy-current-url-smoke-app-20260618-001624.log`
- Webtui state trace:
  `logs/ghostboard-geometry-copy-current-url-smoke-webtui-20260618-001624.log`
- Roamium trace:
  `logs/ghostboard-geometry-copy-current-url-smoke-roamium-20260618-001624.log`
- Clipboard target: `http://127.0.0.1:50307/copy-20260618-001624.html`
- Additional assertions:
  - AppKit emitted `copy_current_url_feedback` with `message=URL copied`.
  - Browse-mode Cmd+C did not emit `CopyCurrentUrl`.
  - Browse-mode Cmd+C did not emit AppKit `copy_current_url`.
  - Browse-mode Cmd+C left the clipboard sentinel unchanged.

Verification run:

1. `zig fmt ghostboard/src/apprt/termsurf.zig ghostboard/src/main_c.zig`.
2. `cargo fmt -- webtui/src/main.rs`.
3. `prettier --write --prose-wrap always --print-width 80 docs/keybindings.md issues/0816-ghostboard-browser-state-interruptions/README.md issues/0816-ghostboard-browser-state-interruptions/07-prove-copy-current-url.md`.
4. `bash -n scripts/ghostboard-geometry-matrix.sh`.
5. `cargo check -p webtui`.
6. `cargo build -p webtui`.
7. `cd ghostboard && zig build -Demit-macos-app=false`.
8. `git diff --check`.
9. `cd ghostboard && macos/build.nu --configuration Debug --action build`.
10. `scripts/ghostboard-geometry-matrix.sh copy-current-url-smoke`.
11. After completion-review fixes, repeated
    `bash -n scripts/ghostboard-geometry-matrix.sh`.
12. After completion-review fixes, repeated `git diff --check`.
13. After completion-review fixes, repeated `cargo check -p webtui`.
14. After completion-review fixes, repeated
    `cd ghostboard && zig build -Demit-macos-app=false`.
15. After completion-review fixes, repeated
    `cd ghostboard && macos/build.nu --configuration Debug --action build`.
16. After completion-review fixes, repeated
    `scripts/ghostboard-geometry-matrix.sh copy-current-url-smoke`.

The Xcode build succeeded with existing project warnings about umbrella headers,
Swift sendability, a Swift 6 actor warning, and dSYM symbols. Those warnings did
not block the build or the runtime proof.

## Conclusion

Cmd+C copy-current-URL parity is now implemented and covered by a durable smoke
scenario. The ownership finding is important: in Ghostboard, Cmd+C can be
consumed by the AppKit key-equivalent/Ghostty binding path before webtui sees
the key, so the durable fix must live in Ghostboard with TermSurf state as the
source of truth. The webtui handler remains useful for contexts where Cmd+C does
reach the TUI, but the Ghostboard path is the proven runtime path.

Browse-mode non-interference is guarded by `pane.browsing`: the TermSurf URL
query returns nothing while the pane is in Browse mode. The enhanced runtime
test verifies that guard by entering Browse mode, injecting Cmd+C, and proving
the TermSurf URL-copy path does not run or overwrite the clipboard sentinel.

## Completion Review

Fresh-context completion review by Codex subagent `Ohm`:

- **Initial verdict:** Changes requested.
- **Finding 1:** The proven Ghostboard-owned Cmd+C path did not exercise the
  webtui `url copied` feedback, so the historical feedback requirement and
  `docs/keybindings.md` were not satisfied.
- **Fix 1:** Added `termsurfCopyUrlFeedback` state to the AppKit surface,
  rendered a short-lived `URL copied` overlay in the SwiftUI surface, and added
  a `copy_current_url_feedback` geometry trace that the harness asserts.
- **Finding 2:** Browse-mode non-interference was guarded by code but not
  durably verified, which matched the experiment's own Partial criteria.
- **Fix 2:** Extended `copy-current-url-smoke` to enter Browse mode, set a fresh
  clipboard sentinel, inject Cmd+C, assert no Zig/AppKit copy-current-URL logs,
  and assert the clipboard sentinel remains unchanged.
- **Finding 3:** `docs/keybindings.md` claimed feedback that was not true for
  the proven Ghostboard path.
- **Fix 3:** The Ghostboard path now shows the documented `URL copied` feedback.
- **Final verdict:** Approved.
- **Final reviewer notes:** The reviewer confirmed the previous findings were
  addressed: the harness now proves Ghostboard Cmd+C emits feedback, copies the
  exact current URL, and Browse-mode Cmd+C does not run the TermSurf URL-copy
  path or overwrite the clipboard sentinel. The reviewer also confirmed normal
  terminal-copy preservation remains guarded by the existing selection-first
  copy path and no-selection key-equivalent check.
