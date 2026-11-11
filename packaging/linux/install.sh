#!/usr/bin/env bash
# Install a prebuilt release for this user; never builds, downloads or enables autostart.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
prefix=${PREFIX:-"$HOME/.local"}
data=${XDG_DATA_HOME:-"$HOME/.local/share"}
config=${XDG_CONFIG_HOME:-"$HOME/.config"}
for destination in "$prefix" "$data" "$config"; do
  [[ $destination = /* && $destination != *$'\n'* && $destination != *'"'* && $destination != *'%'* && $destination != *'\'* ]] || { echo 'Ungültiger Installationspfad' >&2; exit 2; }
done
if [[ ${ZURUF_INSTALL_TEST:-0} != 1 && -x $prefix/bin/zuruf ]]; then
  "$prefix/bin/zuruf" stop >/dev/null 2>&1 || true
fi
install -Dm755 bin/zuruf "$prefix/bin/zuruf"
install -d "$prefix/lib/zuruf" "$data/applications" "$data/icons/hicolor/scalable/apps" "$data/doc/zuruf"
cp -a lib/zuruf/. "$prefix/lib/zuruf/"
cp -a share/doc/zuruf/. "$data/doc/zuruf/"
install -m644 share/icons/zuruf.svg "$data/icons/hicolor/scalable/apps/zuruf.svg"
cat > "$data/applications/zuruf.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Zuruf
Comment=Webseiten prüfen und Feedback an Coding-Agenten übergeben
Exec="$prefix/bin/zuruf" open
Icon=zuruf
Terminal=false
Categories=Development;
StartupWMClass=zuruf
DESKTOP
if command -v systemctl >/dev/null; then
  install -d "$config/systemd/user"
  sed "s|@BINARY@|$prefix/bin/zuruf|g" share/zuruf.service > "$config/systemd/user/zuruf.service"
fi
if command -v update-desktop-database >/dev/null; then update-desktop-database "$data/applications" >/dev/null 2>&1 || true; fi
echo "Zuruf ist installiert. Öffnen: $prefix/bin/zuruf open (oder über den App-Starter)."
echo 'Die Installation aktiviert keinen Autostart. Persönliche Daten bleiben bei Updates erhalten.'
