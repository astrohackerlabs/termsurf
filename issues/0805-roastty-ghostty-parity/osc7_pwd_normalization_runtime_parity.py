#!/usr/bin/env python3
"""Guard the OSC 7 PWD normalization runtime split for Issue 805 CFG-223."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing {label}: {needle!r}")


def require_all(text: str, needles: list[tuple[str, str]]) -> None:
    for needle, label in needles:
        require(text, needle, label)


def require_row(markdown: str, row_id: str) -> str:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == row_id:
            return line
    raise AssertionError(f"missing inventory row {row_id}")


def main() -> int:
    ghostty_stream = read("vendor/ghostty/src/termio/stream_handler.zig")
    roastty_terminal = read("roastty/src/terminal/terminal.rs")
    roastty_termio = read("roastty/src/termio.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_stream,
        [
            ("fn reportPwd", "Ghostty reportPwd handler"),
            (".raw_path = std.mem.startsWith(u8, url, \"kitty-shell-cwd://\")", "Ghostty kitty raw path option"),
            ("OSC 7 scheme must be file or kitty-shell-cwd", "Ghostty scheme gate"),
            ("OSC 7 uri must contain a hostname", "Ghostty host requirement"),
            ("internal_os.hostname.isLocal(host)", "Ghostty local-host validation"),
            ("const path = try uri.path.toRawMaybeAlloc", "Ghostty normalized path extraction"),
            ("self.surfaceMessageWriter(.{ .pwd_change = req });", "Ghostty PWD surface message"),
            ("try self.windowTitle(path);", "Ghostty PWD title fallback"),
            ("self.surfaceMessageWriter(.{ .pwd_change = .{ .stable = \"\" } });", "Ghostty empty PWD clear message"),
        ],
    )

    require_all(
        roastty_terminal,
        [
            ("pending_pwd_updates: Vec<String>", "Roastty pending PWD queue"),
            ("take_pending_pwd_updates", "Roastty drains pending PWD events"),
            ("fn normalize_report_pwd_url(url: &str) -> Option<String>", "Roastty OSC 7 normalizer"),
            ("scheme != \"file\" && scheme != \"kitty-shell-cwd\"", "Roastty scheme gate"),
            ("hostname::is_local(host.as_bytes())", "Roastty local-host validation"),
            ("\"file\" => {", "Roastty file branch"),
            ("percent_decode_path(path)", "Roastty file path decoding"),
            ("\"kitty-shell-cwd\" => {", "Roastty kitty branch"),
            ("Some(path.to_string())", "Roastty kitty raw path"),
            ("terminal_stream_osc7_pwd_normalization_accepts_local_paths", "terminal accept test"),
            ("terminal_stream_osc7_pwd_normalization_rejects_invalid_urls", "terminal reject test"),
        ],
    )
    require_all(
        roastty_termio,
        [
            ("pub(crate) pwd: Vec<String>", "TermioPump PWD field"),
            ("let pwd = self.terminal.take_pending_pwd_updates();", "Termio drains PWD events"),
            ("|| !pump.pwd.is_empty()", "Termio worker emits PWD pumps"),
            (
                "termio_osc7_pwd_normalization_worker_emits_normalized_pwd_pump",
                "Termio normalized PWD test",
            ),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("const ROASTTY_ACTION_PWD: c_int = 35;", "Roastty PWD action tag"),
            ("pub struct RoasttyActionPwd", "Roastty PWD action payload"),
            ("ROASTTY_ACTION_PWD =>", "Roastty PWD union conversion"),
            ("for pwd in &pump.pwd", "surface consumes PWD pump events"),
            ("self.set_pwd(pwd.as_bytes())", "surface dispatches PWD action"),
            (
                "surface_osc7_pwd_normalization_dispatches_pwd_path",
                "surface normalized PWD action test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B2")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2B2 status"),
            ("OSC 7 local PWD URI validation", "OSC 7 behavior"),
            ("hostname checks", "hostname evidence"),
            ("path normalization", "path normalization evidence"),
            ("surface PWD dispatch", "PWD dispatch evidence"),
            ("title fallback path dispatch", "title fallback evidence"),
            ("osc7_pwd_normalization_runtime_parity.py", "static guard evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2B3")
    require_all(
        row_gap,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3B2B2B2B3 status"),
            ("terminal-runtime residual audit", "terminal residual audit"),
        ],
    )

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("80 rows Oracle complete", "CFG-223 oracle count"),
            ("83 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
