# Migrating from `objc` 0.2 to `objc2`

Migration guide for Wezboard's `window` crate and related code. Based on
analysis of both repos (`vendor/rust-objc` and `vendor/objc2`) and the existing
Wezboard codebase.

## Overview

The `objc` 0.2 crate (by SSheldon) provides raw, untyped Objective-C bindings.
The `objc2` crate (by madsmtm) is its successor — typed, safer, and actively
maintained. Our codebase also uses the `cocoa` crate for AppKit/Foundation
types; `objc2-app-kit` and `objc2-foundation` replace it.

### Dependencies replaced

| Old                    | New                         |
| ---------------------- | --------------------------- |
| `objc` 0.2             | `objc2`                     |
| `cocoa` (AppKit types) | `objc2-app-kit`             |
| `cocoa` (Foundation)   | `objc2-foundation`          |
| (none)                 | `objc2-quartz-core` (if CA) |

## API mapping

### Types

| objc 0.2                     | objc2                             |
| ---------------------------- | --------------------------------- |
| `*mut Object` / `id`         | `Retained<T>` or `&T`             |
| `Object`                     | `AnyObject`                       |
| `Class`                      | `AnyClass`                        |
| `Sel`                        | `Sel` (same)                      |
| `Protocol`                   | `AnyProtocol`                     |
| `BOOL` / `YES` / `NO`        | `bool` (auto-converted)           |
| `StrongPtr`                  | `Retained<T>`                     |
| `WeakPtr`                    | `rc::Weak<T>`                     |
| `ClassDecl`                  | `define_class!` or `ClassBuilder` |
| `ProtocolDecl`               | `extern_protocol!`                |
| `Encode` trait               | `Encode` trait (from objc2)       |
| `MethodImplementation` trait | `MethodImplementation` trait      |

### Macros

| objc 0.2    | objc2                                          |
| ----------- | ---------------------------------------------- |
| `msg_send!` | `msg_send!` (improved, auto-converts `bool`)   |
| `class!`    | `class!` (returns `&'static AnyClass`)         |
| `sel!`      | `sel!` (same syntax)                           |
| (manual)    | `define_class!` (declarative class definition) |
| (manual)    | `extern_class!` (declare external classes)     |
| (manual)    | `extern_methods!` (typed method wrappers)      |
| (manual)    | `extern_protocol!` (protocol declarations)     |

### cocoa crate → objc2-app-kit / objc2-foundation

| cocoa                          | objc2 equivalent                      |
| ------------------------------ | ------------------------------------- |
| `cocoa::appkit::NSWindow`      | `objc2_app_kit::NSWindow`             |
| `cocoa::appkit::NSView`        | `objc2_app_kit::NSView`               |
| `cocoa::appkit::NSEvent`       | `objc2_app_kit::NSEvent`              |
| `cocoa::appkit::NSMenu`        | `objc2_app_kit::NSMenu`               |
| `cocoa::appkit::NSApplication` | `objc2_app_kit::NSApplication`        |
| `cocoa::appkit::NSScreen`      | `objc2_app_kit::NSScreen`             |
| `cocoa::appkit::NSAlert`       | `objc2_app_kit::NSAlert`              |
| `cocoa::appkit::NSCursor`      | `objc2_app_kit::NSCursor`             |
| `cocoa::base::id`              | `Retained<T>` or `&AnyObject`         |
| `cocoa::base::nil`             | `None` (use `Option<&T>`)             |
| `cocoa::foundation::NSString`  | `objc2_foundation::NSString`          |
| `cocoa::foundation::NSArray`   | `objc2_foundation::NSArray`           |
| `cocoa::foundation::NSRect`    | `objc2_foundation::NSRect`            |
| `cocoa::foundation::NSPoint`   | `objc2_foundation::NSPoint`           |
| `cocoa::foundation::NSSize`    | `objc2_foundation::NSSize`            |
| `NSWindowStyleMask::*`         | `objc2_app_kit::NSWindowStyleMask::*` |
| `NSBackingStoreBuffered`       | `NSBackingStoreType::Buffered`        |

## Pattern migrations

### 1. Simple message sends

**Before (objc 0.2):**

```rust
let cls = class!(NSObject);
let obj: *mut Object = msg_send![cls, new];
let hash: usize = msg_send![obj, hash];
let is_kind: BOOL = msg_send![obj, isKindOfClass: cls];
let _: () = msg_send![obj, release];
```

