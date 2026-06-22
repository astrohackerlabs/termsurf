#!/usr/bin/env python3
"""Run tiered Roamium PDF regression guards for Issue 834."""

from __future__ import annotations

import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class Check:
    name: str
    short_dir: str
    command: list[str]
    summary_file: str
    accepted_hops: tuple[str, ...] = ("no-failure-observed",)
    accepted_statuses: tuple[str, ...] = ("pass",)
    accepted_limitation: str | None = None
    tiers: tuple[str, ...] = ("focused",)
    use_temp_log_dir: bool = False
    classifier: str = "default"


@dataclass
class CheckResult:
    name: str
    command: list[str]
    returncode: int
    summary_path: str
    first_failing_hop: str | None
    result: str
    duration_seconds: float
    accepted_limitation: str | None = None
    stdout_path: str | None = None
    stderr_path: str | None = None
    summary_status: str | None = None
    missing_summary: bool = False

    def to_json(self) -> dict[str, Any]:
        data: dict[str, Any] = {
            "name": self.name,
            "command": self.command,
            "returncode": self.returncode,
            "summary_path": self.summary_path,
            "first_failing_hop": self.first_failing_hop,
            "result": self.result,
            "duration_seconds": round(self.duration_seconds, 3),
            "stdout_path": self.stdout_path,
            "stderr_path": self.stderr_path,
            "missing_summary": self.missing_summary,
        }
        if self.summary_status is not None:
            data["summary_status"] = self.summary_status
        if self.accepted_limitation:
            data["accepted_limitation"] = self.accepted_limitation
        return data


def py_script(path: str, *args: str) -> list[str]:
    return [sys.executable, str(ROOT / path), *args]


