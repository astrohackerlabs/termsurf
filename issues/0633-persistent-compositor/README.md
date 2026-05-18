+++
status = "closed"
opened = "2026-02-24"
closed = "2026-02-24"
+++

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

## Experiment 1: Create persistent compositor and set parent_ui_layer_

### Hypothesis

If the profile server creates a persistent `RecyclableCompositorMac` with a root
`ui::Layer` and passes that layer as `parent_ui_layer_` to each
`RenderWidgetHostViewMac`, the `BrowserCompositorMac` will enter
`UseParentLayerCompositor` mode. The `CAContext` will persist across navigations
and the `ca_context_id` will remain stable, eliminating the navigation flicker.

### Chromium branch

`146.0.7650.0-issue-633` (forked from `146.0.7650.0-issue-631`)

### Code changes

#### 1. Add dependencies to BUILD.gn

**File:** `content/chromium_profile_server/BUILD.gn`

Add to the deps list:

```gn
"//ui/compositor",
"//ui/accelerated_widget_mac",
```

#### 2. Add a bridge function to set parent_ui_layer_

`SetParentUiLayer` is on `RenderWidgetHostViewMac` (not the public
`RenderWidgetHostView` interface), so it needs an Obj-C++ bridge — same pattern
as the existing `shell_ca_layer_bridge_mac`.

**New file:**
`content/chromium_profile_server/browser/shell_compositor_bridge_mac.h`

```cpp
#ifndef CONTENT_CHROMIUM_PROFILE_SERVER_BROWSER_SHELL_COMPOSITOR_BRIDGE_MAC_H_
#define CONTENT_CHROMIUM_PROFILE_SERVER_BROWSER_SHELL_COMPOSITOR_BRIDGE_MAC_H_

namespace ui { class Layer; }

namespace content {

class RenderWidgetHostView;

void SetParentUiLayerOnView(RenderWidgetHostView* view, ui::Layer* layer);

}  // namespace content

#endif
```

**New file:**
`content/chromium_profile_server/browser/shell_compositor_bridge_mac.mm`

```objcpp
#include "content/chromium_profile_server/browser/shell_compositor_bridge_mac.h"
#include "content/browser/renderer_host/render_widget_host_view_mac.h"

namespace content {

void SetParentUiLayerOnView(RenderWidgetHostView* view, ui::Layer* layer) {
  auto* mac_view = static_cast<RenderWidgetHostViewMac*>(view);
  mac_view->SetParentUiLayer(layer);
}

}  // namespace content
```

Add both files to the sources list in BUILD.gn.

#### 3. Create the persistent compositor in ShellBrowserMainParts

**File:** `content/chromium_profile_server/browser/shell_browser_main_parts.h`

Add includes and members:

```cpp
#include "ui/accelerated_widget_mac/accelerated_widget_mac.h"
#include "ui/compositor/compositor.h"
#include "ui/compositor/layer.h"

// Inside the class, private section:
#if BUILDFLAG(IS_MAC)
  // Persistent compositor for UseParentLayerCompositor mode (Issue 633).
  std::unique_ptr<ui::AcceleratedWidgetMac> persistent_widget_mac_;
  std::unique_ptr<ui::Compositor> persistent_compositor_;
  std::unique_ptr<ui::Layer> persistent_root_layer_;
#endif
```

**File:** `content/chromium_profile_server/browser/shell_browser_main_parts.cc`

Add includes:

```cpp
#include "content/chromium_profile_server/browser/shell_compositor_bridge_mac.h"
#include "content/public/browser/context_factory.h"
#include "ui/accelerated_widget_mac/accelerated_widget_mac.h"
#include "ui/compositor/compositor.h"
#include "ui/compositor/layer.h"
```

In `CreateTab()`, after the Shell is created and resized (around line 354),
before the tab observer and callback setup, add:

