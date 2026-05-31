# Experiment 3: Replace the Compatibility ABI With Roastty Naming

## Description

Experiment 2 proved that Rust can expose and test a C ABI, but it failed the
architecture goal because it preserved `ghostty_*` compatibility symbols.

This experiment corrects the foundation. Roastty is a renamed Rust adaptation of
Ghostty, not a binary-compatible `libghostty` replacement. The public ABI,
public C header, tests, inventory, and source identifiers in Roastty-owned code
must use `roastty` / `Roastty` names. References to `ghostty` are allowed only
when citing the upstream project or vendored reference paths.

The goal is still only an inert lifecycle skeleton. This experiment must not add
terminal emulation, PTY IO, rendering, fonts, config semantics, Swift app
integration, or TermSurf browser features.

## Changes

1. Replace the failed `ghostty_*` exported ABI with a `roastty_*` ABI.
   - Rename every exported function from the Experiment 2 scope:
     - `ghostty_init` -> `roastty_init`
     - `ghostty_info` -> `roastty_info`
     - `ghostty_string_free` -> `roastty_string_free`
     - `ghostty_config_*` -> `roastty_config_*`
     - `ghostty_app_*` -> `roastty_app_*`
     - `ghostty_surface_*` -> `roastty_surface_*`
   - Rename C-facing Rust types from `Ghostty...` to `Roastty...`.
   - Rename opaque handle aliases to `RoasttyApp`, `RoasttyConfig`, and
     `RoasttySurface`.
   - Rename internal test helpers and variables where they are Roastty-owned.
   - Do not export compatibility `ghostty_*` symbols.

2. Add a Roastty-owned public C header.
   - Create `roastty/include/roastty.h`.
   - The header should be a scoped, renamed adaptation of the subset of
     `vendor/ghostty/include/ghostty.h` needed by this experiment.
   - Public types should use Roastty names:
     - `roastty_app_t`
     - `roastty_config_t`
     - `roastty_surface_t`
     - `roastty_info_s`
     - `roastty_string_s`
     - `roastty_runtime_config_s`
     - `roastty_surface_config_s`
     - `roastty_surface_size_s`
   - Public constants and enums should use `ROASTTY_` prefixes.
   - Keep the header limited to the lifecycle subset. Do not copy the entire
     upstream header into Roastty before those APIs are implemented.

3. Replace the external ABI harness.
   - Update `roastty/tests/abi_harness.c` to include
     `roastty/include/roastty.h`.
   - The harness must call `roastty_*` symbols only.
   - The harness must not include `vendor/ghostty/include/ghostty.h`.
   - Keep the existing lifecycle, string ownership, repeated create/free, and
     null-input coverage from Experiment 2.
   - Do not add committed source-code exceptions that contain the forbidden
     upstream name. The case-insensitive source scan, header include checks, and
     symbol-export checks are the guardrails.

4. Replace the ABI inventory with a renamed mapping.
   - Update `roastty/ABI_INVENTORY.md`.
   - It may cite upstream Ghostty symbol names as reference material, but the
     implemented column must list Roastty names.
   - Include at least:
     - implemented Roastty lifecycle symbols;
     - upstream Ghostty reference symbols that informed the mapping;
     - deferred Roastty symbols derived from Swift-used upstream symbols;
     - non-relevant upstream symbols for this skeleton.
   - Make clear that upstream names are references only and are not app-facing
     Roastty ABI names.

5. Enforce the naming policy mechanically.
   - Add verification commands that prove the built library exports every scoped
     `roastty_*` symbol.
   - Add verification commands that prove the built library exports no
     `ghostty_*` symbols.
   - Search the Roastty-owned implementation and tests case-insensitively for
     forbidden `ghostty` references so old identifiers such as `GhosttyApp` and
     `GHOSTTY_SUCCESS` are caught.
   - Verify the harness includes `roastty.h` and does not include `ghostty.h` or
     a vendored Ghostty header path.
   - Allowed matches are limited to:
     - `roastty/ABI_INVENTORY.md`, where upstream names are cited;
     - issue documentation;
     - vendored upstream paths under `vendor/ghostty/`;
     - future attribution files if introduced later.

