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

# Experiment 607: search Thread — part 2: `Search::feed` + Terminal integration

## Description

This experiment ports `Search.feed` from upstream `terminal/search/Thread.zig`
(~613-714) — the lock-holding step that reconciles the per-screen searchers with
the terminal's screens, honors the viewport-dirty flag, updates the viewport
search, and feeds each screen searcher. It is the second slice of the search
`Thread` (Exp 606 landed the aggregator's `new`/`deinit`/`is_complete`/`tick`).

`feed` is the point where the search subsystem first reaches into `Terminal`, so
it requires new `Terminal` surface that roastty lacks:

1. A `search_viewport_dirty` flag on `TerminalFlags` (upstream
   `Terminal.flags`), set by the renderer when the viewport/active area changes.
   roastty has no renderer port, so the flag is faithfully ported as a field
   with read/clear/set accessors; nothing sets it yet except tests.
2. Screen enumeration as raw pointers: upstream iterates `t.screens.all` (an
   `EnumMap`/hashmap of existing screens) and gets `t.screens.all.get(key)`.
   roastty models screens as `primary: Screen` + `alternate: Option<Screen>`, so
   this becomes an accessor returning the present screens as
   `(TerminalScreenKey, NonNull<Screen>)`.

`notify` and the outer libxev `Thread` remain deferred (the latter is blocked on
a libxev port).

## Pointer / provenance model (the key design question)

`feed` is where a `ScreenSearch`'s cached `NonNull<Screen>` starts pointing at a
**Terminal-owned** screen (`t.screens.primary` / `.alternate`), cached across
`feed` calls and across terminal mutations. This is the same raw-pointer model
the search subsystem already uses (every `ScreenSearch` since Exp 592 holds a
`NonNull<Screen>` dereferenced under a documented "screen alive + caller holds
the lock, no concurrent `&mut`" invariant) — `feed` only extends it so the
screen lives in the `Terminal`.

**Resolved (design review, Required #1):** `feed` takes
**`t: NonNull<Terminal>`**, not `&mut Terminal`. A whole-call `&mut Terminal`
borrow aliases the cached raw screen pointers the `Search` already holds and is
a poor fit for the subsystem's cached-raw-pointer model. Instead:

- All Terminal access goes through **raw-projection associated functions in
  `terminal.rs`** that take `NonNull<Terminal>` and use `core::ptr::addr_of!` /
  `addr_of_mut!` (where the private fields are visible) — they never materialize
  a `&Terminal` / `&mut Terminal`. Reads return `Copy` values or raw `NonNull`s;
  writes mutate through the raw pointer.
- `feed` never holds any terminal-derived reference while dereferencing a cached
  `ScreenSearch` screen pointer. The only short-lived references are isolated:
  `unsafe { screen_ptr.as_ref() }.pages()` to hand the active `&PageList` to
  `viewport.update` (which dereferences no `ScreenSearch`).
- The cached `NonNull<Screen>` validity invariant (screen outlives the search,
  not moved/reallocated; reconciliation drops searchers whose screen pointer
  changed) is documented on `Search::feed`'s `# Safety`, mirroring upstream's
  reliance on stable `*Screen`.

## Upstream behavior (`Thread.zig` `feed`)

```zig
pub fn feed(self: *Search, alloc, t: *Terminal) void {
    // (A) Active screen switch → reset last_screen (forces recalcs/notifications).
    if (t.screens.active_key != self.last_screen.key)
        self.last_screen = .{ .key = t.screens.active_key };

    // (B) Reconcile searchers with the terminal's screens.
    //   Remove: screen gone, or screen pointer changed (reinitialized).
    //   Add:    screen exists but we have no searcher yet.
    ... remove loop (deinit + remove) ...
    ... add loop (ScreenSearch.init(alloc, screen, needle)) ...

    // (C) Viewport-dirty: re-search the active area.
    if (t.flags.search_viewport_dirty) {
        t.flags.search_viewport_dirty = false;
        self.viewport.active_dirty = true;
        if (self.screens.getPtr(t.screens.active_key)) |ss| ss.reloadActive();
    }

    // (D) Update the viewport search over the active pages.
    if (self.viewport.update(&t.screens.active.pages)) |updated|
        if (updated) self.stale_viewport_matches = true;

    // (E) Feed each searcher that needs it.
    var it = self.screens.iterator();
    while (it.next()) |entry|
        if (entry.value.state.needsFeed()) entry.value.feed();
}
```

(OOM `log.warn` arms drop — infallible in roastty.)

## New surface

### `Terminal` (`terminal.rs`, `pub(in crate::terminal)`)

`TerminalFlags` gains `search_viewport_dirty: bool` (default `false`). The
search accessors are **associated functions taking `NonNull<Terminal>`** and
projecting through raw pointers (never materializing a `&Terminal` /
`&mut Terminal`):

```rust
/// Whether the renderer marked the viewport/active area dirty (upstream
/// `Terminal.flags.search_viewport_dirty`). Raw-pointer read for the search thread's `feed`.
/// # Safety: `t` is live.
pub(in crate::terminal) unsafe fn search_viewport_dirty(t: NonNull<Terminal>) -> bool {
    unsafe { core::ptr::addr_of!((*t.as_ptr()).flags.search_viewport_dirty).read() }
}
/// Clear the viewport-dirty flag. # Safety: `t` is live.
pub(in crate::terminal) unsafe fn clear_search_viewport_dirty(t: NonNull<Terminal>) {
    unsafe { core::ptr::addr_of_mut!((*t.as_ptr()).flags.search_viewport_dirty).write(false) }
}
/// Mark the viewport dirty (upstream's renderer write; here for the future renderer port and tests).
/// # Safety: `t` is live.
pub(in crate::terminal) unsafe fn mark_search_viewport_dirty(t: NonNull<Terminal>) {
    unsafe { core::ptr::addr_of_mut!((*t.as_ptr()).flags.search_viewport_dirty).write(true) }
}

/// The active screen key (upstream `t.screens.active_key`). # Safety: `t` is live.
pub(in crate::terminal) unsafe fn active_screen_key(t: NonNull<Terminal>) -> TerminalScreenKey {
    unsafe { core::ptr::addr_of!((*t.as_ptr()).screens.active).read() }
}

/// The present screens as raw pointers (upstream iterating `t.screens.all`). `primary` is always
/// present; `alternate` only when it exists. Pointers are projected with `addr_of_mut!` (no
/// intermediate `&mut Screen`). # Safety: `t` is live.
pub(in crate::terminal) unsafe fn present_screen_ptrs(
    t: NonNull<Terminal>,
) -> Vec<(TerminalScreenKey, NonNull<Screen>)> {
    let screens = unsafe { core::ptr::addr_of_mut!((*t.as_ptr()).screens) };
    let mut out = Vec::new();
    // Primary always exists.
    let primary = unsafe { core::ptr::addr_of_mut!((*screens).primary) };
    out.push((TerminalScreenKey::Primary, NonNull::new(primary).unwrap()));
    // Alternate only when present; project into the `Option`'s payload without a `&mut`.
    if unsafe { (*core::ptr::addr_of!((*screens).alternate)).is_some() } {
        let alt = unsafe { (*core::ptr::addr_of_mut!((*screens).alternate)).as_mut().unwrap() };
        out.push((TerminalScreenKey::Alternate, NonNull::from(alt)));
    }
    out
}
```

The active screen's `&PageList` (for `viewport.update`) is obtained in `feed`
from the active screen pointer (`present`'s entry matching the active key) via
`unsafe { ptr.as_ref() }.pages()` — a short-lived `&Screen` that overlaps no
cached-pointer deref. (`Screen::pages` already exists; its field is private to
`screen.rs`, so it cannot be projected from `terminal.rs`.)

### `ScreenSearch` (`screen.rs`, `pub(in crate::terminal)`)

```rust
/// This search's backing screen pointer (upstream `screen_search.screen`), to detect when the
/// terminal reinitialized a screen.
pub(in crate::terminal) fn screen_ptr(&self) -> NonNull<Screen> { self.screen }

/// Whether the search wants a feed (upstream `self.state.needsFeed()`).
pub(in crate::terminal) fn needs_feed(&self) -> bool { self.state.needs_feed() }
```

## Rust mapping (`thread.rs` `Search::feed`)

```rust
/// Reconcile searchers with the terminal's screens, honor the viewport-dirty flag, update the
/// viewport search, and feed each searcher that needs it (upstream `feed`). Holds the screen lock.
///
/// # Safety
/// `t` must be live and outlive this `Search`, the Terminal must not be moved/reallocated, and the
/// caller holds the screen lock (no concurrent access). The per-screen searchers cache
/// `NonNull<Screen>` into `t`'s screens; reconciliation drops a searcher whose screen vanished or
/// was replaced *without* dereferencing the stale pointer (see step B).
pub(in crate::terminal) unsafe fn feed(&mut self, t: NonNull<Terminal>) {
    use super::super::terminal::TerminalScreenKey::{Alternate, Primary};

    // (A) Active screen switch.
    // SAFETY: `t` live.
    let active_key = unsafe { Terminal::active_screen_key(t) };
    if active_key != self.last_screen.key {
        self.last_screen = ScreenState { key: active_key, total: None, selected: None };
    }

    // (B) Reconcile. Collect present screen pointers (no `&Terminal` retained afterwards).
    // SAFETY: `t` live.
    let present = unsafe { Terminal::present_screen_ptrs(t) };
    for key in [Primary, Alternate] {
        let remove = match self.screens.get(key) {
            None => false,
            Some(ss) => match present.iter().find(|(k, _)| *k == key) {
                None => true,                          // screen gone
                Some((_, ptr)) => ss.screen_ptr() != *ptr, // screen replaced
            },
        };
        if remove {
            // The backing screen was dropped or replaced by the terminal, so its pin storage is
            // already gone. Drop the searcher WITHOUT `deinit` (untracking against a freed screen
            // would be use-after-free). This is a deliberate, roastty-specific divergence from
            // upstream's `entry.value.deinit()`, sound because roastty's `Screen` owns its tracked
            // pins and a dropped/replaced screen takes them with it.
            let _ = self.screens.take(key);
        }
    }
    let needle = self.viewport.needle().to_vec();
    for (key, ptr) in &present {
        if self.screens.get(*key).is_some() { continue; }
        // SAFETY: `ptr` is a live Terminal screen; see `# Safety`. No `&Terminal` is held here.
        let ss = unsafe { ScreenSearch::new(*ptr, &needle) };
        self.screens.insert(*key, ss);
    }

    // (C) Viewport dirty → re-search the active area.
    // SAFETY: `t` live; raw-pointer flag read/write, no reference materialized.
    if unsafe { Terminal::search_viewport_dirty(t) } {
        // SAFETY: `t` live.
        unsafe { Terminal::clear_search_viewport_dirty(t) };
        self.viewport.set_active_dirty(Some(true));
        if let Some(ss) = self.screens.get_mut(active_key) {
            // SAFETY: active screen live; no Terminal reference held here.
            unsafe { ss.reload_active() };
        }
    }

    // (D) Viewport update over the active pages.
    if let Some((_, active_ptr)) = present.iter().find(|(k, _)| *k == active_key) {
        // SAFETY: `active_ptr` is the live active screen; the `&Screen` is used only to read its
        // `&PageList` for `update`, which dereferences no `ScreenSearch` pointer.
        let pages = unsafe { active_ptr.as_ref() }.pages();
        // SAFETY: `pages` is read-only for the call.
        let updated = unsafe { self.viewport.update(pages) };
        if updated {
            self.stale_viewport_matches = true;
        }
    }

    // (E) Feed each searcher that needs it.
    for ss in self.screens.iter_mut() {
        if ss.needs_feed() {
            // SAFETY: screen live; no Terminal reference held here.
            unsafe { ss.feed() };
        }
    }
}
```

### Notes / deviations

- **`feed` takes `NonNull<Terminal>`** (Required #1): all Terminal access is via
  raw-projection associated functions that never materialize a `&Terminal` /
  `&mut Terminal`, so a transient terminal reference can never invalidate the
  cached `ScreenSearch` screen pointers.
- **Remove drops without `deinit`** (Required #2): a vanished/replaced screen
  has already freed its pin storage, so the searcher is dropped without
  untracking (which would be UAF). `Search::deinit` (Exp 606) still untracks for
  the normal-teardown case where the screens are live.
- The `alloc` parameter and OOM `log.warn` arms drop (roastty allocation is
  infallible).
- Step (D)'s `active_ptr.as_ref().pages()` yields a `&PageList` into the active
  screen; `viewport.update` only reads it and dereferences no `ScreenSearch`
  pointer, so there is no overlapping access.

## Verification

- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — no regressions; new tests (construct a real
  `Terminal`, write text via its stream, drive `feed`):
  - `feed_adds_a_searcher_for_the_active_screen` — after `feed`, the active
    screen has a searcher whose `screen_ptr` matches the terminal's;
    `search_all` then finds the needle.
  - `feed_is_idempotent_for_unchanged_screens` — a second `feed` neither
    duplicates nor leaks searchers (tracked-pin count stable), then `deinit`
    returns to baseline.
  - `feed_marks_viewport_stale_on_dirty` — `mark_search_viewport_dirty` then
    `feed` clears the flag and sets `stale_viewport_matches`.
  - `feed_then_deinit_releases_pins` — `feed` tracks pins; `Search::deinit`
    returns the terminal's screens to baseline.
  - `feed_drops_searcher_when_alternate_screen_goes_away` (Optional, adopted) —
    enter the alternate screen, `feed` (a searcher is added for it), leave the
    alternate (the terminal drops it), then `feed` again: the alternate searcher
    is dropped without `deinit` (no UAF), and the primary screen's tracked-pin
    count is unaffected. (If a clean alternate enter/leave API isn't reachable
    in a unit test, this is recorded as a follow-up and the reconciliation
    remove path is covered by a pointer-change variant instead.)
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on touched source — clean.
- `git diff --check` — clean.

Pass = `feed` reconciles searchers with the terminal's screens, honors the
viewport-dirty flag, updates the viewport, feeds the searchers, and leaks no
tracked pins, with no overlapping `Terminal`/screen-pointer access.

## Design Review

Codex reviewed the design and raised **two Required** findings, both adopted:

- **Required (adopted)**: `Search::feed` must take `t: NonNull<Terminal>`, not
  `&mut Terminal` — a whole-call mutable terminal borrow aliases the cached raw
  screen pointers the `Search` holds and is a poor fit for the established
  cached-raw-pointer model. All Terminal access is now via raw-projection
  associated functions in `terminal.rs` (`addr_of!` / `addr_of_mut!`), never
  materializing a `&Terminal` / `&mut Terminal`; the only short-lived reference
  is `active_ptr.as_ref().pages()` for `viewport.update`, which overlaps no
  cached-pointer deref.
- **Required (adopted)**: the remove branch must not `deinit` a searcher whose
  backing screen vanished or was replaced — in roastty an alternate screen is
  dropped by `self.alternate = None` (terminal.rs ~607) and replaced by direct
  assignment (~543), so the cached pointer may already be dangling and `deinit`
  (which untracks against that screen) would be use-after-free. The branch now
  drops the searcher without `deinit`; its pins were owned by the now-freed
  screen. `Search::deinit` still untracks for the live-screen teardown case.
  This is a documented, sound divergence from upstream's `entry.value.deinit()`.
- **Optional (adopted)**: a reconciliation test for alternate
  removal/replacement (the missing/changed-screen path), beyond primary
  idempotence.
- **Optional (confirmed)**: keep the `search_viewport_dirty` field — the right
  faithful surface even though only tests set it until the renderer port lands.

Codex confirmed the `feed` ordering is faithful (active-switch reset → reconcile
→ viewport-dirty + active reload → viewport update → feed needs-feed searchers)
and the accessors are directionally minimal (adjusted to the raw-pointer model).

Review artifacts:

- Prompt: `logs/codex-review/20260605-d607-prompt.md`
- Result: `logs/codex-review/20260605-d607-last-message.md`

## Result

**Result:** Pass

Implemented `Search::feed(&mut self, t: NonNull<Terminal>)` in `thread.rs`, plus
the new `Terminal` surface (`terminal.rs`) and `ScreenSearch` accessors
(`screen.rs`):

- `TerminalFlags.search_viewport_dirty: bool` + raw-projection associated
  functions on `Terminal` (all `unsafe`, taking `NonNull<Terminal>`, projecting
  via `addr_of!` / `addr_of_mut!` with no `&Terminal` / `&mut Terminal`
  materialized): `search_viewport_dirty`, `clear_search_viewport_dirty`,
  `mark_search_viewport_dirty`, `search_active_screen_key`,
  `present_screen_ptrs`.
- `ScreenSearch::needs_feed` and `screen_ptr`.
- `feed` ports upstream's (A) active-switch reset, (B) reconcile (remove on
  missing/pointer-changed — dropping the searcher WITHOUT `deinit` to avoid a
  use-after-free on a freed screen — then add), (C) viewport-dirty → active
  reload, (D) viewport update over the active pages, (E) feed each needs-feed
  searcher.

Six tests (over a real `Terminal` driven by `next_slice`): searcher add + match,
idempotence, dirty-flag clear, deinit pin release, and the RIS-based
alternate-removal path (drop-without-deinit, no UAF). Gates: `cargo fmt --check`
clean, `cargo build -p roastty` no warnings, `cargo test -p roastty` **3328
passed / 0 failed** (3323 → 3328, +5), `git diff --check` clean. The no-ghostty
grep flags only pre-existing `ghostty` test names in `terminal.rs` (present at
HEAD, not introduced here; my additions are clean).

## Completion Review

Codex reviewed the completed experiment and raised **one Required** provenance
finding, fixed and re-confirmed **APPROVED**:

- **Required (fixed)**: `present_screen_ptrs`'s alternate branch materialized a
  transient `&mut Screen` (a mutable retag that could invalidate a cached
  `ScreenSearch` pointer to the same alternate). Changed to project through a
  shared reference (`(*addr_of!(...)).as_ref()` → `NonNull::from(&Screen)`); the
  primary branch stays on the raw `addr_of_mut!` path. The feed tests also now
  derive the terminal pointer via `NonNull::new(addr_of_mut!(terminal))` instead
  of `NonNull::from(&mut terminal)`, avoiding a reference retag after cached
  screen pointers exist.

Codex confirmed the rest is faithful and sound: `feed` takes `NonNull<Terminal>`
(no whole-call `&mut Terminal`); the stale-searcher remove drops without
`deinit`; the ordering matches upstream; the viewport-dirty handling is
faithful; and the RIS test validly exercises the drop-without-deinit path (the
search structs have no `Drop` that dereferences freed screen state).

Review artifacts:

- Design prompt/result:
  `logs/codex-review/20260605-d607-{prompt,last-message}.md`
- Result prompt/result:
  `logs/codex-review/20260605-r607-{prompt,last-message}.md`
- Re-confirmation: `logs/codex-review/20260605-r607b-{prompt,last-message}.md`

## Conclusion

`Search::feed` lands the search subsystem's first reach into `Terminal`, with a
sound raw-pointer provenance model (validated across two design-review rounds)
for caching `NonNull<Screen>` into terminal-owned screens. The remaining search
`Thread` work is:

- **`Search::notify`** + the `Event` / `EventCallback` types — the external
  notification interface (the next experiment).
- **The outer `Thread`** (OS thread + libxev event loop + mailbox + timers) —
  still blocked on a libxev port, a genuine dependency boundary alongside the
  regex/oniguruma and URI-parser blocks elsewhere in Issue 801.
