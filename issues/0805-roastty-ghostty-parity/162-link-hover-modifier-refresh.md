# Experiment 162: Link hover modifier refresh

## Description

Experiment 161 proved deterministic link-hover preview dispatch when the mouse
position changes. Its completion review noted a nearby pinned Ghostty behavior
that remains unproven: Ghostty also refreshes link hover when key/modifier
events change the active mouse modifiers while the cursor stays in place.

Pinned Ghostty commit `2c62d182cec246764ff725096a70b9ef44996f7f` handles this in
`Surface.keyCallback`: when `self.mouse.mods` differs from the incoming key
event modifiers, it calls `modsChanged(event.mods)`, then refreshes links at the
current cursor position if mouse reporting is off or shift overrides reporting.
If mouse reporting is on and shift is not active, Ghostty clears `mouse_shape`
back to the terminal mouse shape and dispatches `mouse_over_link = ""`.

This experiment is intentionally limited to deterministic runtime action
dispatch caused by key/modifier events. It does not claim GUI cursor pixels,
native preview display, or OS behavior.

## Changes

- `roastty/src/lib.rs`
  - Add a key-event modifier refresh path that compares the current mouse mods
    with the remapped key event modifiers.
  - Update surface mouse modifiers through the same binding-modifier semantics
    Ghostty uses for mouse hover.
  - Refresh link hover at the existing mouse position when modifier changes make
    a link eligible, including ctrl/super enabling a regular or OSC8 link while
    the cursor has not moved.
  - Force a link re-evaluation or invalidate the same-cell no-link cache when
    modifiers change, so a stationary modifier press can discover a link at the
    current cell.
  - Match Ghostty's exact reporting branches: refresh when mouse reporting is
    off or when `shift && !mouseShiftCapture(false)`, and clear hover only when
    mouse reporting is active and shift is not active. A reporting state with
    captured shift should neither refresh nor clear.
  - Add focused tests that prove:
    - pressing super while stationary over the default URL link dispatches
      pointer shape and `mouse_over_link`;
    - releasing super while over the same link clears hover;
    - pressing super while stationary over OSC8 text dispatches the OSC8 URL;
    - normal mouse reporting suppresses modifier-driven hover refresh, while
      shift+super allows it through the existing shift override gate.
- `issues/0805-roastty-ghostty-parity/link_hover_modifier_refresh_parity.py`
  - Add a cheap guard for Ghostty anchors, Roastty implementation markers,
    tests, and inventory wording.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Update the existing `RUNTIME-012B2B2B2B2B2` complete row so it explicitly
    includes modifier-driven link-hover refresh.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Update the experiment index and Learnings if the experiment discovers
    reusable guidance.

## Verification

- Run the focused Rust tests:

  ```bash
  cargo fmt --manifest-path roastty/Cargo.toml
  ```

  ```bash
  cargo test --manifest-path roastty/Cargo.toml link_hover_modifier_refresh -- --test-threads=1
  ```

- Run the existing hover dispatch tests to ensure the mouse-position path still
  passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml link_hover_preview_dispatch -- --test-threads=1
  ```

- Run the new and existing parity guards:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/link_hover_modifier_refresh_parity.py
  PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/link_hover_preview_dispatch_parity.py
  ```

- Regenerate and validate the runtime inventory:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
  ```

The experiment passes if Roastty matches pinned Ghostty's deterministic
key/modifier-driven link hover refresh and clear behavior without broadening the
claim to GUI or OS effects.

## Design Review

**Reviewer:** Kant the 2nd (`019eca9d-733d-7983-bfb4-aea186c9a94e`)

**Result:** Changes required

The first review found two required design issues and one optional hardening
point:

- The clear behavior was too broad. Pinned Ghostty clears in the mouse-reporting
  branch only when shift is not active; reporting with captured shift should
  neither refresh nor clear.
- Verification omitted the required Rust formatting step.
- The design should explicitly require bypassing or invalidating the current
  same-cell no-link cache when modifiers change, otherwise a stationary modifier
  press could be skipped.

The design has been updated to require the exact Ghostty branches, include
`cargo fmt`, and call out the same-cell cache interaction.

**Re-review result:** Approved

The reviewer confirmed the required findings were resolved and approved the
design for implementation.

## Result

**Result:** Pass

Roastty now refreshes link hover state from key/modifier events when the active
mouse modifiers change, matching pinned Ghostty's deterministic
`keyCallback`/`modsChanged` branch. The implementation stores binding modifiers,
invalidates the same-cell link cache before allowed refreshes, refreshes when
mouse reporting is off or shift overrides reporting, clears with the current
terminal mouse shape when reporting is active without shift, and leaves captured
shift under reporting alone.

Focused Rust tests prove stationary super press over a regular detected link,
super release clearing, stationary super press over OSC8 text, mouse-reporting
suppression with shift override, and the captured-shift no-op branch. The new
`link_hover_modifier_refresh_parity.py` guard pins the Ghostty anchors, Roastty
implementation markers, tests, and inventory wording.

Verification run:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
```

```bash
cargo test --manifest-path roastty/Cargo.toml link_hover_modifier_refresh -- --test-threads=1
```

Result: 5 passed.

```bash
cargo test --manifest-path roastty/Cargo.toml link_hover_preview_dispatch -- --test-threads=1
```

Result: 5 passed.

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/link_hover_modifier_refresh_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/link_hover_preview_dispatch_parity.py
```

Results: both guards passed.

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
```

Result: `runtime_rows=69`, `oracle_complete=62`, `closed=65`, `incomplete=4`,
`gap=4`, `cfg223=Gap`.

## Conclusion

The deterministic surface-action portion of Ghostty link hover now covers both
mouse-motion and key/modifier-driven refresh paths. This does not reduce the
remaining CFG-223 gap count because the open notification/link row still tracks
native GUI/OS effects: real app hover/cursor UI, native preview display, native
context/menu display, and OS URL-opening walkthrough guards.

## Completion Review

**Reviewer:** Kierkegaard the 2nd (`019ecaa5-eb3a-7b31-abd4-9684ceee9466`)

**Result:** Approved

The reviewer found no required issues. They reviewed the Rust implementation,
issue documentation, runtime inventory and matrix updates, and
`link_hover_modifier_refresh_parity.py`.

The reviewer independently ran:

- `cargo fmt --manifest-path roastty/Cargo.toml --check`
- `cargo test --manifest-path roastty/Cargo.toml link_hover_modifier_refresh -- --test-threads=1`
- `cargo test --manifest-path roastty/Cargo.toml link_hover_preview_dispatch -- --test-threads=1`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/link_hover_modifier_refresh_parity.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/link_hover_preview_dispatch_parity.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
- `git diff --check`

All checks passed. The reviewer also confirmed the plan commit exists at
`33ee7f4b6`.
