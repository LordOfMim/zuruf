#!/usr/bin/env bash
set -euo pipefail
prefix=${PREFIX:-"$HOME/.local"}
data=${XDG_DATA_HOME:-"$HOME/.local/share"}
config=${XDG_CONFIG_HOME:-"$HOME/.config"}
[[ $prefix = /* && $prefix != / && -d $prefix ]] || exit 2
if [[ ${ZURUF_INSTALL_TEST:-0} != 1 ]]; then
  "$prefix/bin/zuruf" stop >/dev/null 2>&1 || true
  if command -v systemctl >/dev/null; then systemctl --user disable --now zuruf.service >/dev/null 2>&1 || true; fi
fi
rm -f "$prefix/bin/zuruf" "$data/applications/zuruf.desktop" "$data/icons/hicolor/scalable/apps/zuruf.svg" "$config/systemd/user/zuruf.service"
for directory in "$prefix/lib/zuruf" "$data/doc/zuruf"; do
  [[ ! -L $directory ]] || { echo 'Symbolischer Link am Installationspfad; Abbruch' >&2; exit 2; }
  rm -rf -- "$directory"
done
echo 'Zuruf wurde entfernt. Sitzungen, Screenshots und Einstellungen bleiben erhalten.'
