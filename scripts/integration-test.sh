#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

assert_file() {
  [ -f "$1" ] || fail "expected file: $1"
}

assert_contains() {
  local file="$1"
  local expected="$2"
  grep -qsF "$expected" "$file" || fail "expected '$expected' in $file"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

mktemp_dir() {
  mktemp -d 2>/dev/null || mktemp -d -t ghostty-wall
}

require_command curl

temp_home="$(mktemp_dir)"
temp_config="$temp_home/config"
temp_dir="$temp_home/tmp"
mkdir -p "$temp_config/ghostty" "$temp_dir"

cleanup() {
  rm -rf "$temp_home"
}
trap cleanup EXIT

cat >"$temp_config/ghostty/wallpaper_repos.txt" <<'EOF'
anime|ThePrimeagen/anime|master|
EOF

printf 'Running live integration test against GitHub...\n'

HOME="$temp_home" \
XDG_CONFIG_HOME="$temp_config" \
GHOSTTY_WALL_TEMP_DIR="$temp_dir" \
GHOSTTY_WALL_OS_TYPE='Linux' \
"$REPO_ROOT/bin/ghostty-wall"

wall_conf="$temp_config/ghostty/wallpaper.conf"
main_conf="$temp_config/ghostty/config"

assert_file "$wall_conf"
assert_file "$main_conf"
assert_contains "$main_conf" "config-file = $wall_conf"

image_path="$(sed -n 's/^background-image=//p' "$wall_conf")"
[ -n "$image_path" ] || fail "wallpaper.conf did not contain a background-image path"
assert_file "$image_path"

printf 'Live integration test passed.\n'
