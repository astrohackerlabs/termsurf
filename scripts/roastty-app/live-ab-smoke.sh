#!/usr/bin/env bash
# Issue 802 / Exp 39 — run the first live Ghostty vs Roastty screenshot smoke.
#
# Screenshots are written outside the repo:
#   ${TERMSURF_SHOT_DIR:-$HOME/.cache/termsurf/shots}
#
# Usage:
#   live-ab-smoke.sh [--recipe smoke|ascii-grid|color-grid|clear-after|alt-screen|scroll-output] [--max-mismatch-ratio N] [--max-mean-channel-delta N]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIR="$ROOT/scripts/roastty-app"
GHOST_DIR="$ROOT/scripts/ghostty-app"
SHOT_DIR="${TERMSURF_SHOT_DIR:-$HOME/.cache/termsurf/shots}"
GHOSTTY_APP="${GHOSTTY_APP:-$ROOT/vendor/ghostty/macos/build/Debug/Ghostty.app}"
ROASTTY_APP="${ROASTTY_APP:-$ROOT/roastty/macos/build/Debug/Roastty.app}"
SWIFT="$(command -v swift || echo /usr/bin/swift)"

max_mismatch_ratio="0"
max_mean_channel_delta="0"
recipe="smoke"
list_recipes=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --recipe)
      recipe="${2:?missing value for --recipe}"
      shift 2
      ;;
    --list-recipes)
      list_recipes=1
      shift
      ;;
    --max-mismatch-ratio)
      max_mismatch_ratio="${2:?missing value for --max-mismatch-ratio}"
      shift 2
      ;;
    --max-mean-channel-delta)
      max_mean_channel_delta="${2:?missing value for --max-mean-channel-delta}"
      shift 2
      ;;
    -h|--help)
      echo "usage: $0 [--recipe smoke|ascii-grid|color-grid|clear-after|alt-screen|scroll-output] [--max-mismatch-ratio N] [--max-mean-channel-delta N]" >&2
      echo "       $0 --list-recipes" >&2
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

recipes=(smoke ascii-grid color-grid clear-after alt-screen scroll-output)
if [ "$list_recipes" -eq 1 ]; then
  printf '%s\n' "${recipes[@]}"
  exit 0
fi
case "$recipe" in
  smoke|ascii-grid|color-grid|clear-after|alt-screen|scroll-output) ;;
  *)
    echo "unknown recipe: $recipe" >&2
    echo "supported recipes:" >&2
    printf '  %s\n' "${recipes[@]}" >&2
    exit 2
    ;;
esac

mkdir -p "$SHOT_DIR"
stamp="$(date +%Y%m%d-%H%M%S)"
case "$recipe" in
  smoke) marker="ISSUE802_AB_SMOKE_$stamp" ;;
  *) marker="ISSUE802_AB_${recipe//-/_}_$stamp" ;;
esac
ghost_pid=""
roast_pid=""
cleanup_done=0
bootstrap_root=""

descendant_pids() {
  local root="$1"
  ps -axo pid=,ppid= | awk -v root="$root" '
    { parent[$1] = $2 }
    END {
      for (pid in parent) {
        current = pid
        while (current in parent && parent[current] != 0) {
          if (parent[current] == root) {
            print pid
            break
          }
          current = parent[current]
        }
      }
    }'
}

kill_launched_tree() {
  local label="$1"
  local pid="$2"
  local expected_path="$3"
  local command descendants
  [ -n "$pid" ] || return
  command="$(ps -p "$pid" -o command= 2>/dev/null || true)"
  if [ -z "$command" ]; then
    echo "$label pid $pid already stopped" >&2
    return
  fi
  if [[ "$command" != *"$expected_path"* ]]; then
    echo "refusing to kill $label pid $pid outside expected app path: $command" >&2
    return
  fi
  descendants="$(descendant_pids "$pid")"
  if [ -n "$descendants" ]; then
    echo "killing $label descendant PIDs: $descendants" >&2
    kill -9 $descendants 2>/dev/null || true
  fi
  echo "killing $label PID: $pid" >&2
  kill -9 "$pid" 2>/dev/null || true
}

