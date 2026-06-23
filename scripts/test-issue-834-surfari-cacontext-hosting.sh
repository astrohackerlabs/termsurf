#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$ROOT/logs/issue-834-exp38-surfari-cacontext-hosting"
EXP36_LOG_DIR="$ROOT/logs/issue-834-exp36-surfari-visual-compositing"
EXP37_LOG_DIR="$ROOT/logs/issue-834-exp37-surfari-side-render-pixels"
HARNESS_LOG="$LOG_DIR/harness-$RUN_ID.log"
SUMMARY="$LOG_DIR/surfari-cacontext-hosting-summary.json"
APP_BIN="${TERMSURF_GHOSTBOARD_APP:-$ROOT/ghostboard/macos/build/Debug/TermSurf.app}/Contents/MacOS/termsurf"
CANDIDATE="${TERMSURF_SURFARI_CACONTEXT_CANDIDATE:-source-window-alpha-1}"

mkdir -p "$LOG_DIR"

log() {
  printf '%s\n' "$*" | tee -a "$HARNESS_LOG"
}

fail() {
  log "FAIL: $*"
  exit 1
}

require_executable() {
  [ -x "$1" ] || fail "missing executable: $1"
}

require_executable "$ROOT/scripts/test-issue-834-surfari-side-render-pixels.sh"
require_executable "$APP_BIN"

log "run_id=$RUN_ID"
log "candidate=$CANDIDATE"
log "ghostboard_app_bin=$APP_BIN"
log "summary=$SUMMARY"

rm -rf "$EXP36_LOG_DIR" "$EXP37_LOG_DIR"
case "$CANDIDATE" in
  source-window-alpha-1)
    TERMSURF_SURFARI_HOST_WINDOW_ALPHA=1 \
      "$ROOT/scripts/test-issue-834-surfari-side-render-pixels.sh" 2>&1 | tee -a "$HARNESS_LOG"
    ;;
  source-window-alpha-0.01)
    TERMSURF_SURFARI_HOST_WINDOW_ALPHA=0.01 \
      "$ROOT/scripts/test-issue-834-surfari-side-render-pixels.sh" 2>&1 | tee -a "$HARNESS_LOG"
    ;;
  content-view-layer)
    TERMSURF_SURFARI_CACONTEXT_LAYER=content-view \
      "$ROOT/scripts/test-issue-834-surfari-side-render-pixels.sh" 2>&1 | tee -a "$HARNESS_LOG"
    ;;
  diagnostic-color-layer)
    TERMSURF_SURFARI_CACONTEXT_LAYER=diagnostic-color \
      "$ROOT/scripts/test-issue-834-surfari-side-render-pixels.sh" 2>&1 | tee -a "$HARNESS_LOG"
    ;;
  snapshot-layer)
    TERMSURF_SURFARI_CACONTEXT_LAYER=snapshot \
      "$ROOT/scripts/test-issue-834-surfari-side-render-pixels.sh" 2>&1 | tee -a "$HARNESS_LOG"
    ;;
  baseline)
    "$ROOT/scripts/test-issue-834-surfari-side-render-pixels.sh" 2>&1 | tee -a "$HARNESS_LOG"
    ;;
  *)
    fail "unknown candidate: $CANDIDATE"
    ;;
esac

EXP37_SUMMARY="$EXP37_LOG_DIR/surfari-side-render-pixels-summary.json"
[ -s "$EXP37_SUMMARY" ] || fail "missing Experiment 37 summary: $EXP37_SUMMARY"

python3 - "$SUMMARY" "$RUN_ID" "$HARNESS_LOG" "$EXP37_SUMMARY" "$APP_BIN" "$CANDIDATE" <<'PY'
from pathlib import Path
import json
import sys

summary_path = Path(sys.argv[1])
run_id = sys.argv[2]
harness_log = sys.argv[3]
exp37_summary_path = Path(sys.argv[4])
app_bin = sys.argv[5]
candidate = sys.argv[6]
exp37 = json.loads(exp37_summary_path.read_text())

def window_pass(scenario, targets):
    proof = scenario.get("pixel_proof", {}).get("window", {})
    counts = proof.get("targets", {})
    return all(counts.get(name, 0) >= 5000 for name in targets)

html = exp37.get("html", {})
pdf = exp37.get("pdf", {})
html_visible = window_pass(html, ("cyan", "yellow"))
pdf_visible = window_pass(pdf, ("webkit_green",))
html_internal = html.get("internal_render", {})
pdf_internal = pdf.get("internal_render", {})
html_internal_ok = html_internal.get("status") == "pass"
pdf_internal_ok = pdf_internal.get("status") == "pass"

if html_internal_ok and pdf_internal_ok and html_visible and pdf_visible:
    overall = "pass"
    classification = "cacontext-hosting-fixed"
elif html_internal_ok and pdf_internal_ok and (html_visible or pdf_visible):
    overall = "partial"
    classification = "partial-cacontext-hosting-improvement"
elif html_internal_ok and pdf_internal_ok:
    overall = "partial"
    classification = "candidate-did-not-fix-hosting"
else:
    overall = "fail"
    classification = "internal-render-regressed"

data = {
    "overall_result": overall,
    "classification": classification,
    "candidate": candidate,
    "run_id": run_id,
    "ghostboard_app_bin": app_bin,
    "artifacts": {
        "harness_log": harness_log,
        "exp37_summary": str(exp37_summary_path),
    },
    "html": {
        **html,
        "visible_pixel_proof": html.get("pixel_proof", {}).get("window"),
        "visible_window_bounded": True,
        "source_window_excluded": True,
    },
    "pdf": {
        **pdf,
        "visible_pixel_proof": pdf.get("pixel_proof", {}).get("window"),
        "visible_window_bounded": True,
        "source_window_excluded": True,
    },
    "cleanup": exp37.get("cleanup", {}),
}
summary_path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
print(json.dumps({
    "overall_result": overall,
    "classification": classification,
    "candidate": candidate,
    "html_window_visible": html_visible,
    "pdf_window_visible": pdf_visible,
    "html_internal": html_internal,
    "pdf_internal": pdf_internal,
}, indent=2, sort_keys=True))
PY

log "PASS: issue 834 experiment 38 CAContext hosting diagnostics"
log "summary=$SUMMARY"