def checks() -> list[Check]:
    return [
        Check(
            name="toolbar-events",
            short_dir="tbe",
            command=py_script(
                "scripts/test-issue-794-pdf-toolbar.py",
                "--serve-bitcoin-pdf",
                "--probe",
                "events",
            ),
            summary_file="pdf-toolbar-summary.json",
            tiers=("smoke", "focused"),
        ),
        Check(
            name="protocol-mouse-click",
            short_dir="pmc",
            command=py_script(
                "scripts/test-issue-794-protocol-mouse.py",
                "--serve-bitcoin-pdf",
                "--action",
                "click",
            ),
            summary_file="protocol-mouse-summary.json",
            tiers=("smoke", "focused"),
        ),
        Check(
            name="save-title-local-contained-print",
            short_dir="stl",
            command=py_script(
                "scripts/test-issue-794-pdf-toolbar.py",
                "--serve-bitcoin-pdf",
                "--probe",
                "save-print-title-local",
                "--enable-pdf-print-intercept",
            ),
            summary_file="pdf-toolbar-summary.json",
        ),
        Check(
            name="protocol-scroll",
            short_dir="psc",
            command=py_script(
                "scripts/test-issue-794-protocol-scroll.py",
                "--serve-bitcoin-pdf",
            ),
            summary_file="protocol-scroll-summary.json",
        ),
        Check(
            name="protocol-resize",
            short_dir="prs",
            command=py_script(
                "scripts/test-issue-794-protocol-resize.py",
                "--serve-bitcoin-pdf",
            ),
            summary_file="protocol-resize-summary.json",
        ),
        Check(
            name="protocol-select-copy",
            short_dir="psc2",
            command=py_script(
                "scripts/test-issue-794-protocol-mouse.py",
                "--serve-bitcoin-pdf",
                "--action",
                "key-select-copy",
            ),
            summary_file="protocol-mouse-summary.json",
        ),
        Check(
            name="pdf-security-guards",
            short_dir="sec",
            command=py_script("scripts/test-issue-796-pdf-security.py"),
            summary_file="issue-796-pdf-security-summary.json",
            use_temp_log_dir=True,
        ),
        Check(
            name="keyboard-page-scroll",
            short_dir="kps",
            command=py_script(
                "scripts/test-issue-834-pdf-navigation.py",
                "--serve-bitcoin-pdf",
                "--probe",
                "keyboard-page-scroll",
            ),
            summary_file="pdf-navigation-summary.json",
        ),
        Check(
            name="toolbar-page-selector",
            short_dir="tps",
            command=py_script(
                "scripts/test-issue-834-pdf-navigation.py",
                "--serve-bitcoin-pdf",
                "--probe",
                "toolbar-page-selector",
            ),
            summary_file="pdf-navigation-summary.json",
        ),
        Check(
            name="internal-link",
            short_dir="iln",
            command=py_script(
                "scripts/test-issue-834-pdf-links.py",
                "--probe",
                "internal-link",
            ),
            summary_file="pdf-links-summary.json",
        ),
        Check(
            name="external-link",
            short_dir="eln",
            command=py_script(
                "scripts/test-issue-834-pdf-links.py",
                "--probe",
                "external-link",
            ),
            summary_file="pdf-links-summary.json",
        ),
        Check(
            name="find-positive",
            short_dir="fnd",
            command=py_script(
                "scripts/test-issue-834-pdf-find.py",
                "--probe",
                "positive-search",
            ),
            summary_file="pdf-find-summary.json",
        ),
        Check(
            name="restrictions-unrestricted-control",
            short_dir="ruc",
            command=py_script(
                "scripts/test-issue-834-pdf-restrictions.py",
                "--probe",
                "unrestricted-control",
            ),
            summary_file="pdf-restrictions-summary.json",
        ),
        Check(
            name="restrictions-restricted-document",
            short_dir="rrd",
            command=py_script(
                "scripts/test-issue-834-pdf-restrictions.py",
                "--probe",
                "restricted-document",
            ),
            summary_file="pdf-restrictions-summary.json",
            accepted_hops=("restricted-download-not-blocked",),
            accepted_limitation=(
                "Chromium PDF permissions block copy for the fixture, but current "
                "Chromium does not expose an original-file download restriction "
                "after load."
            ),
        ),
        Check(
            name="password-unrestricted-control",
            short_dir="puc",
            command=py_script(
                "scripts/test-issue-834-pdf-password.py",
                "--probe",
                "unrestricted-control",
            ),
            summary_file="pdf-password-summary.json",
        ),
        Check(
            name="password-correct-enter",
            short_dir="pce",
            command=py_script(
                "scripts/test-issue-834-pdf-password.py",
                "--probe",
                "password-protected",
                "--credential-flow",
                "correct-only",
                "--submit-mode",
                "enter",
            ),
            summary_file="pdf-password-summary.json",
        ),
        Check(
            name="password-wrong-enter",
            short_dir="pwe",
            command=py_script(
                "scripts/test-issue-834-pdf-password.py",
                "--probe",
                "password-protected",
                "--credential-flow",
                "wrong-only",
                "--submit-mode",
                "enter",
            ),
            summary_file="pdf-password-summary.json",
        ),
        Check(
            name="errors-valid-control",
            short_dir="evc",
            command=py_script(
                "scripts/test-issue-834-pdf-errors.py",
                "--probe",
                "valid-control",
            ),
            summary_file="pdf-error-summary.json",
        ),
        Check(
            name="errors-malformed-fixtures",
            short_dir="emf",
            command=py_script(
                "scripts/test-issue-834-pdf-errors.py",
                "--probe",
                "malformed-fixtures",
            ),
            summary_file="pdf-error-summary.json",
        ),
        Check(
            name="errors-valid-to-malformed-same-tab",
            short_dir="evm",
            command=py_script(
                "scripts/test-issue-834-pdf-errors.py",
                "--probe",
                "valid-to-malformed-same-tab",
            ),
            summary_file="pdf-error-summary.json",
        ),
        Check(
            name="forms-compare",
            short_dir="frm",
            command=py_script(
                "scripts/test-issue-834-pdf-forms.py",
                "--input-path",
                "compare",
            ),
            summary_file="pdf-forms-summary.json",
            tiers=("forms", "focused"),
        ),
        Check(
            name="native-print-cancel",
            short_dir="npc",
            command=py_script(
                "scripts/test-issue-834-pdf-native-print.py",
                "--probe",
                "native-dialog",
                "--allow-native-dialog-click",
            ),
            summary_file="pdf-native-print-summary.json",
            tiers=("native-print",),
            classifier="native-print",
        ),
    ]


