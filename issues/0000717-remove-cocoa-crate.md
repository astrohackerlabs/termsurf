# Issue 717: Remove `cocoa` crate from wezboard

## Goal

Replace all `cocoa` crate usage with `objc2-app-kit` + `objc2-foundation` in the
wezboard macOS code. Remove the `cocoa` and `objc` 0.2 dependencies entirely.

## Background

Issue 716 eliminated all 193 build warnings by migrating `msg_send!`/`class!`/
`sel!` macros from `objc` 0.2 to `objc2`. But the migration kept the `cocoa`
crate for its trait methods (`NSWindow::frame()`, `NSApp()`, etc.) and types
(`id`, `nil`, `NSRect`, `NSSize`, etc.). This created an awkward hybrid where
`objc2::msg_send!` calls must constantly bridge between `cocoa`'s `objc` 0.2
types and `objc2` types.

The bridge workarounds that need cleaning up:

1. **`MsgSendRect` / `MsgSendSize`** — Local types duplicating `CGRect`/`CGSize`
   layout with `objc2::Encode` impls, because cocoa's `NSRect`/`NSSize` can't
   implement the foreign `Encode` trait. Every geometry call site uses
   `std::mem::transmute`.

2. **`{ let __r: *mut AnyObject = ...; __r as id }`** — Boilerplate wrapping
   every `objc2::msg_send!` that returns an object, because `id`
   (`*mut objc::Object`) isn't a valid return type for `objc2::msg_send!`.

3. **`as *const _ as *const AnyObject` casts** — Needed everywhere because
   `objc::Object` ≠ `objc2::AnyObject`, even though they have identical memory
   layout.

4. **`sel2to1()` / `cls1to2()` / `cls2to1()` bridge helpers** — Transmute
   between `objc` and `objc2` selector/class types.

5. **Inline `NSEdgeInsets` struct** — Defined inside a function body with a
   manual `Encode` impl because cocoa doesn't provide it and `objc2-app-kit`
   does.

6. **`NSApplicationPresentationOptions` `from_bits_truncate`** — Roundtrip
   through `usize` because the cocoa bitflags type doesn't impl `Encode`.

All of these disappear once `cocoa` is replaced with `objc2-app-kit` /
`objc2-foundation`, which provide typed wrappers with native `Encode` impls.

## Scope

### Files to migrate

| File                                     | `cocoa::` imports | Trait method calls | Notes                                                                                                    |
| ---------------------------------------- | ----------------- | ------------------ | -------------------------------------------------------------------------------------------------------- |
| `window/src/os/macos/window.rs`          | 9                 | ~50                | Bulk of work. NSWindow, NSView, NSScreen, NSOpenGLContext, NSEvent, NSColor, NSColorSpace, NSAutorelease |
| `window/src/os/macos/connection.rs`      | 3                 | ~10                | NSApp, NSApplication, NSScreen, NSArray                                                                  |
| `window/src/os/macos/menu.rs`            | 3                 | ~8                 | NSApp, NSMenu, NSMenuItem                                                                                |
| `window/src/os/macos/mod.rs`             | 2                 | 2                  | NSString (alloc, init_str, UTF8String, len)                                                              |
| `window/src/os/macos/app.rs`             | 2                 | 0                  | NSApplicationTerminateReply, NSInteger                                                                   |
| `window/src/os/macos/clipboard.rs`       | 3                 | ~6                 | NSPasteboard, NSArray, NSFilenamesPboardType                                                             |
| `wezboard-font/src/locator/core_text.rs` | 1                 | 0                  | `cocoa::base::id` only                                                                                   |

### What gets removed

- `cocoa` dependency from `window/Cargo.toml` and `wezboard-font/Cargo.toml`
- `objc` dependency from `window/Cargo.toml`, `wezboard-font/Cargo.toml`
- `cocoa` and `objc` from workspace `Cargo.toml`
- All bridge helpers: `sel2to1`, `cls1to2`, `cls2to1`, `get_class`
- `MsgSendRect`, `MsgSendSize` types and all `transmute` calls
- All `__r` temporary variables and `as id` casts
- All `as *const _ as *const AnyObject` casts
- Inline `NSEdgeInsets` struct
- `objc::Encode` impls (replaced by `objc2::Encode`)

