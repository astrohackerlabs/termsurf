# Issue 718: Finish `cocoa` and `objc` 0.2 removal from wezboard

## Goal

Complete the migration started in Issue 717. Remove all remaining `cocoa` and
`objc` 0.2 usage from the wezboard codebase, then delete both dependencies.

## Background

Issue 717 migrated 5 of 7 macOS files to pure objc2 and partially migrated
`window.rs` (class registration, callback signatures, geometry types). Two
categories of work remain:

1. `window.rs` still uses `cocoa` trait methods (~50 call sites) and `objc` 0.2
   types (`StrongPtr`, `WeakPtr`, `Object`, `BOOL`/`YES`/`NO`).
2. `wezboard-font/src/locator/core_text.rs` imports `cocoa::base::id`.

Once both are clean, the `cocoa` and `objc` crate dependencies can be deleted.

## Scope

### Files to migrate

| File                                     | Remaining `cocoa` usage                                         | Remaining `objc` 0.2 usage                          |
| ---------------------------------------- | --------------------------------------------------------------- | --------------------------------------------------- |
| `window/src/os/macos/window.rs`          | ~50 trait method calls (NSWindow, NSView, NSEvent, NSScreen...) | `StrongPtr`, `WeakPtr`, `Object`, `BOOL`/`YES`/`NO` |
| `wezboard-font/src/locator/core_text.rs` | `cocoa::base::id`                                               | —                                                   |

### Current `cocoa` imports in `window.rs`

```rust
use cocoa::appkit::{
    self, NSApplication, NSApplicationActivateIgnoringOtherApps,
    NSApplicationPresentationOptions, NSBackingStoreBuffered, NSEvent,
    NSEventModifierFlags, NSOpenGLContext, NSOpenGLPixelFormat, NSPasteboard,
    NSRunningApplication, NSScreen, NSView, NSViewHeightSizable,
    NSViewWidthSizable, NSWindow, NSWindowStyleMask,
};
use cocoa::base::*;
use cocoa::foundation::{
    NSArray, NSAutoreleasePool, NSFastEnumeration, NSPoint, NSRect, NSSize,
    NSString,
};
```

### Current `objc` 0.2 imports in `window.rs`

```rust
use objc::rc::{StrongPtr, WeakPtr};
use objc::runtime::{Object, BOOL, NO, YES};
```

### What gets removed when done

- `cocoa` dependency from `window/Cargo.toml` and `wezboard-font/Cargo.toml`
- `objc` dependency from `window/Cargo.toml`
- `cocoa` and `objc` from workspace `[workspace.dependencies]` in root
  `Cargo.toml`
- All `as id` casts and `id` type usage
- All `StrongPtr`/`WeakPtr` usage
- All callback bridge casts (`let this = &mut *(this as *mut Object)`)
- Remaining `cocoa::foundation` geometry types (`NSPoint`, `NSRect`, `NSSize`)
- `cocoa::base::*` wildcard import (`id`, `nil`, `BOOL`, `YES`, `NO`)

### What replaces it

| `cocoa` / `objc` 0.2                              | `objc2` equivalent                                      |
| ------------------------------------------------- | ------------------------------------------------------- |
| `cocoa::appkit::NSWindow` trait methods           | `objc2_app_kit::NSWindow` methods                       |
| `cocoa::appkit::NSView` trait methods             | `objc2_app_kit::NSView` methods                         |
| `cocoa::appkit::NSScreen` trait methods           | `objc2_app_kit::NSScreen` methods                       |
| `cocoa::appkit::NSEvent` trait methods            | `objc2_app_kit::NSEvent` methods                        |
| `cocoa::appkit::NSApplication` trait methods      | `objc2_app_kit::NSApplication` methods                  |
| `cocoa::appkit::NSOpenGLContext` trait methods    | `objc2_app_kit::NSOpenGLContext` methods or `msg_send!` |
| `cocoa::appkit::NSPasteboard` trait methods       | `objc2_app_kit::NSPasteboard` methods                   |
| `cocoa::appkit::NSWindowStyleMask`                | `objc2_app_kit::NSWindowStyleMask`                      |
| `cocoa::appkit::NSEventModifierFlags`             | `objc2_app_kit::NSEventModifierFlags`                   |
| `cocoa::appkit::NSApplicationPresentationOptions` | `objc2_app_kit::NSApplicationPresentationOptions`       |
| `cocoa::appkit::NSBackingStoreBuffered`           | `objc2_app_kit::NSBackingStoreType::Buffered`           |
| `cocoa::foundation::NSPoint`                      | `objc2_core_foundation::CGPoint`                        |
| `cocoa::foundation::NSRect`                       | `objc2_core_foundation::CGRect`                         |
| `cocoa::foundation::NSSize`                       | `objc2_core_foundation::CGSize`                         |
| `cocoa::foundation::NSArray` trait                | `objc2_foundation::NSArray` methods                     |
| `cocoa::foundation::NSAutoreleasePool` trait      | `objc2_foundation::NSAutoreleasePool`                   |
| `cocoa::foundation::NSString` trait               | `objc2_foundation::NSString` methods                    |
| `cocoa::base::id` (`*mut Object`)                 | `*mut AnyObject` or typed `Retained<NSFoo>`             |
| `cocoa::base::nil`                                | `std::ptr::null_mut()` or `None`                        |
| `objc::rc::StrongPtr`                             | `objc2::rc::Retained<T>`                                |
| `objc::rc::WeakPtr`                               | `objc2::rc::Weak<T>`                                    |
| `objc::runtime::Object`                           | `objc2::runtime::AnyObject`                             |
| `objc::runtime::BOOL` / `YES` / `NO`              | `bool` / `true` / `false`                               |

