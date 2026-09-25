#!/usr/bin/env bash
set -euo pipefail

script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/install-release.sh"
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
mkdir -p "$root/mock" "$root/fixtures" "$root/tmp" "$root/home" "$root/prefix/bin"

cat > "$root/mock/uname" <<'EOF'
#!/usr/bin/env bash
case "$1" in
  -s) printf '%s\n' "${MOCK_OS:-Linux}" ;;
  -m) printf '%s\n' "${MOCK_ARCH:-x86_64}" ;;
esac
EOF
cat > "$root/mock/curl" <<'EOF'
#!/usr/bin/env bash
while [[ $# -gt 0 ]]; do
  case "$1" in
    --output) output="$2"; shift 2 ;;
    --fail|--location|--silent|--show-error) shift ;;
    *) url="$1"; shift ;;
  esac
done
printf '%s\n' "$url" >> "$URL_LOG"
cp "$FIXTURES/${url##*/}" "$output"
EOF
chmod +x "$root/mock/uname" "$root/mock/curl"
export PATH="$root/mock:$PATH" FIXTURES="$root/fixtures" URL_LOG="$root/urls" TMPDIR="$root/tmp" HOME="$root/home"
asset=ghostty-wall-x86_64-unknown-linux-gnu.tar.gz
mkdir -p "$root/pack/ghostty-wall-v1.0.2-x86_64-unknown-linux-gnu"
printf '#!/bin/sh\necho fixture\n' > "$root/pack/ghostty-wall-v1.0.2-x86_64-unknown-linux-gnu/ghostty-wall"
chmod +x "$root/pack/ghostty-wall-v1.0.2-x86_64-unknown-linux-gnu/ghostty-wall"
tar -czf "$FIXTURES/$asset" -C "$root/pack" .
(cd "$FIXTURES" && sha256sum "$asset" > "$asset.sha256")

fail() { printf 'FAIL: %s\n' "$1" >&2; exit 1; }
assert_clean() { [[ -z "$(find "$TMPDIR" -mindepth 1 -print -quit)" ]] || fail 'temporary files remain'; }

bash "$script" > "$root/output" 2>&1 || fail 'default install'
[[ -x "$HOME/.local/bin/ghostty-wall" ]] || fail 'default destination'
grep -qF '/releases/latest/download/' "$URL_LOG" || fail 'latest URL'
grep -qF 'Warning: ' "$root/output" || fail 'missing PATH warning'
grep -qF 'ghostty-wall init' "$root/output" || fail 'next steps'
assert_clean

INSTALL_PREFIX="$root/prefix" GHOSTTY_WALL_VERSION=v1.0.2 MOCK_ARCH=amd64 \
  PATH="$root/prefix/bin/:$PATH" bash "$script" > "$root/output" 2>&1 || fail 'override install'
[[ -x "$root/prefix/bin/ghostty-wall" ]] || fail 'prefix destination'
grep -qF '/releases/download/v1.0.2/' "$URL_LOG" || fail 'version URL'
if grep -qF 'Warning: ' "$root/output"; then fail 'false PATH warning'; fi
assert_clean

if MOCK_OS=Darwin MOCK_ARCH=arm64 bash "$script" > "$root/output" 2>&1; then fail 'unsupported platform accepted'; fi
grep -qF 'Unsupported platform: Darwin arm64' "$root/output" || fail 'unsupported platform message'
if GHOSTTY_WALL_VERSION=invalid bash "$script" > "$root/output" 2>&1; then fail 'invalid version accepted'; fi
assert_clean

printf 'corrupt' >> "$FIXTURES/$asset"
if INSTALL_PREFIX="$root/prefix" bash "$script" > "$root/output" 2>&1; then fail 'bad checksum accepted'; fi
grep -qF 'checksum verification failed' "$root/output" || fail 'checksum error'
cmp "$root/prefix/bin/ghostty-wall" "$root/pack/ghostty-wall-v1.0.2-x86_64-unknown-linux-gnu/ghostty-wall" || fail 'binary changed on checksum failure'
assert_clean

mkdir -p "$root/pack/another"
cp "$root/pack/ghostty-wall-v1.0.2-x86_64-unknown-linux-gnu/ghostty-wall" "$root/pack/another/ghostty-wall"
tar -czf "$FIXTURES/$asset" -C "$root/pack" .
(cd "$FIXTURES" && sha256sum "$asset" > "$asset.sha256")
if INSTALL_PREFIX="$root/prefix" bash "$script" > "$root/output" 2>&1; then fail 'ambiguous binary accepted'; fi
grep -qF 'Expected exactly one executable' "$root/output" || fail 'ambiguous binary error'
assert_clean
printf 'Release installer tests passed.\n'
