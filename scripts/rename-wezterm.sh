#!/usr/bin/env bash
# rename-wezterm.sh — Rename all "wezterm" references to "wezboard"
# Re-runnable after upstream WezTerm merges.
# Usage: scripts/rename-wezterm.sh [dir]
set -euo pipefail

WEZ_DIR="${1:-wezboard}"
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

if [ ! -d "$WEZ_DIR" ]; then
  echo "Error: $WEZ_DIR/ not found"
  exit 1
fi

echo "=== rename-wezterm.sh ==="
echo "Renaming wezterm → wezboard in $WEZ_DIR/"
echo ""

# ─────────────────────────────────────────────────────────────────────
# Phase 1: Text substitutions (single sed pass)
# Order: most specific patterns first, generic last.
# No protected patterns — everything gets renamed.
# ─────────────────────────────────────────────────────────────────────
echo "--- Phase 1: Text substitutions ---"

SED_SCRIPT=$(mktemp)
trap 'rm -f "$SED_SCRIPT"' EXIT

cat > "$SED_SCRIPT" << 'SEDEOF'
# ── Author / identity (most specific) ──
s|wez@wezfurlong\.org|wezboard@termsurf.com|g
s|Wez Furlong|Wez Longboard|g

# ── Domain ──
s|wezfurlong\.org|termsurf.com/wezboard|g

# ── GitHub repos ──
# Protect wezterm org repos that aren't the main repo (e.g. xcb-imdkit-rs)
s|github\.com/wezterm/xcb-imdkit|__PROTECT_XCB__|g
s|wez/wezterm|termsurf/termsurf|g
s|wezterm/wezterm|termsurf/termsurf|g

# ── Package registries ──
s|crates\.io/crates/wezterm|crates.io/crates/wezboard|g
s|docs\.rs/wezterm|docs.rs/wezboard|g

# ── Bundle ID ──
s|org\.wezfurlong\.wezterm|com.termsurf.wezboard|g
s|org\.wezfurlong|com.termsurf|g

# ── XDG / config / data paths (Rust join() calls) ──
# .join("wezterm") with no suffix = directory path → needs termsurf/ parent
s|join("wezterm")|join("termsurf/wezboard")|g

# Dotfile paths in docs and strings
s|\.local/share/wezterm|.local/share/termsurf/wezboard|g
s|\.config/wezterm|.config/termsurf/wezboard|g

# ── Environment variables ──
s|WEZTERM_|WEZBOARD_|g
s|WEZTERM|WEZBOARD|g

# ── Obfuscated email (PKGBUILD "wez at wezfurlong dot org") ──
s|wez at wezfurlong dot org|wezboard at termsurf dot com|g

# ── Account names (patreon, ko-fi, copr, twitter) ──
# Must come after wezfurlong.org and org.wezfurlong patterns
s|WezFurlong|Wezboard|g
s|wezfurlong|wezboard|g

# ── Case-sensitive renames (generic, last) ──
s|Wezterm|Wezboard|g
s|WezTerm|Wezboard|g
s|wezterm|wezboard|g

# ── Restore protected patterns ──
s|__PROTECT_XCB__|github.com/wezterm/xcb-imdkit|g
SEDEOF

# Binary extensions to skip
BINARY_RE='\.(png|ico|icns|jpg|jpeg|gif|bmp|webp|pdf|a|o|dylib|so|metallib|wasm|ttf|otf|woff|woff2|gz|tar|zip|tgz|xz|zst|pyc|class|jar|dmp|DS_Store)$'

count=0
while IFS= read -r file; do
  if grep -qil 'wezterm\|wezfurlong\|Wez Furlong\|WEZTERM' "$file" 2>/dev/null; then
    sed -i '' -f "$SED_SCRIPT" "$file"
    count=$((count + 1))
  fi
done < <(git ls-files "$WEZ_DIR" | grep -v -E "$BINARY_RE")

echo "Processed $count files."
echo ""

# ─────────────────────────────────────────────────────────────────────
# Phase 2: File/directory renames (git mv, idempotent)
# ─────────────────────────────────────────────────────────────────────
echo "--- Phase 2: File/directory renames ---"

