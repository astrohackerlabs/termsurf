# TODO

## Web features

- [x] Loading progress bar (indeterminate pulse via OSC 9;4)
- [x] Browser navigation keybindings (Cmd+[/]/R for back/forward/reload)
- [x] Context menu removal (Content Shell menu didn't fit architecture)
- [x] Fix Ctrl+Esc mode switching (dangling pointer in focused_pane)
- [ ] target="\_blank" handling (OAuth, "open in new tab" links fail)
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

## Future issues

Problems identified but not yet started. Each becomes its own issue doc when
ready.

- [ ] Renderer crash UX — When the Chromium renderer process dies, the user sees
      a blank white screen with no indication of what happened. The progress bar
      continues as if the page is still loading, then times out. Need to detect
      renderer termination, display an error page, clear the progress bar, and
      show what went wrong. Discovered in Issue 655 Experiment 1.
- [x] Wezboard overlay NSView — Promoted to Issue 725.
- [ ] Mojo interface audit — The Content API build is missing handlers for Mojo
      interfaces that a full Chrome browser registers. Every missing binder is a
      ticking time bomb — the renderer crashes when any page's JavaScript calls
      that API. Fixed `blink.mojom.BadgeService` in Issue 655, but there are
      likely many more. Need to systematically review all Mojo interfaces.
      Discovered in Issue 655 Experiment 1.

## 1.0 Milestone

- [ ] Linux, macOS and Windows
- [ ] Chromium, Webkit, Ladybird, and Gecko
- [ ] Ghostty, Wezterm, Kitty, Alacritty, iTerm2
- [ ] All elementary web features (Downloads, Bookmarks, Passwords, etc.)
- [ ] Multi-tab in one pane
- [ ] Scrollback webviews
- [ ] Partially hidden webviews
- [ ] Miniaturized webviews
- [ ] Documentation web page
- [ ] Whitepaper
- [ ] Hosted passwords and bookmarks
- [ ] Remote terminals, i.e. ssh
- [ ] browser.nvim
- [ ] Run VSCode inside TermSurf

## 2.0 Milestone

- [ ] iOS, Android
