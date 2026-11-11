#!/bin/bash
# Development aid for the container: start the real daemon with throw-away data, open the app
# and leave everything running. Prints the API base and token for further calls.
#
#   tools/dev/try-app.sh            start
#   tools/dev/try-app.sh stop       stop the daemon and its windows
here="$(cd "$(dirname "$0")" && pwd)"
target="${CARGO_TARGET_DIR:-$here/../../target}/debug"
root=/tmp/zuruf-try
export DISPLAY=:99
export ZURUF_SHELL="$target/zuruf-shell"
export ZURUF_SHELL_ARGS="--no-sandbox --disable-gpu"
export ZURUF_PORT=47700
export ZURUF_DATA_DIR="$root/data" ZURUF_STATE_DIR="$root/state" ZURUF_CONFIG_DIR="$root/config"

"$target/zuruf" stop >/dev/null 2>&1
sleep 1
pkill -x zuruf-shell
pkill -f '[s]tatic-server.mjs'
[ "${1:-}" = stop ] && exit 0

rm -rf "$root"
mkdir -p "$root"
pgrep -x Xvfb >/dev/null || (Xvfb :99 -screen 0 1600x1000x24 >/tmp/xvfb.log 2>&1 &)
(node "$here/static-server.mjs" "$here/../../tests/browser/fixture" 8700 >/tmp/static.log 2>&1 &)
("$target/zuruf" daemon >"$root/daemon.log" 2>&1 &)
for _ in $(seq 50); do
    curl -s -o /dev/null "http://127.0.0.1:$ZURUF_PORT/" && break
    sleep 0.2
done
"$target/zuruf" open
echo "base=http://127.0.0.1:$ZURUF_PORT/api/v1"
echo "token=$(cat "$root/state/token")"
