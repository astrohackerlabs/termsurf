#!/usr/bin/env bash
#
# bounded-run.sh <logfile> <cmd...>
#
# Run <cmd...>, streaming all output to <logfile>, under TWO independent kills:
#
#   * HARD_CAP (900s = 15 minutes): absolute wall-clock ceiling. The command is
#     SIGKILLed and the wrapper exits no matter what — this is the guarantee that
#     no run can ever take longer than 15 minutes (Issue 26053112000801).
#   * IDLE_CAP (90s): if the log stops growing (no test makes progress), the
#     command is killed early as a likely hang. A `sample` of the process group is
#     captured to <logfile>.hang before the kill.
#
# The wrapper ALWAYS writes a final `STATUS=...` line and exits within HARD_CAP,
# including the degenerate cases where the command dies instantly or produces no
# output (the poll loop ends immediately, `wait` returns, STATUS=COMPLETED). There
# is no code path in which this script runs forever.
#
# Usage (the ONLY supported pattern): launch this as a single tracked background
# task and wait for that task's own completion notification. Do NOT wrap it in a
# `&`-launcher, and do NOT poll a separate watcher for a success marker — both
# create unbounded waits this script exists to eliminate. On wake, read the
# STATUS line: HARD_TIMEOUT / IDLE_KILL is a failure (with a captured sample);
# an empty log or an instant COMPLETED is also a failure to investigate, never a
# silent pass.

set -u

HARD_CAP="${BOUNDED_HARD_CAP:-900}" # 15 minutes, absolute
IDLE_CAP="${BOUNDED_IDLE_CAP:-90}"  # no-progress kill

if [ "$#" -lt 2 ]; then
  echo "usage: bounded-run.sh <logfile> <cmd...>" >&2
  exit 2
fi

log="$1"
shift
mkdir -p "$(dirname "$log")" # so a missing log dir can never silently drop the run
: >"$log"

# Central US (the maintainer's) timezone, stamped at start/end so elapsed time is
# always legible in the maintainer's clock from the log.
central() { TZ='America/Chicago' date '+%Y-%m-%d %H:%M:%S %Z'; }
echo "START=$(central)" >>"$log"
echo "CMD=$*" >>"$log"

"$@" >>"$log" 2>&1 &
pid=$!
start=$(date +%s)

kill_group() {
  /usr/bin/sample "$pid" 3 -file "$log.hang" >/dev/null 2>&1 || true
  pkill -9 -P "$pid" 2>/dev/null || true
  kill -9 "$pid" 2>/dev/null || true
}

while kill -0 "$pid" 2>/dev/null; do
  now=$(date +%s)
  mtime=$(stat -f %m "$log" 2>/dev/null || echo "$now")
  elapsed=$((now - start))
  idle=$((now - mtime))

  if [ "$elapsed" -ge "$HARD_CAP" ]; then
    kill_group
    echo "STATUS=HARD_TIMEOUT elapsed=${elapsed}s END=$(central) (15-min ceiling; sample -> $log.hang)" >>"$log"
    exit 0
  fi
  if [ "$idle" -ge "$IDLE_CAP" ]; then
    kill_group
    echo "STATUS=IDLE_KILL idle=${idle}s elapsed=${elapsed}s END=$(central) (no progress; sample -> $log.hang)" >>"$log"
    exit 0
  fi
  sleep 5
done

wait "$pid"
rc=$?
echo "STATUS=COMPLETED rc=${rc} elapsed=$(($(date +%s) - start))s END=$(central)" >>"$log"
exit 0
