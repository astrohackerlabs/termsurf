# Terminal-Browser Hybrids

A comparison of terminals with embedded browser capabilities.

> **Note:** TermSurf 1.x uses WKWebView (WebKit). TermSurf 2.0 (in development)
> will use CEF (Chromium) via cef-rs, bringing it closer to Wave Terminal's
> approach while maintaining CLI-first philosophy.

## Competitors

### iTerm2 Browser Plugin

- **Website:** https://iterm2.com/documentation-web.html
- **Approach:** UI-focused, profile-based. You create a "Web Browser" profile
  type, and it opens as a session in iTerm2's pane hierarchy.
- **Technology:** WKWebView
- **Invocation:** Create profile in Settings, then open as a new tab/pane via UI
- **Features:** Copy mode, triggers, AI chat integration, reader mode, password
  manager support (1Password, LastPass), instant replay, broadcast input
- **Limitations:** No passkeys, limited ad blocking (Apple restrictions)

### Wave Terminal

- **Website:** https://www.waveterm.dev/
- **Source:** https://github.com/wavetermdev/waveterm
- **Approach:** Block-based workspace. Browser is one "block" type alongside
  terminal, file preview, and AI chat blocks.
- **Technology:** Electron with Chromium
- **Invocation:** GUI-driven, drag/drop blocks to arrange workspace
- **Features:** Inline browser alongside terminals, AI integration, can pipe
  browser console to AI
- **Cross-platform:** macOS, Linux, Windows

### Brow6el

- **Source:** https://codeberg.org/janantos/brow6el
- **Article:** https://www.theregister.com/2026/01/02/brow6el_browser_terminal
- **Approach:** Renders web pages AS terminal graphics using Sixel
- **Technology:** CEF (Chromium) renders headless, libsixel converts to terminal
  graphics
- **Invocation:** Command-line (`brow6el <url>`)
- **Features:** Vim-style modal control, mouse support, JS console, bookmarks,
  ad blocker
- **Limitation:** Requires Sixel-capable terminal (mlterm, foot, wezterm, etc.),
  POC quality code
- **Released:** January 2026

### DomTerm

- **Website:** https://domterm.org/
- **Approach:** Terminal emulator built ON web technologies (inverted model)
- **Technology:** JavaScript terminal running in Electron/Qt/browser
- **Features:** Rich HTML output in terminal, inline images, pretty-printing,
  session management
- **Note:** Not really a competitor - it's a terminal that happens to use a
  browser engine, not a terminal with browser panes

## Comparison

| Aspect             | TermSurf 1.x        | TermSurf 2.0 (planned) | iTerm2           | Wave            | Brow6el          |
| ------------------ | ------------------- | ---------------------- | ---------------- | --------------- | ---------------- |
| **Invocation**     | CLI (`web open`)    | CLI (`web open`)       | UI profile       | GUI blocks      | CLI              |
| **Philosophy**     | Terminal-first      | Terminal-first         | UI-first         | Workspace-first | Terminal-graphics |
| **Browser engine** | WKWebView           | CEF (Chromium)         | WKWebView        | Chromium        | CEF + Sixel      |
| **Platforms**      | macOS only          | macOS, Linux, Windows  | macOS            | Cross-platform  | Linux            |
| **Integration**    | Overlay on pane     | Overlay on pane        | Separate session | Separate block  | Replaces output  |
| **Console bridge** | stdout/stderr       | stdout/stderr          | N/A              | N/A             | JS console only  |
| **Blocking CLI**   | Yes (like `vim`)    | Yes (like `vim`)       | No               | No              | Yes              |
| **Pane navigation**| ctrl+h/j/k/l        | TBD                    | iTerm2 shortcuts | Drag/drop       | N/A              |

## TermSurf's Unique Position

TermSurf is the only terminal that treats the browser as a command-line tool:

1. **CLI-invoked:** `web open google.com` blocks like `vim` or `less`
2. **Console bridging:** `console.log()` goes to stdout, `console.error()` goes
   to stderr
3. **Exit codes:** JavaScript can call `window.termsurf.exit(0)` to return to
   shell with an exit code
4. **Overlay model:** Browser appears on top of terminal pane, terminal
   continues underneath
5. **Modal keyboard:** Control/Browse/Insert modes with vim-like switching

The closest competitor is Brow6el (also CLI-invoked), but it renders pages as
terminal graphics rather than a native browser overlay. iTerm2 and Wave are
fundamentally GUI-driven - you click to open a browser, not type a command.
