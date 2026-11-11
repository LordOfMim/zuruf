#!/bin/bash
# Development aid for the container: install the release build into a throw-away prefix and
# report its size, how long a session takes to become usable and how much memory it holds.
# Needs a release build and a display.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/../.."
root=$(mktemp -d /tmp/zuruf-measure-XXXXXX)
export PREFIX="$root/local" XDG_DATA_HOME="$root/share" XDG_STATE_HOME="$root/state" XDG_CONFIG_HOME="$root/config" ZURUF_INSTALL_TEST=1
unset ZURUF_DATA_DIR ZURUF_STATE_DIR ZURUF_CONFIG_DIR ZURUF_SHELL
./install.sh --skip-build >/dev/null
source scripts/install-paths.sh
export ZURUF_PORT=47643 ZURUF_DATA_DIR ZURUF_STATE_DIR ZURUF_CONFIG_DIR
export ZURUF_SHELL_ARGS="${ZURUF_SHELL_ARGS:---no-sandbox --disable-gpu}"
zuruf="$PREFIX/bin/zuruf"
api() {
    local method=$1 path=$2 body=${3:-}
    curl -s -X "$method" "http://127.0.0.1:$ZURUF_PORT/api/v1$path" \
        -H "authorization: Bearer $(cat "$ZURUF_STATE_DIR/token")" \
        -H 'content-type: application/json' ${body:+-d "$body"}
}
# Proportional memory of every process whose command line mentions the pattern, in MiB.
memory() {
    local total=0 pid
    for pid in $(pgrep -f "$1"); do
        total=$((total + $(awk '/^Pss:/ {print $2}' "/proc/$pid/smaps_rollup" 2>/dev/null || echo 0)))
    done
    echo "$((total / 1024)) MiB in $(pgrep -f "$1" | wc -l) processes"
}

echo "installed: zuruf $(du -h "$zuruf" | cut -f1), browser runtime $(du -sh "$lib_dir" | cut -f1)"
(node "$here/static-server.mjs" tests/browser/fixture 8701 >/dev/null 2>&1 &)

started=$(date +%s%N)
"$zuruf" open >/dev/null
echo "daemon and app window: $(( ($(date +%s%N) - started) / 1000000 )) ms until zuruf open returned"
sleep 3
echo "app window alone:      $(memory "$lib_dir/zuruf-shell")"

project=$(api POST /projects "{\"name\":\"Messung\",\"repositoryPath\":\"$root\",\"baseUrl\":\"http://127.0.0.1:8701/index.html\"}" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
session=$(api POST /sessions "{\"projectId\":\"$project\",\"name\":\"Messung\",\"mode\":\"isolated\",\"agentAccess\":false}" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
started=$(date +%s%N)
api POST "/sessions/$session/start" '{}' >/dev/null
until api GET /state | grep -q '"scriptable":true'; do sleep 0.02; done
echo "session window:        $(( ($(date +%s%N) - started) / 1000000 )) ms until the page accepts hotkeys"
sleep 4
echo "session with one page: $(memory "$lib_dir/zuruf-shell")"
echo "daemon:                $(memory "$zuruf daemon")"

"$zuruf" stop >/dev/null
pkill -f '[s]tatic-server.mjs tests/browser/fixture 8701' || true
sleep 2
rm -rf "$root"