## Ideas for experiments

1. **Replace `StrongPtr`/`WeakPtr` and `Object`** — Migrate `window.rs` struct
   fields and callback bodies from `objc` 0.2 memory management to `objc2`.
   `StrongPtr` → `Retained<AnyObject>`, `WeakPtr` → `Weak<AnyObject>`, `Object`
   → `AnyObject`. Remove the callback bridge pattern.

2. **Replace cocoa trait calls in batches** — Group by AppKit class:
   - NSWindow trait calls → `objc2_app_kit::NSWindow` methods
   - NSView trait calls → `objc2_app_kit::NSView` methods
   - NSEvent trait calls → `objc2_app_kit::NSEvent` methods
   - NSScreen trait calls → `objc2_app_kit::NSScreen` methods
   - NSApplication, NSOpenGLContext, NSPasteboard, etc.

3. **Replace cocoa constants and bitflags** — `NSWindowStyleMask`,
   `NSEventModifierFlags`, `NSApplicationPresentationOptions`,
   `NSBackingStoreBuffered`, `NSViewHeightSizable`/`NSViewWidthSizable`.

4. **Migrate `core_text.rs` + delete dependencies** — Replace `cocoa::base::id`
   with `*mut AnyObject`, then remove `cocoa` and `objc` from all `Cargo.toml`
   files.

## Experiments

### Experiment 1: Replace cocoa constants and bitflags

Replace all `cocoa::appkit` constant and bitflag types with their
`objc2-app-kit` equivalents. Because these types flow into cocoa trait method
calls (`setStyleMask_`, `setPresentationOptions_`, `modifierFlags()`, etc.),
those specific call sites must also be migrated to `msg_send!` or
`objc2-app-kit` typed methods — otherwise the types don't match.

This experiment does NOT migrate all ~50 cocoa trait calls. It only migrates the
~17 that take or return bitflag/constant types. The remaining trait calls
(frame, title, level, etc.) stay as-is for a later experiment.

#### Type replacements

| Old (`cocoa::appkit`)                                                        | New (`objc2-app-kit`)                                                 |
| ---------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| `NSWindowStyleMask::NSTitledWindowMask`                                      | `NSWindowStyleMask::Titled`                                           |
| `NSWindowStyleMask::NSClosableWindowMask`                                    | `NSWindowStyleMask::Closable`                                         |
| `NSWindowStyleMask::NSMiniaturizableWindowMask`                              | `NSWindowStyleMask::Miniaturizable`                                   |
| `NSWindowStyleMask::NSResizableWindowMask`                                   | `NSWindowStyleMask::Resizable`                                        |
| `NSWindowStyleMask::NSFullScreenWindowMask`                                  | `NSWindowStyleMask::FullScreen`                                       |
| `NSWindowStyleMask::NSFullSizeContentViewWindowMask`                         | `NSWindowStyleMask::FullSizeContentView`                              |
| `NSWindowStyleMask::NSBorderlessWindowMask`                                  | `NSWindowStyleMask::Borderless`                                       |
| `NSBackingStoreBuffered`                                                     | `NSBackingStoreType::Buffered`                                        |
| `NSViewHeightSizable \| NSViewWidthSizable`                                  | `NSAutoresizingMaskOptions::ViewHeightSizable \| ...ViewWidthSizable` |
| `NSApplicationActivateIgnoringOtherApps`                                     | `NSApplicationActivationOptions::ActivateIgnoringOtherApps`           |
| `NSApplicationPresentationOptions::NSApplicationPresentationAutoHideMenuBar` | `NSApplicationPresentationOptions::AutoHideMenuBar`                   |
| `NSApplicationPresentationOptions::NSApplicationPresentationAutoHideDock`    | `NSApplicationPresentationOptions::AutoHideDock`                      |
| `NSApplicationPresentationOptions::NSApplicationPresentationDefault`         | `NSApplicationPresentationOptions::Default`                           |
| `NSEventModifierFlags::NSShiftKeyMask`                                       | `NSEventModifierFlags::Shift`                                         |
| `NSEventModifierFlags::NSAlternateKeyMask`                                   | `NSEventModifierFlags::Option`                                        |
| `NSEventModifierFlags::NSControlKeyMask`                                     | `NSEventModifierFlags::Control`                                       |
| `NSEventModifierFlags::NSCommandKeyMask`                                     | `NSEventModifierFlags::Command`                                       |

