+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 431: the custom-shader focus update (update_focus)

## Description

Experiments 429–430 ported the time/resolution and cursor halves of
`updateCustomShaderUniformsForFrame`. This experiment ports its final piece: the
**focus** uniforms. `focus` is set from the window's focused state, and
`time_focus` is stamped with the frame time when focus was _gained_ (a focus
change to focused). The focused state and the "focus changed" flag are
parameters (deferring the live focus tracking); the method returns the updated
"changed" flag (upstream resets it when consumed). This completes
`updateCustomShaderUniformsForFrame`.

## Upstream behavior

In `updateCustomShaderUniformsForFrame` (`renderer/generic.zig`), after the
cursor block:

```zig
// Update focus uniforms
uniforms.focus = @intFromBool(self.focused);

// If we need to update the time our focus state changed then update
// it to our current frame time. …
if (self.custom_shader_focused_changed and self.focused) {
    uniforms.time_focus = uniforms.time;
    self.custom_shader_focused_changed = false;
}
```

`focus` is `1` when focused, `0` otherwise. `time_focus` is set to the frame
`time` only when the focus _changed and is now focused_ (i.e. focus was just
gained); when that fires, the renderer clears its
`custom_shader_focused_changed` flag.

## Rust mapping (`roastty/src/renderer/shadertoy.rs`)

`update_focus` takes the focused state and the "changed" flag, and returns the
flag's new value (cleared when consumed):

```rust
impl CustomShaderUniforms {
    /// Update the focus uniforms (upstream `updateCustomShaderUniformsForFrame`'s
    /// focus block): `focus` is `1` when `focused`, else `0`; `time_focus` is
    /// stamped with the frame `time` when focus was just gained
    /// (`focus_changed && focused`). Returns the new `focus_changed` flag
    /// (cleared to `false` when consumed — upstream resets
    /// `custom_shader_focused_changed`).
    pub(crate) fn update_focus(&mut self, focused: bool, focus_changed: bool) -> bool {
        self.focus = i32::from(focused);
        if focus_changed && focused {
            self.time_focus = self.time;
            return false;
        }
        focus_changed
    }
}
```

`focus = i32::from(focused)` is upstream's `@intFromBool`;
`time_focus = self.time` on a focus-gain (and the returned flag goes to `false`,
mirroring the reset of `custom_shader_focused_changed`); otherwise the flag is
returned unchanged.

## Scope / faithfulness notes

- **Ported (bridged)**: `CustomShaderUniforms::update_focus` — the focus uniform
  block of upstream's per-frame custom-shader update (`focus`, `time_focus`, and
  the focus-changed reset), completing `updateCustomShaderUniformsForFrame`.
- **Faithful**: `focus = 1`/`0` from the focused state; `time_focus = time` only
  when `focus_changed && focused` (focus gained); the returned flag is cleared
  to `false` exactly when that fires (upstream resets the flag) and unchanged
  otherwise.
- **Faithful adaptation**: the focused state and the "changed" flag are
  parameters (upstream reads `self.focused` /
  `self.custom_shader_focused_changed`), and the reset of the flag is returned
  to the caller (Rust has no `self.custom_shader_focused_changed` here).
  `i32::from(bool)` is `@intFromBool`.
- **Deferred**: the live focus tracking (the renderer's `focused` /
  `custom_shader_focused_changed` state and the focus-change message handling),
  the `updateCustomShaderUniformsFromState` group, the live timing source, the
  `Target` enum, and the shader loading. (Consumed by a later slice; this
  experiment lands and tests the focus update.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/shadertoy.rs`:
   - add
     `CustomShaderUniforms::update_focus(&mut self, focused: bool, focus_changed: bool) -> bool`.
2. Tests (in `shadertoy.rs`), with `time = 5.0`:
   - `update_focus(true, true)` → `focus == 1`, `time_focus == 5.0`, returns
     `false` (the flag was consumed);
   - `update_focus(true, false)` → `focus == 1`, `time_focus` unchanged (`0.0`),
     returns `false`;
   - `update_focus(false, true)` → `focus == 0`, `time_focus` unchanged, returns
     `true` (focus changed but not gained — the flag is **not** consumed);
   - `update_focus(false, false)` → `focus == 0`, `time_focus` unchanged,
     returns `false`;
   - and the other uniform fields (e.g. `frame`, `resolution`) untouched.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty update_focus
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `update_focus` sets `focus` from the focused state, stamps `time_focus = time`
  only on a focus-gain (`focus_changed && focused`), and returns the flag
  cleared exactly then (else unchanged) — faithful to upstream's focus block;
- the tests pass (the four `focused`/`focus_changed` combinations and the return
  values; the untouched fields), and the existing tests still pass;
- the live focus tracking and the from-state group stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if `focus` is wrong, `time_focus` is stamped when focus
was not gained, the returned flag is wrong (not cleared on a gain, or cleared
otherwise), an unrelated field changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the design matches upstream's focus block:
`focus = i32::from(focused)` maps to `@intFromBool(self.focused)`; `time_focus`
is stamped only for `focus_changed && focused` (the focus-gained case); and
returning `false` only when that condition fires faithfully models upstream
clearing `self.custom_shader_focused_changed`, while returning the input flag
otherwise preserves the "changed but not gained" state (including the
`focused = false, focus_changed = true` case). It judged taking `focused` and
`focus_changed` as parameters a good bounded slice, consistent with the earlier
custom-shader uniform updates, and the planned four-case test sufficient (the
truth table and the return semantics).

Review artifacts:

- Prompt: `logs/codex-review/20260604-093847-d431-prompt.md` (design)
- Result: `logs/codex-review/20260604-093847-d431-last-message.md` (design)
