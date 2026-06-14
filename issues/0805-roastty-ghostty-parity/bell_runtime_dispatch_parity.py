#!/usr/bin/env python3
"""Check BEL-to-ring-bell runtime dispatch parity for Issue 805."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

GHOSTTY_STREAM_HANDLER = ROOT / "vendor/ghostty/src/termio/stream_handler.zig"
GHOSTTY_SURFACE = ROOT / "vendor/ghostty/src/Surface.zig"
GHOSTTY_CONFIG = ROOT / "vendor/ghostty/src/config/Config.zig"
ROASTTY_TERMINAL = ROOT / "roastty/src/terminal/terminal.rs"
ROASTTY_TERMIO = ROOT / "roastty/src/termio.rs"
ROASTTY_LIB = ROOT / "roastty/src/lib.rs"
ROASTTY_APP = ROOT / "roastty/macos/Sources/Roastty/Roastty.App.swift"
ROASTTY_APP_DELEGATE = ROOT / "roastty/macos/Sources/App/macOS/AppDelegate.swift"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def main() -> int:
    ghostty_stream_handler = read(GHOSTTY_STREAM_HANDLER)
    ghostty_surface = read(GHOSTTY_SURFACE)
    ghostty_config = read(GHOSTTY_CONFIG)
    roastty_terminal = read(ROASTTY_TERMINAL)
    roastty_termio = read(ROASTTY_TERMIO)
    roastty_lib = read(ROASTTY_LIB)
    roastty_app = read(ROASTTY_APP)
    roastty_app_delegate = read(ROASTTY_APP_DELEGATE)

    require(
        "inline fn bell(self: *StreamHandler) void" in ghostty_stream_handler
        and "self.surfaceMessageWriter(.ring_bell);" in ghostty_stream_handler,
        "Pinned Ghostty Termio stream handler no longer sends .ring_bell for BEL",
    )
    require(
        ".ring_bell => bell:" in ghostty_surface
        and "last_bell_time" in ghostty_surface
        and "100 * std.time.ns_per_ms" in ghostty_surface
        and ".ring_bell," in ghostty_surface,
        "Pinned Ghostty Surface ring-bell throttle/action path changed",
    )
    require(
        '@"bell-features": BellFeatures = .{}' in ghostty_config
        and '@"bell-audio-path": ?Path = null' in ghostty_config
        and '@"bell-audio-volume": f64 = 0.5' in ghostty_config,
        "Pinned Ghostty bell config fields changed",
    )

    require(
        "pending_bell_count: usize" in roastty_terminal
        and "pub(crate) fn take_pending_bell_count(&mut self) -> usize" in roastty_terminal
        and "(*self.pending_bell_count).saturating_add(1)" in roastty_terminal,
        "Roastty terminal pending bell counter is missing",
    )
    require(
        "bell_runtime_pending_count_accumulates_without_callback" in roastty_terminal
        and "bell_runtime_pending_count_preserves_callback_effect" in roastty_terminal,
        "Roastty terminal BEL runtime guards are missing",
    )
    require(
        "pub(crate) bell_count: usize" in roastty_termio
        and "let bell_count = self.terminal.take_pending_bell_count();" in roastty_termio
        and "pump.bell_count > 0" in roastty_termio,
        "Roastty Termio bell pump propagation is missing",
    )
    require(
        "termio_bell_pump_reports_child_bel_output" in roastty_termio
        and "termio_bell_worker_emits_bell_only_pump" in roastty_termio,
        "Roastty Termio BEL runtime guards are missing",
    )
    require(
        "const BELL_REPEAT_THROTTLE" in roastty_lib
        and "ROASTTY_ACTION_RING_BELL" in roastty_lib
        and "fn ring_bell(&mut self, now: std::time::Instant)" in roastty_lib
        and re.search(
            r"pump\.bell_count > 0\s*\{\s*self\.ring_bell",
            roastty_lib,
            flags=re.DOTALL,
        )
        is not None,
        "Roastty surface bell pump action dispatch or throttle is missing",
    )
    require(
        "surface_bell_pump_dispatches_ring_bell_action_with_throttle" in roastty_lib
        and "surface_bell_runtime_live_pty_bel_dispatches_ring_bell_action" in roastty_lib,
        "Roastty surface BEL runtime guards are missing",
    )
    require(
        "case ROASTTY_ACTION_RING_BELL" in roastty_app
        and "func ringBell" in roastty_app
        and "bellFeatures.contains(.system)" in roastty_app_delegate
        and "bellFeatures.contains(.audio)" in roastty_app_delegate
        and "bellFeatures.contains(.attention)" in roastty_app_delegate,
        "Roastty macOS ring-bell action handling is missing",
    )

    print("bell_runtime_dispatch_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
