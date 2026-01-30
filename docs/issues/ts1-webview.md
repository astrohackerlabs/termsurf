# WebView Implementation (TermSurf 1.x)

> **Scope:** This document applies to TermSurf 1.x (Ghostty + WKWebView).
> TermSurf 2.0 uses CEF via cef-rs instead of WKWebView.
> See [cef-rs.md](cef-rs.md) and [termsurf2-wezterm-analysis.md](termsurf2-wezterm-analysis.md).

This document covers TermSurf's browser pane implementation, including the API
checklist, implemented features, and implementation notes.

For high-level architecture decisions (why WKWebView, comparison with CEF, etc.),
see [architecture.md](architecture.md).

---

## Implementation Checklist

Track progress on WKWebView API coverage. Check off items as they're implemented.

### WKNavigationDelegate

- [x] `decidePolicyFor:navigationAction:` - Header injection (Upgrade-Insecure-Requests)
- [x] `decidePolicyFor:navigationResponse:` - Download trigger for non-displayable content
- [x] `didStartProvisionalNavigation:` - URL change notification
- [ ] `didReceiveServerRedirectForProvisionalNavigation:` - Redirect tracking
- [ ] `didCommit:` - Content arriving
- [x] `didFinish:` - Navigation complete, focus handling
- [x] `didFail:withError:` - Error logging
- [x] `didFailProvisionalNavigation:withError:` - Error logging
- [x] `didReceiveAuthenticationChallenge:` - HTTP Basic Auth
- [x] `webContentProcessDidTerminate:` - Crash recovery
- [ ] `shouldAllowDeprecatedTLS:` - TLS 1.0/1.1 warning
- [x] `navigationAction:didBecome:` - Download handling
- [x] `navigationResponse:didBecome:` - Download handling

### WKUIDelegate

- [x] `createWebViewWith:for:windowFeatures:` - target="_blank" handling
- [ ] `webViewDidClose:` - window.close() handling
- [x] `runJavaScriptAlertPanelWithMessage:` - alert() dialogs
- [x] `runJavaScriptConfirmPanelWithMessage:` - confirm() dialogs
- [x] `runJavaScriptTextInputPanelWithPrompt:` - prompt() dialogs
- [x] `runOpenPanelWithParameters:` - File uploads
- [x] `requestMediaCapturePermissionForOrigin:` - Camera/mic access
- [ ] `requestDeviceOrientationAndMotionPermission:` - Gyroscope access
- [ ] `contextMenuConfigurationForElement:` - Context menus (macOS uses native)
- [ ] `showLockdownModeFirstUseMessage:` - Lockdown Mode warning

### WKDownloadDelegate

- [x] `download:decideDestinationUsing:` - Download destination with NSSavePanel
- [ ] `download:willPerformHTTPRedirection:` - Redirect during download
- [ ] `download:didReceiveAuthenticationChallenge:` - Auth during download
- [x] `downloadDidFinish:` - Download complete notification
- [x] `download:didFailWithError:resumeData:` - Download failure handling

### WKWebViewConfiguration

- [ ] `processPool` - Process sharing (using default)
- [x] `preferences` - developerExtrasEnabled
- [x] `userContentController` - Console capture, JS API
- [x] `websiteDataStore` - Profile isolation
- [ ] `suppressesIncrementalRendering` - Wait for full load
- [ ] `applicationNameForUserAgent` - Append to UA
- [ ] `allowsAirPlayForMediaPlayback` - AirPlay (using default)
- [ ] `upgradeKnownHostsToHTTPS` - Auto HTTPS upgrade
- [ ] `mediaTypesRequiringUserActionForPlayback` - Autoplay policy (using default)
- [ ] `defaultWebpagePreferences` - Per-page settings
- [ ] `limitsNavigationsToAppBoundDomains` - Domain restriction
- [ ] `allowsInlinePredictions` - Text predictions (using default)

### WKWebView Properties

- [x] `customUserAgent` - Safari UA string
- [ ] `allowsBackForwardNavigationGestures` - Swipe navigation
- [ ] `allowsLinkPreview` - Force Touch preview (using default)
- [ ] `isInspectable` - Web Inspector (macOS 13.3+)
- [x] `pageZoom` - User zoom control (cmd+=, cmd+-, cmd+0)
- [ ] `underPageBackgroundColor` - Bounce background

---

## Implemented Features

### Header Injection: Upgrade-Insecure-Requests

**Problem:** Some websites (notably Google) serve different HTML to WKWebView
than to Safari, even with an identical User-Agent string. This results in
simplified/mobile-style layouts, missing features, or wrong color schemes.

