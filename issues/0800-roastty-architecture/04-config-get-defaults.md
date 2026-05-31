# Experiment 4: Add Roastty Config Lookup Defaults

## Description

Experiment 3 established the correct renamed `roastty_*` ABI skeleton. The next
step is to move one layer beyond inert lifecycle handles by implementing a
small, tested subset of Roastty config lookup behavior.

The upstream Swift frontend uses a generic config getter to retrieve typed
values by key. Roastty will need the same shape, renamed as
`roastty_config_get`. This experiment adds that API and implements a
conservative default-value table for a small set of primitive startup-oriented
config keys.

This is not a full config parser. It does not load real config files, parse
keybinds, implement recursive config includes, or expose the full upstream
config surface. It proves the ABI shape, typed output behavior, default-value
ownership, and tests for the first config lookup layer.

## Scope

Add the following API:

- `roastty_config_get(roastty_config_t, void*, const char*, uintptr_t) -> bool`

Implement default values for this initial key subset. The default source for
this experiment is the upstream `Config.default` / C getter behavior, not the
Swift wrapper's fallback values used when no config handle exists. Any future
Roastty-specific divergence must be designed explicitly in a later experiment.

| Key                              | Output type             | Default                                  |
| -------------------------------- | ----------------------- | ---------------------------------------- |
| `initial-window`                 | `bool`                  | `true`                                   |
| `quit-after-last-window-closed`  | `bool`                  | `false` on macOS                         |
| `window-save-state`              | `const char*`           | `default`                                |
| `window-decoration`              | `const char*`           | `auto`                                   |
| `window-theme`                   | `const char*`           | `auto`                                   |
| `background-opacity`             | `double`                | `1.0`                                    |
| `bell-audio-volume`              | `double`                | `0.5`                                    |
| `notify-on-command-finish-after` | `uintptr_t`             | `5000`                                   |
| `window-position-x`              | `int16_t`               | absent; return `false`                   |
| `window-position-y`              | `int16_t`               | absent; return `false`                   |
| `title`                          | nullable `const char*`  | present as null; return `true` with null |
| `bell-audio-path`                | `roastty_config_path_s` | absent; return `false`                   |

These keys are chosen because the reference Swift config wrapper reads them
during normal app/window setup and they cover the first useful ABI shapes:
booleans, strings, doubles, unsigned durations, optional numeric values,
nullable strings, and struct-valued absent options.

## Changes

1. Extend `roastty/include/roastty.h`.
   - Add `roastty_config_get`.
   - Add the scoped config value structs needed by this experiment:
     - `roastty_config_path_s`
   - Pin `roastty_config_path_s` to this C layout:

     ```c
     typedef struct {
       const char* path;
       bool optional;
     } roastty_config_path_s;
     ```

   - Do not copy the full upstream config type surface into Roastty.

2. Extend `roastty/src/lib.rs`.
   - Store a default config table or equivalent typed match logic in
     `roastty_config_get`.
   - Keep string defaults as static null-terminated strings with stable
     lifetimes.
   - Treat null handles, null output pointers, and null key pointers as safe
     failure cases that return `false`.
   - Match keys using the byte slice `(key, len)`, not null-terminated string
     assumptions.
   - For `const char*` outputs, write a pointer into caller-provided pointer
     storage. The caller passes `const char**` through `void*`.
   - For `title`, write `NULL` into caller-provided pointer storage and return
     `true`.
   - Return `false` for unknown keys.
   - Do not mutate config state in `roastty_config_get`.

3. Preserve lifecycle semantics from Experiment 3.
   - Existing config/app/surface lifecycle tests must continue to pass.
   - Existing string ownership tests must continue to pass.
   - Existing `roastty_*` export and no-`ghostty_*` checks must continue to
     pass.

4. Extend the C ABI harness.
   - Test every key in the initial subset.
   - Test correct typed output values.
   - Test unknown key returns `false`.
   - Test null config, null output pointer, and null key pointer return `false`
     and do not crash.
   - Test key lookup with explicit length so a key buffer containing extra bytes
     after `len` does not affect matching.

5. Add Rust unit tests if useful.
   - Unit tests may cover internal helper behavior, but the C harness remains
     the primary ABI proof.

6. Update `roastty/ABI_INVENTORY.md`.
   - Move the mapping for upstream `ghostty_config_get` to implemented, mapped
     to `roastty_config_get`.
   - Add notes for the initially implemented key subset.
   - Keep upstream names only in this inventory as reference citations.

7. Do not touch Wezboard.

8. Do not modify the vendored upstream checkout.

9. Do not wire the Swift app to Roastty yet.

## Test Parity

Relevant upstream reference behavior:

- `vendor/ghostty/src/config/CApi.zig` implements the generic config getter and
  has tests for bools, strings/enums, optional-null behavior, unknown keys, and
  numeric values.
- `vendor/ghostty/macos/Sources/Ghostty/Ghostty.Config.swift` shows how the
  Swift app calls config lookup for typed values.

Roastty must add equivalent tests for this experiment's subset:

- bool lookup;
- string lookup;
- double lookup;
- unsigned duration lookup;
- absent optional numeric lookup;
- present nullable string lookup;
- absent struct lookup;
- unknown-key lookup;
- null input safety;
- explicit key length behavior.

Do not claim full config compatibility. Keys outside the subset should remain
unknown until later experiments implement them with tests.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/tests/abi_harness.rs
prettier --write --prose-wrap always --print-width 80 \
  roastty/ABI_INVENTORY.md \
  issues/0800-roastty-architecture/04-config-get-defaults.md
cargo test -p roastty
cargo build -p roastty
nm -gU target/debug/libroastty.dylib | rg '_roastty_config_get$'
! nm -gU target/debug/libroastty.dylib | rg 'ghostty_'
for sym in \
  roastty_init \
  roastty_info \
  roastty_string_free \
  roastty_config_new \
  roastty_config_free \
  roastty_config_clone \
  roastty_config_load_cli_args \
  roastty_config_load_file \
  roastty_config_load_default_files \
  roastty_config_load_recursive_files \
  roastty_config_finalize \
  roastty_config_get \
  roastty_config_diagnostics_count \
  roastty_config_get_diagnostic \
  roastty_config_open_path \
  roastty_app_new \
  roastty_app_free \
  roastty_app_tick \
  roastty_app_userdata \
  roastty_app_set_focus \
  roastty_app_update_config \
  roastty_app_needs_confirm_quit \
  roastty_app_has_global_keybinds \
  roastty_app_set_color_scheme \
  roastty_surface_config_new \
  roastty_surface_new \
  roastty_surface_free \
  roastty_surface_userdata \
  roastty_surface_app \
  roastty_surface_update_config \
  roastty_surface_needs_confirm_quit \
  roastty_surface_process_exited \
  roastty_surface_set_content_scale \
  roastty_surface_set_focus \
  roastty_surface_set_occlusion \
  roastty_surface_set_size \
  roastty_surface_size \
  roastty_surface_foreground_pid \
  roastty_surface_tty_name \
  roastty_surface_set_color_scheme \
  roastty_surface_request_close
do
  nm -gU target/debug/libroastty.dylib | rg "_${sym}$"
done
! rg -n -i 'ghostty' roastty -g '!ABI_INVENTORY.md'
rg -n '#include "roastty.h"|#include "roastty/include/roastty.h"' \
  roastty/tests/abi_harness.c
! rg -n '#include "ghostty.h"|vendor/ghostty/include/ghostty.h' \
  roastty/tests roastty/include
cargo check -p webtui
cargo check -p roamium
./scripts/build.sh webtui
./scripts/build.sh roamium
git status --short
```

Expected results:

- `cargo test -p roastty` passes.
- The C harness proves every key in this experiment's subset.
- `roastty_config_get` and all existing scoped Roastty lifecycle symbols are
  exported unmangled.
- No `ghostty_*` symbols are exported.
- Roastty-owned code still has no forbidden upstream-name references outside
  `roastty/ABI_INVENTORY.md`.
- Existing `webtui` and `roamium` checks/build scripts still pass.
- Expected source changes are limited to:
  - `roastty/include/roastty.h`;
  - `roastty/src/lib.rs`;
  - `roastty/tests/abi_harness.c`;
  - `roastty/tests/abi_harness.rs` only if the harness wrapper needs adjustment;
  - `roastty/ABI_INVENTORY.md`;
  - Issue 800 documentation.

## Failure Criteria

This experiment fails if:

- `roastty_config_get` assumes null-terminated keys instead of respecting
  `(key, len)`;
- string values returned by config lookup point to short-lived Rust allocations;
- null config/output/key inputs can crash;
- unknown keys return `true`;
- the experiment claims full config parsing or full upstream config parity;
- any `ghostty_*` symbol is exported from `libroastty`;
- Roastty-owned source files keep forbidden upstream-name references outside
  allowed inventory/documentation citations;
- Wezboard files are modified;
- the vendored upstream checkout is modified;
- `webtui` or `roamium` checks/build scripts regress;
- the experiment proceeds without an approved AI design review and separate plan
  commit;
- the result is recorded without an approved AI completion review and separate
  result commit.

## AI Design Review

Initial review:

- `logs/codex-review/20260531-080058-209378-last-message.md`
- Result: **Needs changes**

Valid findings addressed:

- Stated that defaults come from upstream `Config.default` / C getter behavior,
  not Swift no-config fallbacks.
- Corrected mismatched defaults for `quit-after-last-window-closed`,
  `window-save-state`, `window-decoration`, and `window-theme`.
- Pinned the `roastty_config_path_s` C layout.
- Clarified `const char*` output semantics through caller-provided
  `const char**` storage.
- Restored full symbol export verification for existing scoped Roastty lifecycle
  symbols plus `roastty_config_get`.

Follow-up review:

- `logs/codex-review/20260531-080349-035980-last-message.md`
- Result: **Pass**

Codex confirmed the prior findings are resolved and found no remaining blockers.
Experiment 4 is approved for implementation after this reviewed plan is
committed as its own plan commit.