#### Cocoa trait calls to migrate (17 sites)

These cocoa trait calls take or return the bitflag types above. Each must be
replaced with `msg_send!` or an `objc2-app-kit` typed method so the new types
flow through correctly.

**`NSWindow::styleMask()` → `msg_send!`** (3 sites: lines 870, 1008, 3086):

```rust
// Before:
let style_mask = unsafe { NSWindow::styleMask(self.ns_window) };
// After:
let style_mask: NSWindowStyleMask = unsafe {
    objc2::msg_send![self.ns_window as *const _ as *const AnyObject, styleMask]
};
```

**`.setStyleMask_()` → `msg_send!`** (3 sites: lines 1072, 1186, 1425):

```rust
// Before:
self.window.setStyleMask_(NSWindowStyleMask::NSBorderlessWindowMask);
// After:
let _: () = objc2::msg_send![
    *self.window as *const _ as *const AnyObject,
    setStyleMask: NSWindowStyleMask::Borderless
];
```

**`NSWindow::initWithContentRect_styleMask_backing_defer_()` → `msg_send!`** (1
site: line 498):

```rust
// Before:
let window = StrongPtr::new(NSWindow::initWithContentRect_styleMask_backing_defer_(
    window, rect, style_mask, NSBackingStoreBuffered, NO,
));
// After:
let window: id = objc2::msg_send![window, initWithContentRect: rect
    styleMask: style_mask
    backing: NSBackingStoreType::Buffered
    defer: NO];
let window = StrongPtr::new(window);
```

**`.setAutoresizingMask_()` → `msg_send!`** (1 site: line 580):

```rust
// Before:
view.setAutoresizingMask_(NSViewHeightSizable | NSViewWidthSizable);
// After:
let _: () = objc2::msg_send![
    view as *const _ as *const AnyObject,
    setAutoresizingMask: NSAutoresizingMaskOptions::ViewHeightSizable
        | NSAutoresizingMaskOptions::ViewWidthSizable
];
```

**`.activateWithOptions_()` → `msg_send!`** (1 site: line 1179):

```rust
// Before:
current_app.activateWithOptions_(NSApplicationActivateIgnoringOtherApps);
// After:
let _: () = objc2::msg_send![
    current_app as *const _ as *const AnyObject,
    activateWithOptions: NSApplicationActivationOptions::ActivateIgnoringOtherApps
];
```

**`.setPresentationOptions_()` → `msg_send!`** (3 sites: lines 1054, 1076,
2343):

```rust
// Before:
current_app.setPresentationOptions_(
    NSApplicationPresentationOptions::NSApplicationPresentationAutoHideMenuBar
        | NSApplicationPresentationOptions::NSApplicationPresentationAutoHideDock
);
// After:
let _: () = objc2::msg_send![
    current_app as *const _ as *const AnyObject,
    setPresentationOptions:
        NSApplicationPresentationOptions::AutoHideMenuBar
            | NSApplicationPresentationOptions::AutoHideDock
];
```

**`presentationOptions` via `msg_send!`** (1 site: line 2336) — already uses
`msg_send!`, just change the `from_bits_truncate` to use objc2's type directly:

```rust
// Before:
let bits: usize = objc2::msg_send![...AnyObject, presentationOptions];
NSApplicationPresentationOptions::from_bits_truncate(bits as u64)
// After:
let current_options: NSApplicationPresentationOptions =
    objc2::msg_send![...AnyObject, presentationOptions];
```

**`.modifierFlags()` → `msg_send!`** (4 sites: lines 2446, 2608, 2970, 3001):

```rust
// Before:
let modifier_flags = unsafe { nsevent.modifierFlags() };
// After:
let modifier_flags: NSEventModifierFlags = unsafe {
    objc2::msg_send![nsevent as *const _ as *const AnyObject, modifierFlags]
};
```

