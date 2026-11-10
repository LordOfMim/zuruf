#!/usr/bin/env bash
set -uo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
mkdir -p .spike/acceptance
failed=0
run() {
  local name=$1; shift
  systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- "$@" > ".spike/acceptance/$name.log" 2>&1
  local status=$?
  if (( status == 0 )); then echo "PASS $name"; else echo "FAIL $name ($status): .spike/acceptance/$name.log"; failed=$((failed+1)); fi
}
run workspace mise exec rust@stable -- cargo test --workspace -j 2
run mcp-sdk node tests/mcp/client.mjs
run headed-browser target/debug/examples/lab --check
run captures env ZURUF_E2E_BINARY="$PWD/target/release/zuruf" T3_URL=http://127.0.0.1:3773 node tests/e2e/wiring.mjs --capture-only
run app node tests/e2e/app-smoke.mjs
run install bash tests/install-roundtrip.sh
printf 'Summary: %d failed; real agent MCP requires user approval (manual); follow-up is a separate explicit scenario.\n' "$failed"
(( failed == 0 ))
