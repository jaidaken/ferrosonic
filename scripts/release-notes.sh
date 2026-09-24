#!/usr/bin/env bash
# Usage: scripts/release-notes.sh <version> [changelog]. Prints that CHANGELOG section as a release body.
# GitHub renders every newline in a release body as a break, so hard-wrapped entries are joined onto one line.
set -euo pipefail

version="${1:?usage: release-notes.sh <version> [changelog]}"
changelog="${2:-CHANGELOG.md}"

notes="$(awk -v ver="$version" '
  function emit(s) { print s; started = 1; blank = 0 }
  function flush() { if (buf != "") { emit(buf); buf = "" } }
  { sub(/\r$/, "") }
  $0 ~ "^## \\[" ver "\\]" { capture = 1; next }
  capture && !fenced && /^## \[/ { exit }
  !capture { next }

  /^[[:space:]]*(```|~~~)/ { flush(); fenced = !fenced; emit($0); next }
  fenced { emit($0); next }
  /^[[:space:]]*$/ { flush(); if (started && !blank) { print ""; blank = 1 } next }
  /^#/ || /^(---|\*\*\*|___)[[:space:]]*$/ || /^[[:space:]]*\|/ { flush(); emit($0); next }
  /^[[:space:]]*([-*+]|[0-9]+\.)[[:space:]]/ { flush(); buf = $0; next }
  {
    line = $0
    sub(/^[[:space:]]+/, "", line)
    buf = (buf == "") ? line : buf " " line
  }
  END { flush() }
' "$changelog")"

if [ -z "$notes" ]; then
  echo "No release notes for $version found in $changelog" >&2
  exit 1
fi
printf '%s\n' "$notes"