#### Import changes

**Remove from `cocoa::appkit`:** `NSApplicationActivateIgnoringOtherApps`,
`NSApplicationPresentationOptions`, `NSBackingStoreBuffered`,
`NSEventModifierFlags`, `NSViewHeightSizable`, `NSViewWidthSizable`,
`NSWindowStyleMask`.

**Keep in `cocoa::appkit`:** `self` (for `appkit::` prefixed constants),
`NSApplication`, `NSEvent`, `NSOpenGLContext`, `NSOpenGLPixelFormat`,
`NSPasteboard`, `NSRunningApplication`, `NSScreen`, `NSView`, `NSWindow`.

**Add to `objc2_app_kit` import:**

```rust
use objc2_app_kit::{
    NSApplicationActivationOptions, NSApplicationPresentationOptions,
    NSAutoresizingMaskOptions, NSBackingStoreType, NSEventModifierFlags,
    NSWindowStyleMask,
};
```

**Add features to `objc2-app-kit` in workspace `Cargo.toml`:** `NSWindow`,
`NSView`, `NSGraphics`.

#### `.bits()` compatibility

`cocoa`'s bitflags use the `bitflags` crate and expose `.bits()` returning
`u64`. `objc2-app-kit`'s bitflags use a custom `bitflags!` macro with `.0` field
access (the inner `NSUInteger`). Update `key_modifiers` line 1945/1948 which
call `flags.bits()`:

```rust
// Before:
if flags.contains(NSEventModifierFlags::NSAlternateKeyMask) && (flags.bits() & 0x20) != 0 {
// After:
if flags.contains(NSEventModifierFlags::Option) && (flags.0 & 0x20) != 0 {
```

#### Verification

1. `cd wezboard && cargo build` — zero errors
2. `cargo run --bin wezboard-gui` — app launches, window opens
3. No `cocoa::appkit::NS*Mask` or `cocoa::appkit::NS*Options` in window.rs
4. No `NSBackingStoreBuffered` from cocoa
5. `key_modifiers` function uses objc2 `NSEventModifierFlags`
6. `decoration_to_mask` function uses objc2 `NSWindowStyleMask`

#### Results: PASSED

All 6 verification checks pass. Changes made:

**`wezboard/Cargo.toml`**: Added `"NSGraphics"`, `"NSView"`, `"NSWindow"`
features to `objc2-app-kit`.

**`wezboard/window/src/os/macos/window.rs`**:

- **Imports**: Moved `NSApplicationPresentationOptions`, `NSEventModifierFlags`,
  `NSWindowStyleMask` from `cocoa::appkit` to `objc2_app_kit`. Added
  `NSAutoresizingMaskOptions`, `NSBackingStoreType`. Removed
  `NSApplicationActivateIgnoringOtherApps`, `NSBackingStoreBuffered`,
  `NSViewHeightSizable`, `NSViewWidthSizable`.
- **`initWithContentRect` (1 site)**: Replaced cocoa trait call with
  `msg_send!`, converting `NSRect` → `CGRect` for the argument. Return type is
  `*mut AnyObject`, cast back to `id` for `StrongPtr::new`.
- **`setAutoresizingMask_` (1 site)**: Replaced with `msg_send!` using
  `NSAutoresizingMaskOptions`.
- **`styleMask` getters (3 sites)**: Replaced `NSWindow::styleMask()` with
  `msg_send!` returning `NSWindowStyleMask`.
- **`setStyleMask_` (3 sites)**: Replaced with `msg_send!`.
- **`setPresentationOptions_` (3 sites)**: Replaced with `msg_send!`.
- **`presentationOptions` getter (1 site)**: Simplified from manual
  `from_bits_truncate(bits as u64)` to direct `msg_send!` returning
  `NSApplicationPresentationOptions`.
- **`activateWithOptions_` (1 site)**: Replaced with `msg_send!` using raw value
  `2usize`. Return type is `bool` (not `()`) to match the ObjC method signature.
- **`modifierFlags()` (4 sites)**: Replaced cocoa trait calls with `msg_send!`
  returning `NSEventModifierFlags`.
- **`decoration_to_mask`**: Renamed all `NS*WindowMask` → short names (`Titled`,
  `Closable`, `Miniaturizable`, `Resizable`, `FullSizeContentView`).
- **`key_modifiers`**: Renamed `NSShiftKeyMask` → `Shift`, `NSAlternateKeyMask`
  → `Option`, `NSControlKeyMask` → `Control`, `NSCommandKeyMask` → `Command`.