cleanup() {
  if [ "$cleanup_done" -eq 1 ]; then
    return
  fi
  cleanup_done=1
  kill_launched_tree "Ghostty" "$ghost_pid" "$GHOSTTY_APP/Contents/MacOS/ghostty"
  kill_launched_tree "Roastty" "$roast_pid" "$ROASTTY_APP/Contents/MacOS/roastty"
  if [ -n "$bootstrap_root" ] && [[ "$bootstrap_root" == /tmp/termsurf-ab-bootstrap.* ]]; then
    rm -rf "$bootstrap_root"
  fi
}
trap cleanup EXIT INT TERM

delay() {
  osascript -e "delay ${1:-0.35}" >/dev/null 2>&1
}

activate() {
  local app="$1"
  local process_name frontmost
  process_name="$(basename "$app" .app | tr '[:upper:]' '[:lower:]')"
  osascript >/dev/null <<OSA
tell application "System Events"
  set frontmost of first process whose name is "$process_name" to true
end tell
OSA
  delay 0.8
  frontmost="$(osascript -e 'tell application "System Events" to name of first process whose frontmost is true')"
  if [ "$(printf '%s' "$frontmost" | tr '[:upper:]' '[:lower:]')" != "$(printf '%s' "$process_name" | tr '[:upper:]' '[:lower:]')" ]; then
    echo "failed to activate $process_name; frontmost is $frontmost" >&2
    return 1
  fi
}

dismiss_reopen_dialog() {
  local app="$1"
  local process_name
  process_name="$(basename "$app" .app | tr '[:upper:]' '[:lower:]')"
  osascript >/dev/null <<OSA || true
tell application "System Events"
  if exists process "$process_name" then
    tell process "$process_name"
      if exists window 1 then
        repeat with candidate in buttons of window 1
          set candidate_name to name of candidate as text
          if candidate_name contains "Don" and candidate_name contains "Reopen" then
            click candidate
            exit repeat
          end if
        end repeat
      end if
    end tell
  end if
end tell
OSA
  delay 0.4
}

app_pid() {
  local app="$1"
  local binary
  binary="$(basename "$app" .app | tr '[:upper:]' '[:lower:]')"
  pgrep -f "$app/Contents/MacOS/$binary" | head -1
}

focus_terminal_view() {
  local app="$1"
  local pid line x y w h
  pid="$(app_pid "$app")"
  [ -n "$pid" ] || { echo "no pid found for $app" >&2; return 1; }
  line="$("$SWIFT" "$DIR/list-windows.swift" "$pid" |
    awk '/ name="👻"/ { print; found=1; exit } !found && /layer=0/ { candidate=$0 } END { if (!found && candidate != "") print candidate }')"
  [ -n "$line" ] || { echo "no focusable window bounds found for $app pid $pid" >&2; return 1; }
  read -r x y w h < <(printf '%s\n' "$line" |
    sed -E 's/.*bounds=\(([0-9.-]+),([0-9.-]+) ([0-9.-]+)x([0-9.-]+)\).*/\1 \2 \3 \4/')
  "$SWIFT" "$GHOST_DIR/inject.swift" click "$((x + 120))" "$((y + 140))" left 1
  delay 0.2
}

set_window_bounds() {
  local app="$1"
  local process_name
  process_name="$(basename "$app" .app | tr '[:upper:]' '[:lower:]')"
  osascript >/dev/null <<OSA
tell application "System Events"
  tell process "$process_name"
    set position of front window to {40, 80}
    set size of front window to {800, 632}
  end tell
end tell
OSA
  delay 0.4
}