6. Keep the workspace shape from Experiment 2.
   - `roastty` remains a top-level Cargo workspace member.
   - Wezboard remains outside the top-level workspace.
   - Do not modify Wezboard.
   - Do not modify the Ghostty vendor checkout.

7. Keep the implementation behavior inert.
   - Config/app/surface handles may still store only minimal lifecycle state.
   - String ownership behavior must still match the intended C ABI shape: empty
     string, allocated sentinel string, allocated non-sentinel byte string, and
     matching free function.
   - `roastty_surface_size(surface)` should still round-trip only width/height
     pixels and return zero for terminal-derived fields until terminal and font
     metrics exist.

## Test Parity

This experiment's test parity target remains the ABI/lifecycle subset.

Relevant upstream reference behavior:

- `vendor/ghostty/src/main_c.zig` provides reference tests for string ownership
  shape.
- `vendor/ghostty/include/ghostty.h` provides the reference C layout and API
  concepts that Roastty is renaming and adapting.
- `vendor/ghostty/macos/Sources/` shows which lifecycle APIs the reference Swift
  app needs, but the future Roastty Swift app must call the renamed `roastty_*`
  equivalents.

Roastty must retain equivalent automated tests for:

- empty string result;
- allocated sentinel string result;
- allocated non-sentinel byte string result;
- freeing string results;
- config create/clone/free;
- app create/free and userdata round trip;
- surface create/free, parent app round trip, and size round trip;
- representative null input safety;
- absence of exported `ghostty_*` symbols.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/tests/abi_harness.rs
prettier --write --prose-wrap always --print-width 80 \
  roastty/ABI_INVENTORY.md \
  issues/0800-roastty-architecture/03-roastty-renamed-abi.md
cargo test -p roastty
cargo build -p roastty
cargo metadata --format-version 1 --no-deps
nm -gU target/debug/libroastty.dylib | rg 'roastty_'
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
! rg -n -i 'ghostty' roastty \
  -g '!ABI_INVENTORY.md'
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

- The C harness compiles against `roastty/include/roastty.h`.
- The C harness calls only `roastty_*` symbols.
- `cargo test -p roastty` passes.
- The built dynamic library exports every scoped `roastty_*` symbol unmangled.
- The built dynamic library exports no `ghostty_*` symbols.
- `rg -n -i 'ghostty' roastty -g '!ABI_INVENTORY.md'` returns no matches.
- The harness includes `roastty.h`.
- Roastty-owned headers and tests do not include `ghostty.h` or
  `vendor/ghostty/include/ghostty.h`.
- `cargo metadata` lists `webtui`, `roamium`, and `roastty` as top-level
  workspace members.
- `cargo metadata` does not list any Wezboard crate as a top-level workspace
  member.
- Existing `webtui` and `roamium` checks/build scripts still pass.
- Expected source changes are limited to:
  - `roastty/`;
  - top-level workspace lockfile only if Cargo changes it;
  - Issue 800 documentation.

## Failure Criteria

This experiment fails if:

- any `ghostty_*` symbol is exported from `libroastty`;
- the Roastty C harness includes `vendor/ghostty/include/ghostty.h`;
- Roastty-owned source files keep `ghostty` references outside explicitly
  allowed upstream-reference docs;
- the implementation claims terminal, PTY, renderer, font, input, Swift app, or
  browser-overlay behavior;
- Wezboard files are modified;
- the Ghostty vendor checkout is modified;
- `webtui` or `roamium` checks/build scripts regress;
- the experiment proceeds without an approved AI design review and a separate
  plan commit;
- the result is recorded without an approved AI completion review and a separate
  result commit.

## AI Design Review

Initial review:

- `logs/codex-review/20260531-074951-295964-last-message.md`
- Result: **Needs changes**

Valid findings addressed:

- Made the forbidden-name scan case-insensitive so old identifiers such as
  `GhosttyApp` and `GHOSTTY_SUCCESS` are caught.
- Added a full scoped symbol loop so every required `roastty_*` export is
  checked by name.
- Added explicit header include/exclude checks for `roastty.h`, `ghostty.h`, and
  `vendor/ghostty/include/ghostty.h`.
- Clarified the guardrails for preventing compatibility-name leakage.

