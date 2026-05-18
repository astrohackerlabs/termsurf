+++
status = "closed"
opened = "2026-03-16"
closed = "2026-03-16"
+++

# Issue 757: Disable overlay fade animation on tab switch

## Goal

Browser overlays appear and disappear instantly when switching tabs. No fade
in/out animation.

## Background

When switching between tabs, the browser overlay fades in and out instead of
appearing instantly. This is distracting and makes tab switching feel sluggish.

We previously disabled CALayer animations for overlay repositioning (the
`setDisableActions: YES` calls in `CATransaction` blocks throughout `conn.rs`).
The same approach should work for the show/hide transition.

The fade is caused by `sync_overlay_visibility()` in `conn.rs`, which calls
`setHidden:` on the overlay's flipped layer. By default, CoreAnimation
implicitly animates property changes on CALayers — including `hidden`. The fix
is to wrap the `setHidden:` call in a `CATransaction` with animations disabled,
the same pattern we already use everywhere else.

## Experiments

### Experiment 1: Wrap setHidden in CATransaction

#### Description

Wrap the `setHidden:` call in `sync_overlay_visibility()` (~line 1521 of
`conn.rs`) in a `CATransaction` block with `setDisableActions: YES`. This is the
same pattern used in 4 other places in the same file for disabling implicit
animations on CALayer property changes.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/conn.rs`**

In `sync_overlay_visibility()`, replace:

```rust
unsafe {
    use objc2::msg_send;
    use objc2::runtime::Bool;
    let layer = pane.ca_layer_flipped as *mut objc2::runtime::AnyObject;
    let hidden = if is_active { Bool::NO } else { Bool::YES };
    let _: () = msg_send![layer, setHidden: hidden];
}
```

With:

```rust
unsafe {
    use objc2::msg_send;
    use objc2::runtime::Bool;
    let ca = cls(b"CATransaction\0");
    let _: () = msg_send![ca, begin];
    let _: () = msg_send![ca, setDisableActions: Bool::YES];
    let layer = pane.ca_layer_flipped as *mut objc2::runtime::AnyObject;
    let hidden = if is_active { Bool::NO } else { Bool::YES };
    let _: () = msg_send![layer, setHidden: hidden];
    let _: () = msg_send![ca, commit];
}
```

#### Verification

```bash
scripts/build.sh wezboard
```

| #   | Test                         | Steps                                  | Expected                      |
| --- | ---------------------------- | -------------------------------------- | ----------------------------- |
| 1   | Tab switch hides instantly   | Open webview, switch to another tab    | Overlay disappears instantly  |
| 2   | Tab switch shows instantly   | Switch back to webview tab             | Overlay appears instantly     |
| 3   | No regression on positioning | Split pane with webview, resize window | Overlay repositions correctly |

**Result:** Pass

All three tests pass. Overlays appear and disappear instantly.

#### Conclusion

Wrapping `setHidden:` in a `CATransaction` with `setDisableActions: YES`
disables the implicit fade animation. Same pattern used everywhere else in
`conn.rs`.

## Conclusion

The overlay fade was caused by CoreAnimation implicitly animating the `hidden`
property change. Wrapping it in a `CATransaction` with disabled actions fixes it
— one line of the same pattern already used throughout the file.
