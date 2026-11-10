#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
source scripts/install-paths.sh
if [[ $# = 0 ]]; then
  systemd-run --user --wait --collect --pipe --working-directory="$PWD" -p MemoryMax=3G -p MemorySwapMax=1G -- bash -euc '
    pnpm install --frozen-lockfile
    pnpm --filter @zuruf/ui build
    pnpm --filter @zuruf/extension build
    mise exec rust@stable -- cargo build --release -j 2
  '
elif [[ $# != 1 || $1 != --skip-build ]]; then
  echo 'Aufruf: ./install.sh [--skip-build]' >&2; exit 2
fi
for file in target/release/zuruf apps/ui/dist/index.html apps/extension/dist/manifest.json assets/icon/zuruf.svg; do
  [[ -f $file ]] || { echo "fehlt: $file" >&2; exit 1; }
done
for path in "$PREFIX" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$ZURUF_DATA_DIR" "$ZURUF_STATE_DIR" "$ZURUF_CONFIG_DIR"; do
  [[ $path = /* && $path != *$'\n'* && $path != *'%'* && $path != *'"'* && $path != *'\'* ]] || { echo 'Ungültiger Installationspfad' >&2; exit 2; }
done
install -Dm755 target/release/zuruf "$PREFIX/bin/zuruf"
install -d -m700 "$ZURUF_DATA_DIR" "$ZURUF_STATE_DIR" "$ZURUF_CONFIG_DIR"
install -d "$unit_dir" "$XDG_DATA_HOME/applications" "$XDG_DATA_HOME/icons/hicolor/scalable/apps"
python3 - "$ZURUF_DATA_DIR" <<'PY'
import pathlib, shutil, sys
for name, source in [('ui','apps/ui/dist'),('extension','apps/extension/dist')]:
    target=pathlib.Path(sys.argv[1])/name
    if target.exists(): shutil.rmtree(target)
    shutil.copytree(source,target)
PY
cat > "$unit_dir/zuruf.service" <<UNIT
[Unit]
Description=Zuruf lokaler Review-Dienst
After=graphical-session.target
PartOf=graphical-session.target
BindsTo=graphical-session.target
[Service]
Type=simple
ExecStart="$PREFIX/bin/zuruf" daemon
Environment="ZURUF_DATA_DIR=$ZURUF_DATA_DIR"
Environment="ZURUF_STATE_DIR=$ZURUF_STATE_DIR"
Environment="ZURUF_CONFIG_DIR=$ZURUF_CONFIG_DIR"
MemoryMax=3G
MemorySwapMax=1G
Restart=on-failure
RestartSec=3
[Install]
WantedBy=graphical-session.target
UNIT
cat > "$XDG_DATA_HOME/applications/zuruf.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Zuruf
Comment=Lokales Browser-Review und Agentenaufgaben
Exec="$PREFIX/bin/zuruf" open
Icon=zuruf
Terminal=false
Categories=Development;
StartupWMClass=zuruf
DESKTOP
install -m644 assets/icon/zuruf.svg "$XDG_DATA_HOME/icons/hicolor/scalable/apps/zuruf.svg"
echo 'ok: Zuruf wurde im Benutzerbereich installiert.'
echo 'Nächste Schritte: zuruf doctor ausführen.'
echo 'Danach: systemctl --user daemon-reload; systemctl --user enable --now zuruf.service'
echo 'Anwendung öffnen: zuruf open. Die Installation aktiviert keinen Dienst automatisch.'
