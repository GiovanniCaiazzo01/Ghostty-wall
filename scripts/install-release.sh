#!/usr/bin/env bash
set -euo pipefail

os="$(uname -s)"
arch="$(uname -m)"
if [[ "$os" != Linux || "$arch" != x86_64 && "$arch" != amd64 ]]; then
  printf 'Unsupported platform: %s %s\n\nPrebuilt Ghostty Wall releases currently support Linux x86_64.\nSee the README for source installation.\n' "$os" "$arch" >&2
  exit 1
fi

command -v curl >/dev/null 2>&1 || { printf 'curl is required to install Ghostty Wall.\n' >&2; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { printf 'sha256sum is required to verify Ghostty Wall.\n' >&2; exit 1; }

REPOSITORY="GiovanniCaiazzo01/Ghostty-wall"
TARGET="x86_64-unknown-linux-gnu"
asset="ghostty-wall-$TARGET.tar.gz"
release="latest/download"
if [[ -n "${GHOSTTY_WALL_VERSION:-}" ]]; then
  if [[ ! "$GHOSTTY_WALL_VERSION" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    printf 'Invalid GHOSTTY_WALL_VERSION: expected vMAJOR.MINOR.PATCH.\n' >&2
    exit 1
  fi
  release="download/$GHOSTTY_WALL_VERSION"
fi
url="https://github.com/$REPOSITORY/releases/$release/$asset"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

curl --fail --location --silent --show-error --output "$tmp_dir/$asset" "$url"
curl --fail --location --silent --show-error --output "$tmp_dir/$asset.sha256" "$url.sha256"
if ! (cd "$tmp_dir" && sha256sum --check "$asset.sha256"); then
  printf 'Ghostty Wall archive checksum verification failed; nothing installed.\n' >&2
  exit 1
fi

mkdir "$tmp_dir/extracted"
tar -xzf "$tmp_dir/$asset" -C "$tmp_dir/extracted"
mapfile -d '' -t binaries < <(find "$tmp_dir/extracted" -mindepth 2 -maxdepth 2 -type f -name ghostty-wall -print0)
if [[ ${#binaries[@]} -ne 1 || ! -x "${binaries[0]:-}" ]]; then
  printf 'Expected exactly one executable ghostty-wall in release archive; nothing installed.\n' >&2
  exit 1
fi

prefix="${INSTALL_PREFIX:-$HOME/.local}"
bin_dir="$prefix/bin"
install -d "$bin_dir"
install -m 0755 "${binaries[0]}" "$bin_dir/ghostty-wall"
# Ownership proof prevents self-update from replacing an unrelated installation.
marker_tmp="$(mktemp "$bin_dir/.ghostty-wall-release.sha256.XXXXXX")"
trap 'rm -rf "$tmp_dir"; rm -f "$marker_tmp"' EXIT
(cd "$bin_dir" && sha256sum ghostty-wall) > "$marker_tmp"
chmod 0644 "$marker_tmp"
mv "$marker_tmp" "$bin_dir/.ghostty-wall-release.sha256"
printf 'Installed ghostty-wall to %s/ghostty-wall\n' "$bin_dir"

# Compare resolved directories so PATH entries with trailing slashes or symlinks count.
installed_dir="$(cd "$bin_dir" && pwd -P)"
in_path=false
IFS=: read -r -a path_entries <<< "${PATH:-}"
for entry in "${path_entries[@]}"; do
  if [[ -d "${entry:-.}" && "$(cd "${entry:-.}" && pwd -P)" == "$installed_dir" ]]; then
    in_path=true
    break
  fi
done
if [[ "$in_path" == false ]]; then
  printf 'Warning: %s is not in PATH. Add it to your shell PATH before running ghostty-wall.\n' "$bin_dir" >&2
fi
printf '\nNext:\n  ghostty-wall init\n  ghostty-wall apply welcome\n'
