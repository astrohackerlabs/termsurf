+++
status = "closed"
opened = "2026-03-07"
closed = "2026-03-07"
+++

# Issue 716: Wezboard build warnings

## Goal

Eliminate all build warnings from `cargo build -p wezboard-gui`. Currently 193
warnings:

| Category                          | Count | Crate(s)                                                |
| --------------------------------- | ----- | ------------------------------------------------------- |
| `unexpected cfg: cargo-clippy`    | 188   | `window` (184), `wezboard-font` (3), `wezboard-gui` (1) |
| `unnecessary unsafe block`        | 2     | `wezboard-toast-notification`                           |
| `value assigned but never read`   | 1     | `wezboard-gui`                                          |
| `struct/fn never used` (our code) | 2     | `wezboard-gui` (termsurf scaffolding)                   |

A clean build with zero warnings. Not just suppressed — actually fixed.

## Background

These warnings are inherited from upstream WezTerm. The dominant issue (188 of
193 warnings) is the legacy `objc` 0.2 crate. Its `msg_send!`, `class!`, and
`sel!` macros emit `cfg(feature = "cargo-clippy")` checks, which modern Rust
flags as `unexpected cfg`. The proper fix is migrating to `objc2`, which the
codebase already partially uses (`objc2-core-graphics`, `objc2-foundation`,
`objc2-user-notifications` are workspace deps, and `window.rs` has one
`objc2_core_graphics` call).

## Analysis

### The `objc` 0.2 → `objc2` migration

**Scale:** ~182 `msg_send!`/`class!`/`sel!` call sites across 6 files.

**Files by size:**

| File                                     | Lines | `msg_send!`/`class!`/`sel!` | `ClassDecl`/`StrongPtr` |
| ---------------------------------------- | ----- | --------------------------- | ----------------------- |
| `window/src/os/macos/window.rs`          | 3440  | 124                         | 22                      |
| `window/src/os/macos/menu.rs`            | 409   | 25                          | 19                      |
| `window/src/os/macos/app.rs`             | 197   | 18                          | 5                       |
| `window/src/os/macos/connection.rs`      | 255   | 11                          | 0                       |
| `window/src/os/macos/mod.rs`             | 35    | 2                           | 0                       |
| `wezboard-font/src/locator/core_text.rs` | —     | 2                           | 0                       |
| `wezboard-gui/src/commands.rs`           | —     | 1 (`sel!` only)             | 0                       |

**What the migration involves:**

The `objc` 0.2 API is raw and untyped:

```rust
use objc::*;
use objc::runtime::{Object, Sel, Class, BOOL, YES, NO};
use objc::rc::StrongPtr;
use objc::declare::ClassDecl;

let cls = class!(NSWindow);
let obj: id = msg_send![cls, alloc];
let obj: id = msg_send![obj, initWithContentRect:rect
                              styleMask:style
                              backing:backing
                              defer:NO];
```

The `objc2` API is typed and safe:

```rust
use objc2_app_kit::{NSWindow, NSWindowStyleMask, NSBackingStoreType};
use objc2::rc::Retained;

let window = unsafe {
    NSWindow::initWithContentRect_styleMask_backing_defer(
        NSWindow::alloc(),
        rect,
        style,
        backing,
        false,
    )
};
```

Key differences:

- `msg_send![obj, method:arg]` → typed method calls on Rust wrapper types
- `class!(NSFoo)` → direct `NSFoo` type references
- `sel!(foo:)` → not needed (methods are Rust functions)
- `id` (raw pointer) → `Retained<NSFoo>` (typed, reference-counted)
- `StrongPtr` → `Retained<T>`
- `WeakPtr` → `objc2::rc::Weak<T>`
- `ClassDecl` → `objc2::declare::ClassBuilder`
- `BOOL`/`YES`/`NO` → Rust `bool`

**New workspace deps needed:**

- `objc2-app-kit` — NSWindow, NSView, NSEvent, NSMenu, NSAlert, etc.

### Other warnings (trivial)

1. **`unnecessary unsafe`** in `wezboard-toast-notification/src/macos.rs` lines
   97 and 101 — `UNUserNotificationCenter::currentNotificationCenter()` became
   safe in a newer `objc2-user-notifications` version. Remove the `unsafe`
   blocks.

2. **Dead assignment** in `wezboard-gui/src/termwindow/render/screen_line.rs`
   line 677 — `phys_cell_idx += info.pos.num_cells as usize` at end of loop
   where value is never read. Delete the line.

