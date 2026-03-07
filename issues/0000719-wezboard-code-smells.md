# Issue 719: Wezboard code smells from objc2 migration

## Goal

Fix all code smells introduced during the objc2 migration (Issues 715-718). The
migration was mechanically sound but left behind unwrap-on-fallible-init, magic
numbers, redundant CGRect copies, verbose boilerplate, missing safety comments,
dead code, and inconsistent patterns.

## Background

An audit of the full diff from the rename commit (`eeeefdc`) to HEAD identified
13 code smells. None are showstoppers, but several are real risks (panics on
null ObjC init, UB on null `Box::from_raw`, missing safety docs on raw pointer
arithmetic).

## Smells

### 1. `Retained::from_raw(...).unwrap()` on fallible ObjC inits

`Retained::from_raw` returns `Option`. If `alloc` + `init` returns null (OOM,
init failure), `.unwrap()` panics. The OpenGL cases already use `.ok_or_else()`
— the remaining sites should too.

Sites:

| File                            | Line | Object                   |
| ------------------------------- | ---- | ------------------------ |
| `window/src/os/macos/window.rs` | 539  | `NSWindow`               |
| `window/src/os/macos/window.rs` | 3477 | `NSView` (initWithFrame) |
| `window/src/os/macos/app.rs`    | 206  | AppDelegate              |
| `window/src/os/macos/menu.rs`   | 174  | menu wrapper             |

Fix: Replace `.unwrap()` with `.ok_or_else(|| anyhow!("failed to init X"))` and
propagate with `?`, or use `expect("description")` where the function returns
`()` and panic is acceptable.

### 2. Manual ivar pointer arithmetic without safety comments

`get_view_ivar` and `set_view_ivar` in `window.rs:1982-1991` compute ivar
offsets via raw pointer math. The `.unwrap()` on `instance_variable()` will
panic if the ivar name doesn't match. No `// SAFETY:` comments document
invariants.

Fix: Add `// SAFETY:` comments explaining the invariants (ivar exists because we
registered it in `ClassBuilder`, type is `*mut c_void`, object is a valid
instance of the expected class).

### 3. `Weak::load().unwrap()` double unwrap

`window.rs:1882`: `self.view_id.as_ref().unwrap().load().unwrap()` — panics if
the view has been deallocated. The old code had the same risk (null deref
instead of panic), but this should be proper error handling.

Fix: Return `anyhow::Result` and use `context()`.

### 4. Magic numbers for ObjC constants

Bare integer literals with inline comments replace what were previously named
constants from the `cocoa` crate:

| Value      | Meaning                         | File:Line        |
| ---------- | ------------------------------- | ---------------- |
| `236isize` | `NSOpenGLCPSurfaceOpacity`      | `window.rs:286`  |
| `222isize` | `NSOpenGLCPSwapInterval`        | `window.rs:293`  |
| `2_isize`  | `NSWindowTabbingModeDisallowed` | `window.rs:548`  |
| `1u64`     | `NSWindowMiniaturizeButton`     | `window.rs:1521` |
| `0u64`     | `NSWindowCloseButton`           | `window.rs:1521` |
| `2u64`     | `NSWindowZoomButton`            | `window.rs:1521` |

Fix: Define named constants at the top of the file.

### 5. `Box::from_raw` without null guard in menu dealloc

`menu.rs:317-320`: The `dealloc` callback calls `get_ivar` then
`Box::from_raw(item)` without checking for null. If the ivar is null (e.g., init
failed before setting it), this is instant UB.

Fix: Add a null check before `Box::from_raw`.

### 6. No-op `CGRect` re-wrapping (12 sites)

`CGRect::new(CGPoint::new(frame.origin.x, frame.origin.y), CGSize::new(frame.size.width, frame.size.height))`
copies a `CGRect` into an identical `CGRect`. This exists because the old code
converted between `cocoa::foundation::NSRect` and `CGRect` using a
`cg_to_ns_rect` helper. After the migration both sides are `CGRect`, making the
conversion a no-op.

12 sites in `window.rs`: lines 346, 348, 435, 437, 667, 669, 974, 976, 1129,
2318, 2322, 3475.

Fix: Delete the re-wrapping. Use the `CGRect` directly.

### 7. Verbose `__r` boilerplate pattern (44 sites)

