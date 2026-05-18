+++
status = "closed"
opened = "2026-02-27"
closed = "2026-02-27"
+++

# Issue 655: Substack pages go blank after initial render

## Goal

Substack blog posts should render and remain visible, just like they do in
regular Chrome. Currently, the page renders correctly for about one second, then
switches to a blank white screen.

## Background

Pages that reproduce the issue:

- `https://themasonic.substack.com/p/the-investigation`
- `https://kirschsubstack.com/`

These pages load and render correctly in regular Chrome. In TermSurf's Chromium
browser, the page appears for roughly one second — text, images, layout all look
correct — then the entire viewport goes white. The page content is gone.

Confirmed across multiple Substack blogs, so this is a Substack-wide issue —
their shared frontend codebase triggers the problem.

### What we know

- The initial HTML/CSS renders correctly — the page looks fine for ~1 second.
- JavaScript executes after the initial paint and triggers the blank state.
- Regular Chrome renders the same page without issues.
- The issue is specific to TermSurf's Chromium setup (Content API fork, out-of-
  process via XPC, CALayerHost compositing).

### Substack's behavior in regular Chrome

Substack shows a subscription overlay/modal on blog posts for non-subscribers.
This overlay has a semi-transparent backdrop and a signup form. In regular
Chrome, the overlay appears on top of the content and can be dismissed. The
content remains visible behind the overlay.

### Possible causes

1. **Overlay renders as opaque white.** Substack's subscription modal uses a
   backdrop that covers the page. If our Chromium build is missing something the
   overlay needs (a web component, CSS feature, font), the backdrop could render
   as solid white without the dismiss UI, hiding all content underneath.

2. **`<dialog>` element or `backdrop-filter` CSS.** Substack may use the HTML
   `<dialog>` element or CSS `backdrop-filter` for the overlay. If the Content
   API build doesn't fully support these, the overlay could render as an opaque
   white layer.

3. **Service Worker failure.** Substack uses service workers. If registration
   fails in our environment, the page might redirect or blank itself as a
   fallback.

4. **JavaScript feature detection.** Substack's JS might detect a missing
   browser API (e.g., Notification, Push, Payment) and enter an error code path
   that blanks the page.

5. **Navigation or redirect.** The JavaScript might trigger a client-side
   navigation (e.g., to a login page or error page) that our Chromium setup
   doesn't handle correctly, resulting in a blank page. This would be related to
   the navigation issues investigated in Issues 628–632.

### Diagnostic ideas

- Check the Chromium server logs for errors or warnings when the page goes
  blank.
- If DevTools are available, inspect the DOM after the page goes white — is the
  content still in the DOM but hidden by CSS, or has the DOM changed entirely?
- Try loading the page with JavaScript disabled to confirm JS is the trigger.
- Try other Substack blogs to confirm the issue is Substack-wide. **Done** —
  confirmed on `kirschsubstack.com`.
- Compare the user agent string — Substack might serve different content based
  on the UA.

## Experiments

### Experiment 1: Diagnose the blank screen

**Goal:** Determine what causes Substack pages to go white after the initial
render. Narrow down whether it's JavaScript, a navigation/redirect, or a
rendering issue.

#### Diagnostic steps

1. **Check the Chromium server log.** The server logs to
   `~/.local/state/termsurf/chromium-server.log` (set via `--enable-logging` and
   `--log-file` in `xpc.zig:806-839`). Clear the log, load the Substack page,
   wait for it to go blank, then search the log for errors, warnings, navigation
   events, or JavaScript exceptions. Look for:
   - `ERR` or `ERROR` lines
   - `CONSOLE` messages (JavaScript `console.error`)
   - Navigation-related entries (`DidFinishNavigation`, `DidStartNavigation`)
   - Any mention of `dialog`, `backdrop`, `overlay`, or `service-worker`

2. **Test with JavaScript disabled.** Temporarily add `--disable-javascript` to
   the Chromium server's command-line args in `xpc.zig:844`:

   ```zig
   var child = std.process.Child.init(
       &.{ server_path, xpc_arg, data_arg, hidden_arg, logging_arg, logfile_arg, "--disable-javascript" },
       alloc,
   );
   ```

   Rebuild (`cd gui && zig build`), launch the debug app, and load the Substack
   page. If the page stays visible, JavaScript is confirmed as the trigger. If
   it still goes blank, the issue is in CSS or the rendering pipeline.

   **Revert this change after testing.**

3. **Check the user agent.** Our Chromium fork may send a non-standard user
   agent that causes Substack to serve different content. Check what UA the
   server sends by loading a UA-echo page like `https://httpbin.org/user-agent`
   in TermSurf and comparing it to regular Chrome.