- **`.bits()` → `.0` (5 sites)**: All raw bit access updated for objc2's tuple
  struct representation.
- **`NSFullScreenWindowMask` → `FullScreen`** and **`NSBorderlessWindowMask` →
  `Borderless`**: All sites.
- **`NSApplicationPresentationDefault` → `empty()`**: objc2 doesn't have a
  `Default` constant; `empty()` is the zero-value equivalent.

Two deviations from the plan:

1. `activateWithOptions:` returns `BOOL`, not `void`. Using `let _: bool`
   instead of `let _: ()` to avoid a runtime panic from objc2's return type
   validation.
2. `initWithContentRect:` required converting `NSRect` to `CGRect` since
   `cocoa::foundation::NSRect` doesn't implement `objc2::Encode`.

### Experiment 2: Replace remaining cocoa trait calls with msg_send!

Replace all remaining `cocoa::appkit` trait method calls in `window.rs` with
`objc2::msg_send!`. After this experiment, the `cocoa::appkit` import block can
be deleted entirely. The `cocoa::foundation` and `cocoa::base` imports stay for
now (a later experiment handles those).

This is purely mechanical: each `NSFoo::bar_(obj, arg)` or `obj.bar_(arg)`
becomes `objc2::msg_send![obj as *const _ as *const AnyObject, bar: arg]`.

#### Inventory (46 call sites by class)

**NSWindow (18 sites)**

| Call                                       | Lines                  | Return      | Notes                                      |
| ------------------------------------------ | ---------------------- | ----------- | ------------------------------------------ |
| `NSWindow::frame(w)`                       | 409, 545, 1076, 2239   | `NSRect`    |                                            |
| `NSWindow::contentRectForFrameRect_(w, r)` | 410                    | `NSRect`    |                                            |
| `NSWindow::setFrameOrigin_(w, p)`          | 417                    | `()`        |                                            |
| `window.setBackgroundColor_(c)`            | 524                    | `()`        | arg is `NSColor::clearColor(nil)`          |
| `window.setColorSpace_(c)`                 | 528                    | `()`        | arg is `NSColorSpace::sRGBColorSpace(nil)` |
| `window.cascadeTopLeftFromPoint_(p)`       | 571, 577               | `NSPoint`   |                                            |
| `window.setTitle_(t)`                      | 583                    | `()`        |                                            |
| `window.setAcceptsMouseMovedEvents_(b)`    | 584                    | `()`        |                                            |
| `window.setContentView_(v)`                | 603                    | `()`        |                                            |
| `window.setDelegate_(v)`                   | 604                    | `()`        |                                            |
| `window.windowNumber()`                    | 600, 1188              | `NSInteger` |                                            |
| `window.orderOut_(nil)`                    | 1060, 1086             | `()`        |                                            |
| `window.setFrame_display_(r, b)`           | 1066, 1091             | `()`        |                                            |
| `window.makeKeyAndOrderFront_(nil)`        | 1067, 1092, 1221, 1233 | `()`        |                                            |
| `window.setOpaque_(b)`                     | 1068, 1093, 1112       | `()`        |                                            |
| `window.setHasShadow_(b)`                  | 1134                   | `()`        |                                            |
| `NSWindow::miniaturize_(w, w)`             | 1239                   | `()`        |                                            |
| `NSWindow::setTitle_(w, t)`                | 1299                   | `()`        |                                            |
| `NSWindow::setLevel_(w, l)`                | 1305                   | `()`        |                                            |
| `NSWindow::setContentSize_(w, s)`          | 1321                   | `()`        |                                            |
| `NSWindow::toggleFullScreen_(w, nil)`      | 1016                   | `()`        |                                            |
| `NSWindow::zoom_(w, nil)`                  | 1369, 1377             | `()`        |                                            |
| `window.setTitleVisibility_(v)`            | 1472                   | `()`        | arg uses `appkit::NSWindowTitleVisibility` |
| `window.setTitlebarAppearsTransparent_(b)` | 1481, 1483             | `()`        |                                            |

**NSView (10 sites)**

| Call                                        | Lines                      | Return    | Notes |
| ------------------------------------------- | -------------------------- | --------- | ----- |
| `NSView::frame(v)`                          | 321, 619, 1317, 2294, 3107 | `NSRect`  |       |
| `NSView::convertRectToBacking(v, r)`        | 322, 620, 1318, 2466, 3108 | `NSRect`  |       |
| `NSView::convertPoint_fromView_(v, p, nil)` | 2464                       | `NSPoint` |       |
| `view.contentView()`                        | 1591                       | `id`      |       |
| `view.setWantsLayer(b)`                     | 606                        | `()`      |       |

**NSScreen (9 sites)**

