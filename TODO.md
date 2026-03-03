# TODO

## Web features

- [x] Loading progress bar (indeterminate pulse via OSC 9;4)
- [x] Browser navigation keybindings (Cmd+[/]/R for back/forward/reload)
- [x] Context menu removal (Content Shell menu didn't fit architecture)
- [x] Fix Ctrl+Esc mode switching (dangling pointer in focused_pane)
- [ ] target="_blank" handling (OAuth, "open in new tab" links fail)
- [ ] Drag-n-drop file uploads
- [ ] JavaScript dialogs (alert/confirm/prompt)
- [ ] Downloads
- [ ] File uploads (`<input type="file">`)
- [ ] Page zoom (Cmd+=/-/0)
- [ ] HTTP Basic Auth
- [x] URL normalization (prepend https:// when omitted)
- [ ] Crash recovery (handle Chromium renderer crashes gracefully)
- [ ] Camera/mic permissions
- [ ] Console capture (JS console → terminal output)
- [x] Web Inspector / DevTools
- [ ] Session isolation / incognito mode
- [ ] Bookmarking
- [ ] JavaScript API (window.termsurf)
- [ ] Hide/show webviews (ctrl+z/fg)
- [ ] Multi-webview stacking (multiple webviews per pane)
- [x] Dynamic tab titles (page title → terminal tab)
