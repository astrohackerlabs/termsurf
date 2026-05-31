# Experiment 5: Add Protocol-Mediated JavaScript Dialogs

## Description

Experiment 1 classified JavaScript dialogs as `Automatable after setup`.
Experiment 4 completed generic downloads, so the next high-impact browser API
gap is `alert`, `confirm`, `prompt`, and beforeunload confirmation.

The current Chromium code path falls back to Content Shell's
`ShellJavaScriptDialogManager`. On macOS that opens a native `NSAlert` from the
Roamium process. That is wrong for TermSurf:

- it is not automatable without native UI permissions;
- it bypasses the terminal/TUI interaction model;
- it can appear in the wrong process/window context;
- it does not work for non-macOS future TermSurf GUIs;
- it gives TermSurf no protocol evidence that a page is blocked on a dialog.

TermSurf should own this as a protocol request/reply flow:

1. Chromium receives the JavaScript dialog request.
2. Roamium sends a `JavaScriptDialogRequest` protobuf to connected clients.
3. The TUI displays an inline terminal prompt and sends `JavaScriptDialogReply`.
4. Chromium runs the stored dialog callback with the accepted/canceled result.

The automated harness should not require a real terminal UI. It should connect
as the fake GUI/direct browser client, receive the dialog request, send the
reply, and assert the page observes the expected JavaScript return value.

This experiment intentionally does not add native dialogs, AppKit prompts,
Wezboard-specific UI, or Chrome's product dialog stack.

## Changes

1. Create a new Chromium branch.

   In `chromium/src`, fork from:

   ```text
   148.0.7778.97-issue-799-exp4
   ```

   Name the new branch:

   ```text
   148.0.7778.97-issue-799-exp5
   ```

   Add it to `chromium/README.md` with a description such as:

   ```text
   Add protocol-mediated JavaScript dialogs.
   ```

2. Extend `termsurf.proto`.

   Add two messages and two oneof entries:

   ```text
   JavaScriptDialogRequest javascript_dialog_request = 34;
   JavaScriptDialogReply javascript_dialog_reply = 35;
   ```

   `JavaScriptDialogRequest` should include:
   - `int64 tab_id`;
   - `uint64 request_id`;
   - `string dialog_type` with values `alert`, `confirm`, `prompt`, and
     `beforeunload`;
   - `string origin_url`;
   - `string message`;
   - `string default_prompt_text`;

   `JavaScriptDialogReply` should include:
   - `int64 tab_id`;
   - `uint64 request_id`;
   - `bool accepted`;
   - `string prompt_text`.

   Use new field numbers only. Do not renumber existing protocol fields.

3. Add a TermSurf JavaScript dialog manager in Chromium.

   Add a small Chromium helper under `content/libtermsurf_chromium/`, for
   example:

   ```text
   ts_javascript_dialog_manager.h
   ts_javascript_dialog_manager.cc
   ```

   The helper should implement `content::JavaScriptDialogManager`.

   Required behavior:
   - `RunJavaScriptDialog(...)` assigns a monotonic `request_id`, stores the
     `DialogClosedCallback`, and calls a new C callback such as
     `TsNotifyJavaScriptDialogRequest(...)`;
   - `RunBeforeUnloadDialog(...)` uses the same mechanism with
     `dialog_type = "beforeunload"`;
   - `CancelDialogs(...)` resolves outstanding callbacks as canceled;
   - duplicate dialogs for the same tab are suppressed consistently with
     Chromium's "one dialog at a time" model;
   - `did_suppress_message` is set correctly when suppressing;
   - no native `ShellJavaScriptDialog`, `NSAlert`, or Chrome UI code is used.

   Add a reply entry point callable from Roamium, for example:

   ```text
   ts_reply_javascript_dialog(wc, request_id, accepted, prompt_text)
   ```

   The reply must find the pending callback for the tab/request pair, run it
   exactly once, and erase it.

4. Route Content Shell to the TermSurf dialog manager.

   Prefer a TermSurf-owned `ShellPlatformDelegate` subclass returned by
   `TsBrowserMainParts::CreateShellPlatformDelegate()`. Override
   `CreateJavaScriptDialogManager(...)` to return the TermSurf manager.

   This keeps the patch local to `content/libtermsurf_chromium` and avoids
   patching generic Content Shell behavior. Patch `content/shell` only if there
   is no clean override point, and record why.

