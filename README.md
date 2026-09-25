# Ghostty Wall

<img src="media/mascot.png" alt="Ghostty Wall mascot" width="180">

A CLI for managing Ghostty wallpapers, color palettes, and visual settings as reusable profiles.

Ghostty Wall can generate terminal colors from your wallpaper, preview changes before applying them, switch between profiles, and return to previous environments.

**Linux:** stable  
**macOS:** experimental  
**Windows:** unsupported

> Documentation: [website](https://giovannicaiazzo01.github.io/Ghostty-wall/) · [User Guide](docs/user-guide.md)

## Install

### Quick install

```bash
curl -fsSL https://raw.githubusercontent.com/GiovanniCaiazzo01/Ghostty-wall/main/scripts/install-release.sh | bash
```

Then initialize Ghostty Wall:

```bash
ghostty-wall init
```

### Install with Cargo

Requires Rust 1.85+:

```bash
cargo install --git https://github.com/GiovanniCaiazzo01/Ghostty-wall --locked
```

### Manual installation

Prebuilt Linux binaries and SHA-256 checksums are available from [GitHub Releases](https://github.com/GiovanniCaiazzo01/Ghostty-wall/releases).

## Quick start

Fresh installations include a `welcome` profile, so you can try Ghostty Wall immediately:

```bash
ghostty-wall init
ghostty-wall apply welcome
```

Browse your profiles from the terminal:

```bash
ghostty-wall tui
```

Or inspect exactly what a profile would do before applying it:

```bash
ghostty-wall plan welcome
```

## Commands

| Command | Description |
| --- | --- |
| `ghostty-wall init` | Initialize Ghostty Wall and add the Ghostty integration hook |
| `ghostty-wall init --dry-run` | Show what `init` would change without writing anything |
| `ghostty-wall init --repair` | Repair the managed layout and Ghostty integration |
| `ghostty-wall init --welcome` | Add the bundled welcome profile to an eligible empty installation |
| `ghostty-wall init --migrate-legacy` | Import recognized configuration from Ghostty Wall v0 |
| `ghostty-wall plan PROFILE` | Resolve and inspect a profile without changing anything |
| `ghostty-wall apply PROFILE` | Apply a profile and record it in local history |
| `ghostty-wall previous` | Return to the previous environment |
| `ghostty-wall tui` | Browse, preview, and apply profiles interactively |
| `ghostty-wall doctor` | Check the installation, integration, and durable state |
| `ghostty-wall uninstall` | Remove the integration and generated files while preserving profiles and history |
| `ghostty-wall update` | Check for and install the latest Ghostty Wall release |
| `ghostty-wall --help` | Show CLI usage |
| `ghostty-wall --version` | Show the installed version |

`plan`, `apply`, and `tui` accept `--seed HEX` when using profiles with random wallpaper selection.

`plan PROFILE --json` outputs compact machine-readable JSON.

Run `ghostty-wall update --check` to check for a newer release without installing it. Self-updates require a release-installer-owned binary; Cargo and manual installations should use their original installation method.

## Create a profile

Sources are configured in Ghostty Wall's `config.toml`.

For example, use a local wallpaper directory:

```toml
[sources.wallpapers]
kind = "local-directory"
path = "~/Pictures/wallpapers"
```

Then create `profiles/night.toml`:

```toml
schema_version = 1

[wallpaper]
mode = "source"
source = "wallpapers"
selection = "path"
path = "city/night.png"
fit = "cover"
position = "center"
opacity = 0.1

[colors]
mode = "generated"

[terminal]
font_size = 13.5
background_opacity = 0.94
cursor_style = "bar"
```

Preview it:

```bash
ghostty-wall plan night
```

Apply it:

```bash
ghostty-wall apply night
```

Ghostty Wall can also use GitHub wallpaper sources, named Ghostty themes, explicit color palettes, and deterministic random wallpaper selection.

See the [User Guide](docs/user-guide.md) for all configuration options.

## Terminal browser

```bash
ghostty-wall tui
```

The terminal browser lets you select a profile, preview it, and apply it without leaving the terminal.

Controls:

```text
j / k   navigate
tab     switch pane
enter   preview
a       apply
b       back
q       quit
```

Image previews use Ghostty's Kitty graphics protocol when available and fall back to text otherwise.

## How it works

A **Profile** describes the visual configuration you want.

Ghostty Wall resolves it into an immutable **Environment**, applies the managed Ghostty settings, and records an **Activation** in local history.

This means:

- profiles remain declarative;
- generated colors and selected assets are reproducible;
- previous environments can be restored even if their original source disappears;
- unrelated Ghostty configuration remains untouched.

## Platform support

### Linux

Stable.

Tagged releases currently provide a prebuilt `x86_64-unknown-linux-gnu` binary. Other Linux architectures can build from source.

### macOS

Experimental while real-system verification is completed.

See the [release checklist](docs/release-checklist.md) for current status.

## Migrating from v0

```bash
ghostty-wall init --migrate-legacy --dry-run
ghostty-wall init --migrate-legacy
```

Migration preserves legacy files and imports recognized wallpaper sources.

Ghostty Wall v0 remains available from the [`v0.2.2`](https://github.com/GiovanniCaiazzo01/Ghostty-wall/tree/v0.2.2) tag.

## Uninstall

Remove Ghostty Wall's integration and generated files:

```bash
ghostty-wall uninstall
```

This intentionally preserves your profiles, environments, assets, and history.

Then remove the binary using the same installation method you used to install it.

## Documentation

- [User Guide](docs/user-guide.md)
- [Changelog](CHANGELOG.md)
- [Release Checklist](docs/release-checklist.md)
- [RFCs](docs/rfc/)

## License

MIT — see [LICENSE](LICENSE).