| Call                                    | Lines                    | Return   | Notes |
| --------------------------------------- | ------------------------ | -------- | ----- |
| `NSScreen::mainScreen(nil)`             | 546, 891, 1083           | `id`     |       |
| `NSScreen::screens(nil)`                | 957, 973                 | `id`     |       |
| `NSScreen::frame(s)`                    | 547, 921, 959, 975, 1084 | `NSRect` |       |
| `NSScreen::convertRectToBacking_(s, r)` | 922, 960, 976            | `NSRect` |       |

**NSEvent (7 sites)**

| Call                                    | Lines      | Return       | Notes |
| --------------------------------------- | ---------- | ------------ | ----- |
| `NSEvent::pressedMouseButtons(e)`       | 2473       | `NSUInteger` |       |
| `NSEvent::mouseLocation(e)`             | 2478       | `NSPoint`    |       |
| `NSEvent::buttonNumber(e)`              | 2516, 2609 | `NSInteger`  |       |
| `nsevent.locationInWindow()`            | 2464       | `NSPoint`    |       |
| `nsevent.isARepeat()`                   | 2636       | `BOOL`       |       |
| `nsevent.characters()`                  | 2637, 3002 | `id`         |       |
| `nsevent.charactersIgnoringModifiers()` | 2638       | `id`         |       |
| `nsevent.keyCode()`                     | 2648       | `u16`        |       |
| `nsevent.hasPreciseScrollingDeltas()`   | 2528       | `BOOL`       |       |
| `nsevent.scrollingDeltaY()`             | 2542       | `CGFloat`    |       |
| `nsevent.scrollingDeltaX()`             | 2543       | `CGFloat`    |       |

**NSApplication (2 sites)**

| Call                                    | Lines      | Return | Notes |
| --------------------------------------- | ---------- | ------ | ----- |
| `NSApplication::sharedApplication(nil)` | 1053, 2352 | `id`   |       |

**NSRunningApplication (1 site)**

| Call                                            | Lines | Return | Notes |
| ----------------------------------------------- | ----- | ------ | ----- |
| `NSRunningApplication::currentApplication(nil)` | 1198  | `id`   |       |

**NSOpenGLContext + NSOpenGLPixelFormat (7 sites)**

| Call                                                            | Lines    | Return | Notes                           |
| --------------------------------------------------------------- | -------- | ------ | ------------------------------- |
| `NSOpenGLPixelFormat::alloc(nil).initWithAttributes_(&[...])`   | 223      | `id`   |                                 |
| `NSOpenGLContext::alloc(nil).initWithFormat_shareContext_(...)` | 255      | `id`   |                                 |
| `gl_context.setValues_forParameter_(...)`                       | 262, 272 | `()`   | uses `NSOpenGLContextParameter` |
| `gl_context.setView_(v)`                                        | 267      | `()`   |                                 |
| `gl_context.flushBuffer()`                                      | 303      | `()`   |                                 |
| `gl_context.view()`                                             | 320      | `id`   |                                 |
| `NSOpenGLContext::currentContext(nil)`                          | 333      | `id`   |                                 |
| `gl_context.makeCurrentContext()`                               | 357      | `()`   |                                 |

**NSPasteboard (2 sites)**

| Call                                         | Lines      | Return | Notes                                |
| -------------------------------------------- | ---------- | ------ | ------------------------------------ |
| `NSPasteboard::propertyListForType(pb, ...)` | 3308, 3340 | `id`   | uses `appkit::NSFilenamesPboardType` |

**Other appkit references (4 sites)**

| Call                                               | Lines           | Notes                                   |
| -------------------------------------------------- | --------------- | --------------------------------------- |
| `cocoa::appkit::NSColor::clearColor(nil)`          | 524             | Used as arg to `setBackgroundColor_`    |
| `cocoa::appkit::NSColorSpace::sRGBColorSpace(nil)` | 528             | Used as arg to `setColorSpace_`         |
| `cocoa::appkit::NSOpenGLContextParameter::*`       | 264, 274        | Constants for `setValues_forParameter_` |
| `appkit::NSWindowTitleVisibility::*`               | 1473, 1475      | Constants for `setTitleVisibility_`     |
| `appkit::NSFilenamesPboardType`                    | 616, 3308, 3340 | Pasteboard type string                  |

**NSArray (2 sites)**

| Call                                  | Lines | Notes |
| ------------------------------------- | ----- | ----- |
| `NSArray::arrayWithObject(nil, ...)`  | 616   |       |
| `NSArray::arrayWithObjects(nil, &[])` | 2195  |       |

**NSAutoreleasePool (2 sites)**