### What replaces it

| `cocoa`                                  | `objc2` equivalent                                  |
| ---------------------------------------- | --------------------------------------------------- |
| `cocoa::base::id`                        | `*mut AnyObject` or typed `&NSFoo`                  |
| `cocoa::base::nil`                       | `std::ptr::null_mut()` or `None`                    |
| `cocoa::base::BOOL` / `YES` / `NO`       | `bool`                                              |
| `cocoa::foundation::NSRect`              | `objc2_foundation::CGRect`                          |
| `cocoa::foundation::NSSize`              | `objc2_foundation::CGSize`                          |
| `cocoa::foundation::NSPoint`             | `objc2_foundation::CGPoint`                         |
| `cocoa::foundation::NSInteger`           | `objc2_foundation::NSInteger`                       |
| `cocoa::foundation::NSString`            | `objc2_foundation::NSString`                        |
| `cocoa::foundation::NSArray`             | `objc2_foundation::NSArray`                         |
| `cocoa::appkit::NSApp()`                 | `objc2_app_kit::NSApplication::sharedApplication()` |
| `cocoa::appkit::NSWindow` trait          | `objc2_app_kit::NSWindow` methods                   |
| `cocoa::appkit::NSView` trait            | `objc2_app_kit::NSView` methods                     |
| `cocoa::appkit::NSScreen` trait          | `objc2_app_kit::NSScreen` methods                   |
| `cocoa::appkit::NSEvent` trait           | `objc2_app_kit::NSEvent` methods                    |
| `cocoa::appkit::NSMenu` trait            | `objc2_app_kit::NSMenu` methods                     |
| `cocoa::appkit::NSMenuItem` trait        | `objc2_app_kit::NSMenuItem` methods                 |
| `cocoa::appkit::NSPasteboard` trait      | `objc2_app_kit::NSPasteboard` methods               |
| `cocoa::appkit::NSCursor` (via msg_send) | `objc2_app_kit::NSCursor` methods                   |
| `objc::rc::StrongPtr`                    | `objc2::rc::Retained<T>`                            |
| `objc::rc::WeakPtr`                      | `objc2::rc::Weak<T>`                                |
| `objc::declare::ClassDecl`               | `objc2::declare::ClassBuilder`                      |
| `objc::runtime::Object`                  | `objc2::runtime::AnyObject`                         |
| `objc::runtime::Class`                   | `objc2::runtime::AnyClass`                          |
| `objc::runtime::Sel`                     | `objc2::runtime::Sel`                               |
| `objc::runtime::Protocol`                | `objc2::runtime::AnyProtocol`                       |

## Ideas for experiments

1. **Start with `mod.rs` + `app.rs` + `connection.rs`** — Smallest files.
   Establish the pattern for replacing `id`/`nil`, `NSApp()`, `NSScreen`,
   `StrongPtr`. Remove bridge helpers once no file uses them.

2. **`menu.rs` + `clipboard.rs`** — Medium files. Replace `NSMenu`,
   `NSMenuItem`, `NSPasteboard` trait calls with typed `objc2-app-kit` methods.
   Replace `StrongPtr` fields with `Retained<T>`.

3. **`window.rs`** — The bulk. Replace all `NSWindow`/`NSView`/`NSEvent` trait
   calls, `StrongPtr`/`WeakPtr` fields, `ClassDecl` → `ClassBuilder`, remove
   `MsgSendRect`/`MsgSendSize`/`NSEdgeInsets`, `objc::Encode` impls.

4. **`core_text.rs` + final cleanup** — Remove last `cocoa::base::id` usage,
   remove `cocoa` and `objc` from workspace deps.

## Experiments

### Experiment 1: Migrate `app.rs` and `clipboard.rs`

Migrate the two smallest standalone files to pure `objc2` / `objc2-app-kit` /
`objc2-foundation` — zero `cocoa` or `objc` 0.2 imports. This establishes
patterns for the remaining files.

