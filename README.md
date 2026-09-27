# Ghostty Wall

<img src="media/mascot.png" alt="Ghostty Wall mascot" width="180">

A CLI for managing Ghostty wallpapers, color palettes, and visual settings as reusable profiles.

Ghostty Wall can generate a darkened wallpaper-derived background, tinted text, cursor, selection, and ANSI colors with readable contrast, preview changes before applying them, switch between profiles, and return to previous environments. Monochrome images use black/white text when no image hue is available.

**Linux:** stable  
**macOS:** experimental  
**Windows:** unsupported

Live draft reload on Linux remains [unverified](docs/probes/live-preview-linux.md#ticket-10-preflight-2026-09-27), including cancel restoration and window scope. Editor and sidebar samples are not live reload.

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

Requires Rust 1.88+:

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

Create a complete Profile without editing TOML:

```bash
ghostty-wall create          # Choose an id, then generate a wallpaper or pick your PNG/JPEG
# Or: ghostty-wall create night   # Skip the id prompt
```

Choose **Save**, then **Use now** or **Not now**. Cancelling before Save writes no Profile; Not now keeps it saved without changing the terminal. Generated wallpaper stays fixed after saving.

Run `ghostty-wall` to open the management browser. On first launch, it offers initialization.

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
| `ghostty-wall` / `ghostty-wall tui` | Profile management: Create, Edit, Delete, Use, internal preview |
| `ghostty-wall create [PROFILE]` | Guided wallpaper and readable colors; save, then optionally use now (`create --help` for picker controls) |
| `ghostty-wall new PROFILE [IMAGE]` | Copy PNG/JPEG into a version 2 Profile with an ownership claim (add `--apply` to activate) |
| `ghostty-wall new PROFILE --generate SEED_HEX` | Save a one-time stable gradient-v1 PNG from a 64-character lowercase hex seed |
| `ghostty-wall source add SOURCE local DIRECTORY` | Register local wallpaper Source |
| `ghostty-wall source add SOURCE github OWNER/REPO` | Register GitHub Source (`--ref`, `--path` optional) |
| `ghostty-wall new PROFILE --source SOURCE --path CANDIDATE` | Create Profile from registered Source |
| `ghostty-wall edit [PROFILE]` | Keyboard visual draft editor; confirmed **Save and use** (`edit --help` for controls). Advanced `FIELD VALUE` still saves immediately. |
| `ghostty-wall duplicate PROFILE NEW` / `rename PROFILE NEW` | Manage Profile files; preserve History. Welcome and active Profiles cannot be renamed. |
| `ghostty-wall delete [PROFILE]` | Select or name a Profile, confirm (default: Cancel). Active deletion commits Welcome first; inactive deletion leaves the terminal unchanged. History and replay assets remain. |
| `ghostty-wall list` / `history` | List Profiles or committed Activations |
| `ghostty-wall preview PROFILE` | Human-readable wallpaper, colors, ANSI palette, and terminal options |
| `ghostty-wall plan PROFILE` | Resolve and inspect a profile without changing anything |
| `ghostty-wall apply PROFILE` | Apply a profile and record it in local history |
| `ghostty-wall previous` | Return to the previous environment |
| `ghostty-wall doctor` | Check the installation, integration, and durable state |
| `ghostty-wall uninstall` | Remove the integration and generated files while preserving profiles and history |
| `ghostty-wall update` | Check for and install the latest Ghostty Wall release |
| `ghostty-wall --help` | Show CLI usage |
| `ghostty-wall --version` | Show the installed version |

`plan`, `apply`, and `tui` accept `--seed HEX` when using profiles with random wallpaper selection.

`plan PROFILE --json` outputs compact machine-readable JSON.

Run `ghostty-wall update --check` to check for a newer release without installing it. Self-updates require a release-installer-owned binary; Cargo and manual installations should use their original installation method.

## Delete a Profile

`ghostty-wall delete boy` confirms `boy`; `ghostty-wall delete` shows a numbered selector with `[active]` marked. Enter, EOF, or `n` cancels; `y` confirms removal of the named Profile and only proven-exclusive owned image data. The TUI's `x` action uses the same flow. Welcome is protected. Active deletion commits existing Welcome before removal; fallback/reconciliation failure retains the Profile. Reload is best-effort, not proof of visible change. If removal fails after fallback, Welcome may remain active. Shared/reused/ambiguous images, originals, Sources, History, Environments, and Durable Assets are preserved. Identical-file reuse does not grant cleanup ownership, and deletion cannot follow a substituted `profiles/` symlink. Removal isolates and verifies the captured file before unlinking; unconfirmed replacements are retained. A Profile found again during removal or the renewed ownership check keeps its image and requires fresh deletion confirmation; committed Welcome fallback is not undone. Interrupted removal may leave files in `<Managed Root>/.tmp-delete-<token>/` for inspection, never automatic cleanup. After History replay, apply a Profile before deleting. Older installations without Welcome are not silently changed. See `delete --help` and the [deletion guide](docs/user-guide.md#delete-a-profile-safely).

## Create a profile

`new PROFILE IMAGE` handles single images; `new PROFILE --generate SEED_HEX` stores a one-time generated PNG and recipe. Version 2 generated colors support per-slot `edit PROFILE colors.background/foreground/cursor/selection_background/selection_foreground/palette.0..15 VALUE` (lowercase RGB hex or `auto` to reset); version 1 behavior is unchanged. Version 2 requires a compatible reader for rollback; back up Intent and History before downgrading. Use `source add`, `new PROFILE --source`, and `edit PROFILE` for directory/GitHub Sources and settings without opening files (examples in [User Guide](docs/user-guide.md)). `create [PROFILE]` uses an in-memory draft and inline prompts: generate once (explicit **Another variant** before Save only), or browse system Downloads/Pictures and copy a decoded PNG/JPEG. See the [guided creation controls](docs/user-guide.md#create-a-profile-without-editing-files). `edit [PROFILE]` opens the [keyboard visual editor](docs/user-guide.md#edit-a-profile-visually): Wallpaper, Colors, and Terminal controls, color samples/exact hex, numeric steps/direct entry, and image replacement using the creation picker. Nothing is saved until confirmed **Save and use**; declining returns to the intact draft. Esc/q cancels. The internal sample is approximate, not live reload. The full TUI offers the same guided Create and visual Edit drafts; advanced `edit PROFILE FIELD VALUE` and TUI field actions still save immediately. Browser/CLI previews are read-only and do not live-reload Ghostty. `apply`, including **Use now** after creation, commits an Activation and then attempts best-effort reload. The experimental Linux [library-only draft session](docs/user-guide.md#provisional-editor-sessions-library-only) (RFC 0009) captures the starting Environment, stages validated draft images temporarily, and requests reload on update/cancel/finish without saving Profiles or appending History. Cancel/recovery restores committed Projection and cleans owned temporary images; save and reload outcomes remain separate. It is not connected to the editor or TUI. Adapter success is not evidence of visible change or restoration, nor of which windows reloaded; isolated real-window verification belongs to ticket 10. TOML remains available for advanced workflows.

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
ghostty-wall preview night
```

Apply it:

```bash
ghostty-wall apply night
```

Ghostty Wall can also use GitHub wallpaper sources, named Ghostty themes, explicit color palettes, and deterministic random wallpaper selection.

See the [User Guide](docs/user-guide.md) for all configuration options.

## Profile management center

```bash
ghostty-wall tui
```

The full-screen TUI opens on Profiles, marks the latest Profile Activation `[active]`, and shows the selected Profile's terminal-like sample. Moving selection never activates. History replay has no active-Profile marker.

```text
n       Create: generate or choose image → review → Save → Use now / Not now
e       Edit visual draft: Wallpaper / Colors / Terminal → Save and use
x       Delete with default-cancel confirmation
a       Use selected Profile; remain in the management center
↑ / ↓   select (j/k also work)
Enter/p toggle internal sample/list on small terminals
Tab     switch Profiles / advanced Sources
?       Actions: Sources, History, Previous, settings, maintenance, advanced edits
v       scrollable result/error details
q       quit
```

Wide Create/Edit forms show the sample alongside controls. At 40×12 and larger, smaller layouts switch between form/list and sample rather than squeezing panels. Samples include wallpaper/background blending, normal text, ANSI colors, cursor and selection; they are approximate, **not live Ghostty reload**. Creation uses the same localized image picker as the CLI (line prompts temporarily leave full screen). Edit cancellation discards only the draft. Save, Activation and best-effort reload remain separate outcomes.

The Actions menu retains Source creation, duplicate/rename, Plan JSON, Previous, Doctor, updates, initialization/repair, migration and Uninstall. Advanced `N` imports an image directly; `f/c/t/w` edit fields immediately. Maintenance and deletion use line prompts; destructive operations confirm explicitly. Run `ghostty-wall tui --help` for controls. `i` shows the wallpaper via Ghostty's Kitty graphics protocol, with a text fallback elsewhere. Non-terminal input retains the legacy line-based browser (`n` direct image import and `e` field edit, key then Enter).

For local source builds use `cargo run --release --` (not an older installed `ghostty-wall`); release mode keeps image previews responsive. Full-screen TUI verification: `cargo build --locked && python3 -m unittest discover -s tests -p management_pty.py -v` (also `tui_pty.py` for advanced actions); tests isolate HOME and XDG_CONFIG_HOME and drive a real pseudo-terminal.

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