**After (objc2):**

```rust
let obj: Retained<NSObject> = unsafe { msg_send![NSObject::class(), new] };
let hash: usize = unsafe { msg_send![&obj, hash] };
let is_kind: bool = unsafe { msg_send![&obj, isKindOfClass: cls] };
// No manual release — Retained handles it on drop
```

Key differences:

- Return type is `Retained<T>` for `new`/`alloc`/`init`/`copy` families.
- `BOOL` auto-converts to `bool`.
- Receiver is `&obj` (reference), not raw pointer.
- No manual `release` — `Retained` drops automatically.

### 2. Typed framework methods (preferred over msg_send!)

When `objc2-app-kit` or `objc2-foundation` provides typed methods, use those
instead of `msg_send!`:

**Before:**

```rust
let title: id = msg_send![window, title];
let _: () = msg_send![window, setTitle: nsstring("Hello")];
let _: () = msg_send![window, center];
let _: () = msg_send![window, makeKeyAndOrderFront: nil];
```

**After:**

```rust
let title = window.title();
window.setTitle(ns_string!("Hello"));
window.center();
window.makeKeyAndOrderFront(None);
```

No `unsafe`, no `msg_send!`, fully typed.

### 3. StrongPtr → Retained

**Before:**

```rust
let obj = unsafe {
    let raw: *mut Object = msg_send![class!(NSObject), new];
    StrongPtr::new(raw)
};
let hash: usize = unsafe { msg_send![*obj, hash] };

// Clone retains
let clone = obj.clone();

// Weak reference
let weak = obj.weak();
let strong = weak.load(); // may be null
```

**After:**

```rust
let obj: Retained<NSObject> = unsafe { msg_send![NSObject::class(), new] };
let hash: usize = unsafe { msg_send![&obj, hash] };

// Clone retains
let clone = obj.clone();

// Weak reference
let weak = Weak::from_retained(&obj);
if let Some(strong) = weak.load() {
    // Object still alive
}
```

Key differences:

- `Retained<T>` is typed (knows the class), `StrongPtr` was untyped.
- Deref to `&T` instead of `*mut Object`.
- `Weak::load()` returns `Option<Retained<T>>`, not a possibly-null pointer.

### 4. ClassDecl → define_class!

This is the biggest change. The old API uses imperative runtime registration;
the new API is declarative.

**Before (objc 0.2):**

```rust
lazy_static! {
    static ref MY_VIEW_CLASS: &'static Class = unsafe {
        let superclass = class!(NSView);
        let mut decl = ClassDecl::new("MyView", superclass).unwrap();

        // Add instance variables
        decl.add_ivar::<*mut c_void>("rust_state");

        // Add methods — extern "C" functions with raw Object pointers
        decl.add_method(
            sel!(keyDown:),
            key_down as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(mouseDown:),
            mouse_down as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(drawRect:),
            draw_rect as extern "C" fn(&Object, Sel, NSRect),
        );

        // Add protocol conformance
        let proto = Protocol::get("NSTextInputClient").unwrap();
        decl.add_protocol(proto);
        decl.add_method(
            sel!(insertText:replacementRange:),
            insert_text as extern "C" fn(&Object, Sel, id, NSRange),
        );

        decl.register()
    };
}

// Method implementations are standalone extern "C" functions
extern "C" fn key_down(this: &Object, _sel: Sel, event: id) {
    unsafe {
        let state: *mut c_void = *this.get_ivar("rust_state");
        let state = &mut *(state as *mut MyState);
        let key_code: u16 = msg_send![event, keyCode];
        state.handle_key(key_code);
    }
}
```

**After (objc2):**

```rust
use objc2::define_class;
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSEvent};
use objc2_foundation::NSRange;
use std::cell::RefCell;
use std::ptr::NonNull;

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "MyView"]
    struct MyView {
        rust_state: RefCell<Option<NonNull<MyState>>>,
    }

    // Instance methods
    impl MyView {
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let state_ptr = self.rust_state.borrow();
            if let Some(ptr) = *state_ptr {
                let state = unsafe { ptr.as_ref() };
                state.handle_key(event.keyCode());
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let location = event.locationInWindow();
            // ...
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, rect: NSRect) {
            // ...
        }
    }

    // Protocol conformance
    unsafe impl NSTextInputClient for MyView {
        #[unsafe(method(insertText:replacementRange:))]
        fn insert_text(&self, string: &AnyObject, range: NSRange) {
            // ...
        }
    }
);

impl MyView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(MyViewIvars {
            rust_state: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }
}
```