| Call                          | Lines    | Notes |
| ----------------------------- | -------- | ----- |
| `NSAutoreleasePool::new(nil)` | 302, 332 |       |

#### Translation pattern

Every call follows the same pattern:

```rust
// Before (static style):
let frame = NSWindow::frame(*self.window);
// After:
let frame: NSRect = objc2::msg_send![
    *self.window as *const _ as *const AnyObject, frame
];

// Before (dot style):
self.window.setOpaque_(YES);
// After:
let _: () = objc2::msg_send![
    *self.window as *const _ as *const AnyObject, setOpaque: YES
];
```

Return types that are `NSRect`, `NSPoint`, `NSSize` stay as-is for now — they
are layout-compatible with `CGRect`, `CGPoint`, `CGSize`. A later experiment
replaces them.

#### appkit constants that need replacement

These constants are currently accessed via `cocoa::appkit::` and will need
objc2-app-kit equivalents or raw `msg_send!` lookups:

| Old                                                                 | New                                                                                                 |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `cocoa::appkit::NSColor::clearColor(nil)`                           | `objc2::msg_send![objc2::class!(NSColor), clearColor]`                                              |
| `cocoa::appkit::NSColorSpace::sRGBColorSpace(nil)`                  | `objc2::msg_send![objc2::class!(NSColorSpace), sRGBColorSpace]`                                     |
| `cocoa::appkit::NSOpenGLContextParameter::NSOpenGLCPSurfaceOpacity` | raw value `236`                                                                                     |
| `cocoa::appkit::NSOpenGLContextParameter::NSOpenGLCPSwapInterval`   | raw value `222`                                                                                     |
| `appkit::NSWindowTitleVisibility::NSWindowTitleVisible`             | `0isize`                                                                                            |
| `appkit::NSWindowTitleVisibility::NSWindowTitleHidden`              | `1isize`                                                                                            |
| `appkit::NSFilenamesPboardType`                                     | `objc2_app_kit::NSPasteboardTypeFileURL` or `NSString` literal                                      |
| `NSArray::arrayWithObject(nil, x)`                                  | `objc2::msg_send![objc2::class!(NSArray), arrayWithObject: x]`                                      |
| `NSArray::arrayWithObjects(nil, &[])`                               | `objc2::msg_send![objc2::class!(NSArray), arrayWithObjects: std::ptr::null::<id>(), count: 0usize]` |
| `NSAutoreleasePool::new(nil)`                                       | `objc2_foundation::NSAutoreleasePool::new()` or `autoreleasepool` closure                           |

#### Import changes

**Delete entirely:**

```rust
use cocoa::appkit::{
    self, NSApplication, NSEvent, NSOpenGLContext, NSOpenGLPixelFormat,
    NSPasteboard, NSRunningApplication, NSScreen, NSView, NSWindow,
};
```

**Keep (for now):**

```rust
use cocoa::base::*;
use cocoa::foundation::{
    NSArray, NSAutoreleasePool, NSFastEnumeration, NSPoint, NSRect, NSSize,
    NSString,
};
```

#### Verification

1. `cd wezboard && cargo build` — zero errors
2. `cargo run --bin wezboard-gui` — app launches, window opens
3. No `cocoa::appkit` in `window.rs` imports or body
4. `grep -c 'cocoa::appkit' window/src/os/macos/window.rs` returns 0

#### Results: PASSED

All 4 verification checks pass. Changes made:

**`wezboard/window/src/os/macos/window.rs`**:

- **Deleted `cocoa::appkit` import block** — removed
  `use cocoa::appkit::{self, NSApplication, NSEvent, NSOpenGLContext, NSOpenGLPixelFormat, NSPasteboard, NSRunningApplication, NSScreen, NSView, NSWindow}`.
- **Removed `NSArray`, `NSAutoreleasePool`** from `cocoa::foundation` import
  (all uses migrated to `msg_send!`).
- **Added 18 function key constants** — `NS_UP_ARROW_FUNCTION_KEY` through
  `NS_MODE_SWITCH_FUNCTION_KEY`, replacing `appkit::NS*FunctionKey` references
  in `function_key_to_keycode`.
- **Added `cg_to_ns_rect` helper** — converts `CGRect` → `NSRect` at msg_send
  boundaries since `cocoa::foundation::NSRect` does not implement
  `objc2::Encode`.

**NSOpenGLPixelFormat + NSOpenGLContext (7 sites):**

- `NSOpenGLPixelFormat::alloc(nil).initWithAttributes_(&[...])` → two
  `msg_send!` calls (alloc + initWithAttributes:) with raw u32 attribute
  constants (99, 0x3200, 74, 8, 11, 12, 13, 96, 73, 5).
