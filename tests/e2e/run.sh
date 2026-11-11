#!/usr/bin/env bash
# Runs the independent acceptance scenarios, each inside a memory limit where systemd allows,
# and reports PASS/FAIL with the log path. Build first:
#   pnpm --filter @zuruf/ui build && pnpm --filter @zuruf/page build && cargo build && cargo build --release
set -uo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
mkdir -p .spike/acceptance
target=${CARGO_TARGET_DIR:-$PWD/target}
failed=0
limited() {
  if command -v systemd-run >/dev/null && [[ -S ${XDG_RUNTIME_DIR:-/nonexistent}/systemd/private ]]; then
    systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=4G -p MemorySwapMax=1G \
      --setenv=ZURUF_SHELL="$target/debug/zuruf-shell" --setenv=CARGO_TARGET_DIR="$target" -- "$@"
  else
    ZURUF_SHELL="$target/debug/zuruf-shell" CARGO_TARGET_DIR="$target" "$@"
  fi
}
run() {
  local name=$1; shift
  limited "$@" > ".spike/acceptance/$name.log" 2>&1
  local status=$?
  if (( status == 0 )); then
    echo "PASS $name"
  else
    # Keep the evidence: the next run overwrites the plain log.
    cp ".spike/acceptance/$name.log" ".spike/acceptance/$name.failed-$(date +%H%M%S).log"
    echo "FAIL $name ($status): .spike/acceptance/$name.log"
    failed=$((failed+1))
  fi
}
cargo=(cargo)
if command -v mise >/dev/null; then cargo=(mise exec rust@stable -- cargo); fi
run workspace "${cargo[@]}" test --workspace -j 4
run mcp-sdk node tests/mcp/client.mjs
run browser-session "${cargo[@]}" test -p zuruf-browser --test shell -j 4 -- --ignored
run integrated-app node tests/e2e/shell.mjs
if [[ -n ${HYPRLAND_INSTANCE_SIGNATURE:-} ]]; then run app-window node tests/e2e/app-smoke.mjs; fi
run install bash tests/install-roundtrip.sh
if [[ -n ${T3_URL:-} ]]; then run captures env ZURUF_E2E_BINARY="$target/release/zuruf" node tests/e2e/wiring.mjs --capture-only; fi
if [[ ${ZURUF_E2E_FIRSTRUN:-0} == 1 ]]; then run firstrun node tests/e2e/firstrun.mjs; fi
printf 'Summary: %d failed; real agent MCP requires user approval (manual); follow-up is a separate explicit scenario.\n' "$failed"
(( failed == 0 ))