`mod.rs` stays hybrid: `nsstring()` and `nsstring_to_str()` still return
`StrongPtr` / take `*mut Object`, since every other file depends on them. Bridge
helpers (`sel2to1`, `cls1to2`, `cls2to1`, `get_class`) remain for now — other
files still use them. Both are cleaned up in a later experiment.

#### Changes

**`app.rs`** — Replace all `cocoa` and `objc` 0.2 imports:

| Old (`cocoa` / `objc` 0.2)                       | New (`objc2`)                                                        |
| ------------------------------------------------ | -------------------------------------------------------------------- |
| `cocoa::appkit::NSApplicationTerminateReply`     | `objc2_app_kit::NSApplicationTerminateReply`                         |
| `cocoa::foundation::NSInteger`                   | `objc2_foundation::NSInteger`                                        |
| `objc::declare::ClassDecl`                       | `objc2::runtime::ClassBuilder`                                       |
| `objc::rc::StrongPtr`                            | `objc2::rc::Retained<AnyObject>`                                     |
| `objc::runtime::{Class, Object, Sel, BOOL, ...}` | `objc2::runtime::{AnyClass, AnyObject, Sel}` + `bool`/`true`/`false` |
| `cls1to2(get_objc_class(c"NSObject"))`           | `AnyClass::get(c"NSObject").unwrap()`                                |
| `sel2to1(objc2::sel!(...))`                      | `objc2::sel!(...)` directly                                          |
| `ClassDecl::new(name, superclass)`               | `ClassBuilder::new(c"Name", &superclass)`                            |
| `cls.add_ivar::<BOOL>("launched")`               | `cls.add_ivar::<bool>(c"launched")`                                  |
| `StrongPtr::new(delegate)`                       | `Retained::from_raw(delegate).unwrap()`                              |
| `*delegate` (deref StrongPtr)                    | `&*delegate` or `Retained::as_ptr(&delegate)`                        |
| `(*delegate).set_ivar("launched", NO)`           | `*(*delegate).get_mut_ivar::<bool>(c"launched") = false`             |
| `NSTerminateCancel as u64`                       | `NSApplicationTerminateReply::NSTerminateCancel.0 as u64`            |
| `NSTerminateNow as u64`                          | `NSApplicationTerminateReply::NSTerminateNow.0 as u64`               |

Specific function changes:

- `application_should_terminate` — Return type stays `u64`. Use
  `NSApplicationTerminateReply::NSTerminateCancel.0 as u64` (the struct wraps
  `NSUInteger`).
- `application_did_finish_launching` / `application_open_untitled_file` /
  `application_open_file` — Change `BOOL` params/ivars to `bool`. The callback
  signatures use `*mut AnyObject` instead of `*mut Object`.
- `application_dock_menu` — Return `*mut AnyObject` instead of `*mut Object`.
  The `sel2to1(objc2::sel!(...))` call becomes just `objc2::sel!(...)` since
  `Menu::new_with` still accepts `SEL` (which is `objc::runtime::Sel`). Wait —
  this function calls `MenuItem::new_with` which takes `Option<SEL>` where `SEL`
  is re-exported from `cocoa::base::SEL`. This is actually `objc::runtime::Sel`.
  Since `menu.rs` isn't migrated yet, keep `sel2to1(objc2::sel!(...))` in this
  one call site. Import `sel2to1` only for this.
- `get_class` — Use `ClassBuilder::new(c"WezboardAppDelegate", superclass)` with
  `AnyClass::get(c"NSObject").unwrap()` as superclass. `ClassBuilder` takes
  `objc2::runtime::Sel` directly, so all `sel2to1()` wrappers around
  `add_method` calls are removed. `add_ivar` takes `&CStr` names.
  `cls.register()` returns `&'static AnyClass` — wrap the return type.
