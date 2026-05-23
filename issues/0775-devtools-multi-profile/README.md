+++
status = "open"
opened = "2026-04-11"
+++

# Issue 775: DevTools gets confused with multiple profiles open

## Goal

Fix DevTools targeting so opening DevTools always identifies the exact browser
engine, profile, and tab number to inspect. The DevTools protocol must not infer
the target from "the last tab" or any other global fallback; every DevTools open
request must be unambiguous even when multiple profiles or browser engines are
active.

Also update the `web` TUI UI so browser panes and DevTools panes visibly show
the full target identity: browser engine, profile, and tab number. The tab
number is currently missing from the displayed context and should be shown
alongside browser and profile so users can confirm which tab DevTools is
inspecting.

## Background

DevTools currently gets confused when multiple browser profiles are open. The
root cause is that DevTools requests do not explicitly specify which browser
engine process, profile, and tab they refer to. Current code tries to be helpful
by resolving some DevTools requests against "the last tab," but that fallback is
ambiguous and does not properly account for profile/process boundaries. When
only one profile is active this works by accident, but with multiple profiles
the targeting becomes ambiguous.

Since each profile runs in its own browser engine process (one process per
profile is a hard architectural constraint), DevTools must route to the correct
process. Opening DevTools from a tab should always open DevTools for that
specific tab — this must be guaranteed regardless of how many profiles or
engines are active.

## Analysis

The DevTools protocol messages need to be redesigned to explicitly include:

1. **Browser engine process** — Which engine process (identified by profile or
   process ID) to target.
2. **Profile** — Which profile the tab belongs to.
3. **Tab** — Which specific tab within that profile to inspect.

When a user triggers "open DevTools" from a pane, the GUI already knows which
pane is focused and which browser/profile/tab that pane maps to. This context
must be threaded through the DevTools open request so there is zero ambiguity at
every level of the message path.

This may require changes to:

- The TermSurf protocol (`termsurf.proto`) — DevTools messages may need
  additional fields for profile/engine targeting.
- The GUI's DevTools request handling — Must resolve the focused pane to a
  specific (engine, profile, tab) tuple before sending the request.
- The browser engine's DevTools handler — Must validate that the request targets
  a tab it actually owns.
- The `web` TUI display — Must show browser engine, profile, and tab number for
  browser and DevTools panes so the active target is visible to the user.

## Experiments

### Experiment 1: Explicit DevTools target identity

#### Description

Fix both parts of the issue in one pass:

1. Make DevTools opening protocol-level explicit by requiring browser engine,
   profile, and inspected tab id when opening DevTools.
2. Update the `web` TUI display so both browser panes and DevTools panes show
   browser engine, profile, and tab number.

The current DevTools path uses `inspected_tab_id = 0` as an auto-target marker.
Wezboard then resolves that to `last_browser_pane`. That is the wrong model for
multi-profile TermSurf because tab ids are scoped to a browser process, not
globally unique. DevTools should instead target the exact
`(browser, profile, tab_id)` tuple that the current browser pane already knows.

This experiment should remove the "last tab" fallback from the DevTools open
path. It is acceptable for `web last` to keep querying the last browser pane as
a separate user command, but DevTools must not depend on it.

#### Changes

1. Update the protocol shape in `proto/termsurf.proto`.

   Add explicit browser targeting to DevTools request messages:
   - `QueryDevtoolsRequest`
     - keep `pane_id`;
     - keep `inspected_tab_id`;
     - keep `profile`;
     - add `browser`.

   Also consider adding `profile` and `browser` to `CreateDevtoolsTab` for
   validation/logging symmetry, even though Wezboard already routes the message
   to a single browser process by `(profile, browser)`.

   Regenerate Rust protobuf bindings through the existing project build flow. Do
   not hand-edit generated protobuf files except where the repo already tracks
   generated test fixtures and the build process requires them.

2. Update the `web` TUI DevTools command flow in `webtui/src/main.rs` and
   `webtui/src/ipc.rs`.

   Track the current browser tab id after `BrowserReady`.

   For a normal browser pane:
   - `BrowserReady.tab_id` becomes the pane's current tab number.
   - `:devtools` must call `send_query_devtools` with the current browser,
     profile, and tab id.
   - The split command that launches the DevTools TUI must be explicit, for
     example:

     ```text
     web --browser <browser> --profile <profile> devtools://<tab_id>
     ```

     Preserve the requested split direction.

   For a DevTools pane:
   - keep rejecting `:devtools` from inside DevTools.
   - bare `web devtools` should no longer mean "last tab." It should fail with a
     clear error that DevTools requires an explicit tab id, or be kept only as a
     compatibility path that immediately queries by current pane and resolves to
     an explicit tuple before opening anything. The preferred fix is to remove
     the global last-tab behavior from DevTools entirely.