5. Extend the C FFI and Roamium dispatch.

   In `libtermsurf_chromium.h/.cc`, add:
   - `ts_set_on_javascript_dialog_request(...)`;
   - `ts_reply_javascript_dialog(...)`;
   - the internal `TsNotifyJavaScriptDialogRequest(...)` function.

   In `roamium/src/ffi.rs`, bind those functions.

   In `roamium/src/dispatch.rs`:
   - add an extern callback that maps the Chromium request to
     `JavaScriptDialogRequest`;
   - include the owning `tab_id` by resolving the `TsWebContents` handle;
   - handle incoming `JavaScriptDialogReply` by calling the FFI reply function;
   - log stable lines with a prefix such as `[termsurf-js-dialog]` for request
     and reply.

   Run `cargo fmt` after Rust edits and accept all formatting.

6. Handle JavaScript dialog requests in `webtui`.

   Add `JavaScriptDialogRequest` to the browser connection reader. The TUI
   should show an inline terminal prompt rather than allowing a native dialog.

   Minimal required UI:
   - `alert`: display origin/message and wait for Enter or Esc. Enter accepts;
     Esc cancels.
   - `confirm`: display origin/message and accept `y`/Enter as true, `n`/Esc as
     false.
   - `prompt`: display origin/message/default text and let the user edit a
     single-line response. Enter accepts the text; Esc cancels.
   - `beforeunload`: display the message and accept Enter/`y` as proceed, `n` or
     Esc as stay.

   The exact styling can be simple. The important behavior is that the page's
   JavaScript promise/return value is unblocked by a protocol reply, not by a
   native OS dialog.

7. Ensure initial-load dialogs cannot be lost.

   A page can call `alert()` during initial load. The implementation must not
   rely on the webtui direct browser socket being connected before the first
   dialog request.

   Current Chromium ordering makes this risk concrete:
   - `TsBrowserMainParts::CreateTab()` calls
     `Shell::CreateNewWindow(..., GURL(url), ...)`;
   - `Shell::CreateNewWindow()` immediately calls `LoadURL()` when the URL is
     nonempty;
   - only after that path does TermSurf assign the tab id and fire
     `TsNotifyTabReady()`.

   Therefore the "send `TabReady` early enough" option is not a logging-only
   change. It likely requires changing tab creation ordering so the shell is
   created with `about:blank`, the tab id and browser socket are published, and
   the requested URL is loaded only after the TUI/fake harness has a chance to
   connect. If the implementation chooses that path, it must prove normal
   first-load navigation still works.

   Choose one of these approaches and document the choice in the result:
   - change tab creation ordering so `TabReady` is sent before loading the
     requested URL, then verify with an initial-load alert fixture; or
   - buffer pending `JavaScriptDialogRequest` messages in Roamium until at least
     one direct client is connected, then flush them; or
   - route the request through Wezboard to the pane's TUI connection if the
     direct browser connection is not ready.

   A solution that only works for delayed post-load dialogs is `Partial`.

