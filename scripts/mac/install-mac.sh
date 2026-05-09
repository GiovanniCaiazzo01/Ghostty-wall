install_mac() {
  local prefix bin_dest conf_dir ghostty_dir profile shell_name first_run_status

  if [ -n "${INSTALL_PREFIX:-}" ]; then
    prefix="$INSTALL_PREFIX"
  elif [ -d "/opt/homebrew/bin" ] && [ -w "/opt/homebrew/bin" ]; then
    prefix="/opt/homebrew"
  elif [ -d "/usr/local/bin" ] && [ -w "/usr/local/bin" ]; then
    prefix="/usr/local"
  else
    prefix="$HOME/.local"
  fi

  bin_dest="$prefix/bin"
  conf_dir="${XDG_CONFIG_HOME:-$HOME/.config}"
  ghostty_dir="$conf_dir/ghostty"

  printf '==> Installing to %s\n' "$bin_dest"
  install -d "$bin_dest"
  install -m 0755 "$REPO_ROOT/bin/ghostty-wall" "$bin_dest/ghostty-wall"

  mkdir -p "$ghostty_dir"
  if [ ! -f "$ghostty_dir/wallpaper_repos.txt" ]; then
    cp "$REPO_ROOT/examples/wallpaper_repos.example.txt" "$ghostty_dir/wallpaper_repos.txt"
  fi
  printf '%s\n' "$bin_dest/ghostty-wall" > "$ghostty_dir/install-path"

  first_run_status=0
  if [ "${GHOSTTY_WALL_DISABLE_FIRST_RUN:-0}" != "1" ]; then
    if "$bin_dest/ghostty-wall"; then
      :
    else
      first_run_status=$?
    fi
  fi

  printf '==> Installed: %s/ghostty-wall\n' "$bin_dest"

  if [ "$bin_dest" = "$HOME/.local/bin" ]; then
    shell_name="$(basename "${SHELL:-bash}")"
    if [ -n "${PROFILE_FILE:-}" ]; then
      profile="$PROFILE_FILE"
    elif [ "$shell_name" = "zsh" ]; then
      profile="$HOME/.zshrc"
    else
      profile="$HOME/.bash_profile"
    fi

    if ! printf '%s' ":${PATH:-}:" | grep -qF ":$HOME/.local/bin:"; then
      touch "$profile"
      if ! grep -qsF 'export PATH="$HOME/.local/bin:$PATH"' "$profile"; then
        printf '>> Adding ~/.local/bin to PATH in %s\n' "$profile"
        printf '%s\n' 'export PATH="$HOME/.local/bin:$PATH"' >> "$profile"
      fi
      printf ">> Re-open your terminal or run 'source %s'\n" "$profile"
    fi
  fi

  printf '\nAll set! Try: ghostty-wall\n'
  printf 'More commands: ghostty-wall --list / --add / --remove\n'

  if [ "$first_run_status" -ne 0 ]; then
    printf 'Warning: initial wallpaper setup did not complete. Run ghostty-wall after fixing connectivity or repo configuration.\n' >&2
  fi
}