```rust
let __r: *mut AnyObject = objc2::msg_send![...];
__r as id
```

This exists because `msg_send!` infers return type from context, and the `id`
alias doesn't trigger inference. 44 occurrences in `window.rs`.

Fix: Define an inline helper:

```rust
unsafe fn msg_send_id(args...) -> id {
    objc2::msg_send![...]
}
```

Or use a macro. Alternatively, annotate the return type directly where possible:
`let x: id = objc2::msg_send![...] as id;` — though this may not always work
with `msg_send!` inference.

### 8. `std::env::set_var` thread safety

`listener.rs:21`: `std::env::set_var("TERMSURF_SOCKET", &sock_path)` is unsafe
in Rust 2024 edition and not thread-safe. It's called during startup before
other threads read env vars, but this is fragile.

Fix: Use `unsafe { std::env::set_var(...) }` with a `// SAFETY:` comment, or
pass the socket path through a different channel (e.g., store in a global or
pass as an argument to child process spawning).

### 9. `type id = *mut AnyObject` defined in two files

`window.rs:52` and `core_text.rs:6` both define the same type alias. The alias
is used inconsistently — some code uses `id`, some uses `*mut AnyObject`
directly, and new code uses `Retained<AnyObject>`. The coexistence of raw `id`
and owned `Retained<AnyObject>` in the same functions is confusing.

Fix: Keep the alias for now (removing it touches ~100 sites), but ensure
consistency — new code should not mix `id` and `*mut AnyObject` in the same
function.

### 10. Inconsistent `#[allow(deprecated)]` vs manual ivar helpers

`app.rs` and `menu.rs` use `#[allow(deprecated)]` with `get_ivar`/`set_ivar`.
`window.rs` replaced them with manual `get_view_ivar`/`set_view_ivar` pointer
arithmetic. Two different approaches for the same problem.

Fix: Use `#[allow(deprecated)]` with `get_ivar`/`set_ivar` everywhere — it's
simpler and less error-prone than manual pointer arithmetic. Remove
`get_view_ivar`/`set_view_ivar` and use the deprecated API with suppression
instead.

### 11. Dead `TermSurfState`

`state.rs`: Empty struct behind `lazy_static` mutex, never used. Added as
scaffolding in Issue 715 Experiment 5.

Fix: Delete the file and the `mod state` declaration. Re-add when actually
needed.

### 12. Dead `yes_no!` macro

The audit flagged this but grep shows it no longer exists — already cleaned up.
No action needed.

### 13. Missing `// SAFETY:` comments on unsafe blocks

No `unsafe` blocks in the migration have safety comments. This is consistent
with the upstream WezTerm style but is a Rust anti-pattern.

Fix: Add `// SAFETY:` comments to the new unsafe blocks we introduced (ivar
helpers, `Retained::from_raw`, `Box::from_raw`, `msg_send!` blocks that do
non-obvious things). Don't retrofit comments to inherited upstream code.

## Experiments

### Experiment 1: Quick mechanical fixes

#### Goal

Fix smells 2, 4, 5, 6, 8, 11, 13 — all the changes that are pure cleanup with no
behavior change. Build and run the app to verify nothing breaks.

#### Changes

**Smell 2 — Safety comments on ivar helpers** (`window.rs:1982-1991`)

Add `// SAFETY:` comments to `get_view_ivar` and `set_view_ivar`:

```rust
// SAFETY: The VIEW_CLS_CNAME ivar is registered in the ClassBuilder for
// WezboardWindowView with type *mut c_void. The caller guarantees obj is a
// valid instance of that class.
unsafe fn get_view_ivar(obj: &AnyObject) -> *mut c_void {
```

Same pattern for `set_view_ivar`.

**Smell 4 — Named constants for magic numbers** (`window.rs`)

Add constants near the top of the file (after existing constant definitions):

```rust
const NS_OPENGL_CP_SURFACE_OPACITY: isize = 236;
const NS_OPENGL_CP_SWAP_INTERVAL: isize = 222;
const NS_WINDOW_TABBING_MODE_DISALLOWED: isize = 2;
const NS_WINDOW_CLOSE_BUTTON: u64 = 0;
const NS_WINDOW_MINIATURIZE_BUTTON: u64 = 1;
const NS_WINDOW_ZOOM_BUTTON: u64 = 2;
```

Replace all 6 inline magic numbers with these constants.

