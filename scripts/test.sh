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

assert_executable() {
  [ -x "$1" ] || fail "expected executable: $1"
}

assert_contains() {
  local file="$1"
  local expected="$2"
  grep -qsF "$expected" "$file" || fail "expected '$expected' in $file"
}

assert_output_contains() {
  local output="$1"
  local expected="$2"
  printf '%s' "$output" | grep -qF "$expected" || fail "expected output to contain: $expected"
}

assert_not_contains() {
  local file="$1"
  local unexpected="$2"
  if grep -qsF "$unexpected" "$file"; then
    fail "did not expect '$unexpected' in $file"
  fi
}

assert_occurrences() {
  local file="$1"
  local expected="$2"
  local count="$3"
  local actual

  actual="$(grep -F -c "$expected" "$file" || true)"
  [ "$actual" = "$count" ] || fail "expected $count occurrences of '$expected' in $file, got $actual"
}

run_syntax_checks() {
  bash -n "$REPO_ROOT/bin/ghostty-wall"
  bash -n "$REPO_ROOT/scripts/install.sh"
  bash -n "$REPO_ROOT/scripts/mac/install-mac.sh"
  bash -n "$REPO_ROOT/scripts/linux/install-linux.sh"
  bash -n "$REPO_ROOT/scripts/uninstall.sh"
}

run_cli_tests() {
  local temp_home temp_config list_output help_output list_file invalid_output

  temp_home="$(mktemp -d)"
  temp_config="$temp_home/config"
  list_file="$temp_home/list.out"
  mkdir -p "$temp_config"

  HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" \
    "$REPO_ROOT/bin/ghostty-wall" --list >"$list_file"
  list_output="$(< "$list_file")"
  assert_output_contains "$list_output" "wallpaper_repos.txt"
  assert_file "$temp_config/ghostty/wallpaper_repos.txt"

  HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" \
    "$REPO_ROOT/bin/ghostty-wall" --add 'cats|owner/repo|main|walls'
  assert_contains "$temp_config/ghostty/wallpaper_repos.txt" "cats|owner/repo|main|walls"

  HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" \
    "$REPO_ROOT/bin/ghostty-wall" --add 'a.b|owner/repo2|main|folder with spaces'
  assert_contains "$temp_config/ghostty/wallpaper_repos.txt" "a.b|owner/repo2|main|folder with spaces"

  printf '%s\n' '  spaced  |owner/repo3|main|' >> "$temp_config/ghostty/wallpaper_repos.txt"
  HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" \
    "$REPO_ROOT/bin/ghostty-wall" --remove 'spaced'
  if grep -qsF '  spaced  |owner/repo3|main|' "$temp_config/ghostty/wallpaper_repos.txt"; then
    fail "repo entry with surrounding whitespace should be removed"
  fi

  HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" \
    "$REPO_ROOT/bin/ghostty-wall" --remove cats
  if grep -qsF 'cats|owner/repo|main|walls' "$temp_config/ghostty/wallpaper_repos.txt"; then
    fail "repo entry should be removed"
  fi

  if HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" \
    "$REPO_ROOT/bin/ghostty-wall" --add 'bad name|owner/repo|main|walls' >"$temp_home/invalid.out" 2>&1; then
    fail "invalid repo entry should be rejected"
  fi
  invalid_output="$(< "$temp_home/invalid.out")"
  assert_output_contains "$invalid_output" "Invalid line"

  if HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" \
    "$REPO_ROOT/bin/ghostty-wall" --remove '[' >"$temp_home/remove-invalid.out" 2>&1; then
    fail "invalid repo name should be rejected"
  fi
  invalid_output="$(< "$temp_home/remove-invalid.out")"
  assert_output_contains "$invalid_output" "Invalid repo name"

  if HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" \
    "$REPO_ROOT/bin/ghostty-wall" --lst >"$temp_home/bad-arg.out" 2>&1; then
    fail "unknown flag should exit non-zero"
  fi
  invalid_output="$(< "$temp_home/bad-arg.out")"
  assert_output_contains "$invalid_output" "Usage:"

  HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" \
    "$REPO_ROOT/bin/ghostty-wall" --remove 'a.b'
  if grep -qsF 'a.b|owner/repo2|main|folder with spaces' "$temp_config/ghostty/wallpaper_repos.txt"; then
    fail "repo entry with dot should be removed exactly"
  fi

  help_output="$(HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" "$REPO_ROOT/bin/ghostty-wall" --help)"
  assert_output_contains "$help_output" "Usage:"

  rm -rf "$temp_home"
}