- `create_app_delegate` — Returns `Retained<AnyObject>` instead of `StrongPtr`.
  The allocator pattern becomes:
  ```rust
  let cls = get_class();
  let delegate: *mut AnyObject = objc2::msg_send![cls, alloc];
  let delegate: *mut AnyObject = objc2::msg_send![delegate, init];
  // set ivar through mut ref
  (*delegate).get_mut_ivar::<bool>(c"launched") = false; // ERROR: no set_ivar on AnyObject
  ```
  Actually, `AnyObject` in objc2 doesn't have `set_ivar` or `get_mut_ivar` as
  inherent methods. Use `Ivar::load` + pointer writes, or use `objc2::msg_send!`
  to call setters. The simplest approach: use `*mut AnyObject` and write through
  the ivar pointer using `objc2::runtime::AnyObject`'s `class()` method to get
  the ivar offset, or just use raw `msg_send!` to set the value. Alternatively,
  since `bool` defaults to `false` after `init`, we may not even need to set the
  ivar explicitly — just ensure the ivar is `false` by default (which it is,
  since ObjC zeroes ivars on alloc).

**`clipboard.rs`** — Replace all `cocoa` imports with `objc2-app-kit`:

| Old (`cocoa`)                          | New (`objc2-app-kit`)                                   |
| -------------------------------------- | ------------------------------------------------------- |
| `cocoa::appkit::NSPasteboard` trait    | `objc2_app_kit::NSPasteboard` struct methods            |
| `cocoa::appkit::NSFilenamesPboardType` | `objc2_app_kit::NSFilenamesPboardType`                  |
| `cocoa::appkit::NSStringPboardType`    | `objc2_app_kit::NSStringPboardType`                     |
| `cocoa::base::{id, nil, BOOL, YES}`    | `*mut AnyObject` / `bool` (or typed `Retained` returns) |
| `cocoa::foundation::NSArray` trait     | `objc2_foundation::NSArray` struct or `msg_send!`       |

Specific method replacements:

- `Clipboard::new` — `NSPasteboard::generalPasteboard(nil)` becomes
  `NSPasteboard::generalPasteboard()` which returns `Retained<NSPasteboard>`.
  Store `Retained<NSPasteboard>` instead of `id`.
- `Clipboard::read` —
  `self.pasteboard.propertyListForType(NSFilenamesPboardType)` becomes
  `self.pasteboard.propertyListForType(NSFilenamesPboardType)` returning
  `Option<Retained<AnyObject>>`. The null check becomes `.is_some()`. For
  iterating the plist as an array, downcast to `Retained<NSArray<NSString>>` or
  use `msg_send!` for `count`/`objectAtIndex:`.
  `self.pasteboard.stringForType(NSStringPboardType)` returns
  `Option<Retained<NSString>>`. Convert to `&str` via
  `nsstring_to_str(&*s as *const _ as *mut Object)` (still using the shared
  helper).
- `Clipboard::write` — `self.pasteboard.clearContents()` works directly. Replace
  `writeObjects(NSArray::arrayWithObject(nil, ...))` with
  `self.pasteboard.setString_forType(&ns_str, NSPasteboardTypeString)` — this is
  simpler and avoids the `NSPasteboardWriting` protocol object complexity. Use
  `NSPasteboardTypeString` instead of `NSStringPboardType` (modern constant).
  The `BOOL == YES` check becomes a simple `bool` check.

**`window/Cargo.toml`** — Add `objc2-app-kit` features needed for
`NSPasteboard`:

```toml
objc2-app-kit = { workspace = true, features = ["NSPasteboard", ...] }
```

Check which features are already enabled and add any missing ones.

#### Verification

1. `cd wezboard && cargo build` — must compile with zero errors
2. `cargo build 2>&1 | grep "warning.*cocoa\|warning.*objc[^2]"` — no new
   warnings from the migration
3. Confirm `app.rs` has zero `cocoa::` or `objc::` imports
4. Confirm `clipboard.rs` has zero `cocoa::` or `objc::` imports
5. Manual test: launch wezboard, verify clipboard paste works (Cmd+V), verify
   app quit dialog appears with `AlwaysPrompt` config

#### Results

**Result:** Pass

Both `app.rs` and `clipboard.rs` compile with zero `cocoa::` or `objc::` imports
and zero warnings.

Key findings during implementation:

1. **`bool` doesn't implement `objc2::Encode`.** The plan assumed `bool` could
   replace `BOOL` for ivars and callback return types. In objc2 0.6,
   `objc2::runtime::Bool` is the correct replacement — it wraps the
   platform-specific ObjC BOOL type and implements `Encode`. Used `Bool::YES`,
   `Bool::NO`, and `Bool::as_bool()` for conversions.