4. **Check for client-side navigation.** The Chromium server log (step 1) should
   show navigation events. If there's a second navigation after the initial page
   load (e.g., a redirect to a login page or error page), that would explain the
   blank — especially if it hits a navigation bug from Issues 628–632.

5. **Test a non-Substack page with overlays.** Load a page known to show modal
   overlays (e.g., a Medium paywall article, a cookie consent banner) to see if
   the issue is Substack-specific or affects all overlay-heavy pages.

#### Expected outcome

Steps 1–2 will narrow the cause to either JavaScript or rendering. Step 3 will
rule out UA-based content differences. Step 4 will reveal if a redirect is
involved. The findings will inform what to fix in the next experiment.

**Result: Pass.** Root cause identified in step 1.

#### Findings

Step 1 (Chromium server log) immediately revealed the cause. Two lines at the
moment the page goes white:

```
Terminating render process for bad Mojo message: Received bad user message:
No binder found for interface blink.mojom.BadgeService for the frame/document scope

Terminating renderer for bad IPC message, reason 123
```

The **renderer process is being killed by Chromium's IPC security layer**.
Substack's JavaScript calls the Badging API (`navigator.setAppBadge()` /
`navigator.clearAppBadge()`), which sends a Mojo message to
`blink.mojom.BadgeService`. Our Content API build doesn't register a handler for
this interface. Chromium treats an unbound Mojo interface as a security
violation ("bad message") and terminates the renderer process. The page goes
white because the renderer is dead.

The timeline in the log confirms this:

1. Navigation to the Substack page commits (line 23)
2. Page title sets to "The Investigation - The Masonic" (line 24)
3. Substack's JS runs, including a recruitment banner in `console.log` (line 25)
4. A second navigation commit (line 55) — likely a client-side history push
5. **Renderer killed** for `BadgeService` bad message (lines 56–57)
6. Key view becomes null (line 58) — the renderer is gone

The progress bar staying active is explained: the page never finishes loading
because the renderer is killed mid-load.

Steps 2–5 were not needed — step 1 found the root cause.

#### Root cause

Our Chromium Content API build is missing a Mojo interface binder for
`blink.mojom.BadgeService`. When any page's JavaScript invokes the Badging API,
the renderer process is terminated. This is not Substack-specific — any page
using the Badging API would trigger the same crash. Substack happens to use it
for notification badge counts on their PWA.

#### Conclusion

This experiment exposed three separate problems:

1. **Missing Mojo interface binder (immediate fix).** Our Content API build
   doesn't register a handler for `blink.mojom.BadgeService`. When Substack's JS
   calls the Badging API, Chromium kills the renderer. The immediate fix is to
   register a stub/no-op handler so the renderer survives. But this is almost
   certainly not the only missing interface — any unhandled Mojo interface will
   cause the same fatal crash.

2. **Systematic Mojo interface audit (future work).** We need to review ALL Mojo
   interfaces that a full Chrome browser registers but our Content API build
   does not. Every missing binder is a ticking time bomb — the renderer will
   crash the moment any page's JavaScript calls that API. Rather than fixing
   these one-by-one as users discover them, we should do a systematic audit and
   register stub handlers for all unbound interfaces. This is a separate issue.

