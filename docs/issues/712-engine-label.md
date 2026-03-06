# Issue 712: Show browser engine name in viewport border

## Goal

Display the browser engine name (e.g., "roamium") in the bottom-left corner of
the viewport border so the user always knows which engine is rendering the
current page.

## Background

TermSurf supports multiple browser engines — Roamium (Chromium), Surfari
(WebKit), Waterwolf (Gecko), Girlbat (Ladybird). The `--browser` flag on the
`web` TUI selects which engine to use. Currently there is no visual indicator of
which engine is active. As multi-engine support matures, users need to see at a
glance which engine is rendering their page.

The viewport border already shows contextual information:

- **Top-left:** Page title (or "Viewport" / "DevTools · profile/tab_id")
- **Top-right:** Profile name with user icon

The bottom border is currently empty. Adding the engine name there follows the
existing pattern of putting metadata in the border title areas.

## Analysis

### Current message flow

The TUI and Ghostboard already exchange the browser engine name:

1. **Hello** — On startup, the TUI sends `HelloRequest`. Ghostboard replies with
   `HelloReply` containing `homepage` and `repeated string browsers` — the list
   of registered engine names from the browser registry (e.g., `["roamium"]`).
   The TUI currently **ignores the `browsers` field** (`ipc.rs:124-129`), only
   extracting the homepage.

2. **SetOverlay** — The TUI sends `browser` to Ghostboard in every
   `send_set_overlay` and `send_set_devtools_overlay` call (`main.rs:415,403`).
   Ghostboard reads this and passes it to `getOrCreateServer` to launch the
   correct engine process.

3. **Default resolution** — When `--browser` isn't specified, the TUI sets
   `browser = ""` (`main.rs:209`). Ghostboard silently defaults `""` to
   `"roamium"` in `getOrCreateServer` (`xpc.zig:892`) and `resolveBrowserPath`
   (`xpc.zig:884`). The TUI never learns the resolved name.

### Problem

The TUI should be the one choosing which browser to use, not silently delegating
the choice to Ghostboard. The `browsers` list in `HelloReply` exists precisely
for this — the TUI receives the available engines, picks one (the first entry as
default when `--browser` isn't specified), and sends an explicit name in
`SetOverlay`. This way:

- The TUI always knows which engine is active (for display)
- The TUI sends an explicit engine name (not `""`) to Ghostboard
- Ghostboard doesn't need implicit default logic — it uses what the TUI says

### Display

For display in the viewport border:

- Named engines ("roamium", "chromium", etc.) display as-is
- Absolute paths (from `--browser /path/to/binary`) display just the binary name
  (last path component)
- The label is always present — there is no case where it should be hidden

The label should appear in the bottom-left of the viewport block, styled in the
comment color (dimmed) to avoid competing with the page title and mode border
colors.

## Experiments

### Experiment 1: Read browsers from hello, display in viewport

#### Changes

**1. `webtui/src/ipc.rs` — `send_hello` returns browsers list:**

- Change return type from `Option<String>` to `Option<(String, Vec<String>)>` —
  `(homepage, browsers)`
- Extract `r.browsers` from the `HelloReply` alongside `r.homepage`

**2. `webtui/src/main.rs` — use hello browsers for default:**

- After the hello call (~line 263), extract the browsers list
- If `browser` is still empty (no `--browser` flag) and the browsers list is
  non-empty, set `browser` to the first entry (e.g., `"roamium"`)
- For absolute paths in `browser`, extract the binary name for display:
  `let browser_label = browser.rsplit('/').next().unwrap_or(&browser)`

**3. `webtui/src/main.rs` — pass browser label to `ui()`:**

- Add `browser_label: &str` parameter to the `ui()` function
- Pass it from the call site in the event loop

**4. `webtui/src/main.rs` — render engine label in viewport border:**

- In `ui()`, add a `title_bottom` to the viewport block (bottom-left):
  ```rust
  let engine_label = Line::from(vec![
      Span::raw("\u{F268} ").style(Style::default().fg(COMMENT)),
      Span::raw(browser_label).style(Style::default().fg(DIM)),
  ]);
  ```
- Add `.title_bottom(engine_label)` to the viewport block

#### Verification

1. `cd webtui && cargo build` — compiles clean
2. Launch without `--browser` — viewport bottom-left shows "roamium"
3. Launch with `--browser roamium` — same result
4. Launch with `--browser /full/path/to/roamium` — shows "roamium" (last
   component)