2. **`ClassBuilder::add_method` has a higher-ranked lifetime requirement.** When
   callback functions use `&mut AnyObject` as the receiver (first param),
   casting the named function to a fn pointer with
   `as extern "C" fn(&mut AnyObject, ...)` loses the `for<'a>` lifetime bound
   that `MethodImplementation` requires. Fix: use `*mut AnyObject` as the
   receiver param instead, then dereference inside the function body with
   `&*ptr` / `&mut *ptr`. This avoids the lifetime issue entirely.

3. **`NSApplicationTerminateReply` constants are renamed.** objc2-app-kit uses
   `TerminateCancel` / `TerminateNow` (not `NSTerminateCancel` /
   `NSTerminateNow`). The inner value is accessed via `.0`.

4. **`NSPasteboard::generalPasteboard()` is safe.** No `unsafe` block needed —
   objc2-app-kit marks it as a safe class method.

5. **`nsstring_to_str` bridge works via `.cast()`.** Callbacks receive
   `*mut AnyObject` params. Passing them to `nsstring_to_str` (which takes
   `*mut objc::runtime::Object`) works with a simple `.cast()` — same memory
   layout, no import of `objc::runtime::Object` needed.

6. **Clipboard write simplified.** Replaced
   `writeObjects(NSArray::arrayWith Object(...))` with
   `setString_forType(&NSString, NSPasteboardTypeString)`, using
   `objc2_foundation::NSString::from_str()` directly instead of the shared
   `nsstring()` helper. This avoids `NSPasteboardWriting` protocol complexity
   and the cocoa `nsstring()` return type (`StrongPtr`).

Files modified:

- `wezboard/window/src/os/macos/app.rs` — Full migration
- `wezboard/window/src/os/macos/clipboard.rs` — Full migration
- `wezboard/window/src/os/macos/connection.rs` — One-line caller fix
  (`*delegate as *mut AnyObject` → `&*delegate`)
- `wezboard/Cargo.toml` — Added `NSApplication` and `NSPasteboard` features to
  `objc2-app-kit`

### Experiment 2: Migrate `menu.rs` and `connection.rs`

Migrate the two medium-sized files. `menu.rs` has ~8 cocoa trait calls plus a
`ClassDecl` wrapper class. `connection.rs` has ~10 cocoa calls including
`NSApp()`, `NSScreen`, and `NSArray`. After this experiment, only `window.rs`,
`mod.rs`, and `core_text.rs` still use cocoa.

#### Changes

**Files to modify:**

1. `wezboard/window/src/os/macos/menu.rs` — Full migration
2. `wezboard/window/src/os/macos/connection.rs` — Full migration
3. `wezboard/window/src/os/macos/app.rs` — Remove `sel2to1` import (no longer
   needed)
4. `wezboard/window/src/os/macos/window.rs` — Two one-line caller fixes
5. `wezboard/Cargo.toml` — Add objc2-app-kit feature flags

**`wezboard/Cargo.toml`** — Add features:

```toml
objc2-app-kit = { version = "0.3", features = [
  "NSApplication", "NSEvent", "NSMenu", "NSMenuItem",
  "NSPasteboard", "NSRunningApplication", "NSScreen"
] }
```

`NSRunningApplication` is needed because `NSApplicationActivationPolicy` is
defined there. `NSEvent` is needed for `NSEventModifierFlags`.

**`menu.rs`** — Replace all imports:

| Old (`cocoa` / `objc` 0.2)                       | New (`objc2`)                                      |
| ------------------------------------------------ | -------------------------------------------------- |
| `cocoa::appkit::{NSApp, NSApplication, ...}`     | `objc2_app_kit::NSApplication`                     |
| `cocoa::appkit::{NSMenu, NSMenuItem}`            | `objc2_app_kit::{NSMenu, NSMenuItem}`              |
| `pub use cocoa::appkit::NSEventModifierFlags`    | `pub use objc2_app_kit::NSEventModifierFlags`      |
| `pub use cocoa::base::SEL`                       | Remove (unused externally)                         |
| `cocoa::base::{id, nil}`                         | Typed `Retained<T>` / `*mut AnyObject`             |
| `cocoa::foundation::NSInteger`                   | `objc2_foundation::NSInteger`                      |
| `objc::declare::ClassDecl`                       | `objc2::runtime::ClassBuilder`                     |
| `objc::rc::StrongPtr`                            | `objc2::rc::Retained<T>`                           |
| `objc::runtime::{Class, Object, Sel, BOOL, ...}` | `objc2::runtime::{AnyClass, AnyObject, Bool, Sel}` |
| `cls1to2`, `get_class`, `sel2to1`                | Direct objc2 APIs                                  |

Struct changes:

- `Menu { menu: StrongPtr }` → `Menu { menu: Retained<NSMenu> }`
- `MenuItem { item: StrongPtr }` → `MenuItem { item: Retained<NSMenuItem> }`

Method replacements:

- `Menu::new_with_title` — `NSMenu::alloc(nil).initWithTitle_(...)` →
  `NSMenu::initWithTitle(NSMenu::alloc(), &NSString::from_str(title))`
- `Menu::autorelease` — Return `*mut AnyObject`. Use `Retained::into_raw()` then
  `msg_send![ptr, autorelease]` to match ObjC memory convention.
- `Menu::item_at_index` — `self.menu.itemAtIndex_(index)` →
  `self.menu.itemAtIndex(index)` which returns `Option<Retained<NSMenuItem>>`.
  No null check needed.
- `Menu::assign_as_main_menu` — `NSApp().setMainMenu_(...)` →
  `NSApplication::sharedApplication(mtm).setMainMenu(Some(&self.menu))`. Get
  `MainThreadMarker` with `MainThreadMarker::new().unwrap()`.
- `Menu::get_main_menu` — `NSApp().mainMenu()` →
  `NSApplication::sharedApplication(mtm).mainMenu()` which returns
  `Option<Retained<NSMenu>>`.
- `Menu::assign_as_help_menu` — Use typed
  `ns_app.setHelpMenu(Some(&self.menu))`.
- `Menu::assign_as_windows_menu` — Use typed
  `ns_app.setWindowsMenu(Some(&self.menu))`.
- `Menu::assign_as_services_menu` — Use typed
  `ns_app.setServicesMenu(Some(&self.menu))`.
- `Menu::assign_as_app_menu` — Keep `msg_send!` for `setAppleMenu:` (private API
  not in objc2-app-kit). Use `&*self.menu as *const NSMenu as *const AnyObject`
  or similar cast.
- `Menu::add_item` — `self.menu.addItem_(...)` →
  `self.menu.addItem(&item.item)`.
- `Menu::item_with_title` — Keep `msg_send!` (method exists in objc2-app-kit as
  `itemWithTitle` but simpler to stay consistent). Return wraps in
  `Retained::retain`.
- `Menu::remove_all_items` — Use typed `self.menu.removeAllItems()`.
- `Menu::remove_item` — Use typed `self.menu.removeItem(&item.item)`.
- `Menu::items` — `numberOfItems` via `msg_send!` or typed method.

- `MenuItem::new_separator` — `NSMenuItem::separatorItem(nil)` →
  `NSMenuItem::separatorItem(MainThreadMarker::new().unwrap())`.
- `MenuItem::new_with` — `NSMenuItem::alloc(nil).initWithTitle_action_...` →
  `NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(), ...)`.
  Action param changes from `Option<SEL>` (`cocoa::base::SEL`) to `Option<Sel>`
  (`objc2::runtime::Sel`).
- `MenuItem::with_menu_item(item: id)` — Change param to `*mut AnyObject`. Cast
  internally to `&NSMenuItem` for `Retained::retain`. Window.rs caller adds
  `.cast()`.
- `MenuItem::get_action` — Use typed `self.item.action()` which returns
  `Option<Sel>`.
- `MenuItem::set_target(target: id)` — Change to `*mut AnyObject`, cast to
  `Option<&AnyObject>`.