UNSAFE_MANUAL_CHECKS = [
    {
        "name": "native-print-production-dialog",
        "result": "skipped-unsafe",
        "reason": (
            "Native print now has an explicit safety-gated native-print tier. "
            "The unsafe-manual tier remains dry/list-only and does not run the "
            "production print control."
        ),
        "replacement_tier": "native-print",
    }
]


def load_summary(path: pathlib.Path) -> tuple[dict[str, Any] | None, str | None]:
    if not path.exists():
        return None, "summary-missing"
    try:
        return json.loads(path.read_text(encoding="utf-8")), None
    except json.JSONDecodeError as exc:
        return None, f"summary-json-invalid: {exc}"


def print_queue_unchanged(summary: dict[str, Any]) -> bool:
    before = summary.get("print_queue_before")
    after = summary.get("print_queue_after")
    if not isinstance(before, dict) or not isinstance(after, dict):
        return False
    if not before or set(before.keys()) != set(after.keys()):
        return False
    for name in before:
        before_result = before.get(name)
        after_result = after.get(name)
        if not isinstance(before_result, dict) or not isinstance(after_result, dict):
            return False
        if before_result.get("returncode") != 0 or after_result.get("returncode") != 0:
            return False
        if before_result.get("timed_out") is True or after_result.get("timed_out") is True:
            return False
    return json.dumps(before, sort_keys=True) == json.dumps(after, sort_keys=True)


def native_trace_has(summary: dict[str, Any], event: str) -> bool:
    print_summary = (summary.get("probe_summary") or {}).get("print") or {}
    native_lines = print_summary.get("printNativeLines") or []
    return any(event in line for line in native_lines)


def classify_native_print(summary: dict[str, Any]) -> tuple[str, str | None, str | None]:
    hop = summary.get("first_failing_hop")
    status = summary.get("status")
    watch = summary.get("print_dialog_watch") or {}
    sheet_cancel = watch.get("sheet_cancel") or {}
    sheet_evidence = watch.get("sheet_evidence") or {}
    required = {
        "cancel_hop": hop == "native-print-dialog-seen-cancelled",
        "safety_gate": summary.get("safety_gate_passed") is True,
        "roamium_alive": summary.get("roamium_exited_before_shutdown") is False,
        "queue_unchanged": print_queue_unchanged(summary),
        "cancel_sent": watch.get("cancel_sent") is True,
        "sheet_evidence": sheet_evidence.get("observed") is True,
        "require_sheet": sheet_cancel.get("requireSheet") is True,
        "canceled_trace": native_trace_has(
            summary, "ts-scripted-print-callback-result-canceled"
        ),
    }
    if all(required.values()):
        return "pass", hop, status
    missing = [name for name, ok in required.items() if not ok]
    return "fail", f"native-print-safety-proof-missing:{','.join(missing)}", status


def classify(
    check: Check,
    returncode: int,
    summary: dict[str, Any] | None,
    missing: bool,
) -> tuple[str, str | None, str | None]:
    if missing or summary is None:
        return "automation-gap", None, None
    if check.classifier == "native-print":
        return classify_native_print(summary)
    hop = summary.get("first_failing_hop")
    status = summary.get("status")
    if check.accepted_limitation and hop in check.accepted_hops:
        return "accepted-limitation", hop, status
    if hop in check.accepted_hops or status in check.accepted_statuses:
        return "pass", hop, status
    return "fail", hop, status