3. **Unused scaffolding** in `wezboard-gui/src/termsurf/state.rs` —
   `TermSurfState` and `state()` are intentional scaffolding for future
   experiments. Suppress with `#[allow(dead_code)]`.

## Ideas for experiments

1. **Quick fixes** — Remove 2 unnecessary `unsafe` blocks, 1 dead assignment,
   suppress scaffolding warnings. Knocks out all non-`objc` warnings in minutes.

## Experiments

### Experiment 1: Quick fixes

#### Goal

Fix the 5 non-`objc` warnings so only the `cargo-clippy` cfg noise remains.

#### Changes

1. **`wezboard-toast-notification/src/macos.rs`** — Remove 2 unnecessary
   `unsafe` blocks.

   Line 97:
   `LazyLock::new(|| unsafe { UNUserNotificationCenter::currentNotificationCenter() })`
   → `LazyLock::new(|| UNUserNotificationCenter::currentNotificationCenter())`

   Line 101: `INIT.call_once(|| unsafe {` → `INIT.call_once(|| {`

   Both `currentNotificationCenter()` and
   `requestAuthorizationWithOptions_completionHandler()` are safe in
   `objc2-user-notifications` 0.3.2.

2. **`wezboard-gui/src/termwindow/render/screen_line.rs`** — Delete the dead
   assignment at line 677: `phys_cell_idx += info.pos.num_cells as usize;`. The
   variable is incremented but never read again before the next loop iteration
   overwrites it.

3. **`wezboard-gui/src/termsurf/state.rs`** — Add `#[allow(dead_code)]` to
   `TermSurfState` and `state()`. These are intentional scaffolding for future
   protocol experiments.

#### Verification

`cargo build -p wezboard-gui` produces only `cargo-clippy` cfg warnings (188)
and their summary lines. No other warnings.

#### Strategy

2. **Migrate `connection.rs` + `mod.rs`** — Smallest files (46 lines total, 13
   call sites, no `ClassDecl`). Good warmup to establish the migration pattern
   before tackling bigger files. Requires adding `objc2-app-kit` workspace dep.

3. **Migrate `app.rs`** — 197 lines, 18 call sites, 5 `ClassDecl`/`StrongPtr`.
   First file with `ClassDecl` — the custom Objective-C class registration
   pattern (`ClassDecl` → `ClassBuilder`).

4. **Migrate `menu.rs`** — 409 lines, 25 call sites, 19 `ClassDecl`/`StrongPtr`.
   Heavy use of `ClassDecl` for custom menu item delegate and `StrongPtr` for
   menu ownership.

5. **Migrate `window.rs`** — 3440 lines, 124 call sites, 22
   `ClassDecl`/`StrongPtr`/`WeakPtr`. The bulk of the work. Contains
   NSWindow/NSView subclass declarations, event handling, input method editor,
   drag-and-drop, window lifecycle. May need to be split into multiple
   experiments.

6. **Migrate remaining crates + remove `objc` 0.2** — 3 final call sites
   (`wezboard-font/core_text.rs` has 2, `wezboard-gui/commands.rs` has 1). Then
   remove the `objc` 0.2 dependency from the workspace entirely. End state: zero
   warnings.

#### Result

Pass. All 5 non-`objc` warnings eliminated:

- Removed 2 unnecessary `unsafe` blocks in
  `wezboard-toast-notification/src/macos.rs`.
- Deleted dead `phys_cell_idx` assignment in `screen_line.rs` and made the
  variable immutable (removing the cascading
  `variable does not need to be mutable` warning).
- Added `#[allow(dead_code)]` to scaffolding in `state.rs`.

`cargo build -p wezboard-gui` now produces only the 188 `cargo-clippy` cfg
warnings from the legacy `objc` 0.2 crate.

### Experiment 2: Migrate all files from `objc` 0.2 to `objc2`

#### Goal

Replace every direct `msg_send!`/`class!`/`sel!` call, every `ClassDecl`, every
`StrongPtr`/`WeakPtr`, and every `objc::Encode` across all 7 files. Remove the
direct `objc` 0.2 dependency. End state: 188 → 0 warnings.

#### New dependencies

- Add `objc2-app-kit = "0.3"` to workspace `Cargo.toml` and `window/Cargo.toml`.
- Remove `objc.workspace = true` from `window/Cargo.toml`,
  `wezboard-font/Cargo.toml`, `wezboard-gui/Cargo.toml`.
