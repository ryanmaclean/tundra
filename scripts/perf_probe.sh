#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   scripts/perf_probe.sh [api_url] [project_root]
#
# Example:
#   scripts/perf_probe.sh http://127.0.0.1:9090 /Users/studio/rust-harness

API_URL="${1:-http://127.0.0.1:9090}"
PROJECT_ROOT="${2:-$(pwd)}"
SAMPLES="${SAMPLES:-5}"
TASK_ID="${TASK_ID:-perf-cache-probe}"
BUDGET="${BUDGET:-4096}"

echo "== auto-tundra perf probe =="
echo "api_url:      ${API_URL}"
echo "project_root: ${PROJECT_ROOT}"
echo "samples:      ${SAMPLES}"
echo

echo "== doctor context cache snapshot =="
DOCTOR_JSON="$(cargo run -q -p at-cli -- doctor -u "${API_URL}" -p "${PROJECT_ROOT}" -j || true)"
if [[ -z "${DOCTOR_JSON}" ]]; then
  echo "doctor returned no output"
else
  if command -v jq >/dev/null 2>&1; then
    echo "${DOCTOR_JSON}" | jq '{
      api_ok: .api.ok,
      project_exists: .project_exists,
      skill_count: .skill_count,
      context_cache: .context_cache,
      failures: .failures
    }'
  else
    echo "${DOCTOR_JSON}"
  fi
fi
echo

echo "== endpoint latency quick sample =="
for ep in /api/status /api/beads /api/worktrees /api/context; do
  if [[ "${ep}" == "/api/context" ]]; then
    url="${API_URL}${ep}?task_id=${TASK_ID}&budget=${BUDGET}"
  else
    url="${API_URL}${ep}"
  fi
  samples_file="$(mktemp)"
  for _ in $(seq 1 "${SAMPLES}"); do
    curl -sS -o /dev/null -w "%{time_total}\n" "${url}" >> "${samples_file}" || true
  done
  avg="$(awk '{sum+=$1; n+=1} END {if (n>0) printf "%.6f", sum/n; else print "nan"}' "${samples_file}")"
  p95="$(sort -n "${samples_file}" | awk '{a[NR]=$1} END {if (NR==0) {print "nan"; exit} idx=int((NR*95+99)/100); if (idx<1) idx=1; if (idx>NR) idx=NR; printf "%.6f", a[idx]}')"
  max="$(awk 'BEGIN{m=0} {if($1>m)m=$1} END {if (NR>0) printf "%.6f", m; else print "nan"}' "${samples_file}")"
  echo "${ep}: avg=${avg}s p95=${p95}s max=${max}s"
  rm -f "${samples_file}"
done
echo

echo "== /api/context repeated calls (same daemon process) =="
for i in $(seq 1 "${SAMPLES}"); do
  tmp_json="$(mktemp)"
  total_s="$(
    curl -sS -o "${tmp_json}" -w "%{time_total}" \
      "${API_URL}/api/context?task_id=${TASK_ID}&budget=${BUDGET}" || echo "ERR"
  )"

  if [[ "${total_s}" == "ERR" ]]; then
    echo "sample ${i}: request failed"
    rm -f "${tmp_json}"
    continue
  fi

  if command -v jq >/dev/null 2>&1; then
    hits="$(jq -r '.cache.hits // 0' "${tmp_json}")"
    misses="$(jq -r '.cache.misses // 0' "${tmp_json}")"
    rebuilds="$(jq -r '.cache.rebuilds // 0' "${tmp_json}")"
    bytes="$(wc -c < "${tmp_json}" | tr -d ' ')"
    echo "sample ${i}: total=${total_s}s bytes=${bytes} hits=${hits} misses=${misses} rebuilds=${rebuilds}"
  else
    bytes="$(wc -c < "${tmp_json}" | tr -d ' ')"
    echo "sample ${i}: total=${total_s}s bytes=${bytes}"
  fi

  rm -f "${tmp_json}"
done
echo

echo "== recommended next checks =="
echo "1) API p95 latency: use wrk/hey on /api/status, /api/beads, /api/context."
echo "2) TUI fan-out timing: run with AT_TUI_PROFILE=1 to print endpoint timings."
echo "3) Flamegraph: cargo flamegraph -p at-daemon --bin at-daemon (hot paths)."