Key differences:

- **Struct fields become ivars.** No manual `add_ivar` / `get_ivar`. Fields are
  accessed via auto-generated getters (`self.rust_state`).
- **Methods are regular Rust functions** with typed parameters (`&NSEvent`
  instead of `id`). No `extern "C"`, no `Sel` parameter.
- **Protocol conformance is explicit** with `unsafe impl ProtocolName for Type`.
- **Class registration is automatic** — no `lazy_static!` or manual
  `register()`.
- **Thread safety is declared** with `#[thread_kind = MainThreadOnly]`.

### 5. ClassDecl → ClassBuilder (imperative alternative)

If `define_class!` doesn't work for a specific case, `ClassBuilder` is the
imperative equivalent:

```rust
use objc2::runtime::ClassBuilder;
use std::ffi::CStr;

let superclass = NSObject::class();
let mut builder = ClassBuilder::new(c"MyClass", superclass).unwrap();

builder.add_ivar::<Cell<u32>>(c"_number");

unsafe {
    builder.add_method(sel!(number), get_number as extern "C" fn(&AnyObject, Sel) -> u32);
}

let cls = builder.register();
```

Note: `ClassBuilder` uses `&CStr` for names (with `c"..."` literals), not
`&str`.

### 6. Object ivar access

**Before:**

```rust
unsafe {
    let value: *mut c_void = *this.get_ivar("rust_state");
    let state = &mut *(value as *mut MyState);
}
```

**After (with define_class!):**

```rust
// Ivars are struct fields, accessed directly
let state = self.rust_state.borrow();
```

### 7. NSWindow creation

**Before (cocoa crate):**

```rust
let window = unsafe {
    let cls = class!(NSWindow);
    let w: id = msg_send![cls, alloc];
    let w: id = msg_send![w,
        initWithContentRect: rect
        styleMask: NSWindowStyleMask::NSTitledWindowMask
            | NSWindowStyleMask::NSClosableWindowMask
            | NSWindowStyleMask::NSResizableWindowMask
        backing: NSBackingStoreBuffered
        defer: NO
    ];
    StrongPtr::new(w)
};
```

**After (objc2-app-kit):**

```rust
let window = unsafe {
    NSWindow::initWithContentRect_styleMask_backing_defer(
        NSWindow::alloc(mtm),
        rect,
        NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Resizable,
        NSBackingStoreType::Buffered,
        false,
    )
};
unsafe { window.setReleasedWhenClosed(false) };
```

### 8. NSApplication lifecycle

**Before:**

```rust
let app: id = msg_send![class!(NSApplication), sharedApplication];
let _: () = msg_send![app, setDelegate: delegate];
let _: () = msg_send![app, run];
```

**After:**

```rust
let mtm = MainThreadMarker::new().unwrap();
let app = NSApplication::sharedApplication(mtm);
app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
app.run();
```

### 9. NSEvent access

**Before:**

```rust
let key_code: u16 = msg_send![event, keyCode];
let mods: NSEventModifierFlags = msg_send![event, modifierFlags];
let chars: id = msg_send![event, characters];
```

**After:**

```rust
let key_code = event.keyCode();
let mods = event.modifierFlags();
let chars = event.characters();
```

All typed, no `unsafe`, no `msg_send!`.

### 10. NSString conversion

**Before:**

```rust
fn nsstring(s: &str) -> id {
    unsafe { NSString::alloc(nil).init_str_(s) }
}

fn nsstring_to_str(ns: id) -> &str {
    unsafe {
        let bytes = ns.UTF8String();
        CStr::from_ptr(bytes).to_str().unwrap()
    }
}
```

**After:**

```rust
use objc2_foundation::{NSString, ns_string};

// Static string (zero allocation)
let s = ns_string!("Hello");

// Dynamic string
let s = NSString::from_str("dynamic");

// To Rust string
let rust_str = ns_string.to_string();
```

### 11. Delegate pattern

**Before:**

