# Experiment 120: Phase G — key-table sequence runtime

## Description

Activate configured multi-key sequences inside active key tables.

Experiment 119 activated the root configured sequence trie, but deliberately
left table-local sequences such as `nav/a>b=quit` inert. Upstream Ghostty uses
the same `Binding.Set.Entry` flow for root and table sets: an active sequence is
checked first, then active key tables are searched from inner-most to
outer-most, and any matching table entry can be either a leader or a leaf.

This experiment wires that remaining table-sequence path for Roastty's surface
key handling. It keeps the scope limited to configured surface key tables:
`roastty_app_key`, native keymaps/global shortcuts, `ignore`,
`end_key_sequence`, and `chain=` stay out of scope.

## Changes

- `roastty/src/lib.rs`
  - Add table-sequence lookup helpers that search a table's
    `ConfigKeybindTable::sequences` trie and return either a leader entry or a
    leaf binding, mirroring the root sequence helpers from Experiment 119.
  - Update active table dispatch in `Surface::key` so a matching active-table
    entry can be:
    - a sequence leader, which starts the active key sequence, queues the leader
      key bytes, emits `ROASTTY_ACTION_KEY_SEQUENCE` active, and consumes the
      key;
    - a sequence leaf, which dispatches the configured binding through the same
      configured-binding consumption path as table direct bindings.
  - Preserve upstream search order:
    1. active sequence state;
    2. active key tables, inner-most to outer-most;
    3. root sequence leaders;
    4. root/direct/default/catch-all lookup.
  - Preserve one-shot table semantics for sequence leaders and leaves: if the
    matched entry comes from the currently active one-shot table, deactivate the
    table before handling the matched entry, just as direct table bindings do.
    The nested sequence set is cloned into `active_key_sequence`, so the final
    key still resolves after the one-shot table has popped.
  - Keep direct table bindings authoritative over table-local sequence leaders
    for the same trigger, using the existing storage override rules.
  - Update `Surface::key_is_binding` so active table sequence leaders are
    reported as bindings with flags `0`, and active table sequence leaves report
    their configured flags once their leader is active.
- Tests in `roastty/src/lib.rs`
  - Replace the current inert-table-sequence runtime assertion with coverage
    that `nav/a>b=quit` starts from an active `nav` table, emits the active
    sequence notification, dispatches only on `b`, and emits the inactive/end
    `ROASTTY_ACTION_KEY_SEQUENCE` notification when the leaf completes.
  - Cover nested table sequences such as `nav/a>ctrl+b>c=toggle_fullscreen`.
  - Cover active-table precedence: a table sequence leader beats a root direct
    binding/default for the same first key while the table is active.
  - Cover direct table override: `nav/a=...` prevents `nav/a>b=...` from
    starting a sequence.
  - Cover one-shot table activation: `activate_key_table_once:nav` pops the
    table when the sequence leader matches, but the queued active sequence still
    completes on the final key.
  - Cover invalid non-modifier handling from a table-started sequence: queued
    leader bytes flush, the inactive/end sequence notification is emitted, and
    the current key encodes normally.
  - Cover `surface_key_is_binding` reporting for active table sequence leaders
    with flags `0`.
  - Cover `surface_key_is_binding` reporting for active table sequence leaves
    after their leader is active, including a leaf with nonzero configured
    flags.
  - Keep `roastty_app_key` sequence/table-sequence handling inert in this
    experiment.

## Verification

