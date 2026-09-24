# Ghostty Wall

Ghostty Wall v1 is a Rust CLI for reproducible Ghostty visual Environments. Profiles resolve wallpaper Sources, colors, and supported terminal settings into immutable Environments with durable local History.

Linux support is stable. macOS support is **experimental** pending real-system verification in issue 14. Windows is unsupported.

## Install

### Linux release artifact

Download `ghostty-wall-v1.0.0-x86_64-unknown-linux-gnu.tar.gz` and its `.sha256` file from the GitHub Release, then:

```bash
sha256sum --check ghostty-wall-v1.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf ghostty-wall-v1.0.0-x86_64-unknown-linux-gnu.tar.gz
cd ghostty-wall-v1.0.0-x86_64-unknown-linux-gnu
./install-v1.sh
```

Default destination is `~/.local/bin/ghostty-wall`. Set `INSTALL_PREFIX` to choose another prefix:

```bash
INSTALL_PREFIX=/usr/local ./install-v1.sh
```

### Build from source

Rust 1.85 or newer is required for edition 2024.

```bash
git clone https://github.com/GiovanniCaiazzo01/Ghostty-wall.git
cd Ghostty-wall
./scripts/install-v1.sh
```

Installer builds with `cargo build --locked --release` when no release binary is present. On macOS it prints an experimental-support warning.

## First workflow

Initialize Managed Root and one optional Ghostty Integration Hook:

```bash
ghostty-wall init --dry-run
ghostty-wall init
```

Linux Managed Root:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/ghostty-wall
```

Create local Source in `config.toml`:

```toml
schema_version = 1

[sources.wallpapers]
kind = "local-directory"
path = "~/Pictures/wallpapers"
```

Create `profiles/night.toml`:

```toml
schema_version = 1

[wallpaper]
mode = "source"
source = "wallpapers"
selection = "path"
path = "city/night.png"
fit = "cover"
position = "center"
opacity = 0.12

[colors]
mode = "generated"

[terminal]
font_size = 13.5
background_opacity = 0.94
cursor_style = "bar"
```

Plan without mutation, apply, inspect health, then navigate History:

```bash
ghostty-wall plan night --json
ghostty-wall apply night
ghostty-wall doctor
ghostty-wall previous
```

`apply` commits Asset, Environment, Projection, and Activation before attempting best-effort Ghostty reload. Reload failure never rolls back committed state.

## Profiles

Profiles live at `profiles/<profile-id>.toml`. Supported v1 inputs:

- local-directory and commit-pinned GitHub wallpaper Sources;
- fixed-path or explicit-seed `random-v1` Selection;
- unmanaged, disabled, or managed wallpaper;
- explicit colors, named Ghostty themes, or deterministic `kmeans-v1` generated colors;
- supported font size, opacity, blur, and cursor fields.

Random Selection requires explicit 32-byte hexadecimal seed:

```bash
ghostty-wall plan rotating --seed 000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f --json
ghostty-wall apply rotating --seed 000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
```

GitHub Sources use `GITHUB_TOKEN` when set. Tokens remain transport-only and are never persisted in Plans, Intent, History, logs, or diagnostics.

### Generated colors

Generated colors require managed source wallpaper:

```toml
[wallpaper]
mode = "source"
source = "wallpapers"
selection = "path"
path = "city/night.png"

[colors]
mode = "generated"
```

Resolved colors become Environment content. Algorithm provenance stays in Plan and Activation metadata.

### Named Ghostty theme

```toml
[colors]
mode = "theme"
theme = "TokyoNight"
```

Ghostty Wall resolves theme into managed colors before commit, so replay does not depend on theme file remaining installed.

## Terminal browser

```bash
ghostty-wall tui
# Random Profiles:
ghostty-wall tui --seed 000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
```

Commands are `j`, `k`, `tab`, `enter`, `a`, `b`, and `q`, each followed by Enter. Preview uses Ghostty's Kitty graphics protocol when available and falls back to text. TUI planning and apply use same application services as CLI commands.

## Migrate Bash v0

Migration is explicit, idempotent, and preserves all legacy files:

```bash
ghostty-wall init --migrate-legacy --dry-run
ghostty-wall init --migrate-legacy
```

Recognized `wallpaper_repos.txt` entries become v1 GitHub Sources. Migration removes only strictly recognized legacy Integration Hooks after v1 state is valid. Review imported Sources, then create Profiles manually; migration cannot infer Profile intent.

Bash v0 remains available from repository tag [`v0.2.2`](https://github.com/GiovanniCaiazzo01/Ghostty-wall/tree/v0.2.2). Existing v0 users can stay pinned while validating v1 migration.

## Uninstall

First remove v1 Integration Hooks and disposable Projection/cache while preserving Intent, Profiles, Assets, Environments, and History:

```bash
ghostty-wall uninstall
```

Then remove binary using installation method:

```bash
rm "$HOME/.local/bin/ghostty-wall"       # default install-v1.sh destination
cargo uninstall ghostty-wall              # cargo install
```

There is no destructive `--purge` in v1. Reinstall plus `ghostty-wall init` recognizes preserved state.

## Platform support

### Linux — stable

- Managed Root under XDG Ghostty config.
- Runtime reload through documented `app-com.mitchellh.ghostty.service` systemd user service.
- Tagged releases publish x86_64 GNU/Linux binary archive and SHA-256 checksum.
- Source builds remain available for other Linux architectures.

### macOS — experimental

- Managed Root under `~/Library/Application Support/com.mitchellh.ghostty/ghostty-wall`.
- Reload through Ghostty AppleScript `perform action "reload_config"` API.
- Automated adapter tests verify running, unavailable, probe-failure, and reload-failure outcomes without changing durable Activation success.
- Real Ghostty smoke-test procedure is in [`docs/release-checklist.md`](docs/release-checklist.md); real-system promotion remains issue 14.

## Verification

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
bash scripts/test.sh
shellcheck -x bin/ghostty-wall scripts/*.sh scripts/linux/*.sh scripts/mac/*.sh
```

Live GitHub test is separate:

```bash
cargo test --test github_live -- --ignored
```

See [`docs/release-checklist.md`](docs/release-checklist.md), [`CHANGELOG.md`](CHANGELOG.md), and accepted contracts under [`docs/rfc/`](docs/rfc/).

## License

MIT — see [`LICENSE`](LICENSE).
