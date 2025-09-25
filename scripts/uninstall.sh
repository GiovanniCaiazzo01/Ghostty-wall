#!/usr/bin/env bash
set -euo pipefail

if [ -x "/opt/homebrew/bin/ghostty-wall" ]; then BIN="/opt/homebrew/bin/ghostty-wall"
elif [ -x "/usr/local/bin/ghostty-wall" ]; then BIN="/usr/local/bin/ghostty-wall"
elif [ -x "$HOME/.local/bin/ghostty-wall" ]; then BIN="$HOME/.local/bin/ghostty-wall"
else
  echo "ghostty-wall not found in known locations"; exit 0
fi

echo "==> Removing $BIN"
rm -f "$BIN"
echo "Done (files under ~/.config/ghostty are left untouched)."

