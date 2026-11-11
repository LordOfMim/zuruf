#!/bin/bash
# Development aid for the container: run the integrated end-to-end test under a headless
# Wayland compositor (sway). Must not run as root; sway refuses that.
#
#   su dev -c 'bash tools/dev/e2e-wayland.sh'
set -u
here="$(cd "$(dirname "$0")" && pwd)"

pkill -x zuruf-shell
pkill -x sway
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
export ZURUF_SHELL_ARGS="${ZURUF_SHELL_ARGS:---no-sandbox --disable-gpu}"

node "$here/../../tests/e2e/shell.mjs"
status=$?
pkill -x sway
exit $status
