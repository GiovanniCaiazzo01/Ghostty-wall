# Ghostty Wall

Set a Ghostty wallpaper, generate matching terminal colors, and save looks you can switch between.

![Ghostty running the bundled welcome wallpaper and matching colors](media/screenshots/welcome.png)

*Real Ghostty window using the bundled `welcome` profile.*

![Ghostty Wall TUI with the welcome profile selected](media/screenshots/profile-browser.png)

*Profile browser running in Ghostty. Internal preview, not live Ghostty reload.*

## Install — Linux x86_64

Requires Ghostty, Bash, curl, tar and sha256sum. No Rust toolchain needed.

[Inspect the installer](scripts/install-release.sh) before running it:

```bash
curl -fsSL https://raw.githubusercontent.com/GiovanniCaiazzo01/Ghostty-wall/main/scripts/install-release.sh | bash
```

Installs the latest stable release into `~/.local/bin` and verifies its SHA-256 checksum. Make sure that directory is on your PATH. [Installation help and PATH conflicts](https://giovannicaiazzo01.github.io/Ghostty-wall/installation/).

Linux x86_64 is supported. macOS is experimental with no supported installer; other Linux architectures have no supported installer. Windows is unsupported. The curl installer is the supported installation route.

## Quick start

```bash
ghostty-wall init
ghostty-wall apply welcome
ghostty-wall create
```

Generate in Create/Edit now varies palettes and spatial composition (tiles, ribbons or soft fields), saved once as `composition-v1`. Existing images stay unchanged; advanced `new --generate` retains `gradient-v1`.

Wallpaper controls now offer reversible image blur and visible, adjustable black-overlay darkening. New guided/imported Profiles use role-aware `oklch-v2` colors; existing algorithms (including `oklch-v1` and v4) stay unchanged unless explicitly reset. Infeasible automatic contrast is reported in plans, previews, editor and Apply results: v2 keeps distinct roles as a **non-conforming fallback**, not a valid 4.5:1 palette. Originals, custom colors and captured History stay intact. See [effects and compatibility](docs/advanced-usage.md#reversible-image-blur-and-darkening).

The first two commands try the bundled wallpaper. `create` lets you generate a wallpaper or choose your own PNG/JPEG, then save and optionally use it. If Ghostty does not change, reload its configuration manually.

Run `ghostty-wall` to browse your saved looks. Select a profile to preview it; choose **Use** to apply that prepared image, colors and managed settings. Changed local inputs require a refreshed preview and another Use; loading/failed previews cannot apply. Create/Edit drafts offer `s` **Save** without applying, `u` **Save and use** with confirmation, and Esc/`q` **Cancel**. Editor and browser samples are internal previews, not live Ghostty reload. Browsing and opening Edit do not wait for a full History audit; the active marker stays pending until validation completes. Use still validates durable state. Two recent local previews reuse prepared images in a bounded, in-memory cache; changed watched inputs invalidate them.

Wide layouts put the preview first: compact Profile navigation, a dominant editor preview on the left, contextual controls on the right with visible group/focus, and a short action footer. Compact editors show either Controls or Preview, switched with `p`; keyboard controls remain available. Ghostty editor previews reuse the browser renderer with a reduced image source and asynchronous updates; other terminals show an explicit color-cell fallback. Control position and Home/End hints expose scrolling. Editor details (`v`) explain limitations even before Preview is prepared, without inventing a sampled contrast result. Previews remain approximate, not live reload.

Screen transitions group clearing and redrawing with synchronized output where supported; preview images still arrive asynchronously. Browser/editor chrome uses explicit dark RGB colors, independent of the terminal's ANSI palette and the draft being previewed. Editor headings and action shortcuts are emphasized; navigation hints and separators stay secondary, while results/errors retain normal text contrast. This does not change the host window: Ghostty opacity, background wallpaper, padding, shaders and desktop compositing remain terminal-owned. In particular, `background-opacity-cells = true` can make even explicit RGB backgrounds translucent; the TUI does not override it or modify personal configuration.

[Watch the short browser walkthrough](media/screenshots/profile-browser.gif) (captured frames from a real Ghostty window, earlier layout; internal previews, not live wallpaper switching).

Automatic colors now account for wallpaper brightness and opacity. If the sample warns about low contrast, reduce image opacity or adjust customized colors; these are never overwritten automatically. Existing History replays unchanged. See [readability limits](docs/advanced-usage.md#readability).

## Useful commands

```bash
ghostty-wall edit           # Customize a saved look
ghostty-wall source list    # Inspect configured wallpaper sources
ghostty-wall previous       # Restore the previous look
ghostty-wall update         # Update Ghostty Wall, not Ghostty
ghostty-wall uninstall      # Remove integration; keep profiles and history
```

## Documentation

- [Quick start](https://giovannicaiazzo01.github.io/Ghostty-wall/quick-start/)
- [User guides and command reference](https://giovannicaiazzo01.github.io/Ghostty-wall/)
- [Troubleshooting and recovery](https://giovannicaiazzo01.github.io/Ghostty-wall/troubleshooting/)
- [Source maintenance: show, edit, check and safe removal](https://giovannicaiazzo01.github.io/Ghostty-wall/wallpapers/#maintenance)
- [Advanced behavior and safety notes](docs/advanced-usage.md)
- [Changelog](CHANGELOG.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for branch conventions, PR descriptions and development checks. Pick a task from the [release plan and roadmap](https://github.com/GiovanniCaiazzo01/Ghostty-wall/issues/7), or open an issue to discuss a larger change.

MIT — see [LICENSE](LICENSE).
