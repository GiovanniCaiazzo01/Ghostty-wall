#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source-path=SCRIPTDIR
# shellcheck source=mac/install-mac.sh
source "$SCRIPT_DIR/mac/install-mac.sh"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=linux/install-linux.sh
source "$SCRIPT_DIR/linux/install-linux.sh"

detect_os() {
  case "$(uname)" in
    Darwin) install_mac ;;
    Linux) install_linux ;;
    *)
      printf 'Unsupported operating system: %s\n' "$(uname)" >&2
      exit 1
      ;;
  esac
}

detect_os