**Smell 5 — Null guard on `Box::from_raw`** (`menu.rs:315-324`)

Before:

```rust
extern "C" fn dealloc(this: *mut AnyObject, _sel: Sel) {
    unsafe {
        #[allow(deprecated)]
        let item = (*this).get_ivar::<*mut c_void>(WRAPPER_FIELD_NAME);
        let item = (*item) as *mut RepresentedItem;
        let item = Box::from_raw(item);
        drop(item);
```

After:

```rust
extern "C" fn dealloc(this: *mut AnyObject, _sel: Sel) {
    unsafe {
        #[allow(deprecated)]
        let item = (*this).get_ivar::<*mut c_void>(WRAPPER_FIELD_NAME);
        if !(*item).is_null() {
            let item = (*item) as *mut RepresentedItem;
            drop(Box::from_raw(item));
        }
```

**Smell 6 — Remove no-op CGRect re-wrapping** (`window.rs`, 12 sites)

Every site follows one of two patterns:

Pattern A — intermediate variable that's only used as a msg_send arg:

```rust
// Before:
let frame: CGRect = objc2::msg_send![..., frame];
let frame_cg = CGRect::new(CGPoint::new(frame.origin.x, ...), ...);
let backing_cg: CGRect = objc2::msg_send![..., convertRectToBacking: frame_cg];
let backing_frame = CGRect::new(CGPoint::new(backing_cg.origin.x, ...), ...);

// After:
let frame: CGRect = objc2::msg_send![..., frame];
let backing_frame: CGRect = objc2::msg_send![..., convertRectToBacking: frame];
```

Pattern B — intermediate variable used directly:

```rust
// Before:
let saved_cg = CGRect::new(CGPoint::new(saved_rect.origin.x, ...), ...);
objc2::msg_send![..., setFrame: saved_cg, display: true];

// After:
objc2::msg_send![..., setFrame: saved_rect, display: true];
```

Sites:

| Line | Pattern | What to do                                                   |
| ---- | ------- | ------------------------------------------------------------ |
| 346  | A       | Delete `frame_cg`, pass `frame` to `convertRectToBacking`    |
| 348  | A       | Delete `backing_frame`, use `backing_cg` directly            |
| 435  | A       | Delete `frame_cg`, pass `frame` to `contentRectForFrameRect` |
| 437  | A       | Delete `content_frame`, use `content_cg` directly            |
| 667  | A       | Same as 346                                                  |
| 669  | A       | Same as 348                                                  |
| 974  | A       | Same as 346                                                  |
| 976  | A       | Same as 348                                                  |
| 1129 | B       | Delete `saved_cg`, pass `saved_rect` directly                |
| 2318 | A       | Delete `frame_cg`, pass `frame` to `contentRectForFrameRect` |
| 2322 | A       | Delete `frame_cg`, pass `frame` to `convertRectToBacking`    |
| 3475 | A       | Delete `cg_rect`, pass `rect` directly to `initWithFrame`    |

**Smell 8 — `set_var` safety comment** (`listener.rs:21`)

Before:

```rust
std::env::set_var("TERMSURF_SOCKET", &sock_path);
```

After:

```rust
// SAFETY: Called during startup on the main thread before any child
// processes or threads read TERMSURF_SOCKET.
unsafe { std::env::set_var("TERMSURF_SOCKET", &sock_path) };
```

**Smell 11 — Delete dead `TermSurfState`**

Delete `wezboard-gui/src/termsurf/state.rs` entirely. Remove `pub mod state;`
from `wezboard-gui/src/termsurf/mod.rs`.

**Smell 13 — Safety comments on non-obvious unsafe blocks**

Add `// SAFETY:` comments to the `Retained::from_raw` calls (smells 1 & 3 get
proper error handling in a later experiment; for now just document the
invariants). Add comments to the `Box::from_raw` in menu dealloc (updated
above). Don't retrofit comments to inherited upstream unsafe blocks.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors, zero warnings
2. `cargo run --bin wezboard-gui` — app launches, window opens, terminal works
3. No magic numbers remain (grep for `236isize`, `222isize`, `2_isize`,
   `1u64 /*`, `0u64 /*`, `2u64 /*`)
4. No no-op CGRect re-wrapping remains
5. `state.rs` deleted
6. All new unsafe blocks have `// SAFETY:` comments