3. Update Wezboard's DevTools validation in
   `wezboard/wezboard-gui/src/termsurf/conn.rs`.

   Replace global tab-id lookup with scoped lookup:

   ```text
   server_key = TermSurfState::server_key(profile, browser)
   lookup = (server_key, inspected_tab_id)
   ```

   Validation should pass only if:
   - `browser` is non-empty;
   - `profile` is non-empty;
   - `inspected_tab_id` is non-zero;
   - `(server_key, inspected_tab_id)` exists in `tab_to_pane`;
   - the inspected pane is not itself a DevTools pane;
   - no existing DevTools pane already targets the same
     `(server_key, inspected_tab_id)` tuple.

   Remove the `last_browser_pane` fallback from the DevTools path. Do not remove
   `last_browser_pane` entirely if `web last` or other non-DevTools flows still
   need it.

4. Update DevTools pane creation and routing.

   `SetDevtoolsOverlay` already carries `profile`, `browser`, and
   `inspected_tab_id`. Make sure Wezboard uses those fields as the only routing
   source when choosing the browser server for `CreateDevtoolsTab`.

   If `CreateDevtoolsTab` gains `profile` and `browser`, populate them from the
   pane and have Roamium log or validate them where cheap. The Chromium C API
   can still receive only `inspected_tab_id` because it is already running in
   the selected browser process.

5. Update the `web` TUI viewport identity display.

   The UI should show the full identity for both browser and DevTools panes:
   - browser engine;
   - profile;
   - current tab number for browser panes;
   - inspected tab number for DevTools panes.

   Today the bottom identity label shows profile/browser and the DevTools title
   shows profile/tab. The new display should consistently include all three
   pieces. Keep the layout compact enough for narrow panes; if necessary, use a
   short form such as:

   ```text
   roamium/default#12
   DevTools · roamium/default#12
   ```

   The browser pane should not show `#0` before `BrowserReady`; use a neutral
   loading label until the tab id is known.

6. Build affected components.

   ```bash
   scripts/build.sh webtui
   scripts/build.sh wezboard
   scripts/build.sh roamium
   ```

   Chromium should not need changes for this experiment unless the protocol
   regeneration requires Chromium-side generated test fixtures.

#### Verification

1. Start Wezboard and open two browser panes with different profiles:

   ```bash
   web --profile work example.com
   web --profile personal example.org
   ```

2. Confirm each browser pane's TUI display shows browser, profile, and its tab
   number.

3. In the `work` pane, run:

   ```text
   :devtools right
   ```

   Confirm the DevTools pane opens for the `work` tab, not the `personal` tab.
   The DevTools pane display must show the same browser/profile/tab identity as
   the inspected `work` pane.

4. In the `personal` pane, run:

   ```text
   :devtools right
   ```

   Confirm the DevTools pane opens for the `personal` tab. It must not reuse or
   collide with the `work` DevTools pane.

5. Try to open DevTools again for a tab that already has DevTools open.

   The request should fail with a clear duplicate-target error scoped to the
   exact `(browser, profile, tab_id)` tuple.

6. Launch a bare DevTools request outside a browser pane, if possible:

   ```bash
   web devtools
   ```

   This must not silently open DevTools for "the last tab." It should either
   fail clearly or resolve only through explicit pane context. The preferred
   passing behavior is a clear failure telling the user to open DevTools from a
   browser pane.

7. Run a normal browser flow after the change:
   - navigation still works;
   - `web last` still works if it is intentionally kept;
   - `web status` still works;
   - browser overlays still appear and resize normally.

#### Pass Criteria

The experiment passes if:

- DevTools requests include browser, profile, and non-zero inspected tab id;
- DevTools no longer opens by global last-tab inference;
- Wezboard validates DevTools targets by `(browser, profile, tab_id)`;
- DevTools opens correctly for two simultaneous profiles;
- duplicate DevTools detection is scoped by `(browser, profile, tab_id)`;
- browser and DevTools panes both show browser, profile, and tab number in the
  TUI;
- `webtui`, `wezboard`, and `roamium` build.

#### Partial Criteria

The experiment is Partial if:

- explicit targeting works but the UI label needs further polishing;
- the UI displays the correct identity but one legacy DevTools entry point still
  allows last-tab fallback and is documented as follow-up work;
- builds pass but one generated protobuf fixture remains to be updated.

#### Failure Criteria

The experiment fails if:

- DevTools can still silently target `last_browser_pane`;
- DevTools can open for the wrong profile when two profiles are active;
- tab ids are still treated as globally unique in DevTools validation;
- the fix breaks ordinary browser tab creation, navigation, overlay rendering,
  or existing non-DevTools commands.

#### Expected Interpretation

If this passes, Issue 775's core bug is fixed: DevTools targeting becomes an
explicit part of the protocol instead of a heuristic. The remaining work, if
any, should be limited to UI polish or closing the issue.