type_shell_command() {
  local app="$1"
  local command="$2"
  local tmp="/tmp/termsurf-ab-smoke-command-$$.txt"
  printf '%s\n' "$command" >"$tmp"
  activate "$app"
  focus_terminal_view "$app"
  "$SWIFT" "$GHOST_DIR/inject.swift" key 8 control
  delay 0.2
  "$SWIFT" "$GHOST_DIR/inject.swift" type "$tmp"
  rm -f "$tmp"
  delay 1.0
}

write_bootstrap() {
  local dir="$1"
  local command="$2"
  mkdir -p "$dir"
  cat >"$dir/recipe.sh" <<SH
#!/usr/bin/env bash
$command
SH
  chmod +x "$dir/recipe.sh"
  cat >"$dir/.zshrc" <<'ZSHRC'
# Generated by TermSurf Issue 802 live A/B harness.
ZSHRC
  printf 'bash %q\n' "$dir/recipe.sh" >>"$dir/.zshrc"
  mkdir -p "$dir/nushell"
  cat >"$dir/nushell/config.nu" <<NU
# Generated by TermSurf Issue 802 live A/B harness.
bash "$dir/recipe.sh"
NU
}

launch_with_bootstrap() {
  local label="$1"
  local app="$2"
  local bootstrap_dir="$3"
  local binary stdout_log stderr_log pid
  binary="$(basename "$app" .app | tr '[:upper:]' '[:lower:]')"
  stdout_log="$SHOT_DIR/${binary}-ab-stdout-$stamp.log"
  stderr_log="$SHOT_DIR/${binary}-ab-stderr-$stamp.log"
  ZDOTDIR="$bootstrap_dir" XDG_CONFIG_HOME="$bootstrap_dir" SHELL=/bin/zsh "$app/Contents/MacOS/$binary" >"$stdout_log" 2>"$stderr_log" &
  pid="$!"
  for _ in $(seq 1 20); do
    if ps -p "$pid" >/dev/null 2>&1; then
      echo "$pid"
      return 0
    fi
    delay 0.25
  done
  echo "$label launch timed out or exited: $app" >&2
  return 1
}

recipe_command() {
  case "$recipe" in
    smoke)
      printf '%s' "clear; printf '%s\\n' '$marker'; sleep 2"
      ;;
    ascii-grid)
      printf '%s' "printf '%b' '\\033[2J\\033[H$marker\\nABCDEFGHIJKLMNOPQRSTUVWXYZ\\nabcdefghijklmnopqrstuvwxyz\\n0123456789\\n@#$%^&*()_+-=[]{};:,.<>/?\\n'; sleep 2"
      ;;
    color-grid)
      printf '%s' "printf '%b' '\\033[2J\\033[H$marker\\n\\033[30mBLACK\\033[0m \\033[31mRED\\033[0m \\033[32mGREEN\\033[0m \\033[33mYELLOW\\033[0m \\033[34mBLUE\\033[0m \\033[35mMAGENTA\\033[0m \\033[36mCYAN\\033[0m \\033[37mWHITE\\033[0m\\n\\033[40m bg-black \\033[0m \\033[41m bg-red \\033[0m \\033[42m bg-green \\033[0m \\033[43m bg-yellow \\033[0m \\033[44m bg-blue \\033[0m \\033[45m bg-magenta \\033[0m \\033[46m bg-cyan \\033[0m \\033[47m bg-white \\033[0m\\n\\033[1;30mBRIGHT-BLACK\\033[0m \\033[1;31mBRIGHT-RED\\033[0m \\033[1;32mBRIGHT-GREEN\\033[0m \\033[1;33mBRIGHT-YELLOW\\033[0m\\n\\033[1;34mBRIGHT-BLUE\\033[0m \\033[1;35mBRIGHT-MAGENTA\\033[0m \\033[1;36mBRIGHT-CYAN\\033[0m \\033[1;37mBRIGHT-WHITE\\033[0m\\n\\033[38;2;255;128;0mTRUECOLOR-FG-ORANGE\\033[0m \\033[48;2;30;90;180mTRUECOLOR-BG-BLUE\\033[0m \\033[38;2;120;255;160;48;2;60;20;80mTRUECOLOR-FG-BG\\033[0m\\n'; sleep 2"
      ;;
    clear-after)
      printf '%s' "printf '%b' 'PRE_CLEAR_ONE\\nPRE_CLEAR_TWO\\nPRE_CLEAR_THREE\\n\\033[3J\\033[H\\033[2J$marker\\nAFTER_CLEAR_ROW_1\\nAFTER_CLEAR_ROW_2\\n'; sleep 2"
      ;;
    alt-screen)
      printf '%s' "printf '%b' '\\033[?1049h\\033[2J\\033[5;10H$marker\\033[10;3HALT_ROW_10_COL_3\\033[15;20HALT_ROW_15_COL_20\\033[0m'; sleep 2"
      ;;
    scroll-output)
      printf '%s' "printf '%b' '\\033[2J\\033[H$marker\\n'; for i in {001..080}; do printf 'SCROLL_ROW_%s\\n' \"\$i\"; done; sleep 2"
      ;;
  esac
}

