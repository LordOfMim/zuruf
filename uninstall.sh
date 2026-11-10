#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
source scripts/install-paths.sh
[[ $# = 0 || ( $# = 1 && $1 = --purge ) ]] || { echo 'Aufruf: ./uninstall.sh [--purge]' >&2; exit 2; }
# An overridden destination must never control the real graphical session.
if [[ $unit_dir = "$HOME/.config/systemd/user" && $PREFIX = "$HOME/.local" && -z ${ZURUF_INSTALL_TEST:-} ]]; then
  systemctl --user disable --now zuruf.service || true
fi
python3 - "$PREFIX" "$unit_dir" "$XDG_DATA_HOME" "$ZURUF_DATA_DIR" <<'PY'
from pathlib import Path
import shutil,sys
prefix,unit,data,app=map(Path,sys.argv[1:])
for p in [prefix/'bin/zuruf',unit/'zuruf.service',data/'applications/zuruf.desktop',data/'applications/chrome-127.0.0.1__-Default.desktop',data/'icons/hicolor/scalable/apps/zuruf.svg']: p.unlink(missing_ok=True)
for name in ['ui','extension']:
    p=app/name
    if p.exists(): shutil.rmtree(p)
PY
if [[ ${1:-} = --purge ]]; then
  echo "Dauerhaft löschen: $ZURUF_DATA_DIR, $ZURUF_STATE_DIR, $ZURUF_CONFIG_DIR"
  read -r -p 'Zum Bestätigen exakt LÖSCHEN eingeben: ' reply
  if [[ $reply = LÖSCHEN ]]; then
    python3 - "$ZURUF_DATA_DIR" "$ZURUF_STATE_DIR" "$ZURUF_CONFIG_DIR" <<'PY'
from pathlib import Path
import shutil,sys
paths=[Path(p) for p in sys.argv[1:]]
if any(not p.is_absolute() or p.name!='zuruf' or p.is_symlink() for p in paths): raise SystemExit('Unsicherer Datenpfad; Abbruch')
for p in paths:
    if p.exists(): shutil.rmtree(p)
PY
  else
    echo 'Hinweis: Daten bleiben erhalten.'
  fi
fi
echo 'ok: Zuruf deinstalliert. Ohne bestätigtes --purge bleiben Benutzerdaten erhalten.'
