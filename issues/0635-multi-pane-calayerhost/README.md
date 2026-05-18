+++
status = "closed"
opened = "2026-02-24"
closed = "2026-02-24"
+++

# Issue 635: Multi-Pane Regression with Persistent Compositor

## Goal

Fix the multi-pane regression introduced by the persistent compositor (Issue
633). Opening a second pane with the same profile must create an independent tab
— not cause both panes to navigate together.

## Background

The Issue 634 audit discovered a severe regression: opening a second browser
pane with the same profile causes both webviews to navigate to the new URL. The
profile server is supposed to manage independent tabs — each pane gets its own
`WebContents`, its own URL, its own navigation history. Instead, the persistent
compositor changes from Issue 633 have conflated the two tabs.

This broke a feature that worked correctly under the old
`FrameSinkVideoCapturer` pipeline and in the pre-633 `HasOwnCompositor` mode.

## Analysis

The root cause is in `CreateTab()` in `shell_browser_main_parts.cc`. Two
problems:

### Problem 1: Single persistent compositor shared across all tabs

The persistent compositor has one `persistent_root_layer_`. Every tab's
`RenderWidgetHostViewMac` is given the same parent layer via
`SetParentUiLayerOnView(view, persistent_root_layer_.get())`. In Chrome, each
tab gets its own window with its own compositor. The profile server has no
windows — it needs one persistent compositor **per tab**, not one for the entire
process.

### Problem 2: Bridge callback overwrites on every tab

The `persistent_bridge_->SetCallback()` is called on every `CreateTab`. Each
call replaces the previous callback with one that sends `ca_context_id` for the
new pane. This means:

- Tab 1 creates bridge with callback for pane A
- Tab 2 calls `SetCallback` with callback for pane B
- The bridge now only sends `ca_context_id` for pane B
- Pane A never receives its `ca_context_id` again

### Problem 3: Single `ca_context_id` for all tabs

With one persistent compositor, there is one `CAContext` and one
`ca_context_id`. Both tabs share it. But the GUI creates one `CALayerHost` per
pane, each needing its own `ca_context_id`. A single shared `ca_context_id`
means both panes display the same compositor output — whichever tab rendered
last wins.

## Solution direction

Each tab needs its own persistent compositor with its own
`AcceleratedWidgetMac`, `ui::Compositor`, `ui::Layer`, and
`PersistentCompositorBridge`. The per-tab compositor produces a unique
`ca_context_id` that is sent to the GUI for that specific pane.

This means moving the persistent compositor members from `ShellBrowserMainParts`
(process-level) into `TabState` (per-tab).

## Experiment 1: Per-tab persistent compositor

### Hypothesis

If each tab gets its own `AcceleratedWidgetMac`, `ui::Compositor`, `ui::Layer`,
and `PersistentCompositorBridge`, each tab will produce its own `ca_context_id`.
The GUI will create independent `CALayerHost` instances per pane, and navigation
in one tab will not affect the other.

### Chromium branch

`146.0.7650.0-issue-635` (forked from `146.0.7650.0-issue-633`)

### Code changes

#### 1. Move compositor members into TabState

**File:** `shell_browser_main_parts.h`

Move the four persistent compositor members from `ShellBrowserMainParts` into
`TabState`:

```cpp
struct TabState {
  TabState();
  ~TabState();
  raw_ptr<Shell> shell;
  std::unique_ptr<ShellTabObserver> tab_observer;
#if BUILDFLAG(IS_MAC)
  xpc_connection_t tab_connection = nullptr;
  std::string pane_id;
  // Per-tab persistent compositor (Issue 635).
  std::unique_ptr<ui::AcceleratedWidgetMac> widget_mac;
  std::unique_ptr<ui::Compositor> compositor;
  std::unique_ptr<ui::Layer> root_layer;
  std::unique_ptr<PersistentCompositorBridge> bridge;
#endif
};
```

Remove the process-level members:

```cpp
// DELETE these four lines:
std::unique_ptr<ui::AcceleratedWidgetMac> persistent_widget_mac_;
std::unique_ptr<ui::Compositor> persistent_compositor_;
std::unique_ptr<ui::Layer> persistent_root_layer_;
std::unique_ptr<PersistentCompositorBridge> persistent_bridge_;
```

#### 2. Create compositor per tab in CreateTab()

**File:** `shell_browser_main_parts.cc`

Replace the `if (!persistent_compositor_)` block (which only creates once) with
unconditional per-tab creation. Use a local `TabState*` pointer since the tab
struct is built incrementally:

