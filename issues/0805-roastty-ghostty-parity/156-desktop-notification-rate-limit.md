# Experiment 156: Desktop Notification Rate Limit

## Description

`RUNTIME-012B2B2B` still tracks native notification/link/bell GUI effects,
including Ghostty core desktop-notification rate limiting. Pinned Ghostty limits
desktop notification actions in `Surface.zig::showDesktopNotification`:

- any second notification within one second of the previous delivered desktop
  notification is suppressed;
- an identical title/body notification is additionally suppressed for five
  seconds after the previous delivered desktop notification;
- the timestamp and digest update only when the notification is actually
  delivered to the app action path.

This experiment ports that runtime behavior to Roastty's live surface desktop
notification dispatch. It is narrower than full native notification parity: it
will not claim live macOS Notification Center delivery, authorization prompts,
command-finish notification generation, `app-notifications` toasts, bell side
effects, or link hover/preview/context-menu behavior.

Roastty does not currently carry a Wyhash dependency. Pinned Ghostty feeds title
bytes and body bytes into Wyhash sequentially with no delimiter, so the planned
Rust implementation will store the last delivered delimiterless `title || body`
byte stream after Ghostty-compatible truncation. That preserves the observable
concatenation identity Ghostty rate-limits without adding a hash dependency.

## Changes

- Update `roastty/src/lib.rs`:
  - Add app-level desktop-notification rate-limit state analogous to Ghostty's
    `last_notification_time` / `last_notification_digest`.
  - Factor the limiter into a helper that accepts an explicit
    `std::time::Instant` so tests can exercise time offsets deterministically
    without sleeping.
  - Gate `Surface::perform_desktop_notification` through the app-level limiter
    after `desktop-notifications` config suppression and after
    Ghostty-compatible title/body truncation.
  - Preserve the existing app action payload and target surface dispatch for
    delivered notifications.
  - Add focused tests proving:
    - `desktop-notifications = false` still suppresses before rate-limit state
      is updated;
    - the first notification at `t=0s` dispatches and records app-level limiter
      state;
    - a second notification at `t=999ms` is suppressed even with different
      content and does not update limiter state;
    - a different notification at `t=1001ms` dispatches and updates limiter
      state;
    - an identical delimiterless `title || body` stream at `t=4s` after the last
      delivered notification is suppressed and does not update limiter state;
    - the identical notification dispatches again at `t=5001ms` after the last
      delivered notification and updates limiter state;
    - limiter state is app-level by suppressing an otherwise-valid notification
      from a second surface on the same app inside the one-second window.
- Add a focused static guard:
  - `issues/0805-roastty-ghostty-parity/desktop_notification_rate_limit_runtime_parity.py`
  - Assert pinned Ghostty's app-level rate limiter source markers are present.
  - Assert Roastty has app-level notification limiter state, delimiterless
    `title || body` identity tracking, one-second and five-second thresholds,
    deterministic tests, cross-surface app-level suppression, and the expected
    inventory split.
- Update `config_runtime_inventory.py` to split `RUNTIME-012B2B2B` into:
  - an Oracle complete desktop-notification rate-limit row owned by this
    experiment;
  - a remaining notification/link/bell GUI gap row for command-finish
    notifications, `app-notifications`, live OS banner/sound delivery, actual
    bell side effects, link hover/cursor UI, link previews, and context/menu
    link flows.
- Regenerate `config-runtime-inventory.md` and `config-matrix.md`.
- Update existing runtime parity guards and `terminal_runtime_residual_audit.py`
  for the new CFG-223 row counts and remaining notification/link/bell gap id.
- Update Issue 805 learnings with the rate-limit finding after the result is
  known.

## Verification

Pass criteria:

- Focused Rust tests pass:

```bash
cargo test --manifest-path roastty/Cargo.toml surface_desktop_notification_runtime
```

- The new static rate-limit guard passes:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/desktop_notification_rate_limit_runtime_parity.py
```

- Adjacent notification guards still pass:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/desktop_notification_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_user_notification_runtime_parity.py
```

- The runtime inventory generator reports one additional Oracle complete row and
  the same total number of unresolved CFG-223 gaps unless implementation
  uncovers a real additional gap. Expected output after this split:
  `runtime_rows=64`, `oracle_complete=58`, `closed=60`, `incomplete=4`, `gap=4`,
  and `cfg223=Gap`.

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
```

- All runtime parity guards still pass:

```bash
for guard in issues/0805-roastty-ghostty-parity/*_runtime_parity.py; do
  PYTHONDONTWRITEBYTECODE=1 python3 "$guard" || exit 1
done
```

- Rust formatting and diff hygiene pass:

```bash
cargo fmt --manifest-path roastty/Cargo.toml --check
git diff --check
```

## Design Review

**Reviewer:** Bohr the 2nd

**Verdict:** Approve after fixes

The first review required two design fixes: preserve Ghostty's delimiterless
title/body digest input instead of storing a title/body tuple, and require
deterministic explicit-time tests that prove suppressed notifications do not
update limiter state. The revised design stores `title || body` bytes after
truncation, factors the limiter behind an explicit `Instant` helper, and names
the no-state-update and cross-surface app-level cases. The reviewer approved the
revised design.

## Result

**Result:** Pass

Roastty now applies Ghostty-style app-level desktop notification rate limiting
before dispatching `ROASTTY_ACTION_DESKTOP_NOTIFICATION`:

- `desktop-notifications = false` still suppresses before limiter state updates;
- delivered notifications update app-level limiter state;
- a second notification inside one second is suppressed without updating limiter
  state;
- the identity check uses the delimiterless truncated `title || body` byte
  stream that pinned Ghostty feeds to Wyhash;
- identical notifications before five seconds are suppressed without updating
  limiter state;
- identical notifications after five seconds dispatch again and update state;
- limiter state is shared across surfaces on the same app.

The inventory split is now:

- `RUNTIME-012B2B2B1`: **Oracle complete** for desktop notification rate
  limiting;
- `RUNTIME-012B2B2B2`: **Gap** for command-finish notifications,
  `app-notifications`, live OS banner/sound delivery, actual bell side effects,
  link hover/cursor UI, link previews, and context/menu link flows.

Verification passed:

```text
cargo test --manifest-path roastty/Cargo.toml surface_desktop_notification_runtime
6 passed; 0 failed

desktop_notification_rate_limit_runtime_parity=pass
desktop_notification_runtime_parity=pass
macos_user_notification_runtime_parity=pass
terminal_runtime_residual_audit=pass
```

The runtime inventory generator reported:

```text
runtime_rows=64
oracle_complete=58
closed=60
audit_covered=0
incomplete=4
gap=4
cfg223=Gap
```

All `*_runtime_parity.py` guards passed, and
`cargo fmt --manifest-path roastty/Cargo.toml --check` plus `git diff --check`
passed.

## Conclusion

Desktop notification rate limiting is now closed for runtime parity. The
remaining notification/link/bell row is narrower and no longer includes Ghostty
core notification throttling; future experiments should target live OS
notification delivery, command/app notification generation, actual bell side
effects, or link UI behavior.

## Completion Review

**Reviewer:** Kepler the 2nd

**Verdict:** Approve

The reviewer found no required issues. They independently verified the focused
Rust tests, the new rate-limit guard, adjacent notification/bell guards, the
full `*_runtime_parity.py` loop, terminal residual audit, Rust formatting, and
diff hygiene. They also confirmed the result commit had not been made before the
completion review.
