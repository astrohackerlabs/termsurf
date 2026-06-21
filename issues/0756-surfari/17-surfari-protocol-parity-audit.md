# Experiment 17: Audit Surfari protocol parity

## Description

Experiment 16 proved that Surfari can run as a real TermSurf browser process
against a fake GUI socket. The next Issue 756 checklist item is to audit Surfari
against Roamium and the existing TermSurf protobuf messages, marking every
message supported, unsupported, or requiring a protocol extension.

This experiment should produce a concrete protocol/API support matrix. It should
compare:

- `proto/termsurf.proto`;
- `roamium/src/main.rs` and `surfari/src/main.rs`;
- `roamium/src/ffi.rs` and `roamium/src/dispatch.rs`;
- `surfari/src/ffi.rs` and `surfari/src/dispatch.rs`;
- `surfari/libtermsurf_webkit/include/libtermsurf_webkit.h`;
- `surfari/libtermsurf_webkit/src/libtermsurf_webkit.mm`;
- `surfari/libtermsurf_webkit/README.md`;
- existing evidence from Experiments 5-16.

The output should make the remaining work obvious before Ghostboard integration:
which protocol messages are already implemented and tested for Surfari, which
are wired but only partially proven, which are unsupported because the C ABI is
missing a real implementation, and whether any WebKit capability needs a
`termsurf.proto` extension.

This experiment is an audit/documentation experiment. It should not implement
DevTools, modify `termsurf.proto`, integrate Ghostboard, change webtui browser
selection, or patch WebKit source. It may update the Issue 756 checklist for
items already proven by Experiments 15 and 16.

## Changes

- Add an audit artifact under the Issue 756 folder, for example
  `17-surfari-protocol-parity-audit.md` itself or a companion table if the
  result is too large.
- Build a support matrix for all TermSurf protocol messages relevant to the
  browser engine process:
  - GUI -> engine: `CreateTab`, `CreateDevtoolsTab`, `Resize`, `CloseTab`,
    `Navigate`, `MouseEvent`, `MouseMove`, `ScrollEvent`, `KeyEvent`,
    `FocusChanged`, `SetColorScheme`, `SetGuiActive`, `JavaScriptDialogReply`,
    and `HttpAuthReply`;
  - engine -> GUI/TUI: `ServerRegister`, `TabReady`, `CaContext`, `UrlChanged`,
    `LoadingState`, `TitleChanged`, `CursorChanged`, `TargetUrlChanged`,
    `JavaScriptDialogRequest`, `ConsoleMessage`, `HttpAuthRequest`,
    `RendererCrashed`, and `QueryTabsReply`;
  - request messages handled by the engine directly, such as `QueryTabsRequest`;
  - messages intentionally not engine-owned, such as `SetOverlay`,
    `SetDevtoolsOverlay`, `OpenSplit`, `HelloRequest`, `HelloReply`,
    `QueryLastRequest`, `QueryLastReply`, `QueryDevtoolsRequest`,
    `QueryDevtoolsReply`, `BrowserReady`, and `ModeChanged`.
- For each message, record:
  - owner (`GUI`, `TUI`, `engine`, or not engine-owned);
  - Roamium status;
  - Surfari Rust dispatch status;
  - `libtermsurf_webkit` C ABI status;
  - evidence source, such as a smoke test, fake-GUI log, source line, or prior
    experiment;
  - final classification: `Supported`, `Partial`, `Unsupported`,
    `Not engine-owned`, or `Needs protocol extension`.
- Identify exact gaps that must be solved before Ghostboard integration. The
  current expected gap is DevTools: `CreateDevtoolsTab` is wired in Surfari Rust
  but `ts_create_devtools_web_contents` returns `nullptr`.
- Determine whether the current protocol needs extension for WebKit. If no
  extension is needed yet, state that explicitly and leave the `termsurf.proto`
  checklist item unchecked until the claim is backed by a broader in-app test
  matrix.
- Update the Issue 756 end-to-end checklist only for items already proven by
  committed evidence:
  - mark the Surfari Rust binary item complete, citing Experiment 15;
  - mark the fake-GUI driver item complete, citing Experiment 16;
  - do not mark Ghostboard integration, in-app testing, regression guards, or
    full Ghostboard/Roamium matrix parity complete.
- Keep the issue README's experiment index synchronized.

## Verification

Run source inspections:

```bash
rg -n "Msg::|ts_|message " \
  proto/termsurf.proto \
  roamium/src/main.rs surfari/src/main.rs \
  roamium/src/ffi.rs roamium/src/dispatch.rs \
  surfari/src/ffi.rs surfari/src/dispatch.rs \
  surfari/libtermsurf_webkit/include/libtermsurf_webkit.h \
  surfari/libtermsurf_webkit/src/libtermsurf_webkit.mm
```

Run focused build/check commands:

```bash
cargo build -p surfari
cargo fmt -p surfari -- --check
git diff --check
prettier --check --prose-wrap always --print-width 80 \
  issues/0756-surfari/README.md \
  issues/0756-surfari/17-surfari-protocol-parity-audit.md
git -C webkit/src status --short
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse --is-shallow-repository
```

The audit passes if:

- every TermSurf protocol message is accounted for in the matrix;
- engine-owned messages have a classification with concrete evidence;
- non-engine-owned messages are explicitly separated from Surfari work;
- known Surfari gaps are precise enough to become follow-up experiments;
- the Issue 756 checklist is updated only where existing evidence proves
  completion;
- no source code, protocol, Ghostboard, webtui, or WebKit-source changes are
  made.

**Pass** = the audit matrix is complete, evidence-backed, and identifies exact
remaining gaps without changing implementation.

**Partial** = the audit finds ambiguous ownership or insufficient evidence for
one or more messages. Record the ambiguity and design the next experiment to
produce stronger evidence.

**Fail** = the audit cannot account for protocol ownership, contradicts source
evidence, or discovers a required implementation gap that must be fixed before
the matrix can be meaningful.

## Design Review

Adversarial design review initially found one required issue: the audit included
`ServerRegister`, but the source list omitted `roamium/src/main.rs` and
`surfari/src/main.rs`, where `ServerRegister` is sent. The design now includes
both `main.rs` files in the comparison list and verification command. Re-review
approved the design with no remaining required findings.

## Result

**Result:** Pass

Audited `proto/termsurf.proto`, Roamium's Rust process layer, Surfari's Rust
process layer, and `libtermsurf_webkit`. Surfari now has the same Rust
dispatch/protobuf surface as Roamium, with one known browser-engine gap:
DevTools tab creation is wired through `CreateDevtoolsTab` but the WebKit C ABI
currently returns `nullptr` from `ts_create_devtools_web_contents`.

The audit found no immediate need to change `termsurf.proto` for the currently
implemented WebKit behavior. Several protobuf names remain Chromium-flavored
(`chromium_tabs`, `chromium_browser`, `chromium_devtools`), but they are current
wire-contract names already used by Roamium and Surfari. Renaming them would be
a compatibility/protocol cleanup, not a WebKit capability requirement.

### Engine-Owned Protocol Matrix

