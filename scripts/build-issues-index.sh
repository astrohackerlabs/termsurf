#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
ISSUES_DIR="$REPO_DIR/issues"
OUTPUT="$ISSUES_DIR/README.md"

open_rows=""
closed_rows=""

for dir in "$ISSUES_DIR"/*/; do
  readme="$dir/README.md"
  [ -f "$readme" ] || continue

  basename="$(basename "$dir")"
  num="${basename%%-*}"

  # Parse frontmatter — extract values between +++ delimiters
  frontmatter=$(awk '/^[+][+][+]$/{n++; next} n==1{print} n==2{exit}' "$readme")
  status=$(echo "$frontmatter" | grep '^status' | sed 's/.*"\(.*\)"/\1/' || true)
  opened=$(echo "$frontmatter" | grep '^opened' | sed 's/.*"\(.*\)"/\1/' || true)
  closed=$(echo "$frontmatter" | grep '^closed' | sed 's/.*"\(.*\)"/\1/' || true)

  # Extract H1 title (first "# " line after frontmatter)
  title=$(awk '/^[+][+][+]$/{n++; next} n>=2 && /^# /{sub(/^# /,""); print; exit}' "$readme")
  if [ -z "$title" ]; then
    title="$basename"
  fi

  # Strip issue number prefix from title (e.g. "Issue 756: Surfari" → "Surfari")
  title=$(echo "$title" | sed 's/^Issue [0-9]*: //')

  link="[$title](${basename}/README.md)"

  # Strip leading zeros from issue number for display
  display_num=$(echo "$num" | sed 's/^0*//')

  if [ "$status" = "open" ]; then
    open_rows="${open_rows}| ${display_num} | ${link} | ${opened} |\n"
  else
    closed_rows="${closed_rows}${num}\t| ${display_num} | ${link} | ${opened} | ${closed} |\n"
  fi
done

# Sort closed rows by issue number descending, then strip the sort key
sorted_closed=$(echo -e "$closed_rows" | sort -r -t$'\t' -k1 | cut -f2-)

# Write output
{
  echo "# Issues"
  echo ""
  echo "## Open"
  echo ""
  echo "| # | Issue | Opened |"
  echo "| - | ----- | ------ |"
  if [ -n "$open_rows" ]; then
    echo -e "$open_rows" | sed '/^$/d'
  fi
  echo ""
  echo "## Closed"
  echo ""
  echo "| # | Issue | Opened | Closed |"
  echo "| - | ----- | ------ | ------ |"
  if [ -n "$sorted_closed" ]; then
    echo "$sorted_closed" | sed '/^$/d'
  fi
} > "$OUTPUT"

prettier --write --prose-wrap always --print-width 80 "$OUTPUT" > /dev/null 2>&1

open_count=$(echo -e "$open_rows" | sed '/^$/d' | wc -l | tr -d ' ')
closed_count=$(echo "$sorted_closed" | sed '/^$/d' | wc -l | tr -d ' ')
echo "  issues/README.md: ${open_count} open, ${closed_count} closed"
