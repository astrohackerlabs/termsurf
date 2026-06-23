#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs/issue-834-exp49-surfari-pdf-selection-bounds"
EXP44_LOG_DIR="$ROOT/logs/issue-834-exp44-surfari-pdf-selection-copy"
SUMMARY="$LOG_DIR/surfari-pdf-selection-bounds-summary.json"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
ORIGINAL_CLIPBOARD="$LOG_DIR/original-clipboard-$RUN_ID.txt"
ORIGINAL_RESTORE_STATUS="not-attempted"

mkdir -p "$LOG_DIR"

log() {
  printf '%s\n' "$*" | tee -a "$HARNESS_LOG"
}

restore_original_clipboard() {
  if [ -e "$ORIGINAL_CLIPBOARD" ]; then
    pbcopy <"$ORIGINAL_CLIPBOARD" || return 1
    ORIGINAL_RESTORE_STATUS="restored"
  fi
}

cleanup() {
  restore_original_clipboard || ORIGINAL_RESTORE_STATUS="restore-failed"
}
trap cleanup EXIT

run_cell() {
  local mode="$1"
  local name="$2"
  local start_x="$3"
  local y="$4"
  local end_x="$5"
  local delay_after_drag="$6"
  local direct="$7"
  local cell_id="${mode}-${name}"
  local out_summary="$LOG_DIR/${cell_id}-summary-$RUN_ID.json"
  local out_trace="$LOG_DIR/${cell_id}-copy-trace-$RUN_ID.log"

  rm -rf "$EXP44_LOG_DIR"
  log "cell=$cell_id direct=$direct drag=${start_x},${y}-${end_x},${y} delay=$delay_after_drag"

  if [ "$direct" = "yes" ]; then
    if TERMSURF_ISSUE834_PDF_FIXTURE_MODE=separated-tokens \
      TERMSURF_ISSUE834_PDF_EXPECTED_TOKENS="LEFT834 MID834 RIGHT834" \
      TERMSURF_ISSUE834_PDF_ACCEPTED_SUBSTRING="RIGHT834" \
      TERMSURF_ISSUE834_PDF_COPY_DRAG_START_X_RATIO="$start_x" \
      TERMSURF_ISSUE834_PDF_COPY_DRAG_END_X_RATIO="$end_x" \
      TERMSURF_ISSUE834_PDF_COPY_DRAG_Y_RATIO="$y" \
      TERMSURF_ISSUE834_PDF_COPY_DELAY_AFTER_DRAG="$delay_after_drag" \
      TERMSURF_SURFARI_PDF_COPY_TRACE=1 \
      TERMSURF_SURFARI_PDF_COPY_TRACE_FILE="$out_trace" \
      TERMSURF_SURFARI_PDF_COPY_DIRECT=1 \
      env -u TERMSURF_SURFARI_CACONTEXT_LAYER \
      "$ROOT/scripts/test-issue-834-surfari-pdf-selection-copy.sh" >>"$HARNESS_LOG" 2>&1; then
      :
    fi
  else
    if TERMSURF_ISSUE834_PDF_FIXTURE_MODE=separated-tokens \
      TERMSURF_ISSUE834_PDF_EXPECTED_TOKENS="LEFT834 MID834 RIGHT834" \
      TERMSURF_ISSUE834_PDF_ACCEPTED_SUBSTRING="RIGHT834" \
      TERMSURF_ISSUE834_PDF_COPY_DRAG_START_X_RATIO="$start_x" \
      TERMSURF_ISSUE834_PDF_COPY_DRAG_END_X_RATIO="$end_x" \
      TERMSURF_ISSUE834_PDF_COPY_DRAG_Y_RATIO="$y" \
      TERMSURF_ISSUE834_PDF_COPY_DELAY_AFTER_DRAG="$delay_after_drag" \
      TERMSURF_SURFARI_PDF_COPY_TRACE=1 \
      TERMSURF_SURFARI_PDF_COPY_TRACE_FILE="$out_trace" \
      env -u TERMSURF_SURFARI_CACONTEXT_LAYER \
      "$ROOT/scripts/test-issue-834-surfari-pdf-selection-copy.sh" >>"$HARNESS_LOG" 2>&1; then
      :
    fi
  fi

  if [ -f "$EXP44_LOG_DIR/surfari-pdf-selection-copy-summary.json" ]; then
    python3 - "$EXP44_LOG_DIR/surfari-pdf-selection-copy-summary.json" "$out_summary" "$mode" "$name" "$direct" "$start_x" "$y" "$end_x" "$delay_after_drag" "$out_trace" <<'PY'
import json
import sys
from pathlib import Path

source, target, mode, name, direct, start_x, y, end_x, delay, trace = sys.argv[1:11]
data = json.loads(Path(source).read_text())
data["exp49_cell"] = {
    "mode": mode,
    "name": name,
    "direct_copy": direct == "yes",
    "drag_ratios": {
        "start_x": float(start_x),
        "end_x": float(end_x),
        "y": float(y),
    },
    "copy_delay_after_drag_seconds": float(delay),
    "copy_trace": trace,
}
Path(target).write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
PY
  else
    python3 - "$out_summary" "$mode" "$name" "$direct" "$start_x" "$y" "$end_x" "$delay_after_drag" "$out_trace" <<'PY'
import json
import sys
from pathlib import Path

target, mode, name, direct, start_x, y, end_x, delay, trace = sys.argv[1:10]
Path(target).write_text(json.dumps({
    "overall_result": "missing",
    "classification": "missing-summary",
    "exp49_cell": {
        "mode": mode,
        "name": name,
        "direct_copy": direct == "yes",
        "drag_ratios": {
            "start_x": float(start_x),
            "end_x": float(end_x),
            "y": float(y),
        },
        "copy_delay_after_drag_seconds": float(delay),
        "copy_trace": trace,
    },
}, indent=2, sort_keys=True) + "\n")
PY
  fi
}