**Root Cause:** WKWebView doesn't send the `Upgrade-Insecure-Requests: 1` HTTP
header that Safari sends by default. Sites use this header's absence to detect
embedded webviews.

**Why No Built-in Fix?** Apple's `WKWebViewConfiguration` has no property to
enable this header. The `upgradeKnownHostsToHTTPS` option does something
different—it converts HTTP URLs to HTTPS but doesn't send the header. This is a
known limitation ([Open Radar rdar://50057283](https://openradar.appspot.com/50057283)).

**Our Solution:** Intercept navigation requests via `WKNavigationDelegate` and
inject the header:

```swift
func webView(
  _ webView: WKWebView,
  decidePolicyFor navigationAction: WKNavigationAction,
  decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
) {
  guard let url = navigationAction.request.url,
    (url.scheme == "http" || url.scheme == "https")
  else {
    decisionHandler(.allow)
    return
  }

  if navigationAction.request.value(forHTTPHeaderField: "Upgrade-Insecure-Requests") != nil {
    decisionHandler(.allow)
    return
  }

  decisionHandler(.cancel)
  var modifiedRequest = navigationAction.request
  modifiedRequest.setValue("1", forHTTPHeaderField: "Upgrade-Insecure-Requests")
  webView.load(modifiedRequest)
}
```

**Limitation:** Only works for top-level navigation, not XHR/fetch or subresources.

### User-Agent String

We set a Safari User-Agent to avoid mobile/simplified layouts:

```swift
webView.customUserAgent =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.2 Safari/605.1.15"
```

**Note:** Update the version number periodically to match current Safari.

### Console Capture

WKWebView has no native API for capturing console output. We inject JavaScript
at document start to override `console.log`, `console.error`, etc., and route
messages through `WKScriptMessageHandler`. See [console.md](console.md).

### Developer Tools

Safari Web Inspector works with WKWebView when `developerExtrasEnabled` is set:

```swift
config.preferences.setValue(true, forKey: "developerExtrasEnabled")
```

Access via cmd+alt+i in browse mode.

### Session Isolation

WKWebView supports session isolation via `WKWebsiteDataStore`:

- **Incognito:** `WKWebsiteDataStore.nonPersistent()` - no data persisted
- **Profiles:** `WKWebsiteDataStore(forIdentifier: UUID)` (macOS 14+) - isolated
  storage per profile

### target="_blank" Links

WKWebView ignores `target="_blank"` links by default. We implement
`WKUIDelegate.webView(_:createWebViewWith:for:windowFeatures:)` to load these
in the same webview. See [target-blank.md](target-blank.md).

---

## Implementation Reference

Code snippets documenting key feature implementations.

### JavaScript Dialogs

```swift
func webView(_ webView: WKWebView,
             runJavaScriptAlertPanelWithMessage message: String,
             initiatedByFrame frame: WKFrameInfo,
             completionHandler: @escaping () -> Void) {
    let alert = NSAlert()
    alert.messageText = message
    alert.addButton(withTitle: "OK")
    alert.runModal()
    completionHandler()
}

func webView(_ webView: WKWebView,
             runJavaScriptConfirmPanelWithMessage message: String,
             initiatedByFrame frame: WKFrameInfo,
             completionHandler: @escaping (Bool) -> Void) {
    let alert = NSAlert()
    alert.messageText = message
    alert.addButton(withTitle: "OK")
    alert.addButton(withTitle: "Cancel")
    completionHandler(alert.runModal() == .alertFirstButtonReturn)
}

func webView(_ webView: WKWebView,
             runJavaScriptTextInputPanelWithPrompt prompt: String,
             defaultText: String?,
             initiatedByFrame frame: WKFrameInfo,
             completionHandler: @escaping (String?) -> Void) {
    let alert = NSAlert()
    alert.messageText = prompt
    alert.addButton(withTitle: "OK")
    alert.addButton(withTitle: "Cancel")
    let textField = NSTextField(frame: NSRect(x: 0, y: 0, width: 200, height: 24))
    textField.stringValue = defaultText ?? ""
    alert.accessoryView = textField
    completionHandler(alert.runModal() == .alertFirstButtonReturn ? textField.stringValue : nil)
}
```

### File Upload

```swift
func webView(_ webView: WKWebView,
             runOpenPanelWith parameters: WKOpenPanelParameters,
             initiatedByFrame frame: WKFrameInfo,
             completionHandler: @escaping ([URL]?) -> Void) {
    let panel = NSOpenPanel()
    panel.allowsMultipleSelection = parameters.allowsMultipleSelection
    panel.canChooseDirectories = parameters.allowsDirectories
    panel.begin { response in
        completionHandler(response == .OK ? panel.urls : nil)
    }
}
```

### Downloads

**Limitation:** Cross-origin downloads (e.g., clicking a download link on site A that
points to site B) are not supported due to browser security restrictions. The
`download` attribute only works for same-origin URLs. Blob URL downloads work via
JavaScript interception.

```swift
// Add WKDownloadDelegate conformance to WebViewOverlay

// In WKNavigationDelegate - trigger download policy
func webView(_ webView: WKWebView,
             decidePolicyFor navigationResponse: WKNavigationResponse,
             decisionHandler: @escaping (WKNavigationResponsePolicy) -> Void) {
    if navigationResponse.canShowMIMEType {
        decisionHandler(.allow)
    } else {
        decisionHandler(.download)
    }
}

// Handle download creation
func webView(_ webView: WKWebView,
             navigationResponse: WKNavigationResponse,
             didBecomeDownload download: WKDownload) {
    download.delegate = self
}

// WKDownloadDelegate - choose destination
func download(_ download: WKDownload,
              decideDestinationUsing response: URLResponse,
              suggestedFilename: String,
              completionHandler: @escaping (URL?) -> Void) {
    let panel = NSSavePanel()
    panel.nameFieldStringValue = suggestedFilename
    panel.begin { response in
        completionHandler(response == .OK ? panel.url : nil)
    }
}

func downloadDidFinish(_ download: WKDownload) {
    // Notify user of completion
}

func download(_ download: WKDownload,
              didFailWithError error: Error,
              resumeData: Data?) {
    // Handle error, optionally store resumeData for retry
}
```

### Authentication Challenge

```swift
func webView(_ webView: WKWebView,
             didReceive challenge: URLAuthenticationChallenge,
             completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
    guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodHTTPBasic ||
          challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodHTTPDigest else {
        completionHandler(.performDefaultHandling, nil)
        return
    }

    let alert = NSAlert()
    alert.messageText = "Authentication Required"
    alert.informativeText = "Enter credentials for \(challenge.protectionSpace.host)"
    alert.addButton(withTitle: "Log In")
    alert.addButton(withTitle: "Cancel")

    let stackView = NSStackView(frame: NSRect(x: 0, y: 0, width: 200, height: 52))
    stackView.orientation = .vertical
    let userField = NSTextField(frame: NSRect(x: 0, y: 0, width: 200, height: 24))
    userField.placeholderString = "Username"
    let passField = NSSecureTextField(frame: NSRect(x: 0, y: 0, width: 200, height: 24))
    passField.placeholderString = "Password"
    stackView.addArrangedSubview(userField)
    stackView.addArrangedSubview(passField)
    alert.accessoryView = stackView

    if alert.runModal() == .alertFirstButtonReturn {
        let credential = URLCredential(user: userField.stringValue,
                                       password: passField.stringValue,
                                       persistence: .forSession)
        completionHandler(.useCredential, credential)
    } else {
        completionHandler(.cancelAuthenticationChallenge, nil)
    }
}
```

### Process Crash Recovery

```swift
func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
    let alert = NSAlert()
    alert.messageText = "Web Content Crashed"
    alert.informativeText = "The webpage has crashed. Would you like to reload?"
    alert.addButton(withTitle: "Reload")
    alert.addButton(withTitle: "Close")

    if alert.runModal() == .alertFirstButtonReturn {
        webView.reload()
    } else {
        onClose?(webviewId)
    }
}
```

---

## References

- [WKNavigationDelegate](https://developer.apple.com/documentation/webkit/wknavigationdelegate)
- [WKUIDelegate](https://developer.apple.com/documentation/webkit/wkuidelegate)
- [WKDownloadDelegate](https://developer.apple.com/documentation/webkit/wkdownloaddelegate)
- [WKWebViewConfiguration](https://developer.apple.com/documentation/webkit/wkwebviewconfiguration)
- [WebKit Source Headers](https://github.com/WebKit/webkit/tree/main/Source/WebKit/UIProcess/API/Cocoa)
- [The Ultimate Guide to WKWebView](https://www.hackingwithswift.com/articles/112/the-ultimate-guide-to-wkwebview)
- [Open Radar: Custom Headers](https://openradar.appspot.com/50057283)
- [W3C Upgrade Insecure Requests](https://www.w3.org/TR/upgrade-insecure-requests/)