| Message                   | Direction / owner        | Surfari status | Evidence                                                                                                                                                           |
| ------------------------- | ------------------------ | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `CreateTab`               | GUI -> engine            | Supported      | `surfari/src/dispatch.rs` calls `ts_create_web_contents`; Experiment 16 sends it and receives tab/page state.                                                      |
| `CreateDevtoolsTab`       | GUI -> engine            | Unsupported    | Surfari Rust calls `ts_create_devtools_web_contents`, but `libtermsurf_webkit.mm` returns `nullptr`; `libtermsurf_webkit/README.md` lists DevTools unsupported.    |
| `Resize`                  | GUI -> engine            | Supported      | Rust dispatch calls `ts_set_view_size`; Experiment 16 trace shows `ffi=ts_set_view_size`.                                                                          |
| `CloseTab`                | GUI -> engine            | Supported      | Rust dispatch calls `ts_destroy_web_contents`, destroys context, calls `ts_quit`; Experiment 16 proves clean exit.                                                 |
| `Navigate`                | GUI -> engine            | Supported      | Rust dispatch calls `ts_load_url`; C ABI implements `ts_load_url`; C smoke covers navigation.                                                                      |
| `MouseEvent`              | GUI -> engine            | Supported      | Rust dispatch calls `ts_forward_mouse_event`; C smoke verifies click handling.                                                                                     |
| `MouseMove`               | GUI -> engine            | Supported      | Rust dispatch calls `ts_forward_mouse_move`; C smoke verifies mouse move, hover target, and cursor paths.                                                          |
| `ScrollEvent`             | GUI -> engine            | Supported      | Rust dispatch calls `ts_forward_scroll_event`; C smoke verifies scroll handling.                                                                                   |
| `KeyEvent`                | GUI -> engine            | Supported      | Rust dispatch calls `ts_forward_key_event`; C smoke verifies keyboard input.                                                                                       |
| `FocusChanged`            | GUI -> engine            | Supported      | Rust dispatch calls `ts_set_focus`; C smoke verifies page-visible focus.                                                                                           |
| `SetColorScheme`          | GUI -> engine            | Supported      | Rust dispatch calls `ts_set_color_scheme`; C smoke verifies dark color scheme.                                                                                     |
| `SetGuiActive`            | GUI -> engine            | Supported      | Rust dispatch calls `ts_set_gui_active`; C smoke verifies inactive/active behavior.                                                                                |
| `JavaScriptDialogReply`   | TUI/GUI -> engine        | Supported      | Rust dispatch calls `ts_reply_javascript_dialog`; C smoke verifies alert, confirm, prompt, and stale replies.                                                      |
| `HttpAuthReply`           | TUI/GUI -> engine        | Supported      | Rust dispatch calls `ts_reply_http_auth`; C smoke verifies accepted, rejected, and stale replies.                                                                  |
| `QueryTabsRequest`        | client -> engine         | Supported      | Surfari dispatch builds `QueryTabsReply` from its local tab registry, matching Roamium's Rust layer. Not yet separately exercised by the Surfari fake-GUI harness. |
| `ServerRegister`          | engine -> GUI            | Supported      | Surfari `main.rs` sends `ServerRegister`; Experiment 16 receives `profile=profile`.                                                                                |
| `TabReady`                | engine -> GUI            | Supported      | Registered callback in `main.rs`; dispatch sends `TabReady`; C ABI and Experiment 16 prove positive tab id.                                                        |
| `CaContext`               | engine -> GUI            | Supported      | Registered callback in `main.rs`; dispatch sends `CaContext`; Experiments 2, 3, 5, and 16 prove nonzero context IDs.                                               |
| `UrlChanged`              | engine -> GUI/TUI        | Supported      | C ABI fires URL callbacks; Surfari dispatch sends `UrlChanged`; Experiment 16 proves deterministic URL.                                                            |
| `LoadingState`            | engine -> GUI/TUI        | Supported      | Surfari translates WebKit `(url, loading_bool)` to protocol `loading`/`done`; Experiment 16 now requires both states.                                              |
| `TitleChanged`            | engine -> GUI/TUI        | Supported      | C ABI evaluates document title; Surfari dispatch sends `TitleChanged`; Experiment 16 proves the deterministic title.                                               |
| `CursorChanged`           | engine -> GUI/TUI        | Supported      | Experiments 11-12 added and proved WebKit cursor callback support.                                                                                                 |
| `TargetUrlChanged`        | engine -> GUI/TUI        | Supported      | Experiment 10 proved target URL hover and clear callbacks.                                                                                                         |
| `JavaScriptDialogRequest` | engine -> TUI/automation | Supported      | Experiment 8/9-era C ABI work plus current smoke proves alert, confirm, and prompt request callbacks.                                                              |
| `ConsoleMessage`          | engine -> TUI/automation | Supported      | Experiment 13 proves log/info/warn/error callback payloads.                                                                                                        |
| `HttpAuthRequest`         | engine -> TUI/automation | Supported      | Experiment 9 proves normalized HTTP Basic auth callback fields.                                                                                                    |
| `RendererCrashed`         | engine -> TUI/automation | Supported      | Experiment 14 proves WebKit delegate-path renderer termination callback and Surfari Rust dispatch maps it to protobuf.                                             |
| `QueryTabsReply`          | engine -> client         | Supported      | Surfari dispatch implements the same tab-registry reply as Roamium; fields remain Chromium-named because the protobuf currently names them that way.               |

### Not Engine-Owned Protocol Messages

These messages are part of TermSurf but are not owned by the browser engine
process. Surfari should not implement them directly.