- `MenuItem::set_sub_menu` — `self.item.setSubmenu_(...)` →
  `self.item.setSubmenu(Some(&menu.menu))`.
- `MenuItem::get_sub_menu` — `self.item.submenu()` returns
  `Option<Retained<NSMenu>>`.
- `MenuItem::get_parent_item` — `self.item.parentItem()` returns
  `Option<Retained<NSMenuItem>>`.
- `MenuItem::get_menu` — Use typed `self.item.menu()` returns
  `Option<Retained<NSMenu>>`.
- `MenuItem::set_tag` — Use typed `self.item.setTag(tag)`.
- `MenuItem::get_tag` — Use typed `self.item.tag()`.
- `MenuItem::get_title` — Use typed `self.item.title()` returns
  `Retained<NSString>`.
- `MenuItem::set_title` — Use typed
  `self.item.setTitle(&NSString::from_str(s))`.
- `MenuItem::set_key_equivalent` — Use typed
  `self.item.setKeyEquivalent(&NSString::from_str(s))`.
- `MenuItem::set_tool_tip` — Use typed
  `self.item.setToolTip(Some(&NSString::from_str(s)))`.
- `MenuItem::set_represented_object` — Use typed
  `self.item.setRepresentedObject(...)`.
- `MenuItem::get_represented_object` — Use typed `self.item.representedObject()`
  returns `Option<Retained<AnyObject>>`.
- `MenuItem::set_key_equiv_modifier_mask` — Use typed
  `self.item.setKeyEquivalentModifierMask(mods)`. The parameter type changes
  from `cocoa::appkit::NSEventModifierFlags` to
  `objc2_app_kit::NSEventModifierFlags` — same bit values, different type.

Wrapper class (`get_wrapper_class`):

- `ClassDecl::new(...)` →
  `ClassBuilder::new(c"WezboardNSMenuRepresentedItem", AnyClass::get(c"NSObject").unwrap())`
- `dealloc` callback: Change `&mut Object` to `*mut AnyObject`. Replace
  `superclass(this)` (from window.rs) with
  `(*this).class().superclass().unwrap()` — pure objc2, no dependency on
  window.rs helper. Use deprecated `get_ivar` on `&*this`.
- `is_equal` callback: Change `&mut Object, *mut Object` to
  `*mut AnyObject, *mut AnyObject`. Return `Bool` instead of `BOOL`.
- `RepresentedItem::wrap` — Use `ClassBuilder`-registered class via
  `AnyClass::get(c"...")` instead of `cls1to2(get_wrapper_class())`. Use
  deprecated `get_mut_ivar` on `&mut *ptr`.
- `RepresentedItem::ref_item(wrapper: id)` — Change param to `*mut AnyObject`.
  Use deprecated `get_ivar` on `&*wrapper`.

**`connection.rs`** — Replace all imports:

| Old (`cocoa`)                                     | New (`objc2`)                                                         |
| ------------------------------------------------- | --------------------------------------------------------------------- |
| `cocoa::appkit::{NSApp, NSApplication, ...}`      | `objc2_app_kit::{NSApplication, NSApplicationActivationPolicy}`       |
| `cocoa::appkit::NSScreen`                         | `objc2_app_kit::NSScreen`                                             |
| `cocoa::base::{id, nil}`                          | Typed `Retained<T>` / `*mut AnyObject`                                |
| `cocoa::foundation::{NSArray, NSInteger}`         | `objc2_foundation::NSInteger` (NSArray access via typed NSScreen API) |
| `objc::runtime::Object` (in nsstring_to_str cast) | `.cast()` pattern from experiment 1                                   |

Struct change:

- `Connection { ns_app: id }` → `Connection { ns_app: Retained<NSApplication> }`

Method replacements:

- `create_new` — `NSApp()` → `NSApplication::sharedApplication(mtm)` where
  `mtm = MainThreadMarker::new().unwrap()`. `setActivationPolicy_` →
  `ns_app.setActivationPolicy(NSApplicationActivationPolicy::Regular)`.
- `terminate_message_loop` — `NSApp() as *const AnyObject` →
  `NSApplication::sharedApplication(mtm)`, then use `ns_app.stop(None)` and
  `msg_send![&*ns_app, abortModal]`.