- Run:
  - `cargo test -p roastty sequence`
  - `cargo test -p roastty key_table`
  - `cargo test -p roastty surface_key`
  - `cargo test -p roastty app_key`
  - `cargo test -p roastty --test abi_harness`
  - `cargo test -p roastty -- --test-threads=1`
  - `cargo fmt`
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/120-key-table-sequence-runtime.md issues/0802-libroastty-completion-and-mac-app/README.md`

## Design Review

**Reviewer:** Codex-native adversarial reviewer, fresh context
(`multi_agent_v1.spawn_agent`, agent `019eb78c-81d0-74e2-b768-622e1fb254d9`)

**Initial verdict:** Changes required

**Required finding 1:** The original verification plan did not prove inactive
`ROASTTY_ACTION_KEY_SEQUENCE` notifications for table-sequence leaf completion
or invalid table-started sequence flushes.

**Fix:** Added explicit test criteria for inactive/end sequence notifications
when a table-sequence leaf completes and when an invalid non-modifier key
flushes a table-started sequence.

**Required finding 2:** The original verification plan promised
`Surface::key_is_binding` active table sequence leaf flags, but only tested
leader flags.

**Fix:** Added explicit test criteria for `surface_key_is_binding` on an active
table sequence leaf with nonzero configured flags after the leader is active.

**Final verdict:** Approved

**Final findings:** None.

## Result

**Result:** Pass

Implemented table-local sequence runtime lookup for active key tables. Active
table entries now use the table sequence trie for exact and `catch_all`
leader/leaf lookup. A table sequence leader starts the same active sequence
state as root leaders, queues the leader key, emits the active
`ROASTTY_ACTION_KEY_SEQUENCE` notification, and consumes the event. Table
sequence leaves dispatch through the configured binding path and end the
sequence with the inactive notification. One-shot tables pop when the matching
leader or leaf comes from the currently active one-shot table, while the cloned
nested sequence set still completes after the table has been popped.

Focused and full verification passed:

- `cargo test -p roastty key_table` — 20 passed
- `cargo test -p roastty sequence` — 38 passed
- `cargo test -p roastty surface_key` — 75 passed
- `cargo test -p roastty app_key` — 12 passed
- `cargo test -p roastty --test abi_harness` — 1 passed, with existing enum
  conversion warnings in the C harness
- `cargo test -p roastty -- --test-threads=1` — 4,678 unit tests passed, plus
  the ABI harness and doc tests
- `cargo fmt`
- `cargo fmt --check`
- `git diff --check`

Verification history:

- First `cargo test -p roastty -- --test-threads=1` run — 4,675 passed, 2
  failed:
  - `tests::surface_foreground_pid_reports_worker_foreground_pid_after_start`
  - `tests::surface_mouse_button_reporting_honors_readonly_gate`
- Immediate focused reruns of both failed tests passed.
- Second `cargo test -p roastty -- --test-threads=1` run — 4,676 passed, 1
  failed:
  - `tests::surface_foreground_pid_reports_worker_foreground_pid_after_start`
- After the completion reviewer found the missing `catch_all` sequence-leader
  path and the fix landed, the final full-suite run passed: 4,678 unit tests
  plus the ABI harness and doc tests.

Completion review initially found one required issue: table sequence lookup used
exact lookup plus a flat catch-all fallback, so `nav/catch_all>b=quit` would not
start a sequence. The fix added `ConfigKeybindSet::catch_all_entry` /
`event_entry`, switched active table lookup to the full table sequence set, and
added `surface_key_table_catch_all_sequence_precedes_root_binding`.

## Conclusion

The table-sequence runtime itself is wired and covered by focused tests, closing
the key-table half of the sequence behavior deferred by Experiment 119. Active
table sequences now follow upstream's set lookup shape, including `catch_all`
leaders, direct override behavior, one-shot table popping, invalid-prefix
flushes, and key-binding query flags. The next Phase G slices can move to
`ignore` / `end_key_sequence` / `chain=` semantics, app-key sequence/table
handling, native keymaps/global shortcuts, or broader upstream default binding
coverage.

## Completion Review

**Reviewer:** Codex-native adversarial reviewer, fresh context
(`multi_agent_v1.spawn_agent`, agent `019eb79b-d042-7112-9eb6-119e1f6687d7`)

**Initial verdict:** Changes required

**Required finding:** Active table sequence lookup did not include `catch_all`
sequence leaders. The reviewer pointed out that upstream table dispatch calls
`Binding.Set.getEvent`, which falls back to `catch_all`, while the first
implementation used exact lookup plus a flat direct-binding catch-all fallback.

**Fix:** Added `ConfigKeybindSet::catch_all_entry` and `event_entry`, switched
active table lookup to `table.sequences.event_entry(event)`, and added the
`surface_key_table_catch_all_sequence_precedes_root_binding` regression test.

**Final verdict:** Approved

**Final findings:** None.