run_runtime_tests() {
  local temp_home temp_config temp_dir curl_mock output state_file

  temp_home="$(mktemp -d)"
  temp_config="$temp_home/config"
  temp_dir="$temp_home/tmp"
  curl_mock="$temp_home/mock-curl.sh"
  state_file="$temp_home/curl-state"
  mkdir -p "$temp_config/ghostty" "$temp_dir"

  cat >"$temp_config/ghostty/wallpaper_repos.txt" <<'EOF'
first|owner/one|main|
second|owner/two|main|
EOF

  cat >"$curl_mock" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output_file=""
url=""
state_file="${GHOSTTY_WALL_TEST_STATE:?}"
write_out=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output_file="$2"
      write_out=1
      shift 2
      ;;
    -w)
      shift 2
      ;;
    http://*|https://*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done

case "$url" in
  *api.github.com* )
    if [ ! -f "$state_file" ]; then
      printf '1\n' >"$state_file"
      printf '{"message":"Not Found"}\n' >"$output_file"
      printf '404'
    else
      printf '[{"download_url":"https://example.com/wallpaper.jpg"}]\n' >"$output_file"
      printf '200'
    fi
    ;;
  *example.com/wallpaper.jpg )
    printf 'fake image data\n' >"$output_file"
    ;;
  * )
    printf 'unexpected url: %s\n' "$url" >&2
    exit 1
    ;;
esac
EOF

  chmod +x "$curl_mock"

  output="$(HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" GHOSTTY_WALL_TEMP_DIR="$temp_dir" GHOSTTY_WALL_TEST_STATE="$state_file" GHOSTTY_WALL_OS_TYPE='Linux' CURL_BIN="$curl_mock" "$REPO_ROOT/bin/ghostty-wall" 2>&1)"

  assert_output_contains "$output" "Trying next configured repo"
  assert_output_contains "$output" "Automatic reload is only supported on macOS"
  assert_file "$temp_dir/current_wallpaper.jpg"
  assert_contains "$temp_config/ghostty/wallpaper.conf" "background-image=$temp_dir/current_wallpaper.jpg"
  assert_contains "$temp_config/ghostty/config" "config-file = $temp_config/ghostty/wallpaper.conf"

  output="$(HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" GHOSTTY_WALL_TEMP_DIR="$temp_dir" GHOSTTY_WALL_TEST_STATE="$state_file" CURL_BIN="$curl_mock" "$REPO_ROOT/bin/ghostty-wall" --list)"
  assert_output_contains "$output" "second|owner/two|main|"
  assert_occurrences "$temp_config/ghostty/config" "config-file = $temp_config/ghostty/wallpaper.conf" 1

  rm -rf "$temp_home"
}

run_failure_runtime_tests() {
  local temp_home temp_config temp_dir curl_mock output state_file

  temp_home="$(mktemp -d)"
  temp_config="$temp_home/config"
  temp_dir="$temp_home/tmp"
  curl_mock="$temp_home/mock-curl-fail.sh"
  state_file="$temp_home/curl-state"
  mkdir -p "$temp_config/ghostty" "$temp_dir"

  cat >"$temp_config/ghostty/wallpaper_repos.txt" <<'EOF'
bad|owner/empty|main|folder with spaces
EOF

  cat >"$curl_mock" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output_file="$2"
      shift 2
      ;;
    -w)
      shift 2
      ;;
    http://*|https://*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done

