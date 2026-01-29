# Handling target="_blank" Links (TermSurf 1.x)

> **Scope:** This document applies to TermSurf 1.x (Ghostty + WKWebView).
> TermSurf 2.0 will handle this via CEF's native popup handling.

This document describes how TermSurf handles links that request to open in a new
window/tab (i.e., `target="_blank"` or `window.open()`).

## Current Behavior (v0.1.x)

Links that would normally open in a new window are loaded in the **same
webview**, replacing the current page. This is a deliberate simplification.

### Implementation

In `WebViewOverlay.swift`, we implement `WKUIDelegate`:

```swift
func webView(
  _ webView: WKWebView,
  createWebViewWith configuration: WKWebViewConfiguration,
  for navigationAction: WKNavigationAction,
  windowFeatures: WKWindowFeatures
) -> WKWebView? {
  if navigationAction.targetFrame == nil {
    webView.load(navigationAction.request)
  }
  return nil
}
```

When `targetFrame` is `nil`, it means the link requested a new window. We load
it in the current webview and return `nil` (declining to create a new webview).

### Why This Approach?

1. **Simplicity** - ~15 lines of code, no cross-component coordination
2. **Works for 90% of cases** - Most `target="_blank"` links are just regular
   navigation
3. **No security concerns** - We're not constructing shell commands from
   untrusted URLs

### Limitations

- User loses their place on the original page
- OAuth flows that rely on popups may not work correctly
- Sites that expect the original page to remain open will behave unexpectedly

## Future: Open in New Tab (Planned)

The ideal behavior is to open `target="_blank"` links in a new terminal tab,
running `web open <url>`. This matches browser behavior and preserves the
original page.

### Technical Requirements

1. **Callback chain** - WebViewOverlay needs to signal up through WebViewContainer
   to reach TerminalController or Ghostty.App
2. **URL escaping** - URLs from untrusted web content must be properly escaped
   before being passed to a shell command
3. **URL validation** - Filter out `javascript:`, `blob:`, and other non-http(s)
   URLs
4. **Path resolution** - The `web` binary must be locatable (PATH or absolute)

### Proposed Implementation

```swift
// In WebViewOverlay
var onNewWindowRequested: ((URL) -> Void)?

func webView(...) -> WKWebView? {
  if navigationAction.targetFrame == nil,
     let url = navigationAction.request.url {
    // Validate URL scheme
    guard ["http", "https"].contains(url.scheme?.lowercased()) else {
      return nil
    }

    if let handler = onNewWindowRequested {
      handler(url)
    } else {
      // Fallback: load in same webview
      webView.load(navigationAction.request)
    }
  }
  return nil
}
```

```swift
// In WebViewContainer or WebViewManager
webViewOverlay.onNewWindowRequested = { [weak self] url in
  // Signal to TerminalController to open new tab with command
  NotificationCenter.default.post(
    name: .termsurfOpenURLInNewTab,
    object: nil,
    userInfo: ["url": url]
  )
}
```

```swift
// In TerminalController or AppDelegate
@objc func handleOpenURLInNewTab(_ notification: Notification) {
  guard let url = notification.userInfo?["url"] as? URL else { return }

  // Use SurfaceConfiguration to run command in new tab
  var config = Ghostty.SurfaceConfiguration()
  config.command = "web open '\(url.absoluteString.escapedForShell)'"

  _ = TerminalController.newTab(ghostty, withBaseConfig: config)
}
```

### Security Considerations

**URL escaping is critical.** A malicious website could craft a URL like:
```
https://example.com'; rm -rf /; echo '
```

The URL must be properly escaped before being interpolated into a shell command.
Options:

1. **Single-quote escaping** - Replace `'` with `'\''` and wrap in single quotes
2. **Percent-encoding** - Pass URL as-is (already percent-encoded by WebKit)
3. **Argument passing** - Avoid shell entirely, pass URL as argument array

Option 3 is safest but requires changes to how new tabs are spawned.

### Edge Cases to Handle

- `javascript:` URLs - Block entirely
- `blob:` URLs - Block (not meaningful outside original page)
- `data:` URLs - Consider blocking or handling specially
- `file:` URLs - May need special handling for local development
- Auth popups that close themselves - Need `window.close()` support

## References

- [WKUIDelegate - Apple Developer](https://developer.apple.com/documentation/webkit/wkuidelegate)
- [How to open target="_blank" links in WKWebView](https://nemecek.be/blog/1/how-to-open-target_blank-links-in-wkwebview-in-ios)