| Message                | Owner                      | Classification                                                                                              |
| ---------------------- | -------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `SetOverlay`           | TUI -> GUI                 | Not engine-owned                                                                                            |
| `SetDevtoolsOverlay`   | TUI -> GUI                 | Not engine-owned                                                                                            |
| `OpenSplit`            | TUI -> GUI                 | Not engine-owned                                                                                            |
| `ModeChanged`          | GUI -> TUI                 | Not engine-owned                                                                                            |
| `HelloRequest`         | TUI -> GUI                 | Not engine-owned                                                                                            |
| `HelloReply`           | GUI -> TUI                 | Not engine-owned                                                                                            |
| `QueryLastRequest`     | TUI -> GUI                 | Not engine-owned                                                                                            |
| `QueryLastReply`       | GUI -> TUI                 | Not engine-owned                                                                                            |
| `QueryDevtoolsRequest` | TUI -> GUI                 | Not engine-owned; eventually depends on Surfari DevTools support once GUI routes it to `CreateDevtoolsTab`. |
| `QueryDevtoolsReply`   | GUI -> TUI                 | Not engine-owned; eventually reflects whether Surfari DevTools tabs can be created.                         |
| `BrowserReady`         | GUI -> TUI                 | Not engine-owned                                                                                            |
| `TabInfo`              | Nested in `QueryTabsReply` | Supported as part of Surfari's engine-owned `QueryTabsReply` path.                                          |

### Remaining Gaps

- **DevTools:** `CreateDevtoolsTab` is the only currently unsupported
  engine-owned protocol path. The Rust dispatch is present, but
  `libtermsurf_webkit` returns `nullptr`. A follow-up experiment should decide
  whether to implement WebKit Web Inspector as a separate WebKit-backed surface
  or mark Surfari DevTools intentionally unsupported for the first Ghostboard
  integration pass.
- **QueryTabs runtime proof:** Source parity is present, but the Surfari
  fake-GUI harness does not yet send `QueryTabsRequest`. This is a small
  regression guard candidate.
- **In-app behavior:** Ghostboard launch, profile routing, real pane geometry,
  tabs/windows, input after layout changes, and full Roamium matrix parity are
  still unproven for Surfari.
- **Protocol naming:** No WebKit capability currently requires a new protobuf
  message. Chromium-specific field names in `QueryTabsReply` remain technical
  debt but not a blocking protocol extension.

Updated the Issue 756 checklist for already-proven items:

- Surfari Rust binary complete via Experiment 15.
- Fake-GUI Surfari driver complete via Experiment 16.
- Protocol parity audit complete via this experiment.

The checklist items for protocol modification, Ghostboard integration, in-app
testing, regression guards, and the full Ghostboard/Roamium feature matrix
remain open.

Verification completed:

```bash
rg -n "Msg::|ts_|message " \
  proto/termsurf.proto \
  roamium/src/main.rs surfari/src/main.rs \
  roamium/src/ffi.rs roamium/src/dispatch.rs \
  surfari/src/ffi.rs surfari/src/dispatch.rs \
  surfari/libtermsurf_webkit/include/libtermsurf_webkit.h \
  surfari/libtermsurf_webkit/src/libtermsurf_webkit.mm
cargo build -p surfari
cargo fmt -p surfari -- --check
git diff --check
prettier --check --prose-wrap always --print-width 80 \
  issues/0756-surfari/README.md \
  issues/0756-surfari/17-surfari-protocol-parity-audit.md
git -C webkit/src status --short
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse --is-shallow-repository
```

`webkit/src` remained unchanged:

```text
cdfb8cbf86f7c5e52cef0b2f14e8ab30ceeea91c
webkit-1452a439-issue-756-exp12
true
```

## Completion Review

Adversarial result review approved the completed audit with no findings. The
reviewer independently ran `cargo build -p surfari`,
`cargo fmt -p surfari -- --check`, `git diff --check`, markdown prettier checks,
and WebKit checkout-state checks. The reviewer confirmed every current
`proto/termsurf.proto` oneof message is accounted for in the matrix or the
not-engine-owned table, DevTools is correctly identified as the only unsupported
engine-owned path, only Issue 756 docs changed, the README and experiment both
mark Experiment 17 as `Pass`, and the result commit had not yet been made.

## Conclusion

Surfari's current protocol surface is strong enough to move toward Ghostboard
integration for normal browser tabs: creation, resize, close, navigation, input,
focus, color scheme, GUI active state, URL/loading/title state, CA context,
cursor, target URL, JavaScript dialogs, console messages, HTTP auth, renderer
crash reporting, and tab listing are all accounted for. DevTools remains the one
unsupported engine-owned protocol path. The next experiment should either
implement or explicitly defer Surfari DevTools before Ghostboard integration, or
start a minimal Ghostboard launch experiment that excludes DevTools and
documents that exclusion.