case "$url" in
  *folder%20with%20spaces* )
    printf '[]\n' >"$output_file"
    printf '200'
    ;;
  * )
    printf 'unexpected url: %s\n' "$url" >&2
    exit 1
    ;;
esac
EOF

  chmod +x "$curl_mock"

  if HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" GHOSTTY_WALL_TEMP_DIR="$temp_dir" GHOSTTY_WALL_TEST_STATE="$state_file" CURL_BIN="$curl_mock" "$REPO_ROOT/bin/ghostty-wall" >"$temp_home/failure.out" 2>&1; then
    fail "runtime should fail when no repos produce a wallpaper"
  fi

  output="$(< "$temp_home/failure.out")"
  assert_output_contains "$output" "Unable to fetch a wallpaper from any configured repository"
  assert_not_contains "$temp_home/failure.out" "Image ready"
  [ ! -f "$temp_config/ghostty/wallpaper.conf" ] || fail "wallpaper.conf should not be written on total failure"

  rm -rf "$temp_home"
}

run_api_error_tests() {
  local temp_home temp_config temp_dir curl_mock output

  local output_file
  temp_home="$(mktemp -d)"
  temp_config="$temp_home/config"
  temp_dir="$temp_home/tmp"
  curl_mock="$temp_home/mock-curl-api-error.sh"
  mkdir -p "$temp_config/ghostty" "$temp_dir"

  cat >"$temp_config/ghostty/wallpaper_repos.txt" <<'EOF'
rate|owner/rate|feature+branch|images,[1]
EOF

  cat >"$curl_mock" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output_file="$2"
      shift 2
      ;;
    -w)
      shift 2
      ;;
    http://*|https://*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done

case "$url" in
  *images%2C%5B1%5D*ref=feature%2Bbranch* )
    printf '{"message":"API rate limit exceeded"}\n' >"$output_file"
    printf '403'
    ;;
  * )
    printf 'unexpected url: %s\n' "$url" >&2
    exit 1
    ;;
esac
EOF

  chmod +x "$curl_mock"

  if HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" GHOSTTY_WALL_TEMP_DIR="$temp_dir" CURL_BIN="$curl_mock" "$REPO_ROOT/bin/ghostty-wall" >"$temp_home/api-error.out" 2>&1; then
    fail "runtime should fail on GitHub API error"
  fi

  output="$(< "$temp_home/api-error.out")"
  assert_output_contains "$output" "GitHub API error for owner/rate/images,[1]@feature+branch: API rate limit exceeded"
  assert_output_contains "$output" "Unable to fetch a wallpaper from any configured repository"

  rm -rf "$temp_home"
}

run_download_failure_test() {
  local temp_home temp_config temp_dir curl_mock output

  temp_home="$(mktemp -d)"
  temp_config="$temp_home/config"
  temp_dir="$temp_home/tmp"
  curl_mock="$temp_home/mock-curl-download-fail.sh"
  mkdir -p "$temp_config/ghostty" "$temp_dir"

  cat >"$temp_config/ghostty/wallpaper_repos.txt" <<'EOF'
download|owner/good|main|
EOF

  cat >"$curl_mock" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output_file=""
url=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output_file="$2"
      shift 2
      ;;
    -w)
      shift 2
      ;;
    http://*|https://*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done

case "$url" in
  *api.github.com* )
    printf '[{"download_url":"https://example.com/fail.jpg"}]\n' >"$output_file"
    printf '200'
    ;;
  *example.com/fail.jpg* )
    printf 'partial image data\n' >"$output_file"
    exit 1
    ;;
  * )
    printf 'unexpected url: %s\n' "$url" >&2
    exit 1
    ;;