image_dim() {
  local png="$1"
  local key="$2"
  sips -g "$key" "$png" 2>/dev/null | awk -v k="$key" '$1 == k || $1 == k ":" { print $2 }'
}

roastty_window_bounds() {
  local pid="$1"
  "$SWIFT" "$DIR/list-windows.swift" "$pid" |
    awk '/ name="👻"/ { print; found=1; exit } !found && /layer=0/ { candidate=$0 } END { if (!found && candidate != "") print candidate }'
}

crop_roastty_window() {
  local pid="$1"
  local full_png="$2"
  local out_png="$3"
  local target_w="${4:-}"
  local target_h="${5:-}"
  local line x y w h scale
  line="$(roastty_window_bounds "$pid")"
  [ -n "$line" ] || { echo "no Roastty window bounds found for pid $pid" >&2; return 1; }

  read -r x y w h < <(printf '%s\n' "$line" |
    sed -E 's/.*bounds=\(([0-9.-]+),([0-9.-]+) ([0-9.-]+)x([0-9.-]+)\).*/\1 \2 \3 \4/')
  for value in "$x" "$y" "$w" "$h"; do
    [[ "$value" =~ ^-?[0-9]+([.][0-9]+)?$ ]] || {
      echo "could not parse Roastty bounds: $line" >&2
      return 1
    }
  done

  scale="${TERMSURF_SCREEN_SCALE:-2}"
  read -r px py pw ph < <(awk \
    -v x="$x" -v y="$y" -v w="$w" -v h="$h" -v s="$scale" \
    -v tw="$target_w" -v th="$target_h" \
    'BEGIN {
      maxw = int(w*s); maxh = int(h*s);
      cw = tw == "" ? maxw : int(tw);
      ch = th == "" ? maxh : int(th);
      if (cw > maxw) cw = maxw;
      if (ch > maxh) ch = maxh;
      printf "%d %d %d %d\n", x*s, y*s, cw, ch
    }')
  "$SWIFT" "$DIR/crop.swift" "$full_png" "$out_png" "$px" "$py" "$pw" "$ph" >&2
}

command="$(recipe_command)"
bootstrap_root="$(mktemp -d /tmp/termsurf-ab-bootstrap.XXXXXX)"
write_bootstrap "$bootstrap_root/ghostty" "$command"
write_bootstrap "$bootstrap_root/roastty" "$command"

echo "starting Ghostty and Roastty" >&2
ghost_pid="$(launch_with_bootstrap "Ghostty" "$GHOSTTY_APP" "$bootstrap_root/ghostty")"
roast_pid="$(launch_with_bootstrap "Roastty" "$ROASTTY_APP" "$bootstrap_root/roastty")"
echo "Ghostty pid=$ghost_pid Roastty pid=$roast_pid recipe=$recipe marker=$marker" >&2

