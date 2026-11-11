#!/bin/bash
# Development aid for the container: the session window with the UI's demo data, for judging
# its look.   tools/dev/look.sh <light|dark> <name of the screenshot> [shell switches…]
here="$(cd "$(dirname "$0")" && pwd)"
target="${CARGO_TARGET_DIR:-$here/../../target}/debug"
theme=$1
name=$2
shift 2
export DISPLAY=:99
pkill -x zuruf-shell
bash "$here/display.sh" || exit 1
pgrep -f '[s]tatic-server.mjs' >/dev/null || {
    (node "$here/static-server.mjs" "$here/../../tests/browser/fixture" 8700 >/dev/null 2>&1 &)
    (node "$here/static-server.mjs" "$here/../../apps/ui/dist" 8701 >/dev/null 2>&1 &)
}
sleep 1
rm -rf /tmp/zuruf-profile
("$target/zuruf-shell" --no-sandbox --disable-gpu --zuruf-detached --zuruf-profile=/tmp/zuruf-profile \
    --zuruf-theme="$theme" "--zuruf-panel=http://127.0.0.1:8701/panel.html?demo&theme=$theme" \
    --zuruf-url=http://127.0.0.1:8700/index.html --zuruf-url=http://127.0.0.1:8700/popup.html \
    --zuruf-url=about:blank "$@" >/tmp/shell.log 2>&1 &)
sleep 7
# Optional: click somewhere first, e.g. CLICK="238 190".
if [ -n "${CLICK:-}" ]; then
    xdotool mousemove $CLICK click 1
    sleep 1
fi
import -window root -crop 1500x950+0+0 +repage "$here/$name.png"
pkill -x zuruf-shell