esac
EOF

  chmod +x "$curl_mock"

  if HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" GHOSTTY_WALL_TEMP_DIR="$temp_dir" CURL_BIN="$curl_mock" "$REPO_ROOT/bin/ghostty-wall" >"$temp_home/download-fail.out" 2>&1; then
    fail "runtime should fail when image download fails"
  fi

  output="$(< "$temp_home/download-fail.out")"
  assert_output_contains "$output" "Failed to download image"
  [ ! -e "$temp_dir/.current_wallpaper.jpg.tmp" ] || fail "temporary download file should be removed on failure"
  [ ! -e "$temp_dir/current_wallpaper.jpg" ] || fail "final wallpaper file should not exist on download failure"

  rm -rf "$temp_home"
}

run_macos_reload_tests() {
  local temp_home temp_config temp_dir curl_mock osascript_mock output osascript_log

  temp_home="$(mktemp -d)"
  temp_config="$temp_home/config"
  temp_dir="$temp_home/tmp"
  curl_mock="$temp_home/mock-curl.sh"
  osascript_mock="$temp_home/mock-osascript.sh"
  osascript_log="$temp_home/osascript.log"
  mkdir -p "$temp_config/ghostty" "$temp_dir"

  cat >"$temp_config/ghostty/wallpaper_repos.txt" <<'EOF'
mac|owner/good|main|
EOF

  cat >"$curl_mock" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output_file=""
url=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output_file="$2"
      shift 2
      ;;
    -w)
      shift 2
      ;;
    http://*|https://*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done

case "$url" in
  *api.github.com* )
    printf '[{"download_url":"https://example.com/mac.jpg"}]\n' >"$output_file"
    printf '200'
    ;;
  *example.com/mac.jpg* )
    printf 'fake image data\n' >"$output_file"
    ;;
  * )
    printf 'unexpected url: %s\n' "$url" >&2
    exit 1
    ;;
esac
EOF

  cat >"$osascript_mock" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

log_file="${GHOSTTY_WALL_OSASCRIPT_LOG:?}"
mode="${GHOSTTY_WALL_OSASCRIPT_MODE:?}"

if [ "${1:-}" = "-e" ]; then
  printf 'check\n' >>"$log_file"
  if [ "$mode" = "running" ]; then
    printf 'Ghostty\n'
  elif [ "$mode" = "nightly" ]; then
    printf 'Ghostty Nightly\n'
  else
    printf '\n'
  fi
else
  script_contents="$(cat)"
  printf '%s\n' "$script_contents" >>"$log_file"
  printf 'reload\n' >>"$log_file"
fi
EOF

  chmod +x "$curl_mock" "$osascript_mock"

  output="$(HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" GHOSTTY_WALL_TEMP_DIR="$temp_dir" CURL_BIN="$curl_mock" GHOSTTY_WALL_OS_TYPE='Darwin' OSASCRIPT_BIN="$osascript_mock" GHOSTTY_WALL_OSASCRIPT_LOG="$osascript_log" GHOSTTY_WALL_OSASCRIPT_MODE='running' "$REPO_ROOT/bin/ghostty-wall" 2>&1)"
  assert_output_contains "$output" "Reloading Ghostty config"
  assert_occurrences "$osascript_log" "check" 1
  assert_occurrences "$osascript_log" "reload" 1
  assert_occurrences "$osascript_log" 'system attribute "GHOSTTY_APP"' 1

  : > "$osascript_log"
  output="$(HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" GHOSTTY_WALL_TEMP_DIR="$temp_dir" CURL_BIN="$curl_mock" GHOSTTY_WALL_OS_TYPE='Darwin' OSASCRIPT_BIN="$osascript_mock" GHOSTTY_WALL_OSASCRIPT_LOG="$osascript_log" GHOSTTY_WALL_OSASCRIPT_MODE='stopped' "$REPO_ROOT/bin/ghostty-wall" 2>&1)"
  assert_output_contains "$output" "Ghostty is not running: it will use the new wallpaper on next launch."
  assert_occurrences "$osascript_log" "check" 1
  assert_occurrences "$osascript_log" "reload" 0

  : > "$osascript_log"
  output="$(HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" GHOSTTY_WALL_TEMP_DIR="$temp_dir" CURL_BIN="$curl_mock" GHOSTTY_WALL_OS_TYPE='Darwin' OSASCRIPT_BIN="$osascript_mock" GHOSTTY_WALL_OSASCRIPT_LOG="$osascript_log" GHOSTTY_WALL_OSASCRIPT_MODE='nightly' "$REPO_ROOT/bin/ghostty-wall" 2>&1)"
  assert_output_contains "$output" "Reloading Ghostty Nightly config"
  assert_occurrences "$osascript_log" "reload" 1

  output="$(HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" GHOSTTY_WALL_TEMP_DIR="$temp_dir" CURL_BIN="$curl_mock" GHOSTTY_WALL_OS_TYPE='Darwin' OSASCRIPT_BIN='/definitely/missing/osascript' "$REPO_ROOT/bin/ghostty-wall" 2>&1)"
  assert_output_contains "$output" "Automatic reload is unavailable: osascript not found."

  rm -rf "$temp_home"
}