```rust
// Create delegate class at runtime
let mut decl = ClassDecl::new("MyWindowDelegate", class!(NSObject)).unwrap();
decl.add_method(
    sel!(windowWillClose:),
    window_will_close as extern "C" fn(&Object, Sel, id),
);
let cls = decl.register();

// Set delegate
let delegate: id = msg_send![cls, new];
let _: () = msg_send![window, setDelegate: delegate];
```

**After:**

```rust
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MyWindowDelegate"]
    struct MyWindowDelegate;

    unsafe impl NSObjectProtocol for MyWindowDelegate {}

    unsafe impl NSWindowDelegate for MyWindowDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, notification: &NSNotification) {
            let app = NSApplication::sharedApplication(self.mtm());
            app.terminate(None);
        }
    }
);

// Set delegate
let delegate = MyWindowDelegate::new(mtm);
unsafe { window.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };
```

### 12. nil handling

**Before:**

```rust
let _: () = msg_send![window, makeKeyAndOrderFront: nil];
if obj == nil { /* ... */ }
```

**After:**

```rust
window.makeKeyAndOrderFront(None);
// obj is Option<Retained<T>> — use .is_none()
```

`nil` becomes `None` in the `Option<&T>` or `Option<Retained<T>>` types.

### 13. BOOL handling

**Before:**

```rust
let result: BOOL = msg_send![obj, boolMethod];
if result == YES { /* ... */ }
let _: () = msg_send![obj, setBoolValue: YES];
```

**After:**

```rust
let result: bool = unsafe { msg_send![&obj, boolMethod] };
if result { /* ... */ }
let _: () = unsafe { msg_send![&obj, setBoolValue: true] };
```

`msg_send!` auto-converts between `BOOL` and `bool`.

### 14. Selector creation

**Before:**

```rust
let sel = sel!(performKeyEquivalent:);
```

**After:**

```rust
let sel = sel!(performKeyEquivalent:);  // Same syntax
```

No change needed.

### 15. Error handling pattern

**Before:**

```rust
let mut error: id = nil;
let result: id = msg_send![obj, doSomething: &mut error];
if !error.is_null() { /* handle error */ }
```

**After:**

```rust
let result: Result<Retained<T>, Retained<NSError>> =
    unsafe { msg_send![&obj, doSomethingAndReturnError: _] };
match result {
    Ok(value) => { /* success */ }
    Err(error) => { /* handle error */ }
}
```

The trailing `_` marker converts `NSError**` out-params to `Result`.

## Wezboard-specific notes

### Files to migrate

| File                                | Call sites | ClassDecl | Priority |
| ----------------------------------- | ---------- | --------- | -------- |
| `window/src/os/macos/window.rs`     | 124        | 2 classes | Last     |
| `window/src/os/macos/menu.rs`       | 25         | 1 class   | 4th      |
| `window/src/os/macos/app.rs`        | 18         | 1 class   | 3rd      |
| `window/src/os/macos/connection.rs` | 11         | 0         | 2nd      |
| `window/src/os/macos/mod.rs`        | 2          | 0         | 2nd      |
| `wezboard-font/core_text.rs`        | 2          | 0         | 1st      |
| `wezboard-gui/commands.rs`          | 1          | 0         | 1st      |

### Additional dependency: `cocoa` crate

The `window` crate imports types from the `cocoa` crate (`cocoa::appkit::*`,
`cocoa::foundation::*`, `cocoa::base::*`). These are separate from the `objc`
crate but will also be removed as part of this migration since `objc2-app-kit`
and `objc2-foundation` provide typed replacements for everything `cocoa` offers.

### Thread safety

objc2 enforces thread safety at the type level. All AppKit classes are
`MainThreadOnly`. Functions that create or interact with AppKit objects need a
`MainThreadMarker` parameter to prove they're on the main thread. This may
require threading the marker through existing code.

## References

- objc 0.2 source: `vendor/rust-objc/`
- objc2 source: `vendor/objc2/`
- objc2 examples: `vendor/objc2/examples/`
- `define_class!` macro: `vendor/objc2/crates/objc2/src/__macros/define_class/`
- `Retained<T>`: `vendor/objc2/crates/objc2/src/rc/retained.rs`
- objc2-app-kit: `vendor/objc2/framework-crates/objc2-app-kit/`
- Ecosystem migration tracker: https://github.com/madsmtm/objc2/issues/174
