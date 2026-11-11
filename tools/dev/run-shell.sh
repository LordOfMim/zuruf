#!/bin/bash
# Development aid for the container: starts a virtual display, a fixture server and the shell,
# and leaves them running. Extra arguments go to the shell.
#
#   tools/dev/run-shell.sh --zuruf-url=http://127.0.0.1:8700/index.html
#   echo '{"command":"open","url":"about:blank"}' > /tmp/shell-in     # steer it
#   DISPLAY=:99 import -window root shot.png                          # look at it
set -u
here="$(cd "$(dirname "$0")" && pwd)"
target="${CARGO_TARGET_DIR:-$here/../../target}/debug"

pkill -x zuruf-shell
pkill -f '[s]tatic-server.mjs'
sleep 0.5
pgrep -x Xvfb >/dev/null || (Xvfb :99 -screen 0 1600x1000x24 >/tmp/xvfb.log 2>&1 &)
(node "$here/static-server.mjs" "$here/../../tests/browser/fixture" 8700 >/tmp/static.log 2>&1 &)
sleep 1

export DISPLAY=:99
rm -rf /tmp/zuruf-profile /tmp/shell-in
mkfifo /tmp/shell-in
(tail -f /tmp/shell-in | "$target/zuruf-shell" --no-sandbox --disable-gpu \
    --zuruf-profile=/tmp/zuruf-profile --zuruf-port=9333 "$@" >/tmp/shell.log 2>&1 &)
