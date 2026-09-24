#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OS="$(uname -s)"

case "$OS" in
  Linux) ;;
  Darwin) printf 'Warning: macOS support is experimental.\n' >&2 ;;
  *) printf 'Unsupported operating system: %s\n' "$OS" >&2; exit 1 ;;
esac

PREFIX="${INSTALL_PREFIX:-$HOME/.local}"
DEST="$PREFIX/bin/ghostty-wall"
SOURCE="${GHOSTTY_WALL_BINARY:-}"

if [ -z "$SOURCE" ] && [ -x "$SCRIPT_DIR/ghostty-wall" ]; then
  SOURCE="$SCRIPT_DIR/ghostty-wall"
elif [ -z "$SOURCE" ] && [ -x "$REPO_ROOT/target/release/ghostty-wall" ]; then
  SOURCE="$REPO_ROOT/target/release/ghostty-wall"
elif [ -z "$SOURCE" ]; then
  command -v cargo >/dev/null 2>&1 || {
    printf 'No release binary found and cargo is unavailable.\n' >&2
    exit 1
  }
  cargo build --locked --release --manifest-path "$REPO_ROOT/Cargo.toml"
  SOURCE="$REPO_ROOT/target/release/ghostty-wall"
fi

[ -x "$SOURCE" ] || { printf 'Not an executable: %s\n' "$SOURCE" >&2; exit 1; }
install -d "$PREFIX/bin"
install -m 0755 "$SOURCE" "$DEST"
printf 'Installed %s\n' "$DEST"
printf 'Next: %s init\n' "$DEST"
