# Issue 633: Persistent Compositor for Stable CAContext

## Goal

Eliminate the navigation flicker by switching the profile server from
`HasOwnCompositor` mode to `UseParentLayerCompositor` mode. This gives the
profile server a persistent `CAContext` whose `ca_context_id` never changes
across navigations — matching Chrome's behavior.

## Background

[Issue 632](632-nav-flicker-calayerhost.md) diagnosed the navigation flicker
through four experiments. The root cause: the profile server uses
content_shell's `HasOwnCompositor` mode, which creates a new
`BrowserCompositorMac` → `RecyclableCompositorMac` → `CALayerTreeCoordinator` →
`CAContext` on every navigation. The `ca_context_id` changes each time, forcing
the GUI to swap CALayerHosts, producing a brief blank frame.

Chrome avoids this with `UseParentLayerCompositor` mode. A persistent
window-level `ui::Compositor` owns the `CAContext`. Navigation only changes
which surfaces are embedded within the compositor — the `CAContext` and its
`ca_context_id` persist indefinitely. The GUI's `CALayerHost` never needs to be
swapped.

Issue 632 Experiment 4 confirmed that `UseParentLayerCompositor` can be adopted
without Chrome's `ui/views` framework. All required types (`ui::Compositor`,
`ui::Layer`, `AcceleratedWidgetMac`, `RecyclableCompositorMac`) are in
`ui/compositor` and `ui/accelerated_widget_mac`, which are available to content
embedders.

## How it works in Chrome

`BrowserCompositorMac::UpdateState()` (`browser_compositor_view_mac.mm` line
191) checks `parent_ui_layer_`:

```cpp
if (parent_ui_layer_) {
    TransitionToState(UseParentLayerCompositor);
    return;
}
```

In `UseParentLayerCompositor` mode (`TransitionToState`, line 245):

```cpp
parent_ui_layer_->Add(root_layer_.get());
```

No `RecyclableCompositorMac` is created per view. The `root_layer_` is added as
a child of the parent layer, sharing the parent's compositor. During navigation,
`DidNavigate()` generates a new `LocalSurfaceId` and re-embeds the surface — but
the compositor and `CAContext` persist.

## Implementation plan

### Step 1: Create a persistent compositor in the profile server

In `ShellBrowserMainParts` (or a new helper class), create the persistent
compositor that will outlive all navigations:

```cpp
// Create AcceleratedWidgetMac (bridge to CALayerParams).
auto widget_mac = std::make_unique<ui::AcceleratedWidgetMac>();

// Create ui::Compositor with a persistent FrameSinkId.
ui::ContextFactory* context_factory = content::GetContextFactory();
auto compositor = std::make_unique<ui::Compositor>(
    context_factory->AllocateFrameSinkId(),
    context_factory,
    base::SingleThreadTaskRunner::GetCurrentDefault(),
    false /* enable_pixel_canvas */);
compositor->SetAcceleratedWidget(widget_mac->accelerated_widget());

// Create root layer.
auto root_layer = std::make_unique<ui::Layer>(ui::LAYER_SOLID_COLOR);
root_layer->SetBounds(gfx::Rect(size_dip));
compositor->SetRootLayer(root_layer.get());
compositor->SetScaleAndSize(scale_factor, size_pixels, local_surface_id);
```

This compositor must be created before the first tab and must persist for the
lifetime of the profile server process.

### Step 2: Register for CALayerParams callback

Implement the `AcceleratedWidgetMacNSView` interface to receive the stable
`ca_context_id`:

```cpp
class PersistentCompositorBridge : public ui::AcceleratedWidgetMacNSView {
  void AcceleratedWidgetCALayerParamsUpdated() override {
    const auto* params = widget_mac_->GetCALayerParams();
    if (params && params->ca_context_id != 0 &&
        params->ca_context_id != last_sent_id_) {
      last_sent_id_ = params->ca_context_id;
      // Send ca_context_id via XPC to the GUI.
    }
  }
};
```

Register with `widget_mac->SetNSView(bridge)`. The `ca_context_id` is stable —
it only changes if the GPU process crashes and restarts.

### Step 3: Set parent_ui_layer_ on each RenderWidgetHostViewMac

At tab creation and on every `RenderViewHostChanged` (navigation), call:

```cpp
rwhv_mac->SetParentUiLayer(root_layer.get());
```

This switches the `BrowserCompositorMac` to `UseParentLayerCompositor` mode.
Each navigation's new `BrowserCompositorMac` adds its `root_layer_` as a child
of our persistent root layer, sharing the persistent compositor.

In `ShellTabObserver::RenderViewHostChanged()` (where we already re-register the
CALayerParams callback), add the `SetParentUiLayer` call.

### Step 4: Simplify the CALayerParams callback

The current per-view `SetCALayerParamsCallback` on `RenderWidgetHostViewMac`
won't fire in `UseParentLayerCompositor` mode (no `recyclable_compositor_`).
Replace it with the persistent bridge from Step 2. The callback path changes
from per-navigation to persistent.

### Step 5: Handle resize

When the GUI sends a resize via XPC, update the persistent compositor's size:

```cpp
compositor->SetScaleAndSize(scale_factor, new_size_pixels, new_local_surface_id);
root_layer->SetBounds(gfx::Rect(new_size_dip));
```

The `BrowserCompositorMac` will propagate the size change to the
`DelegatedFrameHost` and the renderer.

## What this changes

| Aspect                   | Before (HasOwnCompositor)   | After (UseParentLayerCompositor) |
| ------------------------ | --------------------------- | -------------------------------- |
| CAContext per navigation | New                         | Same (persistent)                |
| `ca_context_id` changes  | Every navigation            | Never (unless GPU crash)         |
| GUI CALayerHost swap     | Every navigation            | Once at startup                  |
| RecyclableCompositorMac  | Per BrowserCompositorMac    | One persistent instance          |
| CALayerParams callback   | Per RenderWidgetHostViewMac | On persistent bridge             |

## Chromium branch

`146.0.7650.0-issue-633` (forked from `146.0.7650.0-issue-631`)

## Key files to modify

- `content/chromium_profile_server/browser/shell_browser_main_parts.cc` — Create
  persistent compositor, register callback
- `content/chromium_profile_server/browser/shell_browser_main_parts.h` — Store
  persistent compositor members
- `content/chromium_profile_server/browser/shell_tab_observer.cc` — Call
  `SetParentUiLayer` on view swap
- `content/chromium_profile_server/browser/BUILD.gn` — Add `ui/compositor`
  dependency if not already present

## Success criteria

Navigation between pages has no visible blank flash. The `ca_context_id` remains
constant across navigations (verified via logging). The GUI's `CALayerHost` is
created once at startup and never swapped.