```cpp
// Create per-tab persistent compositor (Issue 635).
auto tab = std::make_unique<TabState>();

tab->widget_mac = std::make_unique<ui::AcceleratedWidgetMac>();
ui::ContextFactory* context_factory = content::GetContextFactory();
tab->compositor = std::make_unique<ui::Compositor>(
    context_factory->AllocateFrameSinkId(),
    context_factory,
    base::SingleThreadTaskRunner::GetCurrentDefault(),
    false /* enable_pixel_canvas */);
tab->compositor->SetAcceleratedWidget(
    tab->widget_mac->accelerated_widget());

tab->root_layer = std::make_unique<ui::Layer>(ui::LAYER_SOLID_COLOR);
tab->root_layer->SetColor(SK_ColorTRANSPARENT);

// Set initial size.
RenderWidgetHostView* view =
    shell->web_contents()->GetRenderWidgetHostView();
if (view) {
  float scale = view->GetDeviceScaleFactor();
  gfx::Size size_pixels(pixel_width, pixel_height);
  gfx::Size size_dip(
      static_cast<int>(std::ceil(pixel_width / scale)),
      static_cast<int>(std::ceil(pixel_height / scale)));
  tab->root_layer->SetBounds(gfx::Rect(size_dip));
  viz::LocalSurfaceId local_surface_id =
      viz::LocalSurfaceId(1, 1, base::UnguessableToken::Create());
  tab->compositor->SetScaleAndSize(
      scale, size_pixels, local_surface_id);
}

tab->compositor->SetRootLayer(tab->root_layer.get());
tab->compositor->SetVisible(true);

// Register bridge (Issue 635).
tab->bridge = std::make_unique<PersistentCompositorBridge>(
    tab->widget_mac.get());
```

#### 3. Set parent*ui_layer* using the tab's own root layer

Replace `SetParentUiLayerOnView(view, persistent_root_layer_.get())` with:

```cpp
SetParentUiLayerOnView(view, tab->root_layer.get());
```

And store the tab's root layer on the observer:

```cpp
tab_observer->SetParentUiLayer(tab->root_layer.get());
```

#### 4. Set bridge callback per tab

Replace the `if (persistent_bridge_)` block with:

```cpp
tab->bridge->SetCallback(base::BindRepeating(
    [](const std::string& pane_id, xpc_connection_t conn,
       uint32_t* last_id, const gfx::CALayerParams& params) {
      if (params.ca_context_id == 0 ||
          params.ca_context_id == *last_id)
        return;
      *last_id = params.ca_context_id;
      xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
      xpc_dictionary_set_string(msg, "action", "ca_context");
      xpc_dictionary_set_uint64(msg, "ca_context_id",
                                params.ca_context_id);
      xpc_dictionary_set_string(msg, "pane_id", pane_id.c_str());
      xpc_dictionary_set_uint64(msg, "pixel_width",
                                params.pixel_size.width());
      xpc_dictionary_set_uint64(msg, "pixel_height",
                                params.pixel_size.height());
      xpc_connection_send_message(conn, msg);
      xpc_release(msg);
      LOG(INFO) << "[ProfileServer] Tab compositor ca_context_id="
                << params.ca_context_id << " for pane " << pane_id;
    },
    cb_pane_id, cb_conn,
    base::Unretained(last_id)));
```

#### 5. Update ResizeTab to use the tab's compositor

Replace the `if (persistent_compositor_ && persistent_root_layer_)` block with:

```cpp
if (tab->compositor && tab->root_layer) {
  RenderWidgetHostView* compositor_view =
      tab->shell->web_contents()->GetRenderWidgetHostView();
  if (compositor_view) {
    float scale = compositor_view->GetDeviceScaleFactor();
    gfx::Size size_dip(
        static_cast<int>(std::ceil(pixel_width / scale)),
        static_cast<int>(std::ceil(pixel_height / scale)));
    tab->root_layer->SetBounds(gfx::Rect(size_dip));
    gfx::Size size_pixels(pixel_width, pixel_height);
    viz::LocalSurfaceId local_surface_id =
        viz::LocalSurfaceId(1, 1, base::UnguessableToken::Create());
    tab->compositor->SetScaleAndSize(
        scale, size_pixels, local_surface_id);
  }
}
```

#### 6. Move TabState construction earlier

Currently `TabState` is constructed near the end of `CreateTab()`. Move it to
the top (after Shell creation) so the compositor members can be populated
throughout the function. The shell, tab_observer, pane_id, and tab_connection
assignments stay where they are.

### Verification

1. Build: `autoninja -C out/Default chromium_profile_server`
2. Build GUI: `cd gui && zig build`
3. Launch: `open gui/zig-out/TermSurf.app`
4. Open `web` in one pane, navigate to a URL (e.g., google.com)
5. Open `web` in a second pane with the same profile, navigate to a different
   URL (e.g., github.com)
6. Verify: each pane shows its own page independently
7. Click a link in pane 1 — verify pane 2 is unaffected
8. Click a link in pane 2 — verify pane 1 is unaffected
9. Check logs: each pane should have its own `ca_context_id`

### Success criteria

- Two panes with the same profile display independent pages
- Navigation in one pane does not affect the other
- Each pane has its own `ca_context_id` in the logs
- No flicker on navigation (persistent compositor still works per-tab)

### Result: PASS

Per-tab persistent compositors work. Each pane gets its own `ca_context_id`,
displays its own page independently, and navigation in one pane does not affect
the other. No flicker on navigation.

## Conclusion

The regression was caused by a single persistent compositor shared across all
tabs. Moving the compositor members (`AcceleratedWidgetMac`, `ui::Compositor`,
`ui::Layer`, `PersistentCompositorBridge`) from process-level into per-tab
`TabState` gives each tab its own `CAContext` and `ca_context_id`. The GUI
creates independent `CALayerHost` instances per pane, restoring full multi-tab
isolation within a single profile. Fixed in one experiment.
