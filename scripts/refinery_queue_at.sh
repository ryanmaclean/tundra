#!/usr/bin/env bash
set -euo pipefail

# Serialize at-cli invocations for multi-agent "code refinery" workflows.
# Portable lock implementation (works on macOS without flock).
LOCKDIR="${AT_REFINERY_LOCKDIR:-$HOME/.auto-tundra/refinery.queue.lock}"
SLEEP_SECS="${AT_REFINERY_POLL_SECS:-0.2}"

acquire_lock() {
  while ! mkdir "$LOCKDIR" 2>/dev/null; do
    sleep "$SLEEP_SECS"
  done
}

release_lock() {
  rmdir "$LOCKDIR" 2>/dev/null || true
}

resolve_runner() {
  if [[ -n "${AT_REFINERY_RUNNER:-}" ]]; then
    printf '%s\n' "$AT_REFINERY_RUNNER"
    return
  fi

  if [[ -x "/Users/studio/rust-harness/target/debug/at" ]]; then
    printf '/Users/studio/rust-harness/target/debug/at\n'
    return
  fi

  if [[ -f "/Users/studio/rust-harness/Cargo.toml" ]]; then
    printf 'cargo run -p at-cli --manifest-path /Users/studio/rust-harness/Cargo.toml --\n'
    return
  fi

  echo "Unable to resolve auto-tundra CLI runner." >&2
  exit 1
}

if [[ $# -eq 0 ]]; then
  echo "Usage: $0 <at args...>" >&2
  echo "Example: $0 run -t \"queue-safe task\" --dry-run -j" >&2
  exit 2
fi

acquire_lock
trap release_lock EXIT

RUNNER="$(resolve_runner)"
# shellcheck disable=SC2206
CMD=($RUNNER)
"${CMD[@]}" "$@"
