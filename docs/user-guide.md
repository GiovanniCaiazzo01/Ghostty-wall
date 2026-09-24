# User guide

Ghostty Wall manages **selected visual settings**, not all of Ghostty. A Profile is a recipe; `plan` resolves it into an immutable Environment; `apply` writes the managed Ghostty configuration and records an Activation in local History. Other Ghostty settings remain in your normal Ghostty config.

Linux is supported; macOS is experimental. For installation, see [README](../README.md#install). Run `ghostty-wall --help` for the exact CLI syntax.

## Start with the bundled Profile

```sh
ghostty-wall init --dry-run   # Inspect without writing
ghostty-wall init             # Install managed directory and Ghostty config-file hook
ghostty-wall plan welcome     # Inspect resolved configuration; no mutation
ghostty-wall apply welcome    # Activate wallpaper and generated colors
ghostty-wall doctor           # Check installation and durable state
```

`init` does not overwrite existing Profiles. `init --welcome` can add the example only to an otherwise empty, eligible older v1 installation; it refuses customized or durable state.

On Linux, the Managed Root is `${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/ghostty-wall`. On macOS it is `~/Library/Application Support/com.mitchellh.ghostty/ghostty-wall`. Edit only `config.toml` and `profiles/*.toml` for normal customization. Do not edit generated `current.ghostty`: the next apply can replace it.

## Make a Profile using your own images

Add a Source in `<Managed Root>/config.toml` (keep its existing `schema_version = 1` and any other Sources):

```toml
[sources.wallpapers]
kind = "local-directory"
path = "~/Pictures/wallpapers"
```

Create `<Managed Root>/profiles/night.toml`. The filename `night.toml` defines the Profile ID `night`:

```toml
schema_version = 1

[wallpaper]
mode = "source"
source = "wallpapers"
selection = "path"
path = "city/night.png" # Relative to the Source directory
fit = "cover"
position = "center"
opacity = 0.1

[colors]
mode = "generated"

[terminal]
font_size = 13.5
cursor_style = "bar"
```

Then run `ghostty-wall plan night` and `ghostty-wall apply night`. Source/Profile IDs use lowercase letters, digits, and internal hyphens (e.g. `my-night`); no subdirectories under `profiles/`. Local Source paths can be absolute, start with `~/`, or be relative to `config.toml`; other shell expansions are not supported.

### GitHub Source

Use a GitHub repository instead of a local directory:

```toml
[sources.wallpapers]
kind = "github"
repository = "owner/repo"
ref = "main"       # Optional; default branch when omitted
path = "wallpapers" # Optional; repository root when omitted
```

Use `source = "wallpapers"` in the same Profile above. A requested branch is resolved to a commit for each Plan; it is not a permanent pin in `config.toml`. Set `GITHUB_TOKEN` in your environment if access or rate limits require it. Never put tokens in Profile or Source files.

### Choose image by path or randomly

`selection = "path"` requires `path = "city/night.png"`. To choose from the Source's eligible images, replace those lines with `selection = "random"` and **omit** `path`. Random choice requires a 32-byte seed (64 hex digits) on each plan and apply:

```sh
ghostty-wall plan night --seed 000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
ghostty-wall apply night --seed 000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
```

Same seed and unchanged Source produce same selection. Different seed may select another image. There is no automatic timed rotation in v1.

## What each Profile can manage

All three sections below are optional and independent, except generated colors require a Source wallpaper. Omitted properties are **unmanaged**: Ghostty Wall does not invent defaults for them.

### Wallpaper

- Omit `[wallpaper]` to leave wallpaper unmanaged.
- Use `[wallpaper]` with `mode = "none"` to disable the background image.
- Use `mode = "source"` plus `source` and `selection` to manage an image.

For Source images, optional settings: `fit` (`contain`, `cover`, `stretch`, `none`), `position` (e.g. `center`, `top-left`, `bottom-right`), `opacity` (`0` to `1`), and `repeat` (`true` or `false`). PNG and JPEG are supported. Lower image opacity improves text legibility over bright regions.

### Colors

- Omit `[colors]` to leave colors unmanaged.
- `[colors]` with `mode = "generated"` derives background, foreground, 16 ANSI palette colors, cursor, and selection colors from the selected image.
- `[colors]` with `mode = "theme"` and `theme = "TokyoNight"` resolves a locally available Ghostty theme into managed colors. Replay does not require the theme file.
- `[colors]` with `mode = "explicit"` accepts `background`, `foreground`, and **exactly 16** `palette` colors. Optional: `cursor`, `selection_background`, `selection_foreground`. Colors are six lowercase hexadecimal digits, without `#`.

Example explicit colors (replace all values as desired):

```toml
[colors]
mode = "explicit"
background = "1a1b26"
foreground = "c0caf5"
palette = [
  "15161e", "f7768e", "9ece6a", "e0af68",
  "7aa2f7", "bb9af7", "7dcfff", "a9b1d6",
  "414868", "f7768e", "9ece6a", "e0af68",
  "7aa2f7", "bb9af7", "7dcfff", "c0caf5",
]
cursor = "c0caf5"
selection_background = "33467c"
selection_foreground = "c0caf5"
```

For colored text over wallpaper, Ghostty itself supports `minimum-contrast = 4.5` in your **normal Ghostty root config** (not in the Profile). Ghostty Wall preserves unrelated root-config settings. This is not currently a managed Profile setting.

### Terminal

Add any supported fields; omit those you do not want to manage:

```toml
[terminal]
font_size = 13.5
background_opacity = 0.92
background_blur_intensity = 20
cursor_style = "bar"
```

`font_size` is `1..=1000` with up to 3 decimal places; `background_opacity` is `0..=1` with up to 6. Blur intensity is an integer `0..=255`; cursor styles are `block`, `bar`, `underline`, `block_hollow`. `background_opacity` controls the terminal background; `wallpaper.opacity` controls image mixing separately. An empty `[terminal]` is invalid. Font family, keybindings, and other Ghostty settings remain in your normal Ghostty config.

## Preview, apply, browse, go back

```sh
ghostty-wall plan night         # Readable JSON, without changing files
ghostty-wall plan night --json  # Compact JSON for scripts; structured error on failure
ghostty-wall apply night        # Commit an Activation, then attempt Ghostty reload
ghostty-wall previous           # Replay predecessor from local History
ghostty-wall tui                # Browse Profiles and preview/apply them
```

TUI accepts `j`, `k`, `tab`, `enter`, `a`, `b`, `q` (type each key name then press Enter). Use `tui --seed HEX` for random-selection Profiles. Image preview works in supported Ghostty terminals; otherwise it falls back to text.

`previous` uses saved Environments and images: it works even when the original Source is gone. It records a new Activation. Repeated `apply`, even with identical settings, also records a new Activation. There is no `next`, `history`, or automatic cleanup command in v1. History is stored under `<Managed Root>/history/activations/` and assets under `assets/sha256/`; do not delete these to free cache space. Only `cache/` is disposable.

Reload is best-effort **after** commit: if Ghostty reports reload unavailable/failed, Activation remains committed. Check `ghostty-wall doctor`, ensure the Ghostty config-file hook is active, and reload Ghostty manually if needed. On Linux, reload uses the active systemd user service or the running GTK application's D-Bus action.

## Maintenance

```sh
ghostty-wall doctor                         # Read-only checks: verified / failed / unavailable
ghostty-wall init --repair                  # Explicit conservative layout/hook repair
ghostty-wall init --migrate-legacy --dry-run
ghostty-wall init --migrate-legacy          # Import recognized Bash v0 Sources/hooks
ghostty-wall uninstall                      # Remove hook and generated projection/cache
```

`--repair` does not recreate lost Profiles, Assets, Environments, or History. Legacy migration preserves legacy files and cannot infer Profiles; inspect imported Sources afterward. `uninstall` **preserves** Profiles and durable History/Assets; remove the binary separately if desired. For details, see [README](../README.md#migrate-bash-v0) and [README](../README.md#uninstall).
