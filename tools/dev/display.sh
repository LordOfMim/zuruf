#!/bin/bash
# Development aid for the container: make sure the virtual display :99 is really up.
# A killed Xvfb leaves its lock file behind, and the next one then refuses to start.
if DISPLAY=:99 xdpyinfo >/dev/null 2>&1; then
    exit 0
fi
pkill -x Xvfb
sleep 0.3
rm -f /tmp/.X99-lock /tmp/.X11-unix/X99
(Xvfb :99 -screen 0 1600x1000x24 >/tmp/xvfb.log 2>&1 &)
for _ in $(seq 50); do
    DISPLAY=:99 xdpyinfo >/dev/null 2>&1 && exit 0
    sleep 0.1
done
echo "Xvfb did not start; see /tmp/xvfb.log" >&2
exit 1
