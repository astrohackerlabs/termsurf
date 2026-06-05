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

# Experiment 599: highlight tracking lifecycle (Untracked::track / Tracked::init / deinit)

## Description

This experiment ports the highlight **tracking lifecycle** from upstream
`terminal/highlight.zig` — `Untracked.track`, `Tracked.init`, and
`Tracked.deinit` — which turn an untracked highlight (two plain pins) into a
tracked one (two pins registered with the screen's `PageList`, so they survive
terminal mutations) and back. This is the prerequisite for
`ScreenSearch::select_next` / `select_prev` (the next slice), which track the
selected match so it follows the content. It extends `terminal::highlight` and
adds `track_pin` / `untrack_pin` accessors on `Screen`.

## Upstream behavior

```zig
pub const Untracked = struct {
    start: Pin,
    end: Pin,
    pub fn track(self: *const Untracked, screen: *Screen) Allocator.Error!Tracked {
        return try .init(screen, self.start, self.end);
    }
    pub fn eql(self: Untracked, other: Untracked) bool {
        return self.start.eql(other.start) and self.end.eql(other.end);
    }
};

pub const Tracked = struct {
    start: *Pin,
    end: *Pin,
    pub fn init(screen: *Screen, start: Pin, end: Pin) Allocator.Error!Tracked {
        const start_tracked = try screen.pages.trackPin(start);
        errdefer screen.pages.untrackPin(start_tracked);
        const end_tracked = try screen.pages.trackPin(end);
        errdefer screen.pages.untrackPin(end_tracked);
        return .{ .start = start_tracked, .end = end_tracked };
    }
    pub fn deinit(self: Tracked, screen: *Screen) void {
        screen.pages.untrackPin(self.start);
        screen.pages.untrackPin(self.end);
    }
};
```

- `Untracked.track` registers the highlight's two pins with the screen's
  `PageList` (`trackPin`), returning a `Tracked` holding the two tracked-pin
  pointers. On failure of the second track, it untracks the first (`errdefer`).
- `Tracked.deinit` untracks both pins.
- `Untracked.eql` compares the two highlights pin-by-pin.

## Rust mapping (`roastty/src/terminal/highlight.rs`)