8. Extend the Issue 799 harness.

   Add deterministic local probes:
   - `javascript-alert`: page calls `alert("alpha")`; harness replies accepted;
     page reports that execution resumed after alert.
   - `javascript-confirm-accept`: page calls `confirm("beta")`; harness replies
     accepted; page reports `true`.
   - `javascript-confirm-cancel`: page calls `confirm("gamma")`; harness replies
     canceled; page reports `false`.
   - `javascript-prompt`: page calls `prompt("delta", "default")`; harness
     replies accepted with `typed value`; page reports that exact string.
   - `javascript-prompt-cancel`: page calls `prompt("epsilon", "default")`;
     harness replies canceled; page reports `null`.
   - `javascript-initial-load-alert`: page calls `alert("load")` from an early
     inline script before normal load completion; harness replies accepted; page
     reports resumed execution.
   - `javascript-beforeunload-proceed`: page installs a beforeunload handler,
     the harness gives the page sticky user activation with a real TermSurf
     protocol input event, triggers navigation away, replies accepted/proceed,
     and the new page loads.
   - `javascript-beforeunload-stay`: page installs a beforeunload handler, the
     harness gives the page sticky user activation with a real TermSurf protocol
     input event, triggers navigation away, replies canceled/stay, and the
     original page remains loaded.

   The beforeunload probes must not rely on merely installing a handler and
   navigating away. Chromium only uses the blocking/cancelable beforeunload path
   when the document has sticky user activation. The harness should send a
   contained activation event such as a click on a fixture button via
   `MouseMove` + `MouseEvent down/up` before issuing the navigation. The result
   evidence must show both that the fixture observed activation and that a
   `JavaScriptDialogRequest` with `dialog_type = "beforeunload"` was received
   and replied to.

   The fake harness should auto-reply to `JavaScriptDialogRequest` messages. The
   probe classification must be based on both:
   - captured request/reply protocol evidence with matching `request_id`; and
   - the page report showing the expected JavaScript return value.

   Add `javascript_dialogs` evidence to `probe-results.json`, including request
   type, origin URL, message, default text, accepted flag, prompt text, and
   latency.

