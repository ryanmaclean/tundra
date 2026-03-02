#!/usr/bin/env bash
set -euo pipefail

# Build gate for wasm SIMD enablement.
#
# What it checks:
# 1) baseline wasm release build succeeds
# 2) simd-enabled wasm release build succeeds
# 3) simd artifact size does not exceed baseline by MAX_SIZE_DELTA_PCT (default 20%)
# 4) optional: if wasm-tools is installed, validate module and detect v128 ops

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UI_DIR="${ROOT_DIR}/app/leptos-ui"
TARGET="wasm32-unknown-unknown"
PROFILE="release"
MAX_SIZE_DELTA_PCT="${MAX_SIZE_DELTA_PCT:-20}"

BASELINE_RUSTFLAGS="-C opt-level=s -C codegen-units=1 -C target-feature=-simd128"
SIMD_RUSTFLAGS="-C opt-level=s -C codegen-units=1 -C target-feature=+simd128"

BASELINE_OUT="${ROOT_DIR}/target/wasm-simd-gate/baseline.wasm"
SIMD_OUT="${ROOT_DIR}/target/wasm-simd-gate/simd.wasm"

mkdir -p "${ROOT_DIR}/target/wasm-simd-gate"

build_variant() {
  local label="$1"
  local rustflags="$2"

  echo "== Building ${label} =="
  (
    cd "${UI_DIR}"
    RUSTFLAGS="${rustflags}" cargo build --target "${TARGET}" --${PROFILE}
  )

  local wasm_path="${ROOT_DIR}/target/${TARGET}/${PROFILE}/at_leptos_ui.wasm"
  if [[ ! -f "${wasm_path}" ]]; then
    echo "error: expected wasm artifact not found: ${wasm_path}" >&2
    exit 1
  fi

  if [[ "${label}" == "baseline" ]]; then
    cp "${wasm_path}" "${BASELINE_OUT}"
  else
    cp "${wasm_path}" "${SIMD_OUT}"
  fi
}

rustup target add "${TARGET}" >/dev/null

build_variant "baseline" "${BASELINE_RUSTFLAGS}"
build_variant "simd" "${SIMD_RUSTFLAGS}"

baseline_size=$(wc -c < "${BASELINE_OUT}" | tr -d ' ')
simd_size=$(wc -c < "${SIMD_OUT}" | tr -d ' ')

if [[ "${baseline_size}" -eq 0 ]]; then
  echo "error: baseline wasm size is zero" >&2
  exit 1
fi

size_delta_pct=$(awk -v b="${baseline_size}" -v s="${simd_size}" 'BEGIN { printf "%.2f", ((s-b)/b)*100 }')

printf "baseline_size=%s bytes\n" "${baseline_size}"
printf "simd_size=%s bytes\n" "${simd_size}"
printf "delta_pct=%s%%\n" "${size_delta_pct}"

if command -v wasm-tools >/dev/null 2>&1; then
  echo "== wasm-tools validation =="
  wasm-tools validate "${BASELINE_OUT}"
  wasm-tools validate "${SIMD_OUT}"

  simd_ops=$(wasm-tools print "${SIMD_OUT}" | rg -c "v128|i(8|16|32|64)x|f(32|64)x" || true)
  printf "simd_ops_detected=%s\n" "${simd_ops}"
fi

if awk -v d="${size_delta_pct}" -v max="${MAX_SIZE_DELTA_PCT}" 'BEGIN { exit !(d > max) }'; then
  echo "error: SIMD wasm size regression ${size_delta_pct}% exceeds threshold ${MAX_SIZE_DELTA_PCT}%" >&2
  exit 1
fi

echo "SIMD gate passed"
