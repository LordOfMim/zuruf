# Shared user-scope destinations.
PREFIX=${PREFIX:-"$HOME/.local"}
XDG_DATA_HOME=${XDG_DATA_HOME:-"$PREFIX/share"}
XDG_STATE_HOME=${XDG_STATE_HOME:-"$HOME/.local/state"}
XDG_CONFIG_HOME=${XDG_CONFIG_HOME:-"$HOME/.config"}
ZURUF_DATA_DIR=${ZURUF_DATA_DIR:-"$XDG_DATA_HOME/zuruf"}
ZURUF_STATE_DIR=${ZURUF_STATE_DIR:-"$XDG_STATE_HOME/zuruf"}
ZURUF_CONFIG_DIR=${ZURUF_CONFIG_DIR:-"$XDG_CONFIG_HOME/zuruf"}
unit_dir="$XDG_CONFIG_HOME/systemd/user"
# The browser shell and the Chromium runtime it is linked against.
lib_dir="$PREFIX/lib/zuruf"
