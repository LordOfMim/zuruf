#!/bin/bash
# Development aid for the container: run the shell under a headless Wayland compositor (sway),
# print how the compositor sees its window and take a screenshot. Extra arguments go to the
# shell. Must not run as root; sway refuses that.
#
#   su dev -c 'bash tools/dev/run-wayland.sh --zuruf-url=http://127.0.0.1:8700/index.html'
set -u
here="$(cd "$(dirname "$0")" && pwd)"
target="${CARGO_TARGET_DIR:-$here/../../target}/debug"

pkill -x zuruf-shell
pkill -x sway
pkill -f '[s]tatic-server.mjs'
sleep 0.5

export XDG_RUNTIME_DIR="/tmp/xdg-runtime-$(id -u)"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
rm -f "$XDG_RUNTIME_DIR"/wayland-* "$XDG_RUNTIME_DIR"/sway-ipc.*
unset DISPLAY
printf 'output HEADLESS-1 resolution 1600x1000\n' >"$XDG_RUNTIME_DIR/sway.conf"
(WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
    sway --config "$XDG_RUNTIME_DIR/sway.conf" >"$XDG_RUNTIME_DIR/sway.log" 2>&1 &)
for _ in $(seq 50); do
    [ -S "$XDG_RUNTIME_DIR/wayland-1" ] && break
    sleep 0.1
done
export WAYLAND_DISPLAY=wayland-1
export SWAYSOCK="$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock | head -1)"

(node "$here/static-server.mjs" "$here/../../tests/browser/fixture" 8700 >"$XDG_RUNTIME_DIR/static.log" 2>&1 &)
profile="$XDG_RUNTIME_DIR/profile"
rm -rf "$profile"
# No platform switch: the shell picks Wayland by itself.
("$target/zuruf-shell" --no-sandbox --disable-gpu --zuruf-detached \
    --zuruf-profile="$profile" --zuruf-port=9333 "$@" >"$XDG_RUNTIME_DIR/shell.log" 2>&1 &)
sleep 8
swaymsg -t get_tree | grep -E '"(app_id|name|shell)"' | sort -u
grim "$here/shot.png" && echo "screenshot: $here/shot.png"
echo "shell log: $XDG_RUNTIME_DIR/shell.log"
