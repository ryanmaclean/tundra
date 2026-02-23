#!/usr/bin/env bash
set -euo pipefail

# Source-of-truth mirror: GitHub -> GitLab
# Usage:
#   GITLAB_TOKEN=... ./scripts/sync_github_to_gitlab.sh
# Optional env overrides:
#   GH_URL=https://github.com/ryanmaclean/tundra.git
#   GL_URL=https://gitlab.com/ryanmaclean/tundra.git

GH_URL="${GH_URL:-https://github.com/ryanmaclean/tundra.git}"
GL_URL="${GL_URL:-https://gitlab.com/ryanmaclean/tundra.git}"

if [[ -z "${GITLAB_TOKEN:-}" ]]; then
  echo "ERROR: GITLAB_TOKEN is required" >&2
  exit 2
fi

workdir="$(mktemp -d /tmp/tundra-mirror.XXXXXX)"
cleanup() {
  rm -rf "$workdir"
}
trap cleanup EXIT

mirror_dir="$workdir/mirror.git"
askpass="$workdir/gitlab_askpass.sh"

cat > "$askpass" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "${GITLAB_TOKEN}"
EOF
chmod 700 "$askpass"

echo "Cloning source mirror from GitHub..."
git clone --mirror "$GH_URL" "$mirror_dir" >/dev/null 2>&1

case "$GL_URL" in
  https://*)
    auth_gl_url="https://oauth2@${GL_URL#https://}"
    ;;
  git@gitlab.com:*)
    auth_gl_url="https://oauth2@gitlab.com/${GL_URL#git@gitlab.com:}"
    ;;
  *)
    echo "ERROR: unsupported GL_URL format: $GL_URL" >&2
    echo "Use https://gitlab.com/<namespace>/<repo>.git or git@gitlab.com:<namespace>/<repo>.git" >&2
    exit 2
    ;;
esac

echo "Pushing exact mirror to GitLab..."
(
  cd "$mirror_dir"
  GIT_ASKPASS="$askpass" GIT_TERMINAL_PROMPT=0 git push --mirror "$auth_gl_url" >/dev/null
)

echo "Verifying ref parity..."
git ls-remote --refs "$GH_URL" | sort > "$workdir/gh.refs"
git ls-remote --refs "$GL_URL" | sort > "$workdir/gl.refs"

if diff -u "$workdir/gh.refs" "$workdir/gl.refs" > "$workdir/refs.diff"; then
  refs_count="$(wc -l < "$workdir/gh.refs" | tr -d ' ')"
  echo "OK: GitHub and GitLab refs match (${refs_count} refs)."
else
  echo "ERROR: Mirror mismatch detected." >&2
  head -n 120 "$workdir/refs.diff" >&2
  exit 1
fi