run_missing_command_test() {
  local temp_home temp_config output

  temp_home="$(mktemp -d)"
  temp_config="$temp_home/config"
  mkdir -p "$temp_config/ghostty"
  printf '%s\n' 'repo|owner/repo|main|' > "$temp_config/ghostty/wallpaper_repos.txt"

  if HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" CURL_BIN='definitely-not-a-command' "$REPO_ROOT/bin/ghostty-wall" >"$temp_home/missing.out" 2>&1; then
    fail "runtime should fail when curl command is missing"
  fi

  output="$(< "$temp_home/missing.out")"
  assert_output_contains "$output" "Required command not found"

  rm -rf "$temp_home"
}

run_installer_tests() {
  local temp_home temp_prefix temp_local_prefix temp_config profile_file output curl_fail curl_success temp_home_zsh temp_config_zsh temp_success_home temp_success_config temp_success_prefix

  temp_home="$(mktemp -d)"
  temp_prefix="$(mktemp -d)"
  temp_local_prefix="$temp_home/.local"
  temp_config="$temp_home/config"
  profile_file="$temp_home/profile"
  mkdir -p "$temp_config"

  HOME="$temp_home" \
  XDG_CONFIG_HOME="$temp_config" \
  INSTALL_PREFIX="$temp_prefix" \
  PROFILE_FILE="$profile_file" \
  GHOSTTY_WALL_DISABLE_FIRST_RUN=1 \
    bash "$REPO_ROOT/scripts/install.sh"

  assert_executable "$temp_prefix/bin/ghostty-wall"
  assert_file "$temp_config/ghostty/wallpaper_repos.txt"
  assert_contains "$temp_config/ghostty/install-path" "$temp_prefix/bin/ghostty-wall"

  printf '%s\n' 'keep|owner/repo|main|' > "$temp_config/ghostty/wallpaper_repos.txt"
  curl_fail="$temp_home/curl-fail.sh"
  cat >"$curl_fail" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
  chmod +x "$curl_fail"

  HOME="$temp_home" \
  XDG_CONFIG_HOME="$temp_config" \
  INSTALL_PREFIX="$temp_prefix" \
  PROFILE_FILE="$profile_file" \
  CURL_BIN="$curl_fail" \
    bash "$REPO_ROOT/scripts/install.sh" >"$temp_home/install.out" 2>&1
  output="$(< "$temp_home/install.out")"
  assert_output_contains "$output" "Warning: initial wallpaper setup did not complete"
  assert_contains "$temp_config/ghostty/wallpaper_repos.txt" "keep|owner/repo|main|"

  temp_success_home="$(mktemp -d)"
  temp_success_config="$temp_success_home/config"
  temp_success_prefix="$(mktemp -d)"
  mkdir -p "$temp_success_config"
  curl_success="$temp_success_home/curl-success.sh"
  cat >"$curl_success" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output_file=""
url=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output_file="$2"
      shift 2
      ;;
    -w)
      shift 2
      ;;
    http://*|https://*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done