9. Build and verify.

   Build Chromium:

   ```bash
   cd chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

   Build Rust components after protobuf and Rust edits:

   ```bash
   ./scripts/build.sh roamium
   ./scripts/build.sh webtui
   ./scripts/build.sh wezboard
   ```

   Run the new probes first:

   ```bash
   scripts/test-issue-799-browser-api-audit.py --probe javascript-alert --seconds 8
   scripts/test-issue-799-browser-api-audit.py --probe javascript-confirm-accept --seconds 8
   scripts/test-issue-799-browser-api-audit.py --probe javascript-confirm-cancel --seconds 8
   scripts/test-issue-799-browser-api-audit.py --probe javascript-prompt --seconds 8
   scripts/test-issue-799-browser-api-audit.py --probe javascript-prompt-cancel --seconds 8
   scripts/test-issue-799-browser-api-audit.py --probe javascript-initial-load-alert --seconds 8
   scripts/test-issue-799-browser-api-audit.py --probe javascript-beforeunload-proceed --seconds 8
   scripts/test-issue-799-browser-api-audit.py --probe javascript-beforeunload-stay --seconds 8
   ```

   Then run the full suite:

   ```bash
   scripts/test-issue-799-browser-api-audit.py --seconds 8
   ```

10. Archive and review.

    If the experiment passes, commit the Chromium branch, regenerate the Issue
    799 patch archive, and update `chromium/README.md`.

    Run `codex-review` before recording completion. Include:
    - this experiment file;
    - the protocol diff;
    - the Chromium diff;
    - the Roamium/webtui diffs;
    - the narrow JavaScript dialog harness runs;
    - the full Issue 799 harness run.

    Fix all real findings before marking the experiment `Pass`, `Partial`, or
    `Fail`.

## Verification

This experiment passes if:

- Chromium builds with `autoninja -C out/Default libtermsurf_chromium`;
- Roamium, webtui, and Wezboard build after protobuf regeneration;
- no native `NSAlert`, Content Shell dialog, or Chrome dialog UI appears in the
  automated runs;
- every new dialog probe captures exactly one request and one reply with a
  matching `request_id`;
- `alert()` resumes execution only after the harness reply;
- accepted `confirm()` returns `true`;
- canceled `confirm()` returns `false`;
- accepted `prompt()` returns the supplied prompt text;
- canceled `prompt()` returns `null`;
- an initial-load alert is delivered and replied to rather than lost before the
  direct browser connection is ready;
- accepted beforeunload proceeds to the requested destination page;
- canceled beforeunload keeps the original page loaded;
- beforeunload probes prove sticky user activation was established before
  navigation and that the `beforeunload` dialog request path was actually
  exercised;
- the full Issue 799 harness still passes the previous Payment Request and
  generic download checks;
- Rust edits are formatted with `cargo fmt`;
- Codex reviews the completed experiment and has no blocking findings.

This experiment is partial if:

- delayed post-load dialogs work but initial-load dialogs can still be lost;
- the Chromium/Roamium protocol path works but webtui's interactive prompt UI is
  incomplete;
- `alert`, `confirm`, and `prompt` work but beforeunload needs a follow-up;
- automation can prove request/reply delivery but cannot yet prove one of the
  JavaScript return values;
- the implementation requires a temporary timeout/default policy for no-reply
  cases, but the normal reply path works.

This experiment fails if:

- it opens native dialog UI;
- it requires manual clicking, Screen Recording permission, Accessibility
  permission, or a real Wezboard window to verify;
- it suppresses dialogs unconditionally instead of delivering them through a
  request/reply path;
- it blocks the renderer permanently while waiting for a reply;
- it changes unrelated browser API behavior, PDF behavior, Payment Request
  default-deny behavior, or generic downloads.

## Expected Outcome

After this experiment, pages that use `alert`, `confirm`, or `prompt` should no
longer rely on native Roamium dialogs. Dialogs should become ordinary TermSurf
protocol events with deterministic automated coverage.

If this passes, the next Issue 799 experiment should continue with another
automatable browser feature from Experiment 1, likely page zoom, console
capture, HTTP Basic Auth, or a focused cleanup of remaining actionable empty
binders.

## Result

**Result:** Pass

Experiment 5 added protocol-mediated JavaScript dialogs and removed TermSurf's
dependency on native Content Shell dialog UI for `alert`, `confirm`, `prompt`,
and beforeunload confirmation.

Implemented pieces:

- extended `termsurf.proto` with `JavaScriptDialogRequest` and
  `JavaScriptDialogReply`;
- added `TsJavaScriptDialogManager` in `libtermsurf_chromium`;
- routed Content Shell through a TermSurf `ShellPlatformDelegate`;
- added Chromium C FFI callbacks and reply entry points;
- routed request/reply messages through Roamium and Wezboard;
- added an inline webtui dialog mode for alert, confirm, prompt, and
  beforeunload;
- changed Chromium tab creation to publish `TabReady` before loading the
  requested URL, so initial-load dialogs are not lost;
- extended the Issue 799 harness with eight dialog probes;
- tightened beforeunload automation so navigation only happens after the fixture
  reports activation, and the verifier fails beforeunload probes without that
  activation evidence.

Verification:

- `autoninja -C out/Default libtermsurf_chromium` succeeded on branch
  `148.0.7778.97-issue-799-exp5`;
- `./scripts/build.sh roamium`, `./scripts/build.sh webtui`, and
  `./scripts/build.sh wezboard` succeeded;
- focused strict JavaScript dialog matrix:
  `logs/issue-799-browser-api-audit/20260531-001351-996812`;
- full strict Issue 799 harness:
  `logs/issue-799-browser-api-audit/20260531-001518-237131`.

The focused matrix classified all eight dialog probes as `dialog_completed`:

- `javascript-alert`;
- `javascript-confirm-accept`;
- `javascript-confirm-cancel`;
- `javascript-prompt`;
- `javascript-prompt-cancel`;
- `javascript-initial-load-alert`;
- `javascript-beforeunload-proceed`;
- `javascript-beforeunload-stay`.

The full harness completed 19 probes. The JavaScript dialogs remained
`dialog_completed`, generic downloads remained `download_completed`, and
`missing_interfaces` was empty. The remaining non-green classifications were the
already-known deferred surfaces: `file-system-access` requires contained
activation/permission work, and `webauthn-create` requires virtual authenticator
coverage.

Codex completion review initially found one real blocker: beforeunload
activation was visible in raw logs but not enforced by the harness result. The
harness was updated to require an observed `activated` page report before
navigating and to record `beforeunload_activation` in `probe-results.json`.
Follow-up Codex review reported no blocking findings and said Experiment 5 is
good enough to record as `Pass`.

## Conclusion

JavaScript dialogs are now ordinary TermSurf protocol events instead of native
Roamium dialogs. The implementation covers delayed dialogs, initial-load
dialogs, accepted and canceled confirm/prompt cases, and accepted/canceled
beforeunload navigation.

The next Issue 799 experiment should continue down the automatable queue from
Experiment 1. The best next targets are page zoom, console capture, HTTP Basic
Auth, or a focused cleanup of the remaining actionable empty binders.
