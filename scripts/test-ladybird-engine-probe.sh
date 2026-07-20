#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
LADYBIRD_DIR="$REPO_DIR/forks/ladybird"
PROBE_SRC="$REPO_DIR/rust/ah-ladybirdd/libtermsurf_ladybird/probes/headless-lifecycle"
IN_TREE_PROBE="$LADYBIRD_DIR/Tests/LibWebView/TestLadybirdHeadlessLifecycle.cpp"
PRESET="${TERMSURF_LADYBIRD_PROBE_PRESET:-Debug}"
GUI="${TERMSURF_LADYBIRD_PROBE_GUI:-AppKit}"
JOBS="${TERMSURF_LADYBIRD_PROBE_JOBS:-8}"
TARGET="${TERMSURF_LADYBIRD_PROBE_TARGET:-WebContent}"
BASELINE="${TERMSURF_LADYBIRD_PROBE_BASELINE:-a80d01fc7b290a1a9ce79a32cb3390dc70284cda}"
case "$PRESET" in
  Debug) BUILD_PRESET_DIR="debug" ;;
  All_Debug) BUILD_PRESET_DIR="alldebug" ;;
  Distribution) BUILD_PRESET_DIR="distribution" ;;
  Release) BUILD_PRESET_DIR="release" ;;
  Sanitizer) BUILD_PRESET_DIR="sanitizers" ;;
  *)
    echo "Unknown Ladybird preset: $PRESET" >&2
    exit 1
    ;;
esac
BUILD_DIR="$LADYBIRD_DIR/Build/$BUILD_PRESET_DIR"
PROBE_BUILD_DIR="$REPO_DIR/build/ladybird-headless-lifecycle-probe"
if [ -f "$IN_TREE_PROBE" ]; then
  LOG_DIR="$REPO_DIR/logs/issue-26070112000884/exp5-in-tree-ladybird-headless-probe"
else
  LOG_DIR="$REPO_DIR/logs/issue-26070112000884/exp4-ladybird-headless-probe"
fi
SUMMARY="$LOG_DIR/summary.txt"

mkdir -p "$LOG_DIR"
: > "$SUMMARY"

record() {
  echo "$*" | tee -a "$SUMMARY"
}

record "repo=$REPO_DIR"
record "ladybird_dir=$LADYBIRD_DIR"
record "preset=$PRESET"
record "gui=$GUI"
record "jobs=$JOBS"
record "baseline=$BASELINE"
record "baseline_summary=$(git -C "$LADYBIRD_DIR" log -1 --oneline "$BASELINE")"
record "branch_head=$(git -C "$LADYBIRD_DIR" rev-parse HEAD)"
record "branch_head_summary=$(git -C "$LADYBIRD_DIR" log -1 --oneline)"
record "branch=$(git -C "$LADYBIRD_DIR" branch --show-current)"

if [ -f "$IN_TREE_PROBE" ]; then
  TARGET="TestLadybirdHeadlessLifecycle"
  record "probe_mode=in-tree"
  record "target=$TARGET"

  if [ -n "$(git -C "$LADYBIRD_DIR" status --short)" ]; then
    record "result=tier0"
    record "failure=forks/ladybird is dirty before in-tree probe"
    git -C "$LADYBIRD_DIR" status --short | tee -a "$SUMMARY"
    exit 0
  fi

  record "build_command=forks/ladybird/Meta/ladybird.py build --preset $PRESET --gui $GUI --jobs $JOBS $TARGET"
  if ! (cd "$LADYBIRD_DIR" && ./Meta/ladybird.py build --preset "$PRESET" --gui "$GUI" --jobs "$JOBS" "$TARGET") \
    >"$LOG_DIR/ladybird-build.stdout.log" 2>"$LOG_DIR/ladybird-build.stderr.log"; then
    record "result=tier0"
    record "failure=ladybird in-tree target build failed"
    record "stdout_log=$LOG_DIR/ladybird-build.stdout.log"
    record "stderr_log=$LOG_DIR/ladybird-build.stderr.log"
    tail -60 "$LOG_DIR/ladybird-build.stderr.log" | tee -a "$SUMMARY" || true
    exit 0
  fi

  record "build_dir=$BUILD_DIR"
  record "probe_binary=$BUILD_DIR/bin/$TARGET"
  record "webcontent_helper=$BUILD_DIR/bin/Ladybird.app/Contents/MacOS/WebContent"

  record "ctest_command=ctest -R $TARGET --output-on-failure"
  if ! (cd "$BUILD_DIR" && ctest -R "$TARGET" --output-on-failure) \
    >"$LOG_DIR/ctest.stdout.log" 2>"$LOG_DIR/ctest.stderr.log"; then
    record "result=tier1"
    record "failure=in-tree probe ctest failed after compile/link"
    record "stdout_log=$LOG_DIR/ctest.stdout.log"
    record "stderr_log=$LOG_DIR/ctest.stderr.log"
    tail -80 "$LOG_DIR/ctest.stdout.log" | tee -a "$SUMMARY" || true
    tail -80 "$LOG_DIR/ctest.stderr.log" | tee -a "$SUMMARY" || true
    exit 0
  fi

  record "direct_command=LADYBIRD_SOURCE_DIR=$LADYBIRD_DIR $BUILD_DIR/bin/$TARGET"
  if ! LADYBIRD_SOURCE_DIR="$LADYBIRD_DIR" "$BUILD_DIR/bin/$TARGET" \
    >"$LOG_DIR/direct-run.stdout.log" 2>"$LOG_DIR/direct-run.stderr.log"; then
    record "result=tier1"
    record "failure=in-tree probe direct run failed after ctest"
    record "stdout_log=$LOG_DIR/direct-run.stdout.log"
    record "stderr_log=$LOG_DIR/direct-run.stderr.log"
    tail -80 "$LOG_DIR/direct-run.stdout.log" | tee -a "$SUMMARY" || true
    tail -80 "$LOG_DIR/direct-run.stderr.log" | tee -a "$SUMMARY" || true
    exit 0
  fi

  if grep -q "PASS: Ladybird headless lifecycle probe reached Tier 3" "$LOG_DIR/direct-run.stdout.log"; then
    record "result=tier3"
  elif grep -q "TIER2: constructed HeadlessWebView" "$LOG_DIR/direct-run.stdout.log"; then
    record "result=tier2"
  else
    record "result=tier1"
    record "failure=in-tree probe ran but did not emit tier markers"
  fi
  record "stdout_log=$LOG_DIR/direct-run.stdout.log"
  record "stderr_log=$LOG_DIR/direct-run.stderr.log"
  record "post_status=$(git -C "$LADYBIRD_DIR" status --short | tr '\n' ' ')"
  exit 0