write_summary() {
  python3 - "$SUMMARY" "$RUN_ID" "$ORIGINAL_RESTORE_STATUS" "$LOG_DIR" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
run_id = sys.argv[2]
restore_status = sys.argv[3]
log_dir = Path(sys.argv[4])
expected = ["LEFT834", "MID834", "RIGHT834"]
cells = []
for path in sorted(log_dir.glob(f"*-summary-{run_id}.json")):
    if path.name == summary_path.name:
        continue
    data = json.loads(path.read_text())
    clipboard = data.get("clipboard", {})
    sample = " ".join([
        clipboard.get("after_copy_sample", ""),
        clipboard.get("fallback_select_all_after_sample", ""),
    ])
    present = [token for token in expected if token in sample]
    fixture = data.get("fixture", {})
    cells.append({
        "path": str(path),
        "cell": data.get("exp49_cell", {}),
        "overall_result": data.get("overall_result"),
        "classification": data.get("classification"),
        "fixture_text_extraction_status": fixture.get("text_extraction_status"),
        "clipboard_sample": sample[:160],
        "tokens_present": present,
        "contains_all_tokens": all(token in sample for token in expected),
    })

missing = [cell for cell in cells if cell["overall_result"] == "missing"]
text_oracle_ok = bool(cells) and all(cell["fixture_text_extraction_status"] == "pass" for cell in cells)
standalone_oracle_status = "not-run"
baseline_all = [cell for cell in cells if not cell["cell"].get("direct_copy") and cell["contains_all_tokens"]]
direct_all = [cell for cell in cells if cell["cell"].get("direct_copy") and cell["contains_all_tokens"]]
right_edge_gaps = [
    cell for cell in cells
    if "LEFT834" in cell["tokens_present"]
    and "MID834" in cell["tokens_present"]
    and "RIGHT834" not in cell["tokens_present"]
]

if missing or restore_status != "restored":
    result = "fail"
    classification = "missing-summary-or-restore-failed"
elif not text_oracle_ok or standalone_oracle_status != "pass":
    result = "partial"
    classification = "harness-insufficient"
elif baseline_all:
    result = "pass"
    classification = "geometry-fix-candidate"
elif direct_all:
    result = "pass"
    classification = "direct-copy-geometry-candidate"
elif right_edge_gaps:
    result = "pass"
    classification = "right-edge-selection-gap"
else:
    result = "pass"
    classification = "copy-routing-gap"

summary = {
    "overall_result": result,
    "classification": classification,
    "run_id": run_id,
    "clipboard_restore_status": restore_status,
    "fixture_oracle": {
        "text_extraction": "pass" if text_oracle_ok else "fail",
        "standalone_pdfkit_wkwebview_copy": standalone_oracle_status,
    },
    "expected_tokens": expected,
    "cell_count": len(cells),
    "cells": cells,
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(json.dumps({
    "overall_result": result,
    "classification": classification,
    "cell_count": len(cells),
}, indent=2, sort_keys=True))
if result == "fail":
    sys.exit(1)
PY
}

pbpaste >"$ORIGINAL_CLIPBOARD" || true
log "run_id=$RUN_ID"

run_cell baseline left-only 0.58 0.43 0.72 0.25 no
run_cell baseline left-through-mid 0.58 0.43 0.86 0.25 no
run_cell baseline all-ltr 0.58 0.43 0.99 0.25 no
run_cell baseline all-rtl 0.99 0.43 0.58 0.25 no
run_cell baseline all-y-high 0.58 0.40 0.99 0.25 no
run_cell baseline all-y-low 0.58 0.46 0.99 0.25 no
run_cell baseline overwide-delay-025 0.52 0.43 0.99 0.25 no
run_cell baseline overwide-delay-100 0.52 0.43 0.99 1 no
run_cell baseline overwide-delay-200 0.52 0.43 0.99 2 no

run_cell direct left-only 0.58 0.43 0.72 0.25 yes
run_cell direct left-through-mid 0.58 0.43 0.86 0.25 yes
run_cell direct all-ltr 0.58 0.43 0.99 0.25 yes
run_cell direct all-rtl 0.99 0.43 0.58 0.25 yes
run_cell direct all-y-high 0.58 0.40 0.99 0.25 yes
run_cell direct all-y-low 0.58 0.46 0.99 0.25 yes
run_cell direct overwide-delay-025 0.52 0.43 0.99 0.25 yes
run_cell direct overwide-delay-100 0.52 0.43 0.99 1 yes
run_cell direct overwide-delay-200 0.52 0.43 0.99 2 yes

restore_original_clipboard || ORIGINAL_RESTORE_STATUS="restore-failed"
write_summary
log "summary=$SUMMARY"