def run_check(log_dir: pathlib.Path, check: Check) -> CheckResult:
    check_dir = log_dir / check.short_dir
    if check_dir.exists():
        shutil.rmtree(check_dir)
    check_dir.mkdir(parents=True, exist_ok=True)
    temp_log = tempfile.TemporaryDirectory(prefix=f"ts834-{check.short_dir}-") if check.use_temp_log_dir else None
    run_dir = pathlib.Path(temp_log.name) if temp_log else check_dir
    cmd = [*check.command, "--log-dir", str(run_dir)]
    stdout_path = check_dir / "regression.stdout"
    stderr_path = check_dir / "regression.stderr"
    try:
        start = time.monotonic()
        proc = subprocess.run(
            cmd,
            cwd=str(ROOT),
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        duration = time.monotonic() - start
        if temp_log:
            shutil.copytree(
                run_dir,
                check_dir,
                dirs_exist_ok=True,
                ignore=shutil.ignore_patterns("gui.sock"),
            )
        stdout_path.write_text(proc.stdout, encoding="utf-8")
        stderr_path.write_text(proc.stderr, encoding="utf-8")
        summary_path = check_dir / check.summary_file
        summary, error = load_summary(summary_path)
    finally:
        if temp_log:
            temp_log.cleanup()
    result, hop, status = classify(check, proc.returncode, summary, error is not None)
    if proc.returncode != 0 and result == "pass":
        result = "fail"
    return CheckResult(
        name=check.name,
        command=cmd,
        returncode=proc.returncode,
        summary_path=str(summary_path),
        first_failing_hop=hop or error,
        result=result,
        duration_seconds=duration,
        accepted_limitation=check.accepted_limitation if result == "accepted-limitation" else None,
        stdout_path=str(stdout_path),
        stderr_path=str(stderr_path),
        summary_status=status,
        missing_summary=error is not None,
    )


def selected_checks(tier: str) -> list[Check]:
    if tier == "unsafe-manual":
        return []
    return [check for check in checks() if tier in check.tiers]


def first_failing_hop(results: list[CheckResult]) -> str:
    for result in results:
        if result.result in ("fail", "automation-gap"):
            return result.first_failing_hop or result.result
    return "no-failure-observed"


def overall_result(results: list[CheckResult], skipped: list[dict[str, Any]]) -> str:
    if any(result.result in ("fail", "automation-gap") for result in results):
        return "fail"
    if results:
        return "pass"
    if skipped:
        return "skipped-unsafe"
    return "automation-gap"


def write_summary(
    log_dir: pathlib.Path,
    tier: str,
    results: list[CheckResult],
    skipped: list[dict[str, Any]],
    duration: float,
) -> dict[str, Any]:
    data = {
        "tier": tier,
        "first_failing_hop": first_failing_hop(results),
        "overall_result": overall_result(results, skipped),
        "checks": [result.to_json() for result in results],
        "skipped_unsafe_checks": skipped,
        "duration_seconds": round(duration, 3),
    }
    (log_dir / "roamium-pdf-regression-summary.json").write_text(
        json.dumps(data, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return data


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--log-dir", required=True)
    parser.add_argument(
        "--tier",
        choices=["smoke", "focused", "forms", "native-print", "unsafe-manual"],
        required=True,
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    log_dir = pathlib.Path(args.log_dir).resolve()
    log_dir.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    skipped = UNSAFE_MANUAL_CHECKS if args.tier == "unsafe-manual" else []
    results = [run_check(log_dir, check) for check in selected_checks(args.tier)]
    summary = write_summary(log_dir, args.tier, results, skipped, time.monotonic() - start)
    return 0 if summary["overall_result"] in ("pass", "skipped-unsafe") else 1


if __name__ == "__main__":
    sys.exit(main())
