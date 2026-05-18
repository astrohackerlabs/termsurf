+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# 328: Blinking Text Cursor (Caret)

The blinking text cursor (caret) does not appear in webview text inputs.

## Status

**Resolved.** Focus toggle on first paint enables caret rendering.

## Problem

When a text input has focus in the webview (e.g., Google's search box
auto-focuses on load), the blinking text cursor does not appear. Users can type
and see their text, but there's no visual caret indicating the insertion point.

**Symptoms:**

- Google.com auto-focuses the search box
- User can type and text appears correctly
- No blinking cursor is ever visible
- This affects all text inputs, not just Google

**Impact:** Users have no visual feedback about cursor position, making text
editing difficult.

## Background

### ts3 Current Behavior

In `ts3/termsurf-profile/src/main.rs`, focus is set immediately after browser
creation (line 1061-1063):

```rust
// Ensure browser has focus for clipboard operations (experiment 4)
if let Some(host) = b.host() {
    println!("[FOCUS-DEBUG] Sending initial focus event to browser {}", browser_id);
    host.set_focus(1); // 1 = focused
}
```

This is called inside `create_browser_on_ui_thread`, right after
`create_browser()` returns. The browser may not be fully initialized at this
point.

### ts2 Working Behavior

ts2 handles focus differently in `ts2/wezterm-gui/src/cef_browser/mod.rs` (lines
616-625):

```rust
// Set initial focus on first paint (browser is now ready)
// We unfocus then refocus to properly initialize the focus state,
if !*self.handler.initial_focus_set.borrow() {
    if let Some(browser) = &self.handler.browser {
        if let Some(host) = browser.host() {
            log::info!("[CEF] Setting initial focus on first paint (unfocus then refocus)");
            host.set_focus(0);
            host.set_focus(1);
            *self.handler.initial_focus_set.borrow_mut() = true;
        }
    }
}
```

Key differences:

| Aspect         | ts3                                  | ts2                                        |
| -------------- | ------------------------------------ | ------------------------------------------ |
| When           | Immediately after `create_browser()` | On first `on_paint` callback               |
| How            | Single `set_focus(1)`                | Toggle: `set_focus(0)` then `set_focus(1)` |
| State tracking | None                                 | `initial_focus_set` flag                   |

### Why Timing Matters

CEF's browser initialization is asynchronous. When `create_browser()` returns,
the browser object exists but may not be fully initialized internally. The first
`on_paint` or `on_accelerated_paint` callback indicates the browser has
completed initialization and is ready to render.

Setting focus before the browser is ready may result in CEF's internal focus
state not being properly initialized, causing the caret to never appear.

### Why Toggle Matters

The ts2 comment says "unfocus then refocus to properly initialize the focus
state." This suggests CEF may have an edge case where calling `set_focus(1)` on
a browser that was never unfocused doesn't fully activate focus features like
the caret. The toggle forces CEF through both code paths.

## Proposed Solution

Modify `ts3/termsurf-profile/src/main.rs` to:

1. Add an `initial_focus_set` flag to `BrowserState`
2. Remove the `set_focus(1)` call from `create_browser_on_ui_thread`
3. In `on_accelerated_paint`, on the first paint, do the unfocus/refocus toggle

### Changes

**Add flag to BrowserState:**

```rust
pub struct BrowserState {
    // ... existing fields ...
    /// Whether initial focus has been set (must wait for first paint)
    pub initial_focus_set: AtomicBool,
}
```

**Initialize in create_browser_on_ui_thread:**

```rust
let browser_state = Arc::new(BrowserState {
    // ... existing fields ...
    initial_focus_set: AtomicBool::new(false),
});

// Remove the set_focus(1) call that's currently here
```

**Add to on_accelerated_paint in ProfileRenderHandler:**

```rust
fn on_accelerated_paint(
    &self,
    _browser: Option<&mut Browser>,
    type_: PaintElementType,
    _dirty_rects: Option<&[Rect]>,
    info: Option<&AcceleratedPaintInfo>,
) {
    // ... existing code ...

    // Set initial focus on first paint (browser is now ready)
    // Toggle unfocus/refocus to properly initialize focus state (from ts2)
    if !self.inner.state.initial_focus_set.load(Ordering::Relaxed) {
        if let Some(browser) = self.inner.state.browser.lock().unwrap().as_ref() {
            if let Some(host) = browser.host() {
                println!("[FOCUS] Setting initial focus on first paint (unfocus then refocus)");
                host.set_focus(0);
                host.set_focus(1);
                self.inner.state.initial_focus_set.store(true, Ordering::Relaxed);
            }
        }
    }

    // ... rest of existing code ...
}
```

## Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Google search box
web google.com
# Expected: Blinking caret appears in auto-focused search box

# Test 2: Type and observe caret
# Type "hello"
# Expected: Caret visible after each character

# Test 3: Click in different text field
# Navigate to a page with multiple inputs
# Click in a text field
# Expected: Caret appears at click position

