#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
root=$(mktemp -d /tmp/zuruf-install-XXXXXX)
# No installed service is enabled; a short isolated app smoke uses transient units.
export PREFIX="$root/local" XDG_DATA_HOME="$root/share" XDG_STATE_HOME="$root/state" XDG_CONFIG_HOME="$root/config" ZURUF_INSTALL_TEST=1
unset ZURUF_DATA_DIR ZURUF_STATE_DIR ZURUF_CONFIG_DIR
cleanup() { python3 - "$root" <<'PY'
import shutil,sys
shutil.rmtree(sys.argv[1])
PY
}
trap cleanup EXIT
./install.sh --skip-build
./install.sh --skip-build
[[ -x $PREFIX/bin/zuruf && -f $XDG_DATA_HOME/zuruf/ui/index.html && -f $XDG_DATA_HOME/zuruf/extension/manifest.json ]]
systemd-analyze --user verify "$XDG_CONFIG_HOME/systemd/user/zuruf.service"
if command -v desktop-file-validate >/dev/null; then desktop-file-validate "$XDG_DATA_HOME/applications/zuruf.desktop"; fi
source scripts/install-paths.sh
ZURUF_SMOKE_CONFIG="$ZURUF_CONFIG_DIR" ZURUF_SMOKE_BIN="$PREFIX/bin/zuruf" ZURUF_SMOKE_DATA="$ZURUF_DATA_DIR" ZURUF_SMOKE_STATE="$ZURUF_STATE_DIR" /usr/bin/node tests/e2e/app-smoke.mjs
"$PREFIX/bin/zuruf" doctor || [[ $? = 1 ]]
printf 'preserve\n' > "$XDG_DATA_HOME/zuruf/user-data"
./uninstall.sh
[[ -f $XDG_DATA_HOME/zuruf/user-data && ! -e $PREFIX/bin/zuruf && ! -e $XDG_DATA_HOME/zuruf/ui ]]
./install.sh --skip-build
printf 'LÖSCHEN\n' | ./uninstall.sh --purge
[[ ! -e $XDG_DATA_HOME/zuruf && ! -e $XDG_STATE_HOME/zuruf && ! -e $XDG_CONFIG_HOME/zuruf ]]
[[ -z $(find "$root" -type f -print -quit) ]]
echo 'PASS: idempotent install, unit/desktop validation, data preservation, confirmed purge; no session unit enabled.'