- `get_appearance` — `self.ns_app as *const AnyObject` →
  `&*self.ns_app as *const NSApplication as *const AnyObject` for `msg_send!`
  calls. The `nsstring_to_str` cast uses `.cast()`.
- `run_message_loop` — `self.ns_app.run()` stays the same (Retained derefs).
- `hide_application` — Use typed `self.ns_app.hide(Some(&*self.ns_app))`.
  Actually `hide:` takes `Option<&AnyObject>` as sender — use
  `msg_send![&*self.ns_app, hide: &*self.ns_app as &AnyObject]` or the typed
  method if available.
- `screens` — `NSScreen::screens(nil)` →
  `NSScreen::screens(MainThreadMarker::new().unwrap())` returns
  `Retained<NSArray<NSScreen>>`. Iterate with `.count()` (via `msg_send!` or
  `NSArray::count`) and `.objectAtIndex(i)`. `NSScreen::mainScreen(nil)` →
  `NSScreen::mainScreen(mtm)` returns `Option<Retained<NSScreen>>`.

`nsscreen_to_screen_info`:

- Change signature from
  `pub fn nsscreen_to_screen_info(screen: *mut objc::runtime::Object)` to
  `pub fn nsscreen_to_screen_info(screen: &NSScreen)`.
- `NSScreen::frame(screen)` → `screen.frame()` returns `NSRect` (which is
  `CGRect` from objc2-foundation).
- `NSScreen::convertRectToBacking_(screen, frame)` →
  `screen.convertRectToBacking(frame)`.
- Remove `respondsToSelector:` checks for `localizedName` and
  `maximumFramesPerSecond` — these APIs are available since macOS 10.15 and
  wezboard's minimum deployment target is 10.15+. Use `screen.localizedName()`
  and `screen.maximumFramesPerSecond()` directly.
- `nsstring_to_str(name_obj as *mut objc::runtime::Object)` → use
  `screen.localizedName()` which returns `Retained<NSString>`, then convert via
  `.to_string()` (NSString implements Display) or `nsstring_to_str` with cast.

**`app.rs`** — Remove the `sel2to1` import. The only `sel2to1` call was for
`MenuItem::new_with`'s SEL parameter. After menu.rs migration,
`MenuItem::new_with` takes `Option<Sel>` (objc2), so `app.rs` passes
`Some(objc2::sel!(...))` directly.

**`window.rs`** — Two one-line caller fixes:

1. `MenuItem::with_menu_item(menu_item)` →
   `MenuItem::with_menu_item(menu_item as *mut _ as *mut AnyObject)` (param type
   changed from `id` to `*mut AnyObject`).
2. `nsscreen_to_screen_info(screen)` →
   `nsscreen_to_screen_info(&*(screen as *const NSScreen))` (param type changed
   from `*mut Object` to `&NSScreen`).

**`wezboard/Cargo.toml`** — Add `NSMenu`, `NSMenuItem`, `NSScreen`,
`NSRunningApplication`, `NSEvent` features to `objc2-app-kit`.

#### Key patterns from experiment 1

- Use `*mut AnyObject` (not `&mut AnyObject`) as ClassBuilder callback receiver
  to avoid higher-ranked lifetime issue.
- Use `objc2::runtime::Bool` (not `bool`) for ObjC BOOL ivars and return types.
- Use `.cast()` to bridge `*mut AnyObject` → `*mut objc::runtime::Object` for
  `nsstring_to_str` calls (mod.rs helper not yet migrated).
- `NSApplicationTerminateReply` uses `TerminateCancel` / `TerminateNow` (not
  `NSTerminateCancel`).

#### Verification

1. `cd wezboard && cargo build` — zero errors, zero warnings
2. Confirm `menu.rs` has zero `cocoa::` or `objc::` imports
3. Confirm `connection.rs` has zero `cocoa::` or `objc::` imports
4. Confirm `app.rs` has zero `sel2to1` usage
5. Manual test: launch wezboard, verify menus work (File, Edit, Window, Help),
   verify dock menu, verify screen info in logs
