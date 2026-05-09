#!/usr/bin/env bash
set -euo pipefail

CONF_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
GHOSTTY_DIR="$CONF_DIR/ghostty"
INSTALL_STATE_FILE="$GHOSTTY_DIR/install-path"
candidates=()

if [ -n "${INSTALL_PREFIX:-}" ]; then
  candidates+=("$INSTALL_PREFIX/bin/ghostty-wall")
fi

recorded_path=""

if [ -f "$INSTALL_STATE_FILE" ]; then
  recorded_path="$(tr -d '\n' < "$INSTALL_STATE_FILE")"
  if [ -n "$recorded_path" ]; then
    candidates+=("$recorded_path")
  fi
fi

candidates+=(
  "/opt/homebrew/bin/ghostty-wall"
  "/usr/local/bin/ghostty-wall"
  "/usr/bin/ghostty-wall"
  "$HOME/.local/bin/ghostty-wall"
)

for candidate in "${candidates[@]}"; do
  if [ -x "$candidate" ]; then
    BIN="$candidate"
    break
  fi
done

if [ -n "$recorded_path" ] && [ ! -e "$recorded_path" ]; then
  rm -f "$INSTALL_STATE_FILE"
fi

[ -n "${BIN:-}" ] || {
  echo "ghostty-wall not found in known locations"
  exit 0
}

echo "==> Removing $BIN"
rm -f "$BIN"
if [ -f "$INSTALL_STATE_FILE" ] && [ "$(tr -d '\n' < "$INSTALL_STATE_FILE")" = "$BIN" ]; then
  rm -f "$INSTALL_STATE_FILE"
fi

echo "Done (files under $GHOSTTY_DIR are left untouched)."