3. **Renderer crash UX (future work).** When the renderer dies, the user sees a
   blank white screen with no indication of what happened. The progress bar
   continues as if the page is still loading, then times out. We need to:
   - Detect renderer termination and display an error page (like Chrome's "Aw,
     Snap!" page).
   - Clear the progress bar immediately when the renderer dies.
   - Ideally show what went wrong so the user (or a developer) can diagnose the
     issue. This is also a separate issue.

#### Next step

Fix the immediate problem: register a stub handler for
`blink.mojom.BadgeService` so Substack pages render without killing the
renderer.

### Experiment 2: Register a no-op BadgeService binder

**Goal:** Register a stub handler for `blink.mojom.BadgeService` so the renderer
is not killed when a page calls the Badging API. Substack pages should render
and stay visible.

#### Background

Chromium's headless browser has the exact same problem and solves it with a
`StubBadgeService` class — a minimal implementation of
`blink::mojom::BadgeService` that accepts connections and ignores all calls.
We'll copy this pattern.

The relevant code lives in:

- `headless/lib/browser/headless_content_browser_client.h:138-158` —
  `StubBadgeService` inner class definition
- `headless/lib/browser/headless_content_browser_client.cc:210-218` —
  registration in `RegisterBrowserInterfaceBindersForFrame()`
- `headless/lib/browser/headless_content_browser_client.cc:446-453` —
  `BindBadgeService()` implementation

Our Content API build registers frame-scoped binders in
`content/chromium_profile_server/browser/shell_content_browser_client.cc:604-615`.

#### Chromium branch

- New branch: `146.0.7650.0-issue-655`
- Forked from: `146.0.7650.0-issue-644` (latest branch with all TermSurf
  modifications)

#### Changes

**1. `content/chromium_profile_server/browser/shell_content_browser_client.h`**
— Add the `StubBadgeService` inner class, `BindBadgeService()` method
declaration, and member variable:

```cpp
// After the existing public method declarations, add:
 private:
  // Stub BadgeService that accepts and ignores Badging API calls.
  // Without this, the renderer is killed when any page calls
  // navigator.setAppBadge() — an unbound Mojo interface is treated
  // as a security violation. See Issue 655.
  class StubBadgeService;

  void BindBadgeService(
      RenderFrameHost* render_frame_host,
      mojo::PendingReceiver<blink::mojom::BadgeService> receiver);

  std::unique_ptr<StubBadgeService> stub_badge_service_;
```

Add the include at the top of the file:

```cpp
#include "third_party/blink/public/mojom/badging/badging.mojom.h"
```

**2. `content/chromium_profile_server/browser/shell_content_browser_client.cc`**
— Define the `StubBadgeService` class and register the binder:

```cpp
// Add includes at the top:
#include "mojo/public/cpp/bindings/receiver_set.h"
#include "third_party/blink/public/mojom/badging/badging.mojom.h"

// Define StubBadgeService (before any method definitions):
class ShellContentBrowserClient::StubBadgeService
    : public blink::mojom::BadgeService {
 public:
  StubBadgeService() = default;
  ~StubBadgeService() override = default;

  void Bind(mojo::PendingReceiver<blink::mojom::BadgeService> receiver) {
    receivers_.Add(this, std::move(receiver));
  }

  // blink::mojom::BadgeService:
  void SetBadge(blink::mojom::BadgeValuePtr value) override {}
  void ClearBadge() override {}

 private:
  mojo::ReceiverSet<blink::mojom::BadgeService> receivers_;
};

// Add BindBadgeService method:
void ShellContentBrowserClient::BindBadgeService(
    RenderFrameHost* render_frame_host,
    mojo::PendingReceiver<blink::mojom::BadgeService> receiver) {
  if (!stub_badge_service_)
    stub_badge_service_ = std::make_unique<StubBadgeService>();
  stub_badge_service_->Bind(std::move(receiver));
}

// In RegisterBrowserInterfaceBindersForFrame(), add:
  map->Add<blink::mojom::BadgeService>(base::BindRepeating(
      &ShellContentBrowserClient::BindBadgeService, base::Unretained(this)));
```

#### Verification

1. Create branch `146.0.7650.0-issue-655` from `146.0.7650.0-issue-644`
2. Make the changes above
3. Build: `autoninja -C out/Default chromium_profile_server`
4. Rebuild TermSurf: `cd gui && zig build`
5. Launch `TermSurf-Debug.app`
6. Navigate to `https://themasonic.substack.com/p/the-investigation`
7. The page should render and **stay visible** — no blank white screen
8. Check `~/.local/state/termsurf/chromium-server.log` — no "Terminating render
   process" or "bad Mojo message" errors
9. Test `https://kirschsubstack.com/` — should also render without crashing

**Result: Pass.**

Substack pages render and stay visible. The `StubBadgeService` accepts and
silently ignores Badging API calls, so the renderer is no longer killed.

## Conclusion

Substack pages went blank because Chromium killed the renderer process. The
cause was a missing Mojo interface binder for `blink.mojom.BadgeService` —
Substack's JavaScript calls `navigator.setAppBadge()`, and our Content API build
had no handler registered. Chromium treats an unbound Mojo interface as a
security violation and terminates the renderer.

The fix is a `StubBadgeService` class that implements the `BadgeService`
interface with empty `SetBadge()` and `ClearBadge()` methods. It accepts
connections and does nothing — we don't need badge functionality, we just need
the renderer to survive the call.

Changes are on Chromium branch `146.0.7650.0-issue-655`, in two files:

- `shell_content_browser_client.h` — `StubBadgeService` declaration,
  `BindBadgeService()` method, member variable
- `shell_content_browser_client.cc` — `StubBadgeService` definition, binder
  registration in `RegisterBrowserInterfaceBindersForFrame()`