```cpp
  // Create persistent compositor on first tab (Issue 633).
  if (!persistent_compositor_) {
    persistent_widget_mac_ = std::make_unique<ui::AcceleratedWidgetMac>();
    ui::ContextFactory* context_factory = content::GetContextFactory();
    persistent_compositor_ = std::make_unique<ui::Compositor>(
        context_factory->AllocateFrameSinkId(),
        context_factory,
        base::SingleThreadTaskRunner::GetCurrentDefault(),
        false /* enable_pixel_canvas */);
    persistent_compositor_->SetAcceleratedWidget(
        persistent_widget_mac_->accelerated_widget());

    persistent_root_layer_ =
        std::make_unique<ui::Layer>(ui::LAYER_SOLID_COLOR);
    persistent_root_layer_->SetColor(SK_ColorTRANSPARENT);

    // Set initial size from the first tab's dimensions.
    RenderWidgetHostView* view =
        shell->web_contents()->GetRenderWidgetHostView();
    if (view) {
      float scale = view->GetDeviceScaleFactor();
      gfx::Size size_pixels(pixel_width, pixel_height);
      gfx::Size size_dip(
          static_cast<int>(std::ceil(pixel_width / scale)),
          static_cast<int>(std::ceil(pixel_height / scale)));
      persistent_root_layer_->SetBounds(gfx::Rect(size_dip));
      viz::LocalSurfaceId local_surface_id =
          viz::LocalSurfaceId(1, 1, base::UnguessableToken::Create());
      persistent_compositor_->SetScaleAndSize(
          scale, size_pixels, local_surface_id);
    }

    persistent_compositor_->SetRootLayer(persistent_root_layer_.get());
    persistent_compositor_->SetVisible(true);

    LOG(INFO) << "[ProfileServer] Created persistent compositor (Issue 633)";
  }

  // Set parent_ui_layer_ on the view (Issue 633).
  {
    RenderWidgetHostView* view =
        shell->web_contents()->GetRenderWidgetHostView();
    if (view) {
      SetParentUiLayerOnView(view, persistent_root_layer_.get());
      LOG(INFO) << "[ProfileServer] Set parent_ui_layer_ on initial view";
    }
  }
```

#### 4. Set parent_ui_layer_ on view swap in ShellTabObserver

**File:** `content/chromium_profile_server/browser/shell_tab_observer.h`

Add:

```cpp
  // Store the parent ui layer for re-registration on view swap (Issue 633).
  void SetParentUiLayer(ui::Layer* layer);

  // In private section:
  raw_ptr<ui::Layer> parent_ui_layer_ = nullptr;
```

Add include:

```cpp
namespace ui { class Layer; }
```

**File:** `content/chromium_profile_server/browser/shell_tab_observer.cc`

Add:

```cpp
#include "content/chromium_profile_server/browser/shell_compositor_bridge_mac.h"
```

Add method:

```cpp
void ShellTabObserver::SetParentUiLayer(ui::Layer* layer) {
  parent_ui_layer_ = layer;
}
```

In `RenderViewHostChanged()`, after the existing callback re-registration (after
line 77), add:

```cpp
// Re-set parent_ui_layer_ on the new view (Issue 633).
if (parent_ui_layer_) {
  SetParentUiLayerOnView(view, parent_ui_layer_);
  LOG(INFO) << "[ShellTabObserver] Set parent_ui_layer_ on new view"
            << " pane=" << pane_id_;
}
```

**File:** `content/chromium_profile_server/browser/shell_browser_main_parts.cc`

In `CreateTab()`, after `tab_observer->SetLastCAContextIdPtr(last_id)` (line
437), add:

```cpp
// Store parent ui layer on observer for re-registration (Issue 633).
tab_observer->SetParentUiLayer(persistent_root_layer_.get());
```

#### 5. Handle resize

In `ShellBrowserMainParts::ResizeTab()`, after the existing resize logic, update
the persistent compositor:

```cpp
if (persistent_compositor_ && persistent_root_layer_) {
  RenderWidgetHostView* view =
      tab_state->shell->web_contents()->GetRenderWidgetHostView();
  if (view) {
    float scale = view->GetDeviceScaleFactor();
    gfx::Size size_dip(
        static_cast<int>(std::ceil(pixel_width / scale)),
        static_cast<int>(std::ceil(pixel_height / scale)));
    persistent_root_layer_->SetBounds(gfx::Rect(size_dip));
    gfx::Size size_pixels(pixel_width, pixel_height);
    // TODO: proper LocalSurfaceId allocation.
    persistent_compositor_->SetScaleAndSize(
        scale, size_pixels, viz::LocalSurfaceId());
  }
}
```

#### 6. Keep existing CALayerParams callback (for now)

The existing per-view `SetCALayerParamsCallback` mechanism may or may not fire
in `UseParentLayerCompositor` mode. For this experiment, **keep it in place** —
it's harmless if it doesn't fire. The persistent compositor's
`AcceleratedWidgetMac` will receive the `ca_context_id` via
`UpdateCALayerTree()`, but we don't have a callback registered on it yet.

**If the per-view callback stops firing** (expected — `GetLastCALayerParams()`
returns null in `UseParentLayerCompositor` mode), the GUI won't receive the
`ca_context_id` at all. In that case, as a quick fix for this experiment, add a
callback on the persistent widget:

```cpp
// After persistent_widget_mac_ creation:
// TODO: Implement AcceleratedWidgetMacNSView to receive callback.
// For now, we'll observe whether the existing per-view callback still fires.
```

This is the main unknown: how to get the `ca_context_id` out of the persistent
compositor. If the per-view callback doesn't fire, we'll need to implement
`AcceleratedWidgetMacNSView` in Experiment 2.

### Test

1. Create Chromium branch: `git checkout -b 146.0.7650.0-issue-633` from
   `146.0.7650.0-issue-631`
2. Apply all code changes above
3. Build: `autoninja -C out/Default chromium_profile_server`
4. Build GUI: `cd gui && zig build`
5. Launch: `open gui/zig-out/TermSurf.app`
6. Open `web` TUI, navigate to any page
7. Click a link — observe whether the flicker is gone
8. Check logs for:
   - "Created persistent compositor" — confirms setup
   - "Set parent_ui_layer_ on initial view" — confirms mode switch
   - "Set parent_ui_layer_ on new view" — confirms re-registration on nav
   - "Sent ca_context_id=..." — check if it fires once or per-navigation
9. Navigate multiple times — verify `ca_context_id` stays the same in logs

### Success criteria

- The `ca_context_id` in the logs is the same value across all navigations
- No visible flicker on navigation
- Page content renders correctly after navigation

### Result: FAIL

The persistent compositor is created and `SetParentUiLayer` succeeds — the
`BrowserCompositorMac` enters `UseParentLayerCompositor` mode as intended. But
the webview never appears. No `ca_context_id` is sent to the GUI.

**Root cause:** In `UseParentLayerCompositor` mode, no `RecyclableCompositorMac`
is created per view. The per-view `SetCALayerParamsCallback` calls
`GetLastCALayerParams()`, which returns null without a `recyclable_compositor_`.
The callback never fires, the GUI never receives the `ca_context_id`, and no
`CALayerHost` is created.

The `ca_context_id` now lives on the persistent compositor's
`AcceleratedWidgetMac`. But we never registered an `AcceleratedWidgetMacNSView`
on it to receive the `AcceleratedWidgetCALayerParamsUpdated()` callback.

**Fix:** Implement `AcceleratedWidgetMacNSView` on a bridge class, register it
with `persistent_widget_mac_->SetNSView(bridge)`, and extract the
`ca_context_id` from `GetCALayerParams()` in the callback. This is Experiment 2.

## Experiment 2: Implement AcceleratedWidgetMacNSView callback

### Hypothesis

Experiment 1 proved the persistent compositor works — `BrowserCompositorMac`
enters `UseParentLayerCompositor` mode. The only missing piece is a callback on
the persistent `AcceleratedWidgetMac` to extract the `ca_context_id`.

`AcceleratedWidgetMacNSView` is a one-method interface:

