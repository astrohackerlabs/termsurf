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

Replace all `cocoa::appkit` constant and bitflag types with their `objc2-app-kit`
equivalents. Because these types flow into cocoa trait method calls
(`setStyleMask_`, `setPresentationOptions_`, `modifierFlags()`, etc.), those
specific call sites must also be migrated to `msg_send!` or `objc2-app-kit`
typed methods — otherwise the types don't match.

This experiment does NOT migrate all ~50 cocoa trait calls. It only migrates the
~17 that take or return bitflag/constant types. The remaining trait calls (frame,
title, level, etc.) stay as-is for a later experiment.

#### Type replacements

| Old (`cocoa::appkit`)                                          | New (`objc2-app-kit`)                                         |
| -------------------------------------------------------------- | ------------------------------------------------------------- |
| `NSWindowStyleMask::NSTitledWindowMask`                        | `NSWindowStyleMask::Titled`                                   |
| `NSWindowStyleMask::NSClosableWindowMask`                      | `NSWindowStyleMask::Closable`                                 |
| `NSWindowStyleMask::NSMiniaturizableWindowMask`                | `NSWindowStyleMask::Miniaturizable`                           |
| `NSWindowStyleMask::NSResizableWindowMask`                     | `NSWindowStyleMask::Resizable`                                |
| `NSWindowStyleMask::NSFullScreenWindowMask`                    | `NSWindowStyleMask::FullScreen`                               |
| `NSWindowStyleMask::NSFullSizeContentViewWindowMask`           | `NSWindowStyleMask::FullSizeContentView`                      |
| `NSWindowStyleMask::NSBorderlessWindowMask`                    | `NSWindowStyleMask::Borderless`                               |
| `NSBackingStoreBuffered`                                       | `NSBackingStoreType::Buffered`                                |
| `NSViewHeightSizable \| NSViewWidthSizable`                    | `NSAutoresizingMaskOptions::ViewHeightSizable \| ...ViewWidthSizable` |
| `NSApplicationActivateIgnoringOtherApps`                       | `NSApplicationActivationOptions::ActivateIgnoringOtherApps`   |
| `NSApplicationPresentationOptions::NSApplicationPresentationAutoHideMenuBar` | `NSApplicationPresentationOptions::AutoHideMenuBar` |
| `NSApplicationPresentationOptions::NSApplicationPresentationAutoHideDock`    | `NSApplicationPresentationOptions::AutoHideDock`    |
| `NSApplicationPresentationOptions::NSApplicationPresentationDefault`         | `NSApplicationPresentationOptions::Default`         |
| `NSEventModifierFlags::NSShiftKeyMask`                         | `NSEventModifierFlags::Shift`                                 |
| `NSEventModifierFlags::NSAlternateKeyMask`                     | `NSEventModifierFlags::Option`                                |
| `NSEventModifierFlags::NSControlKeyMask`                       | `NSEventModifierFlags::Control`                               |
| `NSEventModifierFlags::NSCommandKeyMask`                       | `NSEventModifierFlags::Command`                               |

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

**`NSWindow::initWithContentRect_styleMask_backing_defer_()` → `msg_send!`**
(1 site: line 498):

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
`u64`. `objc2-app-kit`'s bitflags use a custom `bitflags!` macro with `.0`
field access (the inner `NSUInteger`). Update `key_modifiers` line 1945/1948
which call `flags.bits()`:

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
