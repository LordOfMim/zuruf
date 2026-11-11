#!/usr/bin/env bash
# Installs Zuruf for the current user: the `zuruf` binary, its browser shell with the Chromium
# runtime next to it, a systemd user unit and a desktop entry. Nothing is enabled or started.
#
#   ./install.sh               build, then install
#   ./install.sh --skip-build  install what is already built
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
source scripts/install-paths.sh

# The Chromium runtime (CEF) is downloaded once per version and kept here between builds.
export CEF_PATH=${CEF_PATH:-"${XDG_CACHE_HOME:-$HOME/.cache}/zuruf/cef"}
target=${CARGO_TARGET_DIR:-target}

build='
  pnpm install --frozen-lockfile
  pnpm --filter @zuruf/ui build
  pnpm --filter @zuruf/page build
  if command -v mise >/dev/null; then
    mise exec rust@stable -- cargo build --release -j 4
  else
    cargo build --release -j 4
  fi
'
if [[ $# = 0 ]]; then
  for tool in pnpm cmake ninja; do
    command -v "$tool" >/dev/null || { echo "fehlt: $tool (wird für den Build benötigt)" >&2; exit 1; }
  done
  if command -v systemd-run >/dev/null && [[ -S ${XDG_RUNTIME_DIR:-/nonexistent}/systemd/private ]]; then
    # The machine is short on memory; keep the build inside a limit.
    systemd-run --user --wait --collect --pipe --working-directory="$PWD" \
      --setenv=CEF_PATH="$CEF_PATH" -p MemoryMax=4G -p MemorySwapMax=1G -- bash -euc "$build"
  else
    bash -euc "$build"
  fi
elif [[ $# != 1 || $1 != --skip-build ]]; then
  echo 'Aufruf: ./install.sh [--skip-build]' >&2; exit 2
fi

cef_dir=$(find "$CEF_PATH" -maxdepth 2 -type d -name 'cef_linux_*' 2>/dev/null | sort -V | tail -n 1)
for file in "$target/release/zuruf" "$target/release/zuruf-shell" apps/ui/dist/index.html assets/icon/zuruf.svg "${cef_dir:-$CEF_PATH}/libcef.so"; do
  [[ -f $file ]] || { echo "fehlt: $file" >&2; exit 1; }
done
for path in "$PREFIX" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$ZURUF_DATA_DIR" "$ZURUF_STATE_DIR" "$ZURUF_CONFIG_DIR"; do
  [[ $path = /* && $path != *$'\n'* && $path != *'%'* && $path != *'"'* && $path != *'\'* ]] || { echo 'Ungültiger Installationspfad' >&2; exit 2; }
done

install -Dm755 "$target/release/zuruf" "$PREFIX/bin/zuruf"
install -d -m700 "$ZURUF_DATA_DIR" "$ZURUF_STATE_DIR" "$ZURUF_CONFIG_DIR"
install -d "$unit_dir" "$XDG_DATA_HOME/applications" "$XDG_DATA_HOME/icons/hicolor/scalable/apps"

# The shell and Chromium's runtime live together; the shell finds its library beside itself.
python3 - "$lib_dir" "$cef_dir" "$target/release/zuruf-shell" "$ZURUF_DATA_DIR" <<'PY'
import pathlib, shutil, sys
lib, cef, shell, data = map(pathlib.Path, sys.argv[1:])
if lib.name != 'zuruf' or lib.is_symlink():
    raise SystemExit('Unsicherer Bibliothekspfad; Abbruch')
if lib.exists():
    shutil.rmtree(lib)
lib.mkdir(parents=True)
shutil.copy2(shell, lib / 'zuruf-shell')
# Everything Chromium needs at run time; the build inputs of the distribution stay behind.
skip = {'CMakeLists.txt', 'archive.json', 'chrome-sandbox', 'cmake', 'include', 'libcef_dll', 'locales'}
for entry in cef.iterdir():
    if entry.name not in skip and entry.is_file():
        shutil.copy2(entry, lib / entry.name)
(lib / 'locales').mkdir()
for pak in (cef / 'locales').iterdir():
    # The interface is German with English as Chromium's fallback.
    if pak.name.startswith(('de', 'en-US')):
        shutil.copy2(pak, lib / 'locales' / pak.name)
target = data / 'ui'
if target.exists():
    shutil.rmtree(target)
shutil.copytree('apps/ui/dist', target)
PY
chmod 755 "$lib_dir/zuruf-shell"
if command -v strip >/dev/null; then
  # The distributed library carries over a gigabyte of symbols.
  strip --strip-unneeded "$lib_dir/libcef.so"
fi

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
# The shell's windows carry the app id "zuruf", which is how the desktop finds this entry.
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
# Left over from installations that used system Chromium.
rm -f "$XDG_DATA_HOME/applications/chrome-127.0.0.1__-Default.desktop"
install -m644 assets/icon/zuruf.svg "$XDG_DATA_HOME/icons/hicolor/scalable/apps/zuruf.svg"
echo 'ok: Zuruf wurde im Benutzerbereich installiert.'
echo 'Nächste Schritte: zuruf doctor ausführen.'
echo 'Danach: systemctl --user daemon-reload; systemctl --user enable --now zuruf.service'
echo 'Anwendung öffnen: zuruf open. Die Installation aktiviert keinen Dienst automatisch.'