roastty's `PageList::track_pin` returns `Option<NonNull<Pin>>` (`None` if the
pin is invalid) rather than upstream's `Allocator.Error`, so `track` / `init`
return `Option<Tracked>` (`None` if either pin can't be tracked, untracking the
first on the second's failure). `screen.pages.trackPin` is reached through the
**existing** `Screen::track_pin` / `untrack_pin` accessors (`pub(super)`,
already present and reachable from the sibling `highlight` module — no new
accessors are added). `Untracked::eql` is roastty's **derived `PartialEq`**
(`Pin` and `Untracked` both `#[derive(PartialEq)]`), so no separate method is
added — a note documents the mapping.

```rust
impl Untracked {
    /// Register this highlight's pins with the screen so they survive terminal mutations (upstream
    /// `track`). `None` if a pin can't be tracked.
    pub(super) fn track(&self, screen: &mut Screen) -> Option<Tracked> {
        Tracked::init(screen, self.start, self.end)
    }
}

impl Tracked {
    /// Track `start` and `end` on `screen`, returning the tracked highlight (upstream
    /// `Tracked.init`). `None` if either pin is invalid; the first is untracked if the second fails.
    pub(super) fn init(screen: &mut Screen, start: Pin, end: Pin) -> Option<Tracked> {
        let start_tracked = screen.track_pin(start)?;
        let Some(end_tracked) = screen.track_pin(end) else {
            screen.untrack_pin(start_tracked);
            return None;
        };
        Some(Tracked {
            start: start_tracked,
            end: end_tracked,
        })
    }

    /// Untrack both pins (upstream `deinit`). Takes `self` by value (a lifecycle signal, matching
    /// upstream; `Tracked` is `Copy`). Do not call on a `Tracked` made via `init_assume`.
    pub(super) fn deinit(self, screen: &mut Screen) {
        screen.untrack_pin(self.start);
        screen.untrack_pin(self.end);
    }
}
```

`Screen::track_pin` / `untrack_pin` already exist (`pub(super)`, delegating to
`self.pages`) and are reachable from the sibling `highlight` module — no new
accessors are added. The tests need read access to the screen's first node and
its tracked-pin count, so two `#[cfg(test)]` Screen accessors are added
(`first_node_ptr_for_tests`, `tracked_pin_count`, delegating to `PageList`).

## Scope / faithfulness notes

- **Ported**: `Untracked.track` → `Untracked::track`; `Tracked.init` →
  `Tracked::init`; `Tracked.deinit` → `Tracked::deinit`; plus the
  `Screen::track_pin` / `untrack_pin` accessors.
- **Faithful**: `track` delegating to `init`; `init` tracking both pins and
  untracking the first if the second fails; `deinit` untracking both.
- **Faithful adaptation**: `Allocator.Error!Tracked` → `Option<Tracked>`
  (roastty's `track_pin` returns `Option`, `None` on an invalid pin; the
  `errdefer` untrack-first becomes an explicit `else` branch); `screen.pages.*`
  goes through the **existing** `Screen::track_pin` / `untrack_pin` accessors
  (no duplicates added); `deinit` takes `self` by value (matching upstream;
  `Tracked` is `Copy`); `Untracked.eql` is the derived `PartialEq` (no separate
  method).
- **Deferred**: `ScreenSearch::select_next` / `select_prev` (the next slice,
  which uses these), the `init` / `reload_active` / `select` cluster, `feed`,
  and the rest of the search subsystem.
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::highlight`; adds two `Screen` accessors.

## Changes

1. `roastty/src/terminal/highlight.rs`: add `Untracked::track`, `Tracked::init`,
   and `Tracked::deinit`; import `Screen` (`super::screen::Screen`).
2. `roastty/src/terminal/screen.rs`: add `#[cfg(test)]`
   `Screen::tracked_pin_count` and `Screen::first_node_ptr_for_tests`
   (delegating to `PageList`) for the highlight tests. (`track_pin` /
   `untrack_pin` already exist.)
3. Tests (in `highlight.rs`):
   - **track then deinit round-trips the tracked-pin count**: a real `Screen`;
     an `Untracked` of two valid pins (`Pin::new(first_node, 0, 0)`);
     `track(&mut screen)` raises the screen's tracked-pin count by `2`;
     `deinit(&mut screen)` returns it to baseline.
   - **second-pin-failure rollback**: a valid start pin and an **invalid** end
     pin (`Pin::test_invalid_for_tests`) → `Tracked::init` returns `None`, and
     the tracked-pin count returns to baseline (the first pin was untracked —
     the `errdefer` path).
   - **`Untracked` equality**: two equal `Untracked`s compare equal; differing
     ones unequal (the derived `eql`).
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::highlight
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/highlight.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Untracked::track` / `Tracked::init` / `deinit` reproduce upstream (track both
  pins, untrack-first on failure, untrack both on deinit) — faithful to
  `terminal/highlight.zig`;
- the tests pass (track/deinit count round-trip / `Untracked` equality), and the
  existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the tracking, the untrack-first-on-failure, or the
deinit diverges from upstream, an unrelated item changes, or any public C
API/ABI changes.

## Design Review

Codex reviewed the design and **confirmed the core mapping sound**:
`Option<Tracked>` matches `PageList::track_pin`'s invalid-pin failure mode; the
explicit untrack-first on second-pin failure faithfully ports the `errdefer`;
the derived `PartialEq` / `Eq` is enough for upstream's `Untracked.eql`; and
safe methods taking `&mut Screen` are appropriate (no raw deref here — that
happens later in `ScreenSearch`). One Required and two Optionals, all adopted:

- **Required (adopted)**: do **not** add duplicate `Screen::track_pin` /
  `untrack_pin` — they already exist (`pub(super)`, delegating to `PageList`)
  and are reachable from the sibling `highlight` module. The slice uses the
  existing methods; only `#[cfg(test)]` Screen accessors (`tracked_pin_count`,
  `first_node_ptr_for_tests`) are added for the tests.
- **Optional (adopted)**: add a rollback test for the second-pin failure path (a
  valid start pin + an invalid end pin → `Tracked::init` returns `None` and the
  tracked-pin count returns to baseline), directly covering the `errdefer`
  untrack-first behavior.
- **Optional (adopted)**: `Tracked::deinit` takes `self` by value (matching
  upstream's `deinit(self, screen)` and the lifecycle signal; `Tracked` is
  `Copy`).

Review artifacts:

- Prompt: `logs/codex-review/20260604-d599-prompt.md`
- Result: `logs/codex-review/20260604-d599-last-message.md`
