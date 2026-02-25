#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UI_DIR="${ROOT_DIR}/app/leptos-ui"
TARGET="wasm32-unknown-unknown"
PROFILE="${PROFILE:-release}"

RUSTFLAGS_SIMD="-C opt-level=s -C codegen-units=1 -C target-feature=+simd128"

echo "Building at-leptos-ui (${TARGET}, ${PROFILE}) with SIMD"
(
  cd "${UI_DIR}"
  rustup target add "${TARGET}" >/dev/null
  RUSTFLAGS="${RUSTFLAGS_SIMD}" cargo build --target "${TARGET}" --${PROFILE}
)

WASM_PATH="${UI_DIR}/target/${TARGET}/${PROFILE}/at_leptos_ui.wasm"
if [[ -f "${WASM_PATH}" ]]; then
  echo "Output: ${WASM_PATH}"
  wc -c "${WASM_PATH}"
else
  echo "error: wasm artifact not found" >&2
  exit 1
fi
