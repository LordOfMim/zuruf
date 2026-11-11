#!/usr/bin/env bash
# Installs into a throw-away prefix, checks what landed there, starts the installed app once
# and removes everything again. Needs a release build (./install.sh without --skip-build once).
#
# With a display (DISPLAY or WAYLAND_DISPLAY) the installed binaries are also run; set
# ZURUF_SHELL_ARGS="--no-sandbox --disable-gpu" inside a container.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
root=$(mktemp -d /tmp/zuruf-install-XXXXXX)
# No installed service is enabled; the smoke run starts its own daemon and stops it again.
export PREFIX="$root/local" XDG_DATA_HOME="$root/share" XDG_STATE_HOME="$root/state" XDG_CONFIG_HOME="$root/config" ZURUF_INSTALL_TEST=1
unset ZURUF_DATA_DIR ZURUF_STATE_DIR ZURUF_CONFIG_DIR ZURUF_SHELL
cleanup() { python3 - "$root" <<'PY'
import shutil,sys
shutil.rmtree(sys.argv[1], ignore_errors=True)
PY
}
trap cleanup EXIT
./install.sh --skip-build
./install.sh --skip-build
lib="$PREFIX/lib/zuruf"
[[ -x $PREFIX/bin/zuruf && -x $lib/zuruf-shell && -f $lib/libcef.so && -f $lib/icudtl.dat && -f $lib/locales/en-US.pak && -f $XDG_DATA_HOME/zuruf/ui/index.html ]]
# Only the two interface languages are installed.
[[ -z $(find "$lib/locales" -type f ! -name 'de*' ! -name 'en-US*' -print -quit) ]]
# Verifying a user unit needs a systemd user session, which containers do not have.
if command -v systemd-analyze >/dev/null && [[ -S ${XDG_RUNTIME_DIR:-/nonexistent}/systemd/private ]]; then
  systemd-analyze --user verify "$XDG_CONFIG_HOME/systemd/user/zuruf.service"
fi
if command -v desktop-file-validate >/dev/null; then desktop-file-validate "$XDG_DATA_HOME/applications/zuruf.desktop"; fi
source scripts/install-paths.sh

# The installed binary finds its shell and the shell finds Chromium, without any help.
doctor=$("$PREFIX/bin/zuruf" doctor || true)
grep -q "^ok: Browser-Shell $lib/zuruf-shell" <<<"$doctor" || { echo "$doctor" >&2; exit 1; }
if [[ -n ${DISPLAY:-}${WAYLAND_DISPLAY:-} ]]; then
  export ZURUF_PORT=47642 ZURUF_DATA_DIR ZURUF_STATE_DIR ZURUF_CONFIG_DIR
  "$PREFIX/bin/zuruf" open >/dev/null || { cat "$ZURUF_STATE_DIR/daemon.log" >&2; exit 1; }
  # The daemon answered, so the app window was started from the installed shell.
  # A first start reads the whole Chromium library from disk; give it time.
  for _ in $(seq 200); do
    pgrep -f "$lib/zuruf-shell" >/dev/null && break
    sleep 0.1
  done
  pgrep -f "$lib/zuruf-shell" >/dev/null || { echo 'App-Fenster wurde nicht gestartet' >&2; cat "$ZURUF_STATE_DIR/daemon.log" >&2; exit 1; }
  "$PREFIX/bin/zuruf" stop >/dev/null
  for _ in $(seq 80); do
    pgrep -f "$lib/zuruf-shell" >/dev/null || break
    sleep 0.1
  done
  ! pgrep -f "$lib/zuruf-shell" >/dev/null || { echo 'App-Fenster läuft nach dem Stopp weiter' >&2; exit 1; }
  unset ZURUF_PORT ZURUF_DATA_DIR ZURUF_STATE_DIR ZURUF_CONFIG_DIR
  source scripts/install-paths.sh
  echo 'ok: installierte App gestartet und beendet.'
fi

printf 'preserve\n' > "$XDG_DATA_HOME/zuruf/user-data"
./uninstall.sh
[[ -f $XDG_DATA_HOME/zuruf/user-data && ! -e $PREFIX/bin/zuruf && ! -e $lib && ! -e $XDG_DATA_HOME/zuruf/ui ]]
./install.sh --skip-build
printf 'LÖSCHEN\n' | ./uninstall.sh --purge
[[ ! -e $XDG_DATA_HOME/zuruf && ! -e $XDG_STATE_HOME/zuruf && ! -e $XDG_CONFIG_HOME/zuruf ]]
[[ -z $(find "$root" -type f -print -quit) ]]
echo 'PASS: idempotent install, unit/desktop validation, installed app runs, data preservation, confirmed purge; no session unit enabled.'