**Result:** Pass

All 7 smells fixed. Build passes with zero errors. App launches, window opens,
TermSurf socket listens, clean shutdown on quit. Grep confirms no magic numbers
or no-op CGRect copies remain. `state.rs` deleted. All new unsafe blocks have
`// SAFETY:` comments.

#### Conclusion

Pure mechanical cleanup — no behavior changes, no regressions. The remaining
smells (1, 3, 7, 9, 10) require deeper refactoring: error handling for fallible
ObjC inits (1, 3), boilerplate reduction (7), type alias consolidation (9), and
ivar approach unification (10). These should be tackled in Experiment 2.

### Experiment 2: Fallible ObjC init error handling

#### Goal

Replace `.unwrap()` on `Retained::from_raw` and `Weak::load` with proper error
handling so that null ObjC init returns produce actionable errors instead of
panics. Fixes smells 1 and 3.

#### Changes

**Smell 1 — `Retained::from_raw(...).unwrap()` on fallible ObjC inits**

All four sites already live in functions that return `anyhow::Result`, so each
`.unwrap()` becomes `.ok_or_else(|| anyhow!("..."))` + `?`.

**Site A: NSWindow init** (`window.rs:555`)

```rust
// Before:
let window = Retained::from_raw(window).unwrap();

// After:
let window = Retained::from_raw(window)
    .ok_or_else(|| anyhow!("failed to init NSWindow"))?;
```

**Site B: NSView initWithFrame** (`window.rs:~3588`)

```rust
// Before:
Retained::from_raw(__r).unwrap()

// After:
Retained::from_raw(__r)
    .ok_or_else(|| anyhow!("failed to init NSView"))?
```

**Site C: AppDelegate init** (`app.rs:206`)

`create_app_delegate` currently returns `Retained<AnyObject>`. Change it to
return `anyhow::Result<Retained<AnyObject>>` and propagate with `?`. Update the
caller in `connection.rs:38` to add `?`.

```rust
// app.rs — before:
pub fn create_app_delegate() -> Retained<AnyObject> {
    ...
    Retained::from_raw(delegate).unwrap()
}

// app.rs — after:
pub fn create_app_delegate() -> anyhow::Result<Retained<AnyObject>> {
    ...
    Retained::from_raw(delegate)
        .ok_or_else(|| anyhow!("failed to init AppDelegate"))
}

// connection.rs — before:
let delegate = create_app_delegate();

// connection.rs — after:
let delegate = create_app_delegate()?;
```

**Site D: menu wrapper init** (`menu.rs:170`)

`RepresentedItem::wrap` currently returns `Retained<AnyObject>`. Change it to
return `anyhow::Result<Retained<AnyObject>>`. Propagate `?` to all callers
(likely `MenuItem` methods that call `.wrap()`).

```rust
// menu.rs — before:
fn wrap(self) -> Retained<AnyObject> {
    ...
    Retained::from_raw(wrapper).unwrap()
}

// menu.rs — after:
fn wrap(self) -> anyhow::Result<Retained<AnyObject>> {
    ...
    Retained::from_raw(wrapper)
        .ok_or_else(|| anyhow!("failed to init menu wrapper"))
}
```

**Smell 3 — `Weak::load().unwrap()` double unwrap** (`window.rs:1909`)

`enable_opengl` already returns `anyhow::Result`, so replace the double unwrap
with proper error handling:

```rust
// Before:
let view = self.view_id.as_ref().unwrap().load().unwrap();

// After:
let view = self
    .view_id
    .as_ref()
    .ok_or_else(|| anyhow!("view_id not set"))?
    .load()
    .ok_or_else(|| anyhow!("view has been deallocated"))?;
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. `cargo run --bin wezboard-gui` — app launches, window opens
3. Grep confirms no `Retained::from_raw(...).unwrap()` remains in our modified
   files
4. Grep confirms no `load().unwrap()` remains on `view_id`

**Result:** Pass

All 5 sites fixed. Build passes with zero errors. App launches, spawns window
(window_id: 1, pane_id: 1), TermSurf socket listens, clean shutdown on quit.
Grep confirms no `Retained::from_raw(...).unwrap()` or `load().unwrap()` remain
in modified files.

Pragmatic deviation from the plan: Site D (`menu.rs:170`) uses `.expect()`
instead of `Result` because `wrap()` is called from
`index_of_item_with_represented_item` (returns `Option<usize>`) and
`set_represented_item` (returns `()`). Making `wrap` return `Result` would
cascade through 4+ callers across `menu.rs`, `commands.rs`, and `app.rs` — none
of which return `Result`. NSObject `alloc`+`init` effectively cannot fail, so
`.expect("NSObject alloc+init returned nil")` gives an actionable message
without the signature cascade.

#### Conclusion

Smells 1 and 3 are fixed. Null ObjC init returns now produce actionable
`anyhow::Error` messages instead of bare panics at all sites where the function
signature supports it. The remaining smells (7, 9, 10) are boilerplate reduction
and consistency improvements — candidates for Experiment 3.

### Experiment 3: Boilerplate reduction and consistency

#### Goal

Fix smells 7, 9, and 10 — the last three smells. All are mechanical refactors
with no behavior change: eliminate the verbose `__r` boilerplate, consolidate
the duplicate `type id` alias, and unify ivar access to use
`#[allow(deprecated)]` everywhere.

#### Changes

**Smell 7 — Eliminate `__r` boilerplate (40 sites in `window.rs`)**

The two-line pattern:

```rust
let foo: id = {
    let __r: *mut AnyObject = objc2::msg_send![...];
    __r as id
};
```

exists because `msg_send!` infers its return type from context, and the `id`
alias (`*mut AnyObject`) doesn't trigger inference when used directly. The fix
is to annotate `__r` inline and collapse the block:

```rust
let foo: id = unsafe { objc2::msg_send![...] };
```

This works because `msg_send!` can infer `*mut AnyObject` when the `let` binding
has type `id` (which is `*mut AnyObject`). The `__r` intermediate and the block
wrapper become unnecessary.

Apply this to all ~40 sites in `window.rs` where the pattern appears. A few
sites use `__r` in more complex ways (e.g., `Retained::from_raw(__r)` or casting
to a different type) — leave those as-is since the `__r` serves a purpose there.

**Smell 9 — Consolidate duplicate `type id` alias**

`type id = *mut AnyObject` is defined in both:

- `wezboard/window/src/os/macos/window.rs:64`
- `wezboard/wezboard-font/src/locator/core_text.rs:6`

These are in separate crates, so they can't share a single definition without
adding a cross-crate dependency. Leave both definitions in place but add a
comment to each referencing the other, so future maintainers know about the
duplication:

```rust
// NOTE: also defined in wezboard-font/src/locator/core_text.rs
type id = *mut AnyObject;
```

This is the minimal fix — no code change, just documentation. A proper fix
(shared type alias crate or removing `id` entirely) would touch ~100 sites and
is out of scope.

**Smell 10 — Unify ivar access to `#[allow(deprecated)]`**

`window.rs` uses manual pointer arithmetic (`get_view_ivar`/`set_view_ivar`)
while `app.rs`, `menu.rs`, and `clipboard.rs` use `#[allow(deprecated)]` with
`get_ivar`/`get_mut_ivar`. The deprecated API is simpler and less error-prone.

Replace `get_view_ivar` and `set_view_ivar` with `#[allow(deprecated)]`
`get_ivar`/`get_mut_ivar` calls at all 4 call sites:

| File      | Line | Function        | Change                                        |
| --------- | ---- | --------------- | --------------------------------------------- |
| window.rs | 2143 | `get_view_ivar` | `#[allow(deprecated)] (*this).get_ivar(...)`  |
| window.rs | 2144 | `set_view_ivar` | `#[allow(deprecated)] set_ivar(..., null)`    |
| window.rs | 3573 | `get_view_ivar` | `#[allow(deprecated)] (*this).get_ivar(...)`  |
| window.rs | 3609 | `set_view_ivar` | `#[allow(deprecated)] get_mut_ivar(...) = ..` |

Then delete the `get_view_ivar` and `set_view_ivar` functions (lines 2015–2029).

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. `cargo run --bin wezboard-gui` — app launches, window opens
3. Grep confirms no `__r as id` remains in `window.rs` (except sites where `__r`
   feeds into `Retained::from_raw` or other non-trivial uses)
4. Grep confirms `get_view_ivar`/`set_view_ivar` functions are deleted
5. Grep confirms both `type id` definitions have cross-reference comments
