#!/usr/bin/env bash
set -euo pipefail

# Read-only parity checker for GitHub vs GitLab refs.
# Optional env overrides:
#   GH_URL=https://github.com/ryanmaclean/tundra.git
#   GL_URL=https://gitlab.com/ryanmaclean/tundra.git

GH_URL="${GH_URL:-https://github.com/ryanmaclean/tundra.git}"
GL_URL="${GL_URL:-https://gitlab.com/ryanmaclean/tundra.git}"

workdir="$(mktemp -d /tmp/tundra-parity.XXXXXX)"
cleanup() {
  rm -rf "$workdir"
}
trap cleanup EXIT

git ls-remote --refs "$GH_URL" | sort > "$workdir/gh.refs"
git ls-remote --refs "$GL_URL" | sort > "$workdir/gl.refs"

if diff -u "$workdir/gh.refs" "$workdir/gl.refs" > "$workdir/refs.diff"; then
  refs_count="$(wc -l < "$workdir/gh.refs" | tr -d ' ')"
  echo "OK: refs are in sync (${refs_count} refs)."
else
  echo "MISMATCH: refs differ between GitHub and GitLab." >&2
  head -n 120 "$workdir/refs.diff" >&2
  exit 1
fi