case "$url" in
  *api.github.com* )
    printf '[{"download_url":"https://example.com/install.jpg"}]\n' >"$output_file"
    printf '200'
    ;;
  *example.com/install.jpg* )
    printf 'fake image data\n' >"$output_file"
    ;;
  * )
    printf 'unexpected url: %s\n' "$url" >&2
    exit 1
    ;;
esac
EOF
  chmod +x "$curl_success"

  HOME="$temp_success_home" \
  XDG_CONFIG_HOME="$temp_success_config" \
  INSTALL_PREFIX="$temp_success_prefix" \
  CURL_BIN="$curl_success" \
  GHOSTTY_WALL_OS_TYPE='Linux' \
    bash "$REPO_ROOT/scripts/install.sh" >"$temp_success_home/install-success.out" 2>&1
  assert_file "$temp_success_config/ghostty/wallpaper.conf"
  assert_contains "$temp_success_config/ghostty/config" "config-file = $temp_success_config/ghostty/wallpaper.conf"

  HOME="$temp_home" \
  XDG_CONFIG_HOME="$temp_config" \
  INSTALL_PREFIX="$temp_local_prefix" \
  PROFILE_FILE="$profile_file" \
  PATH="/usr/bin:/bin" \
  GHOSTTY_WALL_DISABLE_FIRST_RUN=1 \
    bash "$REPO_ROOT/scripts/install.sh"
  assert_occurrences "$profile_file" 'export PATH="$HOME/.local/bin:$PATH"' 1

  temp_home_zsh="$(mktemp -d)"
  temp_config_zsh="$temp_home_zsh/config"
  mkdir -p "$temp_config_zsh"
  HOME="$temp_home_zsh" \
  SHELL='/bin/zsh' \
  XDG_CONFIG_HOME="$temp_config_zsh" \
  INSTALL_PREFIX="$temp_home_zsh/.local" \
  PATH="/usr/bin:/bin" \
  GHOSTTY_WALL_DISABLE_FIRST_RUN=1 \
    bash "$REPO_ROOT/scripts/install.sh"
  assert_file "$temp_home_zsh/.zshrc"
  assert_occurrences "$temp_home_zsh/.zshrc" 'export PATH="$HOME/.local/bin:$PATH"' 1

  HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" INSTALL_PREFIX="$temp_prefix" bash "$REPO_ROOT/scripts/uninstall.sh"
  [ ! -e "$temp_prefix/bin/ghostty-wall" ] || fail "expected uninstall to remove installed binary"

  HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" bash "$REPO_ROOT/scripts/uninstall.sh"
  [ ! -e "$temp_local_prefix/bin/ghostty-wall" ] || fail "expected uninstall to remove recorded install binary"
  [ ! -e "$temp_config/ghostty/install-path" ] || fail "install path state file should be removed"

  printf '%s\n' "$temp_home/missing/bin/ghostty-wall" > "$temp_config/ghostty/install-path"
  HOME="$temp_home" XDG_CONFIG_HOME="$temp_config" bash "$REPO_ROOT/scripts/uninstall.sh"
  [ ! -e "$temp_config/ghostty/install-path" ] || fail "stale install path state file should be removed"

  HOME="$temp_success_home" XDG_CONFIG_HOME="$temp_success_config" INSTALL_PREFIX="$temp_success_prefix" bash "$REPO_ROOT/scripts/uninstall.sh"

  rm -rf "$temp_home" "$temp_prefix" "$temp_home_zsh" "$temp_success_home" "$temp_success_prefix"
}

run_syntax_checks
run_cli_tests
run_runtime_tests
run_failure_runtime_tests
run_api_error_tests
run_download_failure_test
run_macos_reload_tests
run_missing_command_test
run_installer_tests

printf 'All tests passed.\n'
