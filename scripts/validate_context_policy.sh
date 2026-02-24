#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "[policy] $*" >&2
  exit 1
}

[[ -f AGENTS.md ]] || fail "missing AGENTS.md at repo root"
[[ -f todo.md ]] || fail "missing todo.md at repo root"

skills=()
if [[ -d .claude/skills ]]; then
  while IFS= read -r path; do
    skills+=("$path")
  done < <(find .claude/skills -type f -name SKILL.md | sort)
fi

for skill in "${skills[@]}"; do
  first_line="$(head -n 1 "$skill" || true)"
  if [[ "$first_line" != "---" ]]; then
    fail "missing YAML frontmatter start in $skill"
  fi

  frontmatter="$(awk 'NR==1 && $0=="---" {in_fm=1; next} in_fm && $0=="---" {exit} in_fm {print}' "$skill")"

  grep -Eq '^name:[[:space:]]*.+$' <<<"$frontmatter" || fail "frontmatter missing 'name' in $skill"
  grep -Eq '^description:[[:space:]]*.+$' <<<"$frontmatter" || fail "frontmatter missing 'description' in $skill"

  # allow inline list or yaml block key
  if ! grep -Eq '^allowed_tools:[[:space:]]*(\[.*\])?$' <<<"$frontmatter"; then
    fail "frontmatter missing 'allowed_tools' key in $skill"
  fi
done

echo "[policy] context policy checks passed"