# Test 4: Check logs
cat /tmp/termsurf-profile-*.log | grep "FOCUS"
# Expected: "Setting initial focus on first paint" message
```

## Success Criteria

- [ ] Caret appears in auto-focused text inputs (Google search)
- [ ] Caret appears when clicking in text fields
- [ ] Caret blinks at normal rate (~500ms)
- [ ] Caret position updates correctly when typing
- [ ] No regression in keyboard input functionality

## Experiments

### Experiment 1: Focus Toggle on First Paint

**Goal:** Make the blinking caret appear by adopting ts2's focus initialization
pattern — wait for the first paint, then toggle unfocus/refocus.

**Hypothesis:** CEF requires proper focus initialization timing. Setting focus
immediately after `create_browser()` happens before CEF is fully ready. By waiting
until `on_accelerated_paint` fires and toggling `set_focus(0)` then `set_focus(1)`,
CEF's internal focus state will be properly initialized, enabling caret rendering.

**Changes:**

1. **Add `initial_focus_set` to BrowserState** (`main.rs` line ~92)

   ```rust
   struct BrowserState {
       session_id: String,
       gui: Arc<XpcConnection>,
       width: AtomicU32,
       height: AtomicU32,
       browser: Mutex<Option<cef::Browser>>,
       url: Mutex<String>,
       /// Whether initial focus has been set (must wait for first paint)
       initial_focus_set: AtomicBool,
   }
   ```

2. **Initialize the flag** (`create_browser_on_ui_thread`, line ~1005)

   ```rust
   let browser_state = Arc::new(BrowserState {
       session_id: session_id.to_string(),
       gui: Arc::clone(&gui),
       width: std::sync::atomic::AtomicU32::new(width),
       height: std::sync::atomic::AtomicU32::new(height),
       browser: Mutex::new(None),
       url: Mutex::new(url.to_string()),
       initial_focus_set: AtomicBool::new(false),
   });
   ```

3. **Remove early set_focus call** (`create_browser_on_ui_thread`, lines ~1060-1064)

   Delete or comment out:
   ```rust
   // Ensure browser has focus for clipboard operations (experiment 4)
   if let Some(host) = b.host() {
       println!("[FOCUS-DEBUG] Sending initial focus event to browser {}", browser_id);
       host.set_focus(1); // 1 = focused
   }
   ```

4. **Add focus toggle in on_accelerated_paint** (`ProfileRenderHandler`, line ~463)

   Insert after the `PET_VIEW` check, before sending the frame:
   ```rust
   fn on_accelerated_paint(
       &self,
       _browser: Option<&mut Browser>,
       type_: PaintElementType,
       _dirty_rects: Option<&[Rect]>,
       info: Option<&AcceleratedPaintInfo>,
   ) {
       let Some(info) = info else { return };

       // Only handle PET_VIEW (skip popups)
       if type_ != PaintElementType::default() {
           return;
       }

       // Issue 328: Set initial focus on first paint (browser is now ready)
       // Toggle unfocus/refocus to properly initialize focus state (from ts2)
       if !self.inner.state.initial_focus_set.load(Ordering::Relaxed) {
           if let Some(browser) = self.inner.state.browser.lock().unwrap().as_ref() {
               if let Some(host) = browser.host() {
                   println!("[FOCUS] First paint: toggling focus (0 then 1) for caret");
                   host.set_focus(0);
                   host.set_focus(1);
                   self.inner.state.initial_focus_set.store(true, Ordering::Relaxed);
               }
           }
       }

       // ... rest of existing on_accelerated_paint code ...
   }
   ```

5. **Add import for AtomicBool** (if not already present)

   Ensure `AtomicBool` is imported from `std::sync::atomic`.

**Files to modify:**

| File | Changes |
|------|---------|
| `ts3/termsurf-profile/src/main.rs` | Add field to BrowserState, remove early set_focus, add toggle in on_accelerated_paint |

**Verification:**

```bash
# Kill any existing processes
pkill -f termsurf-profile
pkill -f termsurf-launcher

cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Caret in auto-focused input
web google.com
# Expected: Blinking caret visible in search box immediately

# Test 2: Caret after typing
# Type "hello"
# Expected: Caret visible after the "o", blinking

# Test 3: Caret after click
# Click elsewhere in the search box
# Expected: Caret moves to click position

# Test 4: Check logs
cat /tmp/termsurf-profile-*.log | grep "FOCUS"
# Expected: "[FOCUS] First paint: toggling focus (0 then 1) for caret"

# Test 5: Keyboard input still works
# Type more text, use backspace, arrow keys
# Expected: No regression in input functionality

# Test 6: Clipboard still works
# Copy text from elsewhere, Cmd+V to paste
# Expected: Paste still works (was using set_focus before clipboard ops)
```

**Success criteria:**

- [x] Caret appears in Google search box on page load
- [x] Caret blinks (~500ms interval)
- [x] Caret moves when clicking in text field
- [x] Caret follows typed characters
- [x] Keyboard input still works
- [x] Clipboard paste still works
- [x] Log shows focus toggle on first paint

**Status:** Success.

**Risks:**

1. **Clipboard operations** — Issue 317 experiment 4 added `set_focus(1)` before
   clipboard operations. This should still work since we're only removing the
   *initial* focus call, not the clipboard-related ones.

2. **Multiple browsers** — Each browser has its own `BrowserState` with its own
   `initial_focus_set` flag, so multiple webviews should work correctly.

3. **Race condition** — The `browser` mutex lock in `on_accelerated_paint` could
   theoretically race with browser creation, but since `on_accelerated_paint` only
   fires after the browser is created and stored, this should be safe.

## References

- `ts2/wezterm-gui/src/cef_browser/mod.rs` — Working focus implementation (lines
  616-625)
- `ts3/termsurf-profile/src/main.rs` — Current broken implementation (lines
  1061-1063)
- Issue 317 — Keyboard input forwarding (works, but caret missing)
