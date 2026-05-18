+++
status = "closed"
opened = "2026-02-09"
closed = "2026-02-09"
+++

# Issue 408: Two Profiles at 60fps

## Goal

Render two Chromium `BrowserContext` instances side by side in a single macOS
window at 60fps or higher. Issue 407 proved that multiple profiles coexist in
one process and that content_shell renders a single WebContents at 60fps. But
placing two WebContents in one window dropped both to 2-3fps because manual
NSView manipulation broke Chromium's internal visibility tracking. This issue
solves the framerate problem.

## Background

Issue 407 established:

- **Multi-profile works.** Two `ShellBrowserContext` instances with different
  storage paths run in the same process with full isolation (separate cookies,
  localStorage, cache).
- **Single WebContents renders at 60fps.** content_shell's windowed rendering
  path has no framerate ceiling.
- **Two WebContents in one window renders at 2-3fps.** The
  `RenderWidgetHostViewCocoa` has NSView-level visibility tracking that
  overrides explicit `WasShown()` calls. Manually reparenting and resizing views
  causes it to misreport visibility, triggering Chromium's background tab
  throttle.

The throttling chain:

```
RenderWidgetHostViewCocoa (NSView visibility)
  -> RenderWidgetHostImpl::WasShown() / WasHidden()
  -> Blink PageSchedulerImpl::SetPageVisible()
  -> CC SchedulerStateMachine::visible_
  -> ShouldSubscribeToBeginFrames()
  -> vsync subscription ON (60fps) or OFF (~1fps)
```

## Success Criteria

- Two panes in one window, each showing a different localStorage string.
- Both panes render the spinning blue square at 60fps or higher.
- Strings persist across app restarts.
- No custom IPC protocol between the embedder and Chromium.

## Approaches to Investigate

### Approach 1: Chromium `views` framework

content_shell uses raw NSWindows, bypassing Chromium's `views` layer. Chrome
itself uses `views::WebView` to embed WebContents into `views::Widget` windows.
The `views` framework handles visibility, layout, occlusion, and resize
notifications through proper Chromium channels.

**Idea:** Create a `views::Widget` with two `views::WebView` children, each
backed by a different `BrowserContext`. The `views` framework manages visibility
through `NativeWidgetMacNSWindowHost` and `windowDidChangeOcclusionState:`,
which should keep both views at full framerate.

**Risk:** The `views` framework is large and tightly coupled to Chrome's UI.
Using it from a minimal embedder may pull in unwanted dependencies. It may also
assume Chrome-specific infrastructure (like `BrowserView`, `TabStripModel`) that
doesn't exist in our app.

### Approach 2: Off-screen compositing via CopyFromSurface

Use `RenderWidgetHostView::CopyFromSurface()` to capture each WebContents'
rendered output as a bitmap or GPU texture, then composite both into a single
Metal render pass in the host window.

**Idea:** Each WebContents renders into its own off-screen surface. The host app
reads these surfaces on a display-link timer and composites them into the
window. This is conceptually similar to CEF's off-screen rendering but uses
Chromium's Content API directly.

**Risk:** `CopyFromSurface()` may involve GPU readback (GPU -> CPU -> GPU),
which is slow. Need to verify whether there's a zero-copy path that yields an
IOSurface or Metal texture directly. If this is just a glorified screenshot API,
it will have the same throughput ceiling as CEF's OSR.

### Approach 3: Patch RenderWidgetHostViewCocoa visibility

Override or patch `RenderWidgetHostViewCocoa`'s NSView-level visibility
detection so that explicit `WasShown()` calls are respected regardless of the
view's position in the NSView hierarchy.

**Idea:** Find the specific code in `RenderWidgetHostViewCocoa` (or its backing
`RenderWidgetHostViewMac`) that checks NSView/NSWindow visibility and either
disable it or make it configurable. This is the most surgical fix — if the only
problem is the visibility misdetection, patching it should restore 60fps.