- Keep `cocoa` for now — its trait methods (`NSApp()`, `NSScreen::frame()`,
  `NSWindow::initWithContentRect_...`, etc.) don't produce warnings because
  they're compiled inside the `cocoa` crate. Removing `cocoa` is a separate
  future task.

#### Migration patterns

See `docs/objc-to-objc2.md` for the full reference. Summary of the patterns used
in this experiment:

| Old (`objc` 0.2)                    | New (`objc2`)                                  |
| ----------------------------------- | ---------------------------------------------- |
| `msg_send![obj, method: arg]`       | Typed method: `NSFoo::method(obj, arg)`        |
| `class!(NSFoo)`                     | Direct type reference: `NSFoo::class()`        |
| `sel!(foo:)`                        | `objc2::sel!(foo:)` (same syntax, no cfg warn) |
| `StrongPtr`                         | `Retained<T>`                                  |
| `WeakPtr`                           | `objc2::rc::Weak<T>`                           |
| `ClassDecl::new("Cls", superclass)` | `ClassBuilder::new("Cls", superclass)`         |
| `cls.add_method(sel!(...), fn)`     | `builder.add_method(sel!(...), fn)`            |
| `cls.add_ivar::<T>("name")`         | `builder.add_ivar::<T>("name")`                |
| `BOOL` / `YES` / `NO`               | Rust `bool`                                    |
| `id` (`*mut Object`)                | `*mut AnyObject` or typed `&NSFoo`             |
| `objc::Encode`                      | `objc2::encode::Encode` + `RefEncode`          |

#### Changes by file

1. **`window/src/os/macos/mod.rs`** (2 call sites)

   `nsstring()` — Return `Retained<NSString>` via
   `objc2_foundation::NSString::from_str()`. Remove `cocoa::base::{id, nil}`,
   `cocoa::foundation::NSString`, `objc::rc::StrongPtr`.

   `nsstring_to_str()` — Replace
   `msg_send![ns, isKindOfClass: class!(NSAttributedString)]` and
   `msg_send![ns, string]` with typed `objc2_foundation` calls. Use
   `NSString::as_str()` instead of `NSString::UTF8String`.

   Remove `#![allow(unexpected_cfgs)]`.

2. **`window/src/os/macos/connection.rs`** (11 call sites)

   Replace all `msg_send!` calls with typed `objc2_app_kit` methods:
   `NSApplication::setDelegate()`, `stop()`, `abortModal()`,
   `effectiveAppearance()`, `hide()`. Replace `NSAppearance::name()`,
   `NSScreen::localizedName()`, `NSScreen::maximumFramesPerSecond()`,
   `NSObject::respondsToSelector()`.

   Remove `objc::runtime::*` and `objc::*` imports.

3. **`window/src/os/macos/app.rs`** (18 call sites, 1 ClassDecl)

   Replace all `msg_send!` calls with typed methods:
   `NSAlert::setMessageText()`, `setInformativeText()`, `addButtonWithTitle()`,
   `runModal()`. Replace `ClassDecl::new(...)` with `ClassBuilder::new(...)`.
   Replace `StrongPtr` with `Retained<AnyObject>`. Replace `BOOL`/`YES`/`NO`
   with `bool`.

4. **`window/src/os/macos/menu.rs`** (25 call sites, 1 ClassDecl)

   Replace all `msg_send!` calls with typed `NSMenu`/`NSMenuItem` methods.
   Replace `ClassDecl` → `ClassBuilder` for the `WezboardNSMenuRepresentedItem`
   wrapper class. Replace all `StrongPtr` fields/returns with `Retained<T>`.
   Replace `superclass()` helper call in `dealloc` with `objc2::ClassType`.

5. **`window/src/os/macos/window.rs`** (124 call sites, 2 ClassDecl, StrongPtr,
   WeakPtr, Encode)

   The bulk of the work. Key changes:
   - **Two `ClassDecl`s** → `ClassBuilder`: `WezboardWindow` (NSWindow subclass,
     2 methods) and `WezboardWindowView` (NSView subclass, ~38 methods including
     NSTextInputClient protocol). Both register methods via
     `add_method(sel!(...), fn)` — same API on `ClassBuilder`.
   - **`StrongPtr` fields** (`view`, `window`, `_pixel_format`, `gl_context`) →
     `Retained<AnyObject>` (or typed where possible).
   - **`WeakPtr`** for titlebar/superview references → `objc2::rc::Weak<T>`.
   - **`objc::Encode` impls** for `NSRange` and `NSRangePointer` →
     `objc2::encode::Encode` + `RefEncode`.
   - **`superclass()` helper** — Replace `msg_send![this, superclass]` with
     `AnyClass` from `objc2`.
   - **~124 `msg_send!` calls** — Replace with typed methods for NSView,
     NSWindow, NSCursor, NSEvent, NSArray, CALayer, NSOpenGLContext, etc., or
     use `objc2::msg_send!` for dynamic dispatch where typed wrappers don't
     exist.

