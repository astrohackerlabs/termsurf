#!/usr/bin/env bash
# Issue 802 / Exp 7 — copy Ghostty's macOS app and mechanically rename ghostty→roastty.
# Reproducible from the pinned vendored upstream. Preserves RoasttyKit.xcframework (Exp 6).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SRC="$ROOT/vendor/ghostty/macos"; DST="$ROOT/roastty/macos"

# 1. copy source only (exclude build artifacts; --delete protects the excluded xcframework)
mkdir -p "$DST"
rsync -a --delete --exclude 'build/' --exclude '*.xcframework' --exclude '.DS_Store' "$SRC"/ "$DST"/

# 2. strip out-of-tree resource inputs (../zig-out/share/* and ../images/*.icon) from pbxproj
python3 - "$DST/Ghostty.xcodeproj/project.pbxproj" <<'PY'
import re,sys
p=sys.argv[1]; lines=open(p).read().split('\n'); bad=set()
for ln in lines:
    if 'PBXFileReference' in ln and ('zig-out/share' in ln or ('/images/' in ln and '.icon' in ln)):
        m=re.match(r'\s*([0-9A-F]{24})',ln);  bad.add(m.group(1)) if m else None
for ln in lines:
    if 'PBXBuildFile' in ln:
        fr=re.search(r'fileRef = ([0-9A-F]{24})',ln)
        if fr and fr.group(1) in bad:
            m=re.match(r'\s*([0-9A-F]{24})',ln); bad.add(m.group(1)) if m else None
out=[ln for ln in lines if not any(u in ln for u in bad)]
open(p,'w').write('\n'.join(out))
print(f"  stripped {len(bad)} resource refs/buildfiles")
PY

# 3. content find/replace in text files (skip the xcframework; -I skips binaries)
grep -rIl --exclude-dir='RoasttyKit.xcframework' -e Ghostty -e ghostty -e GHOSTTY "$DST" \
 | while IFS= read -r f; do
     perl -pi -e 's/Ghostty/Roastty/g; s/ghostty/roastty/g; s/GHOSTTY/ROASTTY/g' "$f"
   done

# 4. rename files/dirs (deepest first so parents rename after children)
find "$DST" -depth -iname '*ghostty*' -not -path '*RoasttyKit.xcframework*' | while IFS= read -r p; do
  d=$(dirname "$p"); b=$(basename "$p")
  nb=$(printf '%s' "$b" | sed 's/Ghostty/Roastty/g; s/ghostty/roastty/g; s/GHOSTTY/ROASTTY/g')
  [ "$b" != "$nb" ] && mv "$p" "$d/$nb"
done
echo "rename complete -> $DST"