**Risk:** The visibility detection may exist for good reason (power savings,
correctness). Disabling it could cause subtle rendering bugs, excessive GPU
usage, or break other Chromium features that depend on accurate visibility
state.

### Approach 4: Two Shell windows, shared parent

Create two full `Shell` instances through the proper `Shell::CreateNewWindow()`
pipeline (each with its own NSWindow, proper platform delegate setup, correct
visibility tracking). Then reparent their content views into a shared parent
NSWindow.

**Idea:** Each Shell gets its own invisible NSWindow for Chromium's internal
bookkeeping, but the actual `RenderWidgetHostViewCocoa` NSViews are moved to a
visible parent window for display. Chromium thinks each view is in its own
window (preserving the one-WebContents-per-window assumption), while the user
sees a single window.

**Risk:** Reparenting NSViews between windows may trigger the same visibility
issues as Issue 407. NSViews removed from a key window may be treated as hidden.
Need to verify whether Chromium tracks the NSView's window or the Shell's
original window for visibility purposes.

### Approach 5: DelegatedFrameHost / IOSurface extraction

Intercept the compositor output at the `DelegatedFrameHost` or `ui::Compositor`
level to get each WebContents' rendered frames as IOSurfaces before they reach
the NSView, then composite both IOSurfaces into a single Metal render pass.

**Idea:** This is the "proper" version of off-screen compositing — instead of
screenshotting via `CopyFromSurface()`, tap into the compositor's frame
submission pipeline to get zero-copy access to the rendered surfaces. This is
how Chrome's tab preview thumbnails and picture-in-picture work internally.

**Risk:** The compositor internals (`viz`, `DelegatedFrameHost`,
`FrameSinkManager`) are complex and poorly documented for external use. This
approach requires deep understanding of Chromium's GPU compositing pipeline.

## How Electron Solves This