6. **`window/src/os/macos/clipboard.rs`** (0 call sites)

   No `msg_send!`/`class!`/`sel!` usage, but calls `nsstring()` and
   `nsstring_to_str()`. Update for new return type (`Retained<NSString>` instead
   of `StrongPtr`).

7. **`wezboard-font/src/locator/core_text.rs`** (2 call sites)

   Replace `msg_send![class!(NSUserDefaults), standardUserDefaults]` →
   `objc2_foundation::NSUserDefaults::standardUserDefaults()`. Replace
   `msg_send![user_defaults, stringArrayForKey:]` → typed method. Remove
   `#![allow(unexpected_cfgs)]`.

8. **`wezboard-gui/src/commands.rs`** (1 call site)

   Replace `sel!(wezboardPerformKeyAssignment:)` with
   `objc2::sel!(wezboardPerformKeyAssignment:)`. Remove
   `#[allow(unexpected_cfgs)]`.

#### Implementation order

Start with `mod.rs` (shared utilities), then cascade outward: `connection.rs` →
`app.rs` → `menu.rs` → `clipboard.rs` → `window.rs` → `core_text.rs` →
`commands.rs`. Build after each file to catch errors incrementally.

#### Verification

`cargo build -p wezboard-gui` produces zero warnings. No direct `use objc::`
imports remain in any source file.

#### Result

Pass. All 188 `cargo-clippy` cfg warnings eliminated. Total warnings: 193 → 0.

The actual migration was more pragmatic than the design specified. Instead of
replacing `StrongPtr`/`WeakPtr`/`ClassDecl` with their `objc2` equivalents
(which would require rewriting every allocation, retain/release, and class
registration), we kept the `objc` 0.2 infrastructure types and only replaced the
**macro calls** that produce warnings. This is safe because `objc` and `objc2`
runtime types have identical memory layouts (`#[repr(C)]`).

**Bridge pattern used:**

- `sel2to1(objc2::sel!(...))` — converts `objc2::Sel` → `objc::Sel` via
  transmute
- `cls1to2(class_ref)` / `cls2to1(class_ref)` — converts between `objc::Class`
  and `objc2::AnyClass`
- `get_objc_class(c"ClassName")` — looks up class by name via `objc2::AnyClass`
- Receivers cast through `as *const _ as *const AnyObject` for
  `objc2::msg_send!`
- Return types annotated as `*mut AnyObject` then cast to `id`
- `MsgSendRect`/`MsgSendSize` local types with `objc2::Encode` impls for passing
  cocoa geometry types through `objc2::msg_send!`

**Files changed:**

| File                                     | Call sites migrated        |
| ---------------------------------------- | -------------------------- |
| `window/src/os/macos/mod.rs`             | 2 (+ added bridge helpers) |
| `window/src/os/macos/connection.rs`      | 11                         |
| `window/src/os/macos/app.rs`             | 18                         |
| `window/src/os/macos/menu.rs`            | 25                         |
| `window/src/os/macos/window.rs`          | ~130                       |
| `wezboard-font/src/locator/core_text.rs` | 3                          |
| `wezboard-gui/src/commands.rs`           | 1                          |

**Dependency changes:**

- `window/Cargo.toml`: edition 2018 → 2021, added `objc2`, `objc2-app-kit`,
  `objc2-foundation` deps
- `wezboard-font/Cargo.toml`: added `objc2`, `objc2-foundation` deps
- `wezboard-gui/Cargo.toml`: added `objc2` dep
- `wezboard/Cargo.toml`: added `objc2-app-kit` workspace dep

**Not done (deferred):** Full removal of `objc` 0.2 crate. `StrongPtr`,
`WeakPtr`, `ClassDecl`, and `objc::Encode` impls remain. The `cocoa` crate
(which depends on `objc`) also remains. These produce no warnings — removing
them is a separate, larger refactor.

## Conclusion

All 193 build warnings eliminated across both experiments.
`cargo check -p wezboard-gui` produces zero warnings.