```cpp
class AcceleratedWidgetMacNSView {
  virtual void AcceleratedWidgetCALayerParamsUpdated() = 0;
};
```

When the GPU process renders a frame,
`AcceleratedWidgetMac::UpdateCALayerTree()` stores the `CALayerParams` and calls
`AcceleratedWidgetCALayerParamsUpdated()` on the registered NSView. The NSView
calls `widget->GetCALayerParams()` to read the `ca_context_id`.

If we implement this interface, register it on the persistent widget, and send
the `ca_context_id` via XPC, the GUI will receive a stable ID that never changes
across navigations.

### Chromium branch

`146.0.7650.0-issue-633` (continues from Experiment 1)

### Code changes

#### 1. Add PersistentCompositorBridge class

**File:**
`content/chromium_profile_server/browser/shell_compositor_bridge_mac.h`

Expand the existing header to declare the bridge class:

```cpp
#include "base/functional/callback.h"
#include "ui/accelerated_widget_mac/accelerated_widget_mac.h"
#include "ui/gfx/ca_layer_params.h"

class PersistentCompositorBridge : public ui::AcceleratedWidgetMacNSView {
 public:
  using CALayerParamsCallback =
      base::RepeatingCallback<void(const gfx::CALayerParams&)>;

  explicit PersistentCompositorBridge(ui::AcceleratedWidgetMac* widget);
  ~PersistentCompositorBridge() override;

  void SetCallback(CALayerParamsCallback callback);

  // ui::AcceleratedWidgetMacNSView:
  void AcceleratedWidgetCALayerParamsUpdated() override;

 private:
  raw_ptr<ui::AcceleratedWidgetMac> widget_;
  CALayerParamsCallback callback_;
};
```

#### 2. Implement PersistentCompositorBridge

**File:**
`content/chromium_profile_server/browser/shell_compositor_bridge_mac.mm`

Add the implementation after the existing `SetParentUiLayerOnView`:

```objcpp
PersistentCompositorBridge::PersistentCompositorBridge(
    ui::AcceleratedWidgetMac* widget)
    : widget_(widget) {
  widget_->SetNSView(this);
}

PersistentCompositorBridge::~PersistentCompositorBridge() {
  widget_->ResetNSView();
}

void PersistentCompositorBridge::SetCallback(CALayerParamsCallback callback) {
  callback_ = std::move(callback);
}

void PersistentCompositorBridge::AcceleratedWidgetCALayerParamsUpdated() {
  const auto* params = widget_->GetCALayerParams();
  if (params && callback_)
    callback_.Run(*params);
}
```

#### 3. Create bridge and wire up XPC callback in CreateTab()

**File:** `content/chromium_profile_server/browser/shell_browser_main_parts.h`

Add member:

```cpp
std::unique_ptr<PersistentCompositorBridge> persistent_bridge_;
```

Add forward declaration or include for the bridge class.

**File:** `content/chromium_profile_server/browser/shell_browser_main_parts.cc`

In `CreateTab()`, after `persistent_compositor_->SetVisible(true)`, create the
bridge and set the callback. The callback sends `ca_context_id` via XPC using
the tab connection — same pattern as the existing per-view callback, but now on
the persistent widget:

```cpp
persistent_bridge_ = std::make_unique<PersistentCompositorBridge>(
    persistent_widget_mac_.get());
```

After the tab connection is created and `tab_ready` is sent, set the callback on
the bridge:

```cpp
persistent_bridge_->SetCallback(base::BindRepeating(
    [](const std::string& pane_id, xpc_connection_t conn,
       uint32_t* last_id, const gfx::CALayerParams& params) {
      if (params.ca_context_id == 0 || params.ca_context_id == *last_id)
        return;
      *last_id = params.ca_context_id;
      xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
      xpc_dictionary_set_string(msg, "action", "ca_context");
      xpc_dictionary_set_uint64(msg, "ca_context_id", params.ca_context_id);
      xpc_dictionary_set_string(msg, "pane_id", pane_id.c_str());
      xpc_dictionary_set_uint64(msg, "pixel_width",
                                params.pixel_size.width());
      xpc_dictionary_set_uint64(msg, "pixel_height",
                                params.pixel_size.height());
      xpc_connection_send_message(conn, msg);
      xpc_release(msg);
      LOG(INFO) << "[ProfileServer] Persistent compositor ca_context_id="
                << params.ca_context_id << " for pane " << pane_id;
    },
    cb_pane_id, cb_conn, base::Owned(last_id)));
```

