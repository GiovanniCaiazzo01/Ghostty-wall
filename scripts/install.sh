#!/usr/bin/env bash
set -euo pipefail

# Detect install prefix (Apple Silicon -> /opt/homebrew, Intel -> /usr/local)
if [ -d "/opt/homebrew/bin" ]; then
  PREFIX="/opt/homebrew"
elif [ -d "/usr/local/bin" ]; then
  PREFIX="/usr/local"
else
  # fallback user-space (no sudo)
  PREFIX="$HOME/.local"
  mkdir -p "$PREFIX/bin"
fi

BIN_DEST="$PREFIX/bin"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "==> Installing to $BIN_DEST"

# 1) install bin
install -d "$BIN_DEST"
install -m 0755 "$SCRIPT_DIR/bin/ghostty-wall" "$BIN_DEST/ghostty-wall"

# 2) ensure Ghostty config dir & default files (do not overwrite existing)
CONF_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
GHOSTTY_DIR="$CONF_DIR/ghostty"
mkdir -p "$GHOSTTY_DIR"

[ -f "$GHOSTTY_DIR/wallpaper_repos.txt" ] || \
  cp "$SCRIPT_DIR/examples/wallpaper_repos.example.txt" "$GHOSTTY_DIR/wallpaper_repos.txt"

# 3) first run to generate wallpaper.conf and include into Ghostty config
"$BIN_DEST/ghostty-wall" || true

echo "==> Installed: $BIN_DEST/ghostty-wall"

# 4) PATH hint if we used ~/.local/bin
if [[ "$BIN_DEST" == "$HOME/.local/bin" ]]; then
  SHELL_NAME="$(basename "${SHELL:-}")"
  if [ "$SHELL_NAME" = "zsh" ]; then
    PROFILE="$HOME/.zshrc"
  else
    PROFILE="$HOME/.bash_profile"
  fi
  if ! printf "%s" "$PATH" | grep -q "$HOME/.local/bin"; then
    echo ">> Adding ~/.local/bin to PATH in $PROFILE"
    echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$PROFILE"
    echo ">> Re-open your terminal or 'source' the profile"
  fi
fi

echo
echo "All set! Try:  ghostty-wall   (picks a wallpaper right away)"
echo "More commands: ghostty-wall --list / --add / --remove / --daemon"

