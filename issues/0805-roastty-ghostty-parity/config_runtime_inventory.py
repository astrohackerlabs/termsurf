#!/usr/bin/env python3
"""Inventory Roastty config runtime/UI effect coverage for Issue 805.

This is a bounded markdown/source inventory for CFG-223. It records config
effects that must be proven in the running app/terminal/renderer surface and
keeps broad runtime/UI parity honest by requiring row-level evidence.
"""

from __future__ import annotations

import argparse
import dataclasses
from collections import Counter
from pathlib import Path


@dataclasses.dataclass(frozen=True)
class RuntimeRow:
    id: str
    behavior: str
    ghostty_reference: str
    roastty_reference: str
    family: str
    status: str
    evidence: str
    missing_evidence: str
    guard_tier: str
    guard_command: str


ROWS = [
    RuntimeRow(
        id="RUNTIME-001",
        behavior="app-level clipboard read/write policy effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` clipboard access fields; `vendor/ghostty/src/Surface.zig` clipboard actions",
        roastty_reference="`roastty/src/lib.rs` clipboard callbacks and app config fields",
        family="clipboard",
        status="Oracle complete",
        evidence=(
            "Clipboard callback tests cover read/write allow/deny/ask policy "
            "dispatch through the runtime callbacks, and app/surface update "
            "tests prove clipboard policies refresh existing surfaces."
        ),
        missing_evidence="None for clipboard read/write policy runtime dispatch.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml clipboard_read && cargo test --manifest-path roastty/Cargo.toml clipboard_write`",
    ),
    RuntimeRow(
        id="RUNTIME-002",
        behavior="clipboard copy/paste transformation effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` clipboard transform fields; `vendor/ghostty/src/Surface.zig` copy/paste paths",
        roastty_reference="`roastty/src/lib.rs::copy_to_clipboard`, `clipboard_paste_is_unsafe`, paste helpers",
        family="clipboard",
        status="Oracle complete",
        evidence=(
            "Clipboard and paste tests cover bracketed paste safety, paste "
            "protection, codepoint-map copy transformation, trimming trailing "
            "spaces, and selection-clear-on-copy behavior."
        ),
        missing_evidence="None for covered clipboard transform runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml clipboard && cargo test --manifest-path roastty/Cargo.toml paste`",
    ),
    RuntimeRow(
        id="RUNTIME-003",
        behavior="selection behavior effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` selection fields; `vendor/ghostty/src/Surface.zig` selection and copy paths",
        roastty_reference="`roastty/src/lib.rs` selection gesture, selection read, and selection clear paths",
        family="selection",
        status="Oracle complete",
        evidence=(
            "`app_and_surface_update_config_sync_selection_behavior` plus "
            "selection gesture/read tests prove selection clear-on-typing, "
            "selection word character boundaries, copy-on-select-adjacent "
            "selection state, and selection runtime update behavior."
        ),
        missing_evidence="None for covered selection config runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml selection`",
    ),
    RuntimeRow(
        id="RUNTIME-004A",
        behavior="mouse-reporting config and toggle_mouse_reporting runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` mouse/click fields; `vendor/ghostty/src/Surface.zig` mouse handlers",
        roastty_reference="`roastty/src/lib.rs` `mouse_reporting`, `mouse_report_context`, `roastty_surface_mouse_captured`, and `toggle_mouse_reporting`",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`mouse_runtime_reporting_config_and_toggle_gate_capture` proves "
            "the configured `mouse-reporting` value gates terminal mouse "
            "capture, the `toggle_mouse_reporting` runtime action flips that "
            "gate, and surface config update refreshes the existing surface."
        ),
        missing_evidence="None for mouse-reporting config/toggle runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml mouse_runtime_reporting_config_and_toggle_gate_capture`",
    ),
    RuntimeRow(
        id="RUNTIME-004B",
        behavior="mouse-shift-capture config and XTSHIFTESCAPE runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `mouse-shift-capture`; `vendor/ghostty/src/Surface.zig::mouseShiftCapture`",
        roastty_reference="`roastty/src/lib.rs::mouse_shift_capture`; `roastty/src/config/mod.rs::MouseShiftCapture::capture_shift`",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`mouse_runtime_shift_capture_uses_app_config_and_terminal_flag` "
            "proves surface mouse shift capture combines app config and the "
            "terminal XTSHIFTESCAPE flag, including `never` and `always` "
            "overrides."
        ),
        missing_evidence="None for mouse-shift-capture runtime decision behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml mouse_runtime_shift_capture_uses_app_config_and_terminal_flag`",
    ),
    RuntimeRow(
        id="RUNTIME-004C",
        behavior="mouse-scroll-multiplier runtime scroll step effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `mouse-scroll-multiplier`; `vendor/ghostty/src/Surface.zig::scrollCallback`",
        roastty_reference="`roastty/src/lib.rs::mouse_scroll_steps` and surface config update",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`mouse_runtime_scroll_multiplier_drives_precision_and_discrete_steps` "
            "proves precision and discrete scroll paths use configured "
            "multipliers and that surface config update refreshes the runtime "
            "scroll multiplier."
        ),
        missing_evidence="None for mouse-scroll-multiplier runtime step behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml mouse_runtime_scroll_multiplier_drives_precision_and_discrete_steps`",
    ),
    RuntimeRow(
        id="RUNTIME-004D",
        behavior="click-repeat-interval selection timing effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `click-repeat-interval`; `vendor/ghostty/src/Surface.zig` selection gesture press repeat",
        roastty_reference="`roastty/src/lib.rs::click_repeat_interval_ns` and `selection_press`",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`mouse_runtime_click_repeat_interval_drives_selection_timing` "
            "proves configured click-repeat timing controls whether repeated "
            "left clicks advance the selection gesture or restart it."
        ),
        missing_evidence="None for click-repeat-interval selection timing behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml mouse_runtime_click_repeat_interval_drives_selection_timing`",
    ),
    RuntimeRow(
        id="RUNTIME-004E",
        behavior="cursor-click-to-move runtime prompt movement effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `cursor-click-to-move`; `vendor/ghostty/src/Surface.zig` prompt click movement",
        roastty_reference="`roastty/src/lib.rs` mouse handlers and terminal prompt tracking; `roastty/src/terminal/terminal.rs` prompt click action",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`cursor_click_to_move_click_events_writes_sgr_mouse_press`, "
            "`cursor_click_to_move_line_mode_writes_cursor_keys`, and "
            "`cursor_click_to_move_line_mode_same_cell_consumes_release` "
            "prove eligible prompt clicks write Ghostty-style SGR click-event "
            "bytes or cursor-key movement bytes, including eligible no-op "
            "line clicks. `cursor_click_to_move_surface_gates_ineligible_clicks` "
            "proves disabled config, missing prompt-click support, active "
            "selection, dragged clicks, and clicks before the prompt are not "
            "handled."
        ),
        missing_evidence="None for cursor-click-to-move prompt click-event and line-movement runtime behavior.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml cursor_click_to_move`",
    ),
    RuntimeRow(
        id="RUNTIME-004F",
        behavior="mouse-hide-while-typing runtime cursor visibility effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `mouse-hide-while-typing`; `vendor/ghostty/src/Surface.zig` key input mouse hide/show paths",
        roastty_reference="`roastty/src/lib.rs` key input and macOS mouse shape/action callbacks",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`mouse_hide_while_typing_*` tests prove enabled text-key presses "
            "emit hidden once, key releases and empty-text presses do not hide, "
            "mouse position/button/scroll events emit visible when hidden, "
            "config update disables hiding and shows a hidden mouse, and "
            "unconsumed configured bindings still hide before encoded "
            "fallthrough input."
        ),
        missing_evidence="None for libroastty mouse-hide-while-typing runtime visibility actions.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml mouse_hide_while_typing`",
    ),
    RuntimeRow(
        id="RUNTIME-004G",
        behavior="right-click-action runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `right-click-action`; `vendor/ghostty/src/Surface.zig` right-click action dispatch",
        roastty_reference="`roastty/src/lib.rs` mouse button handlers and app runtime actions",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`right_click_action_*` tests prove right-button press honors "
            "`ignore`, `paste`, `copy`, `copy-or-paste`, and `context-menu`; "
            "clears or preserves selections according to pinned Ghostty "
            "semantics; refreshes existing surfaces on config update; and "
            "does not bypass terminal mouse reporting."
        ),
        missing_evidence="None for non-link right-click-action surface runtime behavior; link-specific context-menu behavior remains tracked by RUNTIME-012B2.",
        guard_tier="Tier 3",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml right_click_action`",
    ),
    RuntimeRow(
        id="RUNTIME-004H",
        behavior="middle-click-action runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `middle-click-action`; `vendor/ghostty/src/Surface.zig` middle-click action dispatch",
        roastty_reference="`roastty/src/lib.rs` mouse button handlers, middle-click action state, and clipboard paste paths",
        family="mouse",
        status="Oracle complete",
        evidence=(
            "`middle_click_action_*` tests prove middle-button press honors "
            "`primary-paste` and `ignore`, chooses the standard or selection "
            "clipboard according to `copy-on-select` and selection clipboard "
            "support, refreshes existing surfaces on config update, and does "
            "not bypass terminal mouse reporting."
        ),
        missing_evidence="None for middle-click-action runtime dispatch behavior.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml middle_click_action`",
    ),
    RuntimeRow(
        id="RUNTIME-005",
        behavior="keyboard remap and keybind dispatch effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` key-remap/keybind fields; `vendor/ghostty/src/Surface.zig` key dispatch",
        roastty_reference="`roastty/src/lib.rs` key remap, keybind, and key table helpers",
        family="input",
        status="Oracle complete",
        evidence=(
            "Keybinding tests cover focused/global/all scope dispatch, key "
            "tables, key sequences, catch-all and unconsumed behavior; "
            "`surface_key_remap_*` tests prove remap affects binding detection, "
            "encoded input, and app/surface config updates."
        ),
        missing_evidence=(
            "None for key remap/keybind dispatch runtime behavior; "
            "command-palette UI dispatch remains tracked by `RUNTIME-011`."
        ),
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_key_remap && cargo test --manifest-path roastty/Cargo.toml surface_key_table`",
    ),
    RuntimeRow(
        id="RUNTIME-006",
        behavior="color, palette, theme, and color-scheme runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` colors/palette/theme fields; `vendor/ghostty/src/Surface.zig` config change rendering paths",
        roastty_reference="`roastty/src/lib.rs::derived_config_palette`, color scheme reload tests, renderer state",
        family="color",
        status="Oracle complete",
        evidence=(
            "`surface_apply_config_updates_palette_defaults`, "
            "`surface_apply_config_updates_generated_palette_defaults`, and "
            "color-scheme reload tests prove palette/generated-palette defaults "
            "and conditional theme/color-scheme runtime updates."
        ),
        missing_evidence="None for covered color/palette/theme runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_apply_config_updates_palette && cargo test --manifest-path roastty/Cargo.toml color_scheme`",
    ),
    RuntimeRow(
        id="RUNTIME-007",
        behavior="font selection, shaping, fallback, metrics, and font-size runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` font fields; `vendor/ghostty/src/Surface.zig` font grid/runtime update paths",
        roastty_reference="`roastty/src/lib.rs` font-size runtime state; `roastty/src/font`",
        family="font",
        status="Gap",
        evidence=(
            "Experiment 105 proves reload font-size behavior, but CFG-223 still "
            "needs runtime proof for the broader font surface: configured font "
            "families, fallback, shaping, glyph metrics, feature/variation "
            "effects, and renderer-visible font changes."
        ),
        missing_evidence="Add focused font runtime/renderer oracles beyond parser/formatter/default coverage.",
        guard_tier="Tier 2",
        guard_command="TBD by future CFG-223 font runtime experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-008A",
        behavior="renderer scheduler, cursor blink, focus, occlusion, and live renderer rebuild control runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `window-vsync` and `cursor-style-blink`; `vendor/ghostty/src/renderer/generic.zig` vsync renderer config; `vendor/ghostty/src/Surface.zig` cursor blink and renderer update paths",
        roastty_reference="`roastty/src/lib.rs` present driver, cursor blink helpers, live renderer visibility, focus, occlusion, and config update paths",
        family="renderer",
        status="Oracle complete",
        evidence=(
            "Experiment 125 split out the proven renderer control slice. "
            "`present_driver_*` tests prove `window-vsync` drives fallback or "
            "display-link present scheduling for new live surfaces, display id "
            "updates reach active display links, and present drivers stop "
            "before surface drop. `live_cursor_blink_*` tests prove cursor "
            "blink ticks, output reset throttling, terminal-output-only reset, "
            "focus-loss pause, and focus-gain reset behavior. "
            "`live_renderer_options_*` tests prove occlusion gates live "
            "presentation, live config updates request a renderer rebuild, "
            "and ABI-only surfaces stay quiet. "
            "`renderer_control_runtime_parity.py` statically checks pinned "
            "Ghostty's `window-vsync`, `cursor-style-blink`, "
            "`renderer/generic.zig` vsync config, and surface renderer "
            "control markers against Roastty's runtime/test markers."
        ),
        missing_evidence="None for renderer scheduler, cursor blink, focus, occlusion, and live renderer rebuild control runtime behavior.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml present_driver && cargo test --manifest-path roastty/Cargo.toml live_cursor_blink && cargo test --manifest-path roastty/Cargo.toml live_renderer_options && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_control_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-008B",
        behavior="visible opacity, blur, padding, cursor style shape/rendering, window padding color, custom shader output, and remaining renderer-visible effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` renderer/window visual fields; `vendor/ghostty/src/renderer/generic.zig` derived renderer config; `vendor/ghostty/src/Surface.zig` renderer config messages",
        roastty_reference="`roastty/src/lib.rs` live renderer and render state; `roastty/src/renderer`",
        family="renderer",
        status="Gap",
        evidence=(
            "Experiment 125 split out the proven scheduler/cursor/focus/"
            "occlusion/rebuild control slice, but CFG-223 still needs "
            "representative runtime or GUI proof for visible opacity, blur, "
            "padding, cursor style shape/rendering, window padding color, "
            "custom shader output, and other renderer-visible effects."
        ),
        missing_evidence="Add renderer/runtime or GUI smoke rows for visible opacity, blur, padding, cursor style shape/rendering, window padding color, custom shader output, and other renderer-visible effects.",
        guard_tier="Tier 3",
        guard_command="TBD by future CFG-223 renderer visual experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-009A",
        behavior="vt-kam-allowed terminal key gating effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `vt-kam-allowed`; terminal ANSI KAM mode behavior",
        roastty_reference="`roastty/src/lib.rs` `vt_kam_allowed` config state, terminal ANSI mode 2 gate, and key dispatch",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "`vt_kam_allowed_*` tests prove the configured `vt-kam-allowed` "
            "value gates terminal KAM key blocking, KAM-disabled terminals "
            "continue to accept input, live config update toggles the existing "
            "surface gate, and configured keybindings run before the KAM gate."
        ),
        missing_evidence="None for vt-kam-allowed terminal key gating behavior.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml vt_kam_allowed`",
    ),
    RuntimeRow(
        id="RUNTIME-009B1",
        behavior="scrollback-limit zero/no-history and alternate-screen no-scrollback terminal effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `scrollback-limit`; `vendor/ghostty/src/termio/Termio.zig` terminal init; alternate-screen terminal behavior",
        roastty_reference="`roastty/src/lib.rs::start_termio`, `roastty/src/termio.rs`, and `roastty/src/terminal` scrollback/alternate-screen behavior",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 117 adds `config_scrollback_limit_runtime_*` tests "
            "proving parsed config `scrollback-limit = 0` disables "
            "scrollback rows on PTY-backed surfaces while default/nonzero "
            "behavior still allows history. "
            "`terminal_stream_alt_screen_has_no_scrollback_and_formatter_reads_active_screen` "
            "proves the terminal-core alternate-screen no-scrollback behavior."
        ),
        missing_evidence="None for parsed config scrollback-limit = 0 no-history behavior and alternate-screen no-scrollback terminal-core behavior covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_scrollback_limit_runtime && cargo test --manifest-path roastty/Cargo.toml terminal_stream_alt_screen_has_no_scrollback_and_formatter_reads_active_screen`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2A",
        behavior="title-report CSI 21t gate for OSC-driven terminal titles",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `title-report`; `vendor/ghostty/src/Surface.zig` `report_title` gate and CSI 21t response",
        roastty_reference="`roastty/src/terminal/terminal.rs` title-report gate; `roastty/src/termio.rs` startup options; `roastty/src/lib.rs` surface config update wiring",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 122 adds `terminal_stream_title_report_*` tests proving "
            "CSI `21t` produces no PTY response by default, reports "
            "`ESC ] l <title> ESC \\\\` when enabled, and can be disabled and "
            "re-enabled without losing the stored OSC title. "
            "`config_title_report_runtime_startup_and_update_gate` proves "
            "parsed `title-report` reaches PTY-backed surfaces at startup and "
            "through `roastty_app_update_config`. "
            "`title_report_runtime_parity.py` checks pinned Ghostty's disabled "
            "default and Surface gate plus Roastty's parser, terminal gate, "
            "`TermioSpawnOptions` startup wiring, and live config update wiring."
        ),
        missing_evidence="None for the title-report CSI 21t gate for OSC-driven terminal titles covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml terminal_stream_title_report && cargo test --manifest-path roastty/Cargo.toml config_title_report_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/title_report_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B1",
        behavior="shell-integration feature env, terminal identity, resource-backed TERMINFO, env override order, and zsh bootstrap runtime effects",
        ghostty_reference="`vendor/ghostty/src/termio/Exec.zig` terminal identity env setup; `vendor/ghostty/src/termio/shell_integration.zig` shell feature and zsh/XDG setup",
        roastty_reference="`roastty/src/termio.rs` Termio spawn env setup; `roastty/src/termio/shell_integration.rs` shell feature and zsh/XDG setup",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 124 split out the proven Termio shell-integration and "
            "terminal identity slice. `termio_env_*` tests prove fallback "
            "`TERM=xterm-256color`/`COLORTERM=truecolor`, resource-backed "
            "configured `TERM`, `TERMINFO`, `ROASTTY_RESOURCES_DIR`, stale "
            "env overwrites, and explicit env override order. "
            "`spawn_with_options_sets_shell_feature_env_even_when_integration_is_none` "
            "proves configured shell feature env, including cursor blink/steady, "
            "reaches child processes. `zsh_integration_spawn_with_options_*` "
            "proves forced zsh integration reaches child env and sources an "
            "inherited `ZDOTDIR` bootstrap. `shell_integration` tests guard "
            "the helper rewrites for supported shells, and "
            "`shell_integration_runtime_parity.py` statically checks pinned "
            "Ghostty's corresponding terminal identity and shell setup markers."
        ),
        missing_evidence="None for this shell-integration feature env, terminal identity, resource-backed TERMINFO, env override order, and zsh bootstrap runtime slice.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml termio_env && cargo test --manifest-path roastty/Cargo.toml spawn_with_options_sets_shell_feature_env_even_when_integration_is_none && cargo test --manifest-path roastty/Cargo.toml zsh_integration_spawn_with_options && cargo test --manifest-path roastty/Cargo.toml shell_integration && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/shell_integration_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2A",
        behavior="configured/static surface-title startup/update and non-empty OSC title dispatch/suppression effects",
        ghostty_reference="`vendor/ghostty/src/Surface.zig` configured title, direct-command title, static-title suppression, and config-update title paths; `vendor/ghostty/src/termio/stream_handler.zig` non-empty title messages",
        roastty_reference="`roastty/src/lib.rs` surface title app actions and static-title gate; `roastty/src/termio.rs` title pump field",
        family="terminal",
        status="Oracle complete",
        evidence=(
            "Experiment 126 split out configured/static surface-title runtime "
            "behavior. `surface_title_runtime_*` tests prove configured "
            "titles dispatch `ROASTTY_ACTION_SET_TITLE` at startup and config "
            "update, direct command argv[0] dispatches as the title, shell "
            "commands do not dispatch command-derived titles, non-empty OSC "
            "titles dispatch through the surface action path when no static "
            "title is configured, and static configured titles suppress later "
            "non-empty OSC title app actions. `termio_title_*` proves live "
            "PTY title changes travel through `TermioPump` without terminal "
            "callbacks, and `worker_rejects_terminal_with_callbacks` keeps "
            "callback rejection guarded. `surface_title_runtime_parity.py` "
            "statically checks pinned Ghostty's corresponding title branches "
            "and Roastty's runtime/test markers."
        ),
        missing_evidence="None for configured/static surface-title startup/update and non-empty OSC title dispatch/suppression behavior covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_title_runtime && cargo test --manifest-path roastty/Cargo.toml termio_title && cargo test --manifest-path roastty/Cargo.toml worker_rejects_terminal_with_callbacks && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/surface_title_runtime_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-009B2B2B",
        behavior="exact nonzero scrollback byte quota, empty-title/PWD fallback semantics, remaining shell-specific startup rewrite coverage, and other remaining terminal behavior effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` nonzero `scrollback-limit`; `vendor/ghostty/src/terminal/Screen.zig` byte-quota scrollback; `vendor/ghostty/src/termio/stream_handler.zig` empty-title/PWD fallback and remaining shell/title behavior; shell integration startup paths",
        roastty_reference="`roastty/src/lib.rs` terminal/termio config use; `roastty/src/termio.rs`; `roastty/src/terminal`",
        family="terminal",
        status="Gap",
        evidence=(
            "Experiments 117, 122, 124, and 126 split out zero/no-history "
            "scrollback, alternate-screen no-scrollback, CSI `21t` "
            "title-report gating, shell-integration feature env and terminal "
            "identity, zsh bootstrap, and configured/static non-empty surface "
            "title behavior. Exact nonzero `scrollback-limit` byte quota "
            "parity, empty-title/PWD fallback semantics, remaining "
            "shell-specific startup rewrite coverage, and other remaining "
            "terminal behavior toggles still need focused CFG-223 runtime "
            "proof or fixes."
        ),
        missing_evidence="Add runtime proof or fixes for exact nonzero scrollback byte quota behavior, empty-title/PWD fallback semantics, remaining shell-specific startup rewrite coverage, and other remaining terminal behavior effects.",
        guard_tier="Tier 2",
        guard_command="TBD by future CFG-223 terminal runtime experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-010A",
        behavior="PTY/process initial-command, environment, and working-directory launch effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `initial-command`, `env`, and `working-directory` fields",
        roastty_reference="`roastty/src/lib.rs::start_termio`, inherited config helpers, and `roastty/src/termio.rs` spawn options",
        family="process",
        status="Oracle complete",
        evidence=(
            "`first_surface_uses_app_initial_command`, "
            "`later_surface_after_close_ignores_app_initial_command`, and "
            "`surface_inherited_config_*` tests prove initial-command and "
            "working-directory inheritance behavior. `spawn_with_cwd_*` and "
            "`termio_env_*` tests prove the PTY boundary runs children in the "
            "requested working directory, passes explicit env values, inherits "
            "process env values, tolerates non-Unicode inherited env values, "
            "and lets explicit env values override inherited/identity/shell "
            "integration env values."
        ),
        missing_evidence="None for initial-command, environment, and working-directory launch effects covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml first_surface_uses_app_initial_command && cargo test --manifest-path roastty/Cargo.toml later_surface_after_close_ignores_app_initial_command && cargo test --manifest-path roastty/Cargo.toml surface_inherited_config && cargo test --manifest-path roastty/Cargo.toml spawn_with_cwd && cargo test --manifest-path roastty/Cargo.toml termio_env`",
    ),
    RuntimeRow(
        id="RUNTIME-010B1",
        behavior="PTY/process config command, config input, and default-shell launch effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `command` and `input` fields; `vendor/ghostty/src/Surface.zig` command selection; `vendor/ghostty/src/termio/Termio.zig` startup input queuing",
        roastty_reference="`roastty/src/lib.rs::start_termio`, `config_startup_input_bytes`, and surface launch fields",
        family="process",
        status="Oracle complete",
        evidence=(
            "Experiment 116 adds `config_command_input_runtime_*` tests proving "
            "parsed config command precedence, parsed config input delivery, "
            "decoded raw input escape handling, config input path reads, and "
            "explicit surface command/input override behavior. Existing "
            "`surface_start_without_command_*` guards prove the no-command "
            "default-shell and idempotent-start fallback slice."
        ),
        missing_evidence="None for config command, config input, decoded raw input, path input, explicit surface override, and default-shell/no-command launch effects covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_command_input_runtime && cargo test --manifest-path roastty/Cargo.toml surface_start_without_command`",
    ),
    RuntimeRow(
        id="RUNTIME-010B2A",
        behavior="PTY/process wait-after-command child-exit close/hold effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `wait-after-command`; `vendor/ghostty/src/Surface.zig::childExited`; `vendor/ghostty/src/apprt/embedded.zig` embedded surface options",
        roastty_reference="`roastty/src/lib.rs` surface wait-after-command state, child-exit handling, and close callback dispatch",
        family="process",
        status="Oracle complete",
        evidence=(
            "Experiment 118 adds `wait_after_command_runtime_*` tests proving "
            "normal child-exit close/hold behavior for default parsed config, "
            "parsed `wait-after-command = true`, embedded "
            "`RoasttySurfaceConfig.wait_after_command = true`, and explicit "
            "surface commands that force hold behavior. The tests run commands "
            "beyond a configured abnormal-exit threshold and prove EOF-only "
            "events do not close or suppress a later child-exit close."
        ),
        missing_evidence="None for normal wait-after-command child-exit close/hold behavior covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime && cargo test --manifest-path roastty/Cargo.toml close_surface && cargo test --manifest-path roastty/Cargo.toml process_exited`",
    ),
    RuntimeRow(
        id="RUNTIME-010B2B1",
        behavior="PTY/process child-exit exit-code/runtime payload capture and show_child_exited action dispatch",
        ghostty_reference="`vendor/ghostty/src/termio/Termio.zig` child-exit payload; `vendor/ghostty/src/Surface.zig::childExited` `.show_child_exited` action dispatch",
        roastty_reference="`roastty/src/termio.rs` child-exit payload capture; `roastty/src/lib.rs` `ROASTTY_ACTION_SHOW_CHILD_EXITED` dispatch",
        family="process",
        status="Oracle complete",
        evidence=(
            "Experiment 119 adds `child_exited_payload_runtime_*` tests proving "
            "PTY child exit status is captured as exit code plus runtime "
            "milliseconds, the typed `ROASTTY_ACTION_SHOW_CHILD_EXITED` payload "
            "reaches the app action callback before default close handling, "
            "wait-after-command surfaces still hold after dispatch, false "
            "action results do not suppress existing close/hold behavior, and "
            "representative above-threshold and at-or-below-threshold runtime "
            "cases both dispatch the payload."
        ),
        missing_evidence="None for child-exit payload capture and show_child_exited action dispatch covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml child_exited_payload_runtime && cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime && cargo test --manifest-path roastty/Cargo.toml process_exited && cargo test --manifest-path roastty/Cargo.toml close_surface`",
    ),
    RuntimeRow(
        id="RUNTIME-010B2B2A",
        behavior="PTY/process terminal fallback child-exit text and abnormal-command-exit-runtime close/hold policy",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `abnormal-command-exit-runtime`; `vendor/ghostty/src/Surface.zig::childExited` and `childExitedAbnormally`",
        roastty_reference="`roastty/src/lib.rs` child-exit fallback text, abnormal runtime classification, action handling, and close/hold policy",
        family="process",
        status="Oracle complete",
        evidence=(
            "Experiment 120 adds `child_exited_fallback_policy_runtime_*` "
            "tests proving normal unhandled exits write the pinned normal "
            "fallback text and still close by default, normal handled exits "
            "skip fallback text and still use normal close/hold policy, "
            "abnormal handled exits hold without fallback text, abnormal "
            "unhandled exits write the pinned Ghostty abnormal fallback labels "
            "plus launched command and runtime text and hold, equality with "
            "`abnormal-command-exit-runtime` is abnormal, and above-threshold "
            "runtime is normal."
        ),
        missing_evidence="None for child-exit terminal fallback text and abnormal-command-exit-runtime close/hold policy covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml child_exited_fallback_policy_runtime && cargo test --manifest-path roastty/Cargo.toml child_exited_payload_runtime && cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime && cargo test --manifest-path roastty/Cargo.toml process_exited && cargo test --manifest-path roastty/Cargo.toml close_surface`",
    ),
    RuntimeRow(
        id="RUNTIME-010B2B2B1",
        behavior="macOS app quit-after-last-window-closed config bridge",
        ghostty_reference="`vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift::applicationShouldTerminateAfterLastWindowClosed`; `vendor/ghostty/macos/Sources/Ghostty/Ghostty.Config.swift::shouldQuitAfterLastWindowClosed`; `vendor/ghostty/src/config/Config.zig` `quit-after-last-window-closed`",
        roastty_reference="`roastty/macos/Sources/App/macOS/AppDelegate.swift::applicationShouldTerminateAfterLastWindowClosed`; `roastty/macos/Sources/Roastty/Roastty.Config.swift::shouldQuitAfterLastWindowClosed`; `roastty/src/lib.rs::roastty_config_get`",
        family="process",
        status="Oracle complete",
        evidence=(
            "Experiment 121 adds "
            "`config_get_quit_after_last_window_closed_runtime` to prove "
            "`roastty_config_get` returns the macOS default `false`, parsed "
            "`true`, reset/default `false`, and rejects invalid null handles or "
            "outputs for `quit-after-last-window-closed`. "
            "`macos_quit_lifecycle_parity.py` proves the copied Roastty macOS "
            "`applicationShouldTerminateAfterLastWindowClosed`, "
            "`DerivedConfig.shouldQuitAfterLastWindowClosed`, and Swift config "
            "getter blocks match pinned Ghostty after expected app-name "
            "renaming, and that the embedded C ABI exposes the same config key."
        ),
        missing_evidence="None for the copied macOS quit-after-last-window-closed config bridge covered by these guards.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_get_quit_after_last_window_closed_runtime && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_quit_lifecycle_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-010B2B2B2",
        behavior="macOS app quit-after-last-window-closed-delay effect",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` documents `quit-after-last-window-closed-delay` as Linux-only; `vendor/ghostty/src/apprt/gtk/class/application.zig` implements the GTK/Linux quit timer",
        roastty_reference="`roastty/macos/Sources`; Roastty's Issue 805 target is the copied macOS app/runtime",
        family="process",
        status="Not applicable",
        evidence=(
            "Experiment 121's `macos_quit_lifecycle_parity.py` verifies "
            "pinned Ghostty documents `quit-after-last-window-closed-delay` as "
            "only implemented on Linux, verifies Ghostty's GTK app consumes "
            "the delay through the quit timer path, and verifies neither "
            "Ghostty nor Roastty macOS Swift sources consume "
            "`quit-after-last-window-closed-delay`."
        ),
        missing_evidence="None for Roastty's copied macOS app; Linux/GTK quit delay behavior is outside the Issue 805 macOS app target.",
        guard_tier="Tier 2",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_quit_lifecycle_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-011",
        behavior="macOS app/window/tab/split/menu and command palette UI effects",
        ghostty_reference="`vendor/ghostty/macos/Sources`; app/window/tab/split config-driven UI behavior",
        roastty_reference="`roastty/macos/Sources`; Roastty app wrapper and Swift UI",
        family="macOS app",
        status="Gap",
        evidence=(
            "Feature and walkthrough matrices only prove launch/cleanup and "
            "keyboard delivery. CFG-223 still needs real app walkthrough or "
            "focused macOS tests for config-driven windows, tabs, splits, "
            "menus, titlebar, fullscreen, quick terminal, and command palette UI."
        ),
        missing_evidence="Add focused macOS app walkthrough rows and GUI guards.",
        guard_tier="Tier 3",
        guard_command="TBD by future CFG-223 macOS app walkthrough experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-012A",
        behavior="link URL matching, renderer highlighting, open-url dispatch, and copy-url binding effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` `link` and `link-url`; `vendor/ghostty/src/Surface.zig` link action dispatch",
        roastty_reference="`roastty/src/config/mod.rs` default URL link; `roastty/src/renderer/link.rs`; `roastty/src/lib.rs` open-url and copy-url actions",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "`config_link_url_finalize` proves the configured default URL link "
            "is enabled or removed by `link-url`. `renderer_link_*` tests prove "
            "link highlight matching, modifier-gated ranges, and contiguous "
            "range merging. `surface_open_url_*` tests prove explicit open-url "
            "runtime action dispatch preserves kind, pointer, length, callback "
            "result, and detached/no-callback false paths. "
            "`surface_binding_action_copy_url_to_clipboard_*` tests prove OSC8 "
            "copy-url-to-clipboard binding behavior and false paths."
        ),
        missing_evidence="None for this narrow link/open-url action and renderer matching slice.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_open_url && cargo test --manifest-path roastty/Cargo.toml surface_binding_action_copy_url_to_clipboard && cargo test --manifest-path roastty/Cargo.toml renderer_link && cargo test --manifest-path roastty/Cargo.toml config_link_url_finalize`",
    ),
    RuntimeRow(
        id="RUNTIME-012B1",
        behavior="terminal BEL to live surface ring-bell action dispatch",
        ghostty_reference="`vendor/ghostty/src/termio/stream_handler.zig` BEL `.ring_bell`; `vendor/ghostty/src/Surface.zig` ring-bell throttle/action path",
        roastty_reference="`roastty/src/terminal/terminal.rs` pending bell count; `roastty/src/termio.rs` bell pump count; `roastty/src/lib.rs` `ROASTTY_ACTION_RING_BELL` dispatch",
        family="notifications",
        status="Oracle complete",
        evidence=(
            "Experiment 123 proves terminal BEL reaches the live PTY-backed "
            "surface action path without installing forbidden terminal "
            "callbacks. `bell_runtime_pending_count_*` guards terminal-core "
            "BEL counting and existing callback preservation. "
            "`termio_bell_*` guards PTY and worker pump propagation. "
            "`surface_bell_*` guards `ROASTTY_ACTION_RING_BELL` dispatch and "
            "the 100ms repeated-BEL throttle. "
            "`bell_runtime_dispatch_parity.py` statically checks the pinned "
            "Ghostty BEL `.ring_bell` path and Roastty runtime/action wiring."
        ),
        missing_evidence="None for terminal BEL to live surface ring-bell action dispatch.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml bell_runtime && cargo test --manifest-path roastty/Cargo.toml termio_bell && cargo test --manifest-path roastty/Cargo.toml surface_bell && PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/bell_runtime_dispatch_parity.py`",
    ),
    RuntimeRow(
        id="RUNTIME-012B2",
        behavior="bell feature UI/audio effects, command-finish notifications, app-notifications, hover/cursor UI, link previews, and context/menu link flows",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` notification, bell feature, link preview, and app-notification fields; `vendor/ghostty/src/Surface.zig` link hover/menu paths; macOS app notification/bell feature handling",
        roastty_reference="`roastty/macos/Sources` notification, pointer, preview, and context/menu handling; app bell feature presentation beyond action dispatch",
        family="notifications",
        status="Gap",
        evidence=(
            "Experiment 115 split out the proven deterministic link/open-url "
            "runtime slice. Experiment 123 split out terminal BEL to live "
            "surface ring-bell action dispatch. Bell feature UI/audio effects "
            "such as system beep, custom audio, attention, title/border "
            "presentation, command-finish notifications, app-notifications, "
            "link hover/cursor UI, link previews in the real app, and "
            "context/menu link flows still need focused runtime or GUI proof."
        ),
        missing_evidence="Add bell feature UI/audio, notification, app hover/cursor, preview, and context/menu link runtime or GUI walkthrough guards.",
        guard_tier="Tier 3",
        guard_command="TBD by future CFG-223 notification/link GUI or runtime experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-013",
        behavior="platform-specific or unsupported runtime effects",
        ghostty_reference="Pinned Ghostty GTK/Linux/platform-specific config runtime behavior",
        roastty_reference="Roastty macOS app and libroastty runtime",
        family="platform",
        status="Oracle complete",
        evidence=(
            "`platform-runtime-classification.md` accounts for every `gtk-*`, "
            "`linux-*`, and `macos-*` canonical option from the regenerated "
            "config inventory. GTK and Linux runtime effects are marked not "
            "applicable to Roastty's macOS runtime; `macos-option-as-alt` points "
            "to existing key translation guards; remaining macOS app effects "
            "stay owned by `RUNTIME-011`."
        ),
        missing_evidence="None for platform-specific runtime classification; macOS app behavior gaps remain tracked by RUNTIME-011.",
        guard_tier="Tier 0",
        guard_command="`PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/platform_runtime_classification.py --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/platform-runtime-classification.md`",
    ),
    RuntimeRow(
        id="RUNTIME-014",
        behavior="accepted runtime divergences cross-link",
        ghostty_reference="Pinned Ghostty runtime helpers and public ABI behavior",
        roastty_reference="`issues/0805-roastty-ghostty-parity/divergences.md`",
        family="divergence",
        status="Intentional divergence",
        evidence=(
            "`DIV-001` records `roastty_translate` identity behavior and "
            "`DIV-002` records unsupported benchmark CLI behavior, with ABI "
            "guards. These are accepted non-parity runtime outcomes."
        ),
        missing_evidence="None for currently accepted runtime divergences.",
        guard_tier="Tier 0",
        guard_command="Inspect `issues/0805-roastty-ghostty-parity/divergences.md` and run the ABI harness listed there.",
    ),
]

EXPECTED_IDS = [
    "RUNTIME-001",
    "RUNTIME-002",
    "RUNTIME-003",
    "RUNTIME-004A",
    "RUNTIME-004B",
    "RUNTIME-004C",
    "RUNTIME-004D",
    "RUNTIME-004E",
    "RUNTIME-004F",
    "RUNTIME-004G",
    "RUNTIME-004H",
    "RUNTIME-005",
    "RUNTIME-006",
    "RUNTIME-007",
    "RUNTIME-008A",
    "RUNTIME-008B",
    "RUNTIME-009A",
    "RUNTIME-009B1",
    "RUNTIME-009B2A",
    "RUNTIME-009B2B1",
    "RUNTIME-009B2B2A",
    "RUNTIME-009B2B2B",
    "RUNTIME-010A",
    "RUNTIME-010B1",
    "RUNTIME-010B2A",
    "RUNTIME-010B2B1",
    "RUNTIME-010B2B2A",
    "RUNTIME-010B2B2B1",
    "RUNTIME-010B2B2B2",
    "RUNTIME-011",
    "RUNTIME-012A",
    "RUNTIME-012B1",
    "RUNTIME-012B2",
    "RUNTIME-013",
    "RUNTIME-014",
]


def validate_rows(rows: list[RuntimeRow]) -> None:
    ids = [row.id for row in rows]
    if ids != EXPECTED_IDS:
        raise ValueError(f"runtime row manifest mismatch: {ids!r}")

    duplicate_ids = [item for item, count in Counter(ids).items() if count > 1]
    if duplicate_ids:
        raise ValueError(f"duplicate runtime row IDs: {duplicate_ids}")

    behaviors = [row.behavior for row in rows]
    duplicate_behaviors = [
        item for item, count in Counter(behaviors).items() if count > 1
    ]
    if duplicate_behaviors:
        raise ValueError(f"duplicate runtime behavior names: {duplicate_behaviors}")

    valid_statuses = {
        "Oracle complete",
        "Audit covered",
        "Gap",
        "Intentional divergence",
        "Not applicable",
    }
    invalid_statuses = sorted({row.status for row in rows} - valid_statuses)
    if invalid_statuses:
        raise ValueError(f"invalid runtime statuses: {invalid_statuses}")

    for row in rows:
        if not row.guard_tier or not row.guard_command:
            raise ValueError(f"missing guard field for {row.id}")
        if not row.ghostty_reference or not row.roastty_reference:
            raise ValueError(f"missing evidence anchor for {row.id}")
        if row.status == "Gap" and not row.guard_command.startswith("TBD"):
            raise ValueError(f"gap row has non-TBD guard: {row.id}")


def emit_inventory(rows: list[RuntimeRow], output: Path) -> None:
    status_counts = Counter(row.status for row in rows)
    family_counts = Counter(row.family for row in rows)

    lines = [
        "# Config Runtime/UI Effects Inventory",
        "",
        "Generated by `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`",
        "for Issue 805 CFG-223 runtime/UI effect experiments.",
        "",
        "## Counts",
        "",
        "| Category | Count |",
        "| --- | ---: |",
        f"| Runtime rows | {len(rows)} |",
        f"| Oracle complete rows | {status_counts.get('Oracle complete', 0)} |",
        f"| Intentional divergence rows | {status_counts.get('Intentional divergence', 0)} |",
        f"| Not applicable rows | {status_counts.get('Not applicable', 0)} |",
        f"| Audit covered rows | {status_counts.get('Audit covered', 0)} |",
        f"| Gap rows | {status_counts.get('Gap', 0)} |",
        "",
        "## Runtime Families",
        "",
        "| Runtime family | Count |",
        "| --- | ---: |",
    ]
    for family, count in sorted(family_counts.items()):
        lines.append(f"| {family} | {count} |")

    lines.extend(["", "## Expected Row Manifest", ""])
    lines.extend(f"- `{row_id}`" for row_id in EXPECTED_IDS)

    lines.extend(
        [
            "",
            "## Rows",
            "",
            "| ID | Behavior | Ghostty reference | Roastty reference | Family | Status | Evidence | Missing evidence | Guard tier | Guard command |",
            "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
        ]
    )
    for row in rows:
        lines.append(
            f"| {row.id} | {row.behavior} | {row.ghostty_reference} | "
            f"{row.roastty_reference} | {row.family} | {row.status} | "
            f"{row.evidence} | {row.missing_evidence} | {row.guard_tier} | "
            f"{row.guard_command} |"
        )
    output.write_text("\n".join(lines) + "\n")


def update_cfg223(
    matrix: Path,
    runtime_inventory_path: Path,
    closed_count: int,
    oracle_count: int,
    incomplete_count: int,
    gap_count: int,
) -> None:
    lines = matrix.read_text().splitlines()
    updated: list[str] = []
    found = False
    for line in lines:
        if line.startswith("| CFG-223 |"):
            found = True
            status = "Pass" if incomplete_count == 0 and gap_count == 0 else "Gap"
            notes = (
                f"Runtime inventory coverage: {oracle_count} rows Oracle complete; "
                f"{closed_count} rows closed; {incomplete_count} rows are "
                f"incomplete and {gap_count} rows are runtime gaps."
            )
            line = (
                "| CFG-223 | Runtime and UI effects | "
                "Ghostty config options that affect app, renderer, input, font, "
                "terminal, and platform behavior produce equivalent runtime effects. | "
                "Roastty runtime/UI effects are inventoried by pinned Ghostty "
                "config-driven runtime domains. | "
                f"{status} | Generated runtime/UI inventory plus matrix consistency "
                "assertion. | "
                f"`{runtime_inventory_path}` | Tier 3 | "
                "`PYTHONDONTWRITEBYTECODE=1 python3 "
                "issues/0805-roastty-ghostty-parity/config_runtime_inventory.py "
                "--output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md "
                "--matrix issues/0805-roastty-ghostty-parity/config-matrix.md` | "
                "Before closing Issue 805 and when config-driven runtime behavior changes. | "
                "CFG-223 only passes when every runtime/UI inventory row is "
                "`Oracle complete`, `Not applicable`, or an accepted documented "
                f"divergence; audit coverage alone is insufficient. | Experiment 106 | {notes} |"
            )
        updated.append(line)

    if not found:
        raise ValueError("CFG-223 row not found in config matrix")

    matrix.write_text("\n".join(updated) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--matrix", type=Path, required=True)
    args = parser.parse_args()

    rows = list(ROWS)
    validate_rows(rows)
    emit_inventory(rows, args.output)

    complete_statuses = {"Oracle complete", "Intentional divergence", "Not applicable"}
    oracle_count = sum(row.status == "Oracle complete" for row in rows)
    closed_count = sum(row.status in complete_statuses for row in rows)
    incomplete_count = sum(row.status not in complete_statuses for row in rows)
    gap_count = sum(row.status == "Gap" for row in rows)
    audit_count = sum(row.status == "Audit covered" for row in rows)
    update_cfg223(
        args.matrix,
        args.output,
        closed_count,
        oracle_count,
        incomplete_count,
        gap_count,
    )

    print(f"runtime_rows={len(rows)}")
    print(f"oracle_complete={oracle_count}")
    print(f"closed={closed_count}")
    print(f"audit_covered={audit_count}")
    print(f"incomplete={incomplete_count}")
    print(f"gap={gap_count}")
    print(f"cfg223={'Pass' if incomplete_count == 0 and gap_count == 0 else 'Gap'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