**Note:** The bridge callback uses the same dedup gate (`last_id`) and XPC
connection as the old per-view callback. Since the persistent bridge replaces
the per-view callback, the existing per-view `SetCALayerParamsCallback` can
remain in place harmlessly — it won't fire.

### Test

1. Build: `autoninja -C out/Default chromium_profile_server`
2. Build GUI: `cd gui && zig build`
3. Launch: `open gui/zig-out/TermSurf.app`
4. Open `web` TUI, navigate to any page
5. Check logs for "Persistent compositor ca_context_id=" — confirms callback
   fires
6. Navigate multiple times — verify `ca_context_id` is the same value every time
7. Click links — verify no flicker

### Success criteria

- The `ca_context_id` appears in logs and is sent to the GUI
- The same `ca_context_id` persists across all navigations
- Page content renders correctly
- No visible flicker on navigation

### Failure modes

- **Callback never fires:** The persistent compositor's
  `AcceleratedWidgetMac::UpdateCALayerTree()` is never called because the GPU
  process doesn't know about this compositor's `FrameSinkId`. The persistent
  compositor may need additional registration with the viz host.
- **Content renders in wrong compositor:** The per-view content renders into the
  persistent compositor's CAContext but the layer tree is wrong (blank, wrong
  size, etc.). May need `ui::Layer` configuration.
- **Crash in SetNSView:** `SetNSView` DCHECKs that view is not already set. Must
  only be called once.

### Result: PASS

The persistent compositor bridge works. The
`AcceleratedWidgetCALayerParamsUpdated` callback fires, the `ca_context_id` is
sent to the GUI via XPC, and it remains stable across navigations. No flicker on
navigation. Page content renders correctly.

The `UseParentLayerCompositor` mode with a persistent `ui::Compositor` and
`PersistentCompositorBridge` implementing `AcceleratedWidgetMacNSView` is the
correct architecture. The per-view `SetCALayerParamsCallback` is now dead code
(harmless — it never fires in this mode).

## Conclusion

Navigation flicker is eliminated. The profile server now matches Chrome's own
compositor architecture: a persistent `ui::Compositor` owns a single `CAContext`
whose `ca_context_id` never changes across navigations. The GUI creates one
`CALayerHost` at startup and never swaps it.

Two experiments got us here:

1. **Experiment 1** created the persistent compositor and switched
   `BrowserCompositorMac` to `UseParentLayerCompositor` mode via
   `SetParentUiLayer`. The mode switch worked, but the `ca_context_id` never
   reached the GUI — the per-view `SetCALayerParamsCallback` is dead in this
   mode because no `RecyclableCompositorMac` exists per view.

2. **Experiment 2** added `PersistentCompositorBridge`, a class implementing
   `AcceleratedWidgetMacNSView`. It registers with the persistent
   `AcceleratedWidgetMac` via `SetNSView` and receives
   `AcceleratedWidgetCALayerParamsUpdated` callbacks whenever the GPU process
   renders a frame. The bridge extracts the stable `ca_context_id` from
   `GetCALayerParams()` and sends it to the GUI via XPC.

The key insight from this issue: content_shell's `HasOwnCompositor` mode creates
a new `CAContext` per navigation, which is fine for a throwaway test shell but
fatal for an embedding that needs stable compositing. Chrome avoids this with
`UseParentLayerCompositor`, and now the profile server does too. The persistent
compositor outlives all navigations, and the bridge provides the callback path
that content_shell never needed.