dismiss_reopen_dialog "$GHOSTTY_APP"
dismiss_reopen_dialog "$ROASTTY_APP"

activate "$GHOSTTY_APP"
set_window_bounds "$GHOSTTY_APP"
activate "$ROASTTY_APP"
set_window_bounds "$ROASTTY_APP"

activate "$GHOSTTY_APP"
ghost_full="$SHOT_DIR/ghostty-ab-full-$stamp.png"
ghost_png="$SHOT_DIR/ghostty-ab-crop-$stamp.png"
screencapture -x "$ghost_full"
crop_roastty_window "$ghost_pid" "$ghost_full" "$ghost_png"
ghost_w="$(image_dim "$ghost_png" pixelWidth)"
ghost_h="$(image_dim "$ghost_png" pixelHeight)"
[ -n "$ghost_w" ] && [ -n "$ghost_h" ] || {
  echo "could not read Ghostty capture dimensions: $ghost_png" >&2
  exit 1
}

activate "$ROASTTY_APP"
roast_full="$SHOT_DIR/roastty-ab-full-$stamp.png"
roast_crop="$SHOT_DIR/roastty-ab-crop-$stamp.png"
screencapture -x "$roast_full"
crop_roastty_window "$roast_pid" "$roast_full" "$roast_crop" "$ghost_w" "$ghost_h"

for png in "$ghost_png" "$roast_crop"; do
  [ -s "$png" ] || { echo "missing or empty capture: $png" >&2; exit 1; }
done

diff_args=(
  "$ghost_png"
  "$roast_crop"
  --max-mismatch-ratio "$max_mismatch_ratio"
  --max-mean-channel-delta "$max_mean_channel_delta"
)
diff_status=0
diff_json="$("$SWIFT" "$DIR/pngdiff.swift" "${diff_args[@]}")" || diff_status=$?

roast_w="$(image_dim "$roast_crop" pixelWidth)"
roast_h="$(image_dim "$roast_crop" pixelHeight)"
harness_verdict="FAIL"
[ "$diff_status" -eq 0 ] && harness_verdict="PASS"

python3 - "$harness_verdict" "$diff_status" "$ghost_pid" "$roast_pid" "$recipe" "$marker" \
  "$ghost_png" "$roast_full" "$roast_crop" "$ghost_w" "$ghost_h" "$roast_w" "$roast_h" \
  "$max_mismatch_ratio" "$max_mean_channel_delta" "$diff_json" <<'PY'
import json
import sys

(
    verdict,
    diff_status,
    ghost_pid,
    roast_pid,
    recipe,
    marker,
    ghost_png,
    roast_full,
    roast_crop,
    ghost_w,
    ghost_h,
    roast_w,
    roast_h,
    max_mismatch_ratio,
    max_mean_channel_delta,
    diff_json,
) = sys.argv[1:]

try:
    diff = json.loads(diff_json)
except json.JSONDecodeError:
    diff = {"error": "invalid_diff_json", "raw": diff_json}

summary = {
    "verdict": verdict,
    "recipe": recipe,
    "marker": marker,
    "ghostty_pid": int(ghost_pid),
    "roastty_pid": int(roast_pid),
    "ghostty_png": ghost_png,
    "roastty_full_png": roast_full,
    "roastty_crop_png": roast_crop,
    "ghostty_size": {"width": int(ghost_w or 0), "height": int(ghost_h or 0)},
    "roastty_crop_size": {"width": int(roast_w or 0), "height": int(roast_h or 0)},
    "max_mismatch_ratio": float(max_mismatch_ratio),
    "max_mean_channel_delta": float(max_mean_channel_delta),
    "diff_exit_status": int(diff_status),
    "diff": diff,
}
print(json.dumps(summary, sort_keys=True, separators=(",", ":")))
PY

exit "$diff_status"
