# Experiment 35: Port PageList Tracked Pins

## Description

Port the basic tracked-pin lifecycle:

- `trackPin`
- `untrackPin`
- `countTrackedPins`
- `trackedPins`

Experiment 33 introduced a stable boxed viewport pin and a tracked-pin list.
Experiment 34 added untracked point-to-pin conversion. This experiment should
allow callers to convert any valid untracked `Pin` into a stable tracked pin and
later untrack it, matching upstream's ownership model without implementing
mutation fixups yet.

This experiment should not port scrolling, grow/prune, reset, erase, resize,
split, tracked-pin remapping, selection, highlighting, or screen/parser
integration.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/PageList.zig` as the source of truth for:
     - `trackPin`;
     - `untrackPin`;
     - `countTrackedPins`;
     - `trackedPins`;
     - `pinIsValid`.
   - Inspect later tests that create tracked pins for context, but do not port
     grow/erase/reset behavior in this experiment.
   - Do not modify `vendor/ghostty/`.

2. Add stable owned storage for non-viewport tracked pins.
   - Keep the viewport pin boxed and always tracked.
   - Add a `tracked_pin_storage` or equivalent owned collection for arbitrary
     tracked pins.
   - Prefer `Vec<Box<Pin>>` for this slice:
     - each tracked pin has a stable address;
     - `tracked_pins: Vec<NonNull<Pin>>` can continue to mirror upstream's set
       of tracked pin pointers;
     - moving the `Vec<Box<Pin>>` does not move the allocated `Pin`.
   - Do not store tracked references to inline/movable `Pin` values.

3. Port `track_pin`.
   - Shape it as an internal Rust method:

     ```rust
     fn track_pin(&mut self, pin: Pin) -> Option<NonNull<Pin>>
     ```

     or `Result<NonNull<Pin>, TrackPinError>` if the implementation has a
     meaningful error type.

   - Validate the input with existing `pin_is_valid`.
   - Allocate/store a stable owned copy of the `Pin`.
   - Add its pointer to `tracked_pins`.
   - Return the stable pointer.
   - Do not deduplicate identical pin coordinates; upstream tracks each
     allocation independently.

4. Port `untrack_pin`.
   - Shape it as an internal Rust method taking the returned stable pin handle.
   - Match upstream semantics:
     - untracking the viewport pin is not allowed and should assert/panic;
     - if the pin is tracked, remove it from `tracked_pins` and free/remove its
       owned storage;
     - if the pin is not tracked, do nothing.
   - Ensure removing one tracked pin does not invalidate the remaining tracked
     pin addresses.

5. Port read helpers.
   - Add `count_tracked_pins() -> usize`.
   - Add `tracked_pins() -> &[NonNull<Pin>]` or a safer internal iterator if
     that is clearer in Rust.
   - Include the viewport pin in the count/slice, matching upstream.

6. Preserve integrity.
   - Existing integrity should continue to validate every pointer in
     `tracked_pins`.
   - Add a test proving that tracking an invalid pin is rejected before the bad
     pointer enters the tracked set.
   - Add a test proving that untracking removes the pointer from integrity
     consideration.

7. Add tests.
   - Initial PageList has exactly one tracked pin: the viewport pin.
   - `track_pin(pin(Point::active(...)))` increases the count and stores a valid
     stable pin.
   - Tracking two identical untracked pins creates two distinct tracked handles.
   - `tracked_pins()` includes the viewport pin and newly tracked pins.
   - `untrack_pin(handle)` removes the arbitrary pin and decrements the count.
   - `untrack_pin(handle)` is idempotent after the first removal, matching
     upstream's no-op behavior for missing pins.
   - `untrack_pin(viewport_pin)` panics/asserts.
   - Tracking an invalid/out-of-bounds pin is rejected and leaves the count
     unchanged.

8. Preserve scope.
   - Do not implement:
     - tracked-pin updates during grow/erase/reset/resize/split;
     - scrolling or viewport offset caches;
     - scrollbar;
     - selection or highlighting;
     - screen/parser integration;
     - public C ABI additions.
   - Do not add `ghostty` names except when citing upstream paths or test
     provenance.

9. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

10. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - owned tracked-pin storage shape;
      - APIs added;
      - tests added;
      - verification command output summary.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- arbitrary valid pins can be tracked and untracked;
- viewport pin remains always tracked and cannot be untracked;
- tracked pin handles are stable and distinct per tracking call;
- invalid pins are rejected before entering the tracked set;
- no grow/erase/reset/resize/split tracked-pin remapping or public ABI is
  introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- tracking/untracking works, but a later mutation experiment needs to adjust the
  storage type before pin remapping can be implemented safely.

The experiment fails if:

- tracked pin addresses can move while still listed in `tracked_pins`;
- untracking the viewport pin succeeds silently;
- invalid pins are tracked;
- duplicate tracked pins are incorrectly deduplicated;
- the implementation expands into grow/erase/reset/resize/split, scrolling,
  screen/parser behavior, or public ABI;
- tests or formatting fail.

## Result

**Result:** Pass

Implemented arbitrary tracked-pin lifecycle in
`roastty/src/terminal/page_list.rs`.

The owned storage shape is:

- viewport pin remains boxed and permanently tracked;
- arbitrary tracked pins live in `tracked_pin_storage: Vec<Box<Pin>>`;
- `tracked_pins: Vec<NonNull<Pin>>` mirrors upstream's tracked pointer set;
- moving the `Vec<Box<Pin>>` moves box handles, not the allocated `Pin` values,
  so returned tracked-pin handles remain stable.

Added PageList methods for:

- `track_pin`;
- `untrack_pin`;
- `count_tracked_pins`;
- `tracked_pins`.

The implementation intentionally rejects invalid pins before tracking them. That
is a safe Rust adaptation of upstream's slow-runtime-safety assertion. Duplicate
pin coordinates are not deduplicated; each `track_pin` call creates a distinct
tracked handle.

`untrack_pin` matches upstream semantics:

- untracking the viewport pin panics;
- untracking a tracked arbitrary pin removes it from both the tracked pointer
  list and owned storage;
- untracking a missing/stale handle is a no-op and uses pointer comparison only.

Added tests for:

- initial PageList tracking exactly the viewport pin;
- tracking a valid active pin;
- tracking duplicate coordinates as distinct handles;
- `tracked_pins()` including viewport and arbitrary pins;
- untracking an arbitrary pin;
- idempotent untrack after the first removal;
- panic when untracking the viewport pin;
- rejecting invalid pins without changing counts;
- untracking removing a corrupt pin from integrity consideration.

Verification passed:

```bash
cargo fmt
cargo test -p roastty terminal::page_list
cargo test -p roastty
```

The targeted PageList suite reported 34 passing tests. The full `roastty` suite
reported 315 unit tests, the ABI harness, and doc tests passing.

## Conclusion

Roastty now supports the upstream tracked-pin ownership lifecycle for
initialized PageLists. Arbitrary pins can become stable tracked handles, be
listed and counted with the always-tracked viewport pin, and be removed without
destabilizing other tracked pins. Mutation-time remapping remains a later
PageList operation.