safe_mv() {
  if [ -e "$1" ]; then
    if git mv "$1" "$2" 2>/dev/null; then
      echo "  $1 → $2"
    else
      echo "  SKIP (untracked): $1"
    fi
  fi
}

# --- Crate directories (19 total) ---
safe_mv "$WEZ_DIR/wezterm"                     "$WEZ_DIR/wezboard"
safe_mv "$WEZ_DIR/wezterm-blob-leases"         "$WEZ_DIR/wezboard-blob-leases"
safe_mv "$WEZ_DIR/wezterm-cell"                "$WEZ_DIR/wezboard-cell"
safe_mv "$WEZ_DIR/wezterm-char-props"          "$WEZ_DIR/wezboard-char-props"
safe_mv "$WEZ_DIR/wezterm-client"              "$WEZ_DIR/wezboard-client"
safe_mv "$WEZ_DIR/wezterm-dynamic"             "$WEZ_DIR/wezboard-dynamic"
safe_mv "$WEZ_DIR/wezterm-escape-parser"       "$WEZ_DIR/wezboard-escape-parser"
safe_mv "$WEZ_DIR/wezterm-font"                "$WEZ_DIR/wezboard-font"
safe_mv "$WEZ_DIR/wezterm-gui"                 "$WEZ_DIR/wezboard-gui"
safe_mv "$WEZ_DIR/wezterm-gui-subcommands"     "$WEZ_DIR/wezboard-gui-subcommands"
safe_mv "$WEZ_DIR/wezterm-input-types"         "$WEZ_DIR/wezboard-input-types"
safe_mv "$WEZ_DIR/wezterm-mux-server"          "$WEZ_DIR/wezboard-mux-server"
safe_mv "$WEZ_DIR/wezterm-mux-server-impl"     "$WEZ_DIR/wezboard-mux-server-impl"
safe_mv "$WEZ_DIR/wezterm-open-url"            "$WEZ_DIR/wezboard-open-url"
safe_mv "$WEZ_DIR/wezterm-ssh"                 "$WEZ_DIR/wezboard-ssh"
safe_mv "$WEZ_DIR/wezterm-surface"             "$WEZ_DIR/wezboard-surface"
safe_mv "$WEZ_DIR/wezterm-toast-notification"  "$WEZ_DIR/wezboard-toast-notification"
safe_mv "$WEZ_DIR/wezterm-uds"                 "$WEZ_DIR/wezboard-uds"
safe_mv "$WEZ_DIR/wezterm-version"             "$WEZ_DIR/wezboard-version"

# --- Assets ---
safe_mv "$WEZ_DIR/assets/flatpak/org.wezfurlong.wezterm.appdata.template.xml" \
        "$WEZ_DIR/assets/flatpak/com.termsurf.wezboard.appdata.template.xml"
safe_mv "$WEZ_DIR/assets/flatpak/org.wezfurlong.wezterm.json" \
        "$WEZ_DIR/assets/flatpak/com.termsurf.wezboard.json"
safe_mv "$WEZ_DIR/assets/flatpak/org.wezfurlong.wezterm.template.json" \
        "$WEZ_DIR/assets/flatpak/com.termsurf.wezboard.template.json"
safe_mv "$WEZ_DIR/assets/macos/WezTerm.app" \
        "$WEZ_DIR/assets/macos/TermSurf Wezboard.app"
safe_mv "$WEZ_DIR/assets/open-wezterm-here" \
        "$WEZ_DIR/assets/open-wezboard-here"
safe_mv "$WEZ_DIR/assets/shell-integration/wezterm.sh" \
        "$WEZ_DIR/assets/shell-integration/wezboard.sh"
safe_mv "$WEZ_DIR/assets/wezterm-nautilus.py" \
        "$WEZ_DIR/assets/wezboard-nautilus.py"
safe_mv "$WEZ_DIR/assets/wezterm.appdata.xml" \
        "$WEZ_DIR/assets/wezboard.appdata.xml"
safe_mv "$WEZ_DIR/assets/wezterm.desktop" \
        "$WEZ_DIR/assets/wezboard.desktop"