fi

record "probe_mode=out-of-tree"
record "target=$TARGET"

if [ -n "$(git -C "$LADYBIRD_DIR" status --short)" ]; then
  record "result=tier0"
  record "failure=forks/ladybird is dirty before probe"
  git -C "$LADYBIRD_DIR" status --short | tee -a "$SUMMARY"
  exit 0
fi

record "build_command=forks/ladybird/Meta/ladybird.py build --preset $PRESET --gui $GUI --jobs $JOBS $TARGET"
if ! (cd "$LADYBIRD_DIR" && ./Meta/ladybird.py build --preset "$PRESET" --gui "$GUI" --jobs "$JOBS" "$TARGET") \
  >"$LOG_DIR/ladybird-build.stdout.log" 2>"$LOG_DIR/ladybird-build.stderr.log"; then
  record "result=tier0"
  record "failure=ladybird build command failed"
  record "stdout_log=$LOG_DIR/ladybird-build.stdout.log"
  record "stderr_log=$LOG_DIR/ladybird-build.stderr.log"
  tail -40 "$LOG_DIR/ladybird-build.stderr.log" | tee -a "$SUMMARY" || true
  exit 0
fi

record "build_dir=$BUILD_DIR"
record "webcontent_candidates=$(find "$BUILD_DIR" -path '*WebContent*' -type f 2>/dev/null | tr '\n' ' ')"
record "compile_commands=$(find "$BUILD_DIR" -name compile_commands.json -type f 2>/dev/null | tr '\n' ' ')"
record "library_candidates=$(find "$BUILD_DIR" -maxdepth 4 -type f \( -name 'libLibWebView*' -o -name 'LibWebView*' \) 2>/dev/null | tr '\n' ' ')"

rm -rf "$PROBE_BUILD_DIR"
mkdir -p "$PROBE_BUILD_DIR"

record "probe_configure=cmake -S $PROBE_SRC -B $PROBE_BUILD_DIR -DLADYBIRD_SOURCE_DIR=$LADYBIRD_DIR -DLADYBIRD_BUILD_DIR=$BUILD_DIR"
if ! cmake -S "$PROBE_SRC" -B "$PROBE_BUILD_DIR" \
  -DLADYBIRD_SOURCE_DIR="$LADYBIRD_DIR" \
  -DLADYBIRD_BUILD_DIR="$BUILD_DIR" \
  >"$LOG_DIR/probe-configure.stdout.log" 2>"$LOG_DIR/probe-configure.stderr.log"; then
  record "result=tier0"
  record "failure=probe cmake configure failed"
  record "stdout_log=$LOG_DIR/probe-configure.stdout.log"
  record "stderr_log=$LOG_DIR/probe-configure.stderr.log"
  tail -40 "$LOG_DIR/probe-configure.stderr.log" | tee -a "$SUMMARY" || true
  exit 0
fi

record "probe_build=cmake --build $PROBE_BUILD_DIR --target ladybird_headless_lifecycle_probe"
if ! cmake --build "$PROBE_BUILD_DIR" --target ladybird_headless_lifecycle_probe \
  >"$LOG_DIR/probe-build.stdout.log" 2>"$LOG_DIR/probe-build.stderr.log"; then
  record "result=tier0"
  record "failure=probe build/link failed"
  record "stdout_log=$LOG_DIR/probe-build.stdout.log"
  record "stderr_log=$LOG_DIR/probe-build.stderr.log"
  tail -60 "$LOG_DIR/probe-build.stderr.log" | tee -a "$SUMMARY" || true
  exit 0
fi

record "result=tier1"
record "probe_binary=$PROBE_BUILD_DIR/ladybird_headless_lifecycle_probe"
record "runtime_prerequisites=WebContent helper, Core::EventLoop, theme buffer, resource root, fonts/themes"
record "note=Tier 2/3 runtime execution is deferred until required Ladybird runtime prerequisites are fully identified."
record "post_status=$(git -C "$LADYBIRD_DIR" status --short | tr '\n' ' ')"