- `NSOpenGLContext::alloc(nil).initWithFormat_shareContext_(...)` → two
  `msg_send!` calls with `std::ptr::null::<AnyObject>()` for nil shareContext.
- `setValues_forParameter_` → `msg_send!` with `isize` parameter type (236 =
  NSOpenGLCPSurfaceOpacity, 222 = NSOpenGLCPSwapInterval).
- `setView_`, `flushBuffer`, `makeCurrentContext`, `view()`,
  `currentContext(nil)` → `msg_send!`.
- `NSAutoreleasePool::new(nil)` → `msg_send![class!(NSAutoreleasePool), new]`.

**NSWindow (22 sites):**

- `frame`, `contentRectForFrameRect:`, `setFrameOrigin:`, `setBackgroundColor:`,
  `setColorSpace:`, `cascadeTopLeftFromPoint:`, `setTitle:`,
  `setAcceptsMouseMovedEvents:`, `windowNumber`, `setContentView:`,
  `setDelegate:`, `orderOut:`, `setFrame:display:`, `makeKeyAndOrderFront:`,
  `setOpaque:`, `setHasShadow:`, `miniaturize:`, `setTitle:`, `setLevel:`,
  `setContentSize:`, `toggleFullScreen:`, `zoom:`, `center`, `close`,
  `setTitleVisibility:`, `setTitlebarAppearsTransparent:`,
  `setReleasedWhenClosed:`, `setResizeIncrements:`, `standardWindowButton:` —
  all replaced with `msg_send!`.
- `NSColor::clearColor` and `NSColorSpace::sRGBColorSpace` → class-level
  `msg_send!`.
- `NSWindowButton` constants → raw u64 values (0=Close, 1=Miniaturize, 2=Zoom).
- `NSWindowTitleVisibility` → raw isize (0=Visible, 1=Hidden).

**NSView (11 sites):**

- `frame`, `convertRectToBacking:`, `convertPoint:fromView:`, `contentView`,
  `setWantsLayer:` — all replaced with `msg_send!`.

**NSScreen (9 sites):**

- `mainScreen`, `screens`, `frame`, `convertRectToBacking:` — all replaced with
  `msg_send!` using `objc2::class!(NSScreen)`.

**NSEvent (11 sites):**

- `pressedMouseButtons`, `mouseLocation`, `buttonNumber`, `locationInWindow`,
  `isARepeat`, `characters`, `charactersIgnoringModifiers`, `keyCode`,
  `hasPreciseScrollingDeltas`, `scrollingDeltaY`, `scrollingDeltaX` — all
  replaced with `msg_send!`.

**NSApplication + NSRunningApplication (3 sites):**

- `sharedApplication`, `currentApplication` → class-level `msg_send!`.

**NSPasteboard (2 sites):**

- `propertyListForType(pb, appkit::NSFilenamesPboardType)` →
  `msg_send![... propertyListForType: ...]` with
  `nsstring("NSFilenamesPboardType")`.

**NSArray (2 sites):**

- `arrayWithObject` and `arrayWithObjects` → `msg_send!` with
  `objc2::class!(NSArray)`.

**NSArray `.count()` / `.objectAtIndex()` (3 sites):**

- Replaced cocoa `NSArray` trait method calls with `msg_send!` returning `usize`
  and `*mut AnyObject` respectively.

**`window.load().level()` (1 site):**

- Replaced cocoa `NSWindow` trait `level()` call with `msg_send!` returning
  `i64`.

**CGRect/CGPoint conversion pattern:**

All `msg_send!` calls that return or accept geometry types use
`CGRect`/`CGPoint` (from `objc2_core_foundation`) since
`cocoa::foundation::NSRect`/`NSPoint` do not implement `objc2::Encode`.
Conversions via `cg_to_ns_rect` and manual `NSPoint::new(cg.x, cg.y)` are used
where downstream code still expects `NSRect`/`NSPoint`.

**`nil` argument pattern:**

All `nil` arguments to `msg_send!` replaced with `std::ptr::null::<AnyObject>()`
since cocoa's `nil` (`*mut objc::runtime::Object`) does not implement
`objc2::RefEncode`.

**Runtime crash fix:**

Initial build succeeded but the app crashed at launch with SIGABRT in
`setValues:forParameter:`. Root cause: `NSOpenGLContextParameter` is `NSInteger`
(`isize`/i64 on ARM64), but the raw constant was passed as `i32`. objc2's
debug-mode method signature verification caught the type mismatch. Fixed by
changing `236i32` → `236isize` and `222i32` → `222isize`.