# --- Icons ---
for f in wezterm-ghifarit53-1.svg wezterm-ghifarit53-2.svg \
         wezterm-ghifarit53-3.svg wezterm-icon.svg; do
  new=$(echo "$f" | sed 's/wezterm/wezboard/')
  safe_mv "$WEZ_DIR/assets/icon/$f" "$WEZ_DIR/assets/icon/$new"
done

# --- CI ---
safe_mv "$WEZ_DIR/.github/workflows/wezterm_ssh.yml" \
        "$WEZ_DIR/.github/workflows/wezboard_ssh.yml"
safe_mv "$WEZ_DIR/ci/wezterm-homebrew-macos.rb.template" \
        "$WEZ_DIR/ci/wezboard-homebrew-macos.rb.template"
safe_mv "$WEZ_DIR/ci/wezterm-linuxbrew.rb.template" \
        "$WEZ_DIR/ci/wezboard-linuxbrew.rb.template"

# --- Docs: lua module directories ---
safe_mv "$WEZ_DIR/docs/config/lua/wezterm.color" \
        "$WEZ_DIR/docs/config/lua/wezboard.color"
safe_mv "$WEZ_DIR/docs/config/lua/wezterm.gui" \
        "$WEZ_DIR/docs/config/lua/wezboard.gui"
safe_mv "$WEZ_DIR/docs/config/lua/wezterm.mux" \
        "$WEZ_DIR/docs/config/lua/wezboard.mux"
safe_mv "$WEZ_DIR/docs/config/lua/wezterm.plugin" \
        "$WEZ_DIR/docs/config/lua/wezboard.plugin"
safe_mv "$WEZ_DIR/docs/config/lua/wezterm.procinfo" \
        "$WEZ_DIR/docs/config/lua/wezboard.procinfo"
safe_mv "$WEZ_DIR/docs/config/lua/wezterm.serde" \
        "$WEZ_DIR/docs/config/lua/wezboard.serde"
safe_mv "$WEZ_DIR/docs/config/lua/wezterm.time" \
        "$WEZ_DIR/docs/config/lua/wezboard.time"
safe_mv "$WEZ_DIR/docs/config/lua/wezterm.url" \
        "$WEZ_DIR/docs/config/lua/wezboard.url"
safe_mv "$WEZ_DIR/docs/config/lua/wezterm" \
        "$WEZ_DIR/docs/config/lua/wezboard"

# --- Docs: CLI synopsis files ---
for f in "$WEZ_DIR"/docs/examples/cmd-synopsis-wezterm*; do
  [ -e "$f" ] || continue
  new=$(echo "$f" | sed 's/wezterm/wezboard/')
  safe_mv "$f" "$new"
done

# --- Termwiz data ---
safe_mv "$WEZ_DIR/termwiz/data/wezterm" \
        "$WEZ_DIR/termwiz/data/wezboard"
safe_mv "$WEZ_DIR/termwiz/data/w/wezterm" \
        "$WEZ_DIR/termwiz/data/w/wezboard"
safe_mv "$WEZ_DIR/termwiz/data/wezterm.terminfo" \
        "$WEZ_DIR/termwiz/data/wezboard.terminfo"

# --- Test data ---
safe_mv "$WEZ_DIR/test-data/braille-wezterm-logo.txt" \
        "$WEZ_DIR/test-data/braille-wezboard-logo.txt"

echo ""

# ─────────────────────────────────────────────────────────────────────
# Phase 3: Verify
# ─────────────────────────────────────────────────────────────────────
echo "--- Phase 3: Verify ---"

echo ""
echo "Checking for leftover __PROTECT_ placeholders..."
leftover=$(grep -r '__PROTECT_' "$WEZ_DIR" 2>/dev/null | head -5 || true)
if [ -n "$leftover" ]; then
  echo "ERROR: Leftover placeholders found:"
  echo "$leftover"
  exit 1
else
  echo "  None found."
fi

echo ""
echo "Remaining wezterm/wezfurlong references:"
remaining=$(git grep -in 'wezterm\|wezfurlong' "$WEZ_DIR" 2>/dev/null || true)
if [ -z "$remaining" ]; then
  echo "  None — fully renamed."
else
  count=$(echo "$remaining" | wc -l | tr -d ' ')
  echo "  $count references remain. Sampling:"
  echo "$remaining" | head -30
fi

echo ""
echo "=== Done ==="