Follow-up review:

- `logs/codex-review/20260531-075157-675481-last-message.md`
- Result: **Needs one correction**

Codex found that a proposed `#pragma GCC poison` guard would itself contain
forbidden upstream-name tokens and conflict with the case-insensitive source
scan.

Final review:

- `logs/codex-review/20260531-075257-672567-last-message.md`
- Result: **Pass**

Codex confirmed the contradiction is resolved and found no remaining blockers.
Experiment 3 is approved for implementation after this reviewed plan is
committed as its own plan commit.

## Result

**Result:** Pass

Replaced the failed `ghostty_*` compatibility ABI with a Roastty-owned
`roastty_*` lifecycle ABI.

Changes made:

- Renamed the Roastty-owned exported functions from `ghostty_*` to `roastty_*`.
- Renamed Rust C-facing types from `Ghostty...` to `Roastty...`.
- Added `roastty/include/roastty.h` as the public scoped Roastty lifecycle
  header.
- Updated the external C harness to include `roastty.h` and call only
  `roastty_*` symbols.
- Updated the Rust harness wrapper to include from `roastty/include` instead of
  the vendored upstream header directory.
- Rewrote `roastty/ABI_INVENTORY.md` as an upstream-reference-to-Roastty mapping
  so upstream names are citations only and implemented ABI names are Roastty
  names.
- Kept the implementation inert: no terminal emulation, PTY IO, rendering,
  fonts, config semantics, Swift app integration, or browser-overlay behavior.
- Did not modify Wezboard.
- Did not modify the vendored upstream checkout.

Verification run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/tests/abi_harness.rs
prettier --write --prose-wrap always --print-width 80 \
  roastty/ABI_INVENTORY.md \
  issues/0800-roastty-architecture/03-roastty-renamed-abi.md
cargo test -p roastty
cargo build -p roastty
cargo metadata --format-version 1 --no-deps | jq -r '.packages[] | "\(.name) \(.manifest_path)"'
nm -gU target/debug/libroastty.dylib | rg 'roastty_'
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

All verification commands passed.

The metadata check listed exactly the top-level TermSurf-owned workspace
members:

```text
webtui /Users/ryan/dev/termsurf/webtui/Cargo.toml
roamium /Users/ryan/dev/termsurf/roamium/Cargo.toml
roastty /Users/ryan/dev/termsurf/roastty/Cargo.toml
```

The complete symbol loop found every scoped `roastty_*` export. The negative
symbol check found no `ghostty_*` exports. The case-insensitive source scan
found no forbidden upstream-name references in Roastty-owned code outside
`roastty/ABI_INVENTORY.md`.

The only expected uncommitted files after implementation were:

```text
M issues/0800-roastty-architecture/README.md
M issues/0800-roastty-architecture/03-roastty-renamed-abi.md
M roastty/ABI_INVENTORY.md
M roastty/src/lib.rs
M roastty/tests/abi_harness.c
M roastty/tests/abi_harness.rs
?? roastty/include/
```

## AI Completion Review

- `logs/codex-review/20260531-075759-117272-last-message.md`
- Result: **Pass**

Codex found no blocking implementation findings. It confirmed the implementation
satisfies Experiment 3 and the Issue 800 naming policy: Roastty-owned code uses
`roastty` / `Roastty` names, the C harness includes `roastty.h`, no `ghostty_*`
symbols are exported, and the forbidden-name scan found no disallowed
upstream-name references outside `roastty/ABI_INVENTORY.md`.

The only commit-readiness items were to record this completion review and ensure
the new public header is included in the result commit.

## Conclusion

Roastty now has the correct first ABI skeleton: a Rust-built `libroastty` with
Roastty-owned `roastty_*` public symbols, a Roastty-owned public C header, and
an external C harness that proves the lifecycle ABI without relying on upstream
compatibility names.

The next experiment can move past naming repair and start adding the next
behavioral layer. The likely next target is config and lifecycle behavior:
implement a small, tested subset of config lookup/defaults and app/surface
lifecycle semantics behind the renamed Roastty ABI, still without terminal
emulation, PTY IO, rendering, fonts, Swift app integration, or browser features.