Electron faces the exact same problem — multiple WebContents in a single
BrowserWindow — and solves it with **three coordinated patches to Chromium**
that disable visibility throttling at every layer of the rendering pipeline. A
single-layer fix (like our Experiment 2's `WasShown()` call) is insufficient
because Chromium has three independent throttling systems that all must be
bypassed.

### Layer 1: RenderWidgetHost — `disable_hidden.patch`

Adds a `disable_hidden_` flag to `RenderWidgetHostImpl`. When set, `WasHidden()`
becomes a no-op — the widget never enters the hidden state:

```cpp
void RenderWidgetHostImpl::WasHidden() {
  if (disable_hidden_)
    return;  // Skip hidden-state processing entirely
  // ...
}
```

This is the layer our Experiment 2 tried to address with `WasShown()`, but
`WasShown()` can be overridden by subsequent `WasHidden()` calls from the
NSView-level visibility tracker. Electron's approach is more robust — it
prevents `WasHidden()` from taking effect at all.

### Layer 2: Blink scheduler — `allow_disabling_blink_scheduler_throttling_per_renderview.patch`

Adds `SetSchedulerThrottling()` to `WebViewImpl`. When disabled, the Blink page
scheduler is forced to treat the page as visible regardless of actual visibility
state:

```cpp
void WebViewImpl::SetPageLifecycleStateInternal(...) {
  if (!scheduler_throttling_allowed_)
    new_state->visibility = PageVisibilityState::kVisible;
  // ...
}
```

Even if Layer 1 failed, this would prevent the Blink scheduler from throttling
`requestAnimationFrame` callbacks and timer wake-ups.

### Layer 3: GPU compositor — `fix_disabling_background_throttling_in_compositor.patch`

Patches `ui::Compositor::SetVisible()` so that `SetDisplayVisible()` always
passes `true` when background throttling is disabled:

```cpp
void Compositor::SetVisible(bool visible) {
  if (display_private_)
    display_private_->SetDisplayVisible(
        background_throttling_ ? visible : true);
}
```

This prevents the viz DisplayScheduler from unsubscribing from vsync — the
mechanism that directly controls BeginFrame delivery.

### How Electron wires it up

Electron exposes a `backgroundThrottling` property on each WebContents. When set
to `false`, all three layers are activated:

```cpp
void WebContents::SetBackgroundThrottling(bool allowed) {
  background_throttling_ = allowed;
  rwh_impl->disable_hidden_ = !allowed;                          // Layer 1
  web_contents()->GetRenderViewHost()->SetSchedulerThrottling(allowed);  // Layer 2
  // Layer 3 handled by compositor patch
  if (rwh_impl->IsHidden())
    rwh_impl->WasShown({});  // Force visible immediately
}
```

### View composition

Electron uses Chromium's **`views` framework** for the actual layout — a
`views::Widget` (the window) with `views::WebView` children (one per
WebContents). But the `views` framework alone does not solve the framerate
problem. The three patches are what prevent throttling. The `views` framework
just handles layout, resize, and compositing of the already-rendering surfaces.

### macOS-specific patches

- **`disable_compositor_recycling.patch`** — Prevents expensive compositor
  destruction when a view is hidden but still attached to the window hierarchy.
  Without this, hiding and reshowing a WebContents causes a full compositor
  rebuild.
- **`revert_code_health_clean_up_stale_macwebcontentsocclusion.patch`** —
  Restores macOS occlusion detection that Chrome removed but Electron needs.

### What this means for us

Our Experiment 2 (`WasShown()` only) addressed one layer out of three, which is
why it failed. Electron's approach confirms that all three throttling systems
must be disabled simultaneously. The patches are surgical and well-defined — we
can apply the same pattern to our Chromium fork.

## Approach Selection

**Approach 3 (patch visibility), informed by Electron's three-layer pattern.**
Apply Electron's three patches to our Chromium fork:

1. Add `disable_hidden_` to `RenderWidgetHostImpl` and set it on our WebContents
   instances.
2. Add `SetSchedulerThrottling()` to `WebViewImpl` and disable it for our
   WebContents.
3. Patch `ui::Compositor::SetVisible()` to bypass `SetDisplayVisible(false)`
   when throttling is disabled.

This is the proven path — Electron ships this to millions of users. The patches
are small, targeted, and don't require restructuring the app.

Additionally, adopt Electron's use of the **`views` framework** for view
composition. Replace the raw NSView manipulation from Issue 407's experiments
with `views::Widget` + `views::WebView` children. This gives proper layout,
resize, and compositing through Chromium's own channels.

If the three-patch approach works (high confidence given Electron's track
record), further approaches are unnecessary.

## Electron's Full Patch Inventory

Electron maintains 147 patches against Chromium (version 146.0.7650.0). The
patches live at `electron/patches/chromium/` in our repo. Below is a complete
inventory organized by category, with an assessment of relevance to TermSurf.

### Summary by category

| Category                         | Count | Relevance to TermSurf |
| -------------------------------- | ----- | --------------------- |
| Build system / compilation       | 22    | None                  |
| Node.js / V8 integration         | 18    | None                  |
| Window management / UI           | 16    | Some                  |
| WebContents / renderer hooks     | 12    | Some                  |
| Network / protocol handling      | 8     | Low                   |
| Visibility / throttling          | 7     | **Critical**          |
| macOS-specific fixes             | 7     | Some                  |
| Windows-specific fixes           | 7     | None (macOS-only now) |
| Linux-specific fixes             | 7     | None (macOS-only now) |
| Chrome dependency removal        | 7     | Some                  |
| Off-screen rendering (OSR)       | 5     | Worth studying        |
| Security / sandboxing            | 5     | Low                   |
| Crash reporting / stability      | 5     | Low                   |
| Media keys / input               | 5     | None                  |
| Spellcheck                       | 4     | None                  |
| Desktop capture / screen sharing | 3     | None                  |
| Process singleton / lifecycle    | 3     | None                  |
| Printing                         | 2     | None                  |
| Notifications                    | 1     | None                  |
| DOM storage                      | 1     | Maybe                 |
| USB                              | 1     | None                  |
| Accelerator / shortcuts          | 1     | None                  |
| Service process                  | 1     | None                  |

### Visibility / throttling (7 patches) — CRITICAL

These are the patches that directly solve our 2-3fps problem.

- **`disable_hidden.patch`** — Adds `disable_hidden_` flag to
  `RenderWidgetHostImpl`. When set, `WasHidden()` is a no-op.
- **`allow_disabling_blink_scheduler_throttling_per_renderview.patch`** — Adds
  `SetSchedulerThrottling()` to `WebViewImpl`. Forces page to appear visible to
  the Blink scheduler.
- **`fix_disabling_background_throttling_in_compositor.patch`** — Patches
  `ui::Compositor::SetVisible()` to keep `SetDisplayVisible(true)` when
  throttling is disabled.
- **`disable_compositor_recycling.patch`** — Prevents compositor destruction
  when a macOS view is hidden but still attached. Without this, show/hide cycles
  cause expensive compositor rebuilds.
- **`revert_remove_the_allowaggressivethrottlingwithwebsocket_feature.patch`** —
  Re-adds `AllowAggressiveThrottlingWithWebSocket` feature flag that Chromium
  removed.
- **`revert_code_health_clean_up_stale_macwebcontentsocclusion.patch`** —
  Restores legacy macOS WebContents occlusion behavior.
- **`refactor_unfilter_unresponsive_events.patch`** — Removes renderer
  unresponsive event filters so the embedder can handle them.

### Build system / compilation (22 patches) — NOT NEEDED

All specific to Electron's build configuration: `is_electron_build` and
`is_mas_build` flags, BoringSSL extensions for Node.js, grit resource IDs,
libc++ ABI settings, `.gitignore` entries for Electron directories, license
credits. None apply to our build.

### Node.js / V8 integration (18 patches) — NOT NEEDED

Electron embeds Node.js inside Chromium's renderer process. These patches add V8
platform hooks, isolate holders, module import/meta hooks, worker context
destruction callbacks, and ScriptState hardening for Node.js contexts. TermSurf
does not embed Node.js.

### Window management / UI (16 patches) — SOME RELEVANT

Mostly Electron-specific UI features (aspect ratio control, frameless windows,
picture-in-picture, dark mode override, menu item customization, fullscreen
access). A few may be useful:

- **`webview_fullscreen.patch`** — Propagates fullscreen state from webview
  child frames to the embedder. May be needed if users full-screen a web page.
- **`feat_add_set_theme_source_to_allow_apps_to.patch`** — Overrides NativeTheme
  light/dark. Useful if TermSurf needs to control dark mode for web content.

### WebContents / renderer hooks (12 patches) — SOME RELEVANT

Hooks for intercepting window creation, site instance creation, per-window
WebPreferences, cursor changes, and script injection. Potentially useful:

- **`can_create_window.patch`** — Intercepts `window.open()` calls. Needed to
  prevent popups or redirect them to new TermSurf panes.
- **`allow_in-process_windows_to_have_different_web_prefs.patch`** — Per-window
  WebPreferences (context isolation, etc.). Useful for different security
  settings per profile.

### Off-screen rendering (5 patches) — WORTH STUDYING

Electron's OSR patches show what's needed to make off-screen rendering work with
the viz compositor. Even though we're targeting windowed rendering, these are
instructive:

- **`feat_enable_offscreen_rendering_with_viz_compositor.patch`** — Custom
  `HostDisplayClient` and `LayeredWindowUpdater` for OSR via viz.
- **`osr_shared_texture_remove_keyed_mutex_on_win_dxgi.patch`** — Removes DXGI
  keyed mutex for OSR texture sharing (performance fix, Windows-only).
- **`patch_osr_control_screen_info.patch`** — Makes `GetNewScreenInfosForUpdate`
  virtual for custom screen info in OSR.

### Chrome dependency removal (7 patches) — SOME RELEVANT

Decouples Electron from Chrome-specific infrastructure (Profile, themes,
ExtensionSystem). We face the same problem — content_shell doesn't have Chrome's
`Profile` class. Patches like `proxy_config_monitor.patch` and
`chore_patch_out_profile_methods.patch` show how Electron avoids these
dependencies.

### macOS-specific fixes (7 patches) — SOME RELEVANT

- **`render_widget_host_view_mac.patch`** — Overrides `acceptsFirstMouse` and
  `disableAutoHideCursor`. May be needed for mouse handling in web panes.
- **`fix_restore_original_resize_performance_on_macos.patch`** — Reverts an
  Android-specific change that regressed macOS resize performance. Likely
  needed.
- **`fix_add_macos_memory_query_fallback_to_avoid_crash.patch`** — Fallback for
  memory query to avoid sandbox crash. May be needed.

### Network / protocol handling (8 patches) — LOW RELEVANCE

Custom protocol handlers, raw response headers, streaming protocols, service
worker fetching. Most are Electron API features. One may be useful:

- **`feat_allow_code_cache_in_custom_schemes.patch`** — Enables V8 code cache
  for custom schemes. Relevant if TermSurf uses custom URL schemes.

### Windows/Linux-specific (14 patches) — NOT NEEDED NOW

TermSurf is macOS-only for now. These can be revisited if/when we go
cross-platform.

### Everything else (25 patches) — NOT NEEDED

Security/permissions, crash reporting, media keys, spellcheck, desktop capture,
process singleton, printing, notifications, USB, accelerators, service process.
All Electron-specific features that TermSurf does not use.

## Conclusion

### What we learned

The 2-3fps throttling is not a single bug — it is three independent throttling
systems in Chromium's rendering pipeline, each of which must be individually
disabled. Our Experiment 2 (`WasShown()` only) addressed one layer and failed.
Electron solved this years ago by patching all three layers:

1. `RenderWidgetHostImpl` — prevent `WasHidden()` from firing
2. Blink `PageSchedulerImpl` — force the page to appear visible
3. `ui::Compositor` / viz `DisplayScheduler` — keep vsync subscription active

These patches are small, surgical, and proven in production across millions of
Electron installations.

### Why we're adopting Electron's full patch set

Rather than cherry-picking the 7 visibility patches and guessing which of the
remaining 140 we might need later, we will apply Electron's entire 147-patch
set to our Chromium fork. The reasoning:

1. **Simplicity.** Analyzing 147 patches to decide which subset we need is more
   work than just applying all of them. Patches we don't need are either inert
   hooks (callback points that nothing calls) or gated behind flags we don't
   set (`is_electron_build`, `is_mas_build`).

2. **Tested combination.** Electron tests these 147 patches against Chromium
   146.0.7650.0. Applying the full set to the same version is a known-good
   configuration. A custom subset is untested.

3. **Future-proofing.** Patches that seem irrelevant today (fullscreen
   propagation, per-window WebPreferences, resize performance fixes) may become
   necessary as TermSurf matures. Having them already applied means we
   discover their value rather than their absence.

4. **Patch-on-patch escape hatch.** If any Electron patch causes a problem, we
   can apply our own patch on top to fix it. This is unlikely in the short term,
   but the option exists.

This approach means our `termsurf-chromium` submodule tracks the same Chromium
version as Electron and applies the same patch set. Our own modifications
(like `content/two_profiles/`) go on top. This is the subject of Issue 409.

## Relationship to Other Issues

| Issue   | Relationship                                                                      |
| ------- | --------------------------------------------------------------------------------- |
| 325-350 | Proved CEF's off-screen rendering caps at ~31fps on macOS                         |
| 403     | Proved IOSurface compositing at 60fps with colored rectangles                     |
| 406     | Proved multiple profiles work in one Chromium process                             |
| 407     | Proved multi-profile in practice; identified visibility throttling as the blocker |
| 408     | This issue — researched the fix; decided to adopt Electron's patch set            |
| 409     | Applies the Electron patch set to our Chromium fork                               |
