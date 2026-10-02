# Ghostty Wall

<img src="media/mascot.png" alt="Ghostty Wall mascot" width="180">

A CLI for managing Ghostty wallpapers, color palettes, and visual settings as reusable profiles.

Ghostty Wall can generate a darkened wallpaper-derived background, tinted text, cursor, selection, and ANSI colors with readable contrast, preview changes before applying them, switch between profiles, and return to previous environments. Monochrome images use black/white text when no image hue is available.

- **Linux x86_64:** stable; curl installer
- **macOS:** experimental; no supported installer
- **Windows:** unsupported

Live draft reload on Linux remains [unverified](https://giovannicaiazzo01.github.io/Ghostty-wall/troubleshooting/#reload), including cancel restoration and window scope. Editor and sidebar samples are not live reload.

> Documentation: [website](https://giovannicaiazzo01.github.io/Ghostty-wall/) · [Quick start](https://giovannicaiazzo01.github.io/Ghostty-wall/quick-start/)

## Install

The curl installer is currently the **only supported installation method**, for **Linux x86_64**. You need Ghostty, Bash, curl, tar and sha256sum. No Rust toolchain is needed.

```bash
curl -fsSL https://raw.githubusercontent.com/GiovanniCaiazzo01/Ghostty-wall/main/scripts/install-release.sh | bash
```

The installer downloads the latest stable release, verifies its SHA-256 checksum and installs `~/.local/bin/ghostty-wall` by default. `INSTALL_PREFIX` selects a different prefix. Before initializing or updating, check which executable your shell selects:

```bash
command -v ghostty-wall
ghostty-wall --version
```

An older Cargo or manual installation, such as `~/.cargo/bin/ghostty-wall`, can take precedence in PATH. The curl installer does not remove or upgrade copies in other directories. Put `~/.local/bin` before the old directory in your shell's PATH, then restart your shell; see [PATH conflict guidance](https://giovannicaiazzo01.github.io/Ghostty-wall/installation/#path-conflicts). You can check the new binary directly with `~/.local/bin/ghostty-wall --version` (adjust the path for a custom prefix). Your Profiles and History are unaffected.

Then initialize Ghostty Wall:

```bash
ghostty-wall init
```

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

Run `ghostty-wall update --check` to check the latest stable release without installing or changing user data. `ghostty-wall update` updates the curl-managed installation, including custom prefixes; it updates the executable being invoked, not another copy on PATH. Check the selected path as described under [Install](#install) if an older version still runs. Updates target Ghostty Wall, not Ghostty itself. They never run automatically or change Profiles, Sources, History, Environments, Durable Assets or Ghostty integration.

The updater downloads the version-pinned official Linux x86_64 archive, verifies SHA-256, validates its bounded archive layout/platform and executable version, then refreshes the binary and `.ghostty-wall-release.sha256` together.

Progress distinguishes checking, preparing, verifying and installing. Already-current versions are a no-op. Keep other installers idle during an update. Missing tools, read-only directories, unsupported atomic-exchange filesystems, inconsistent ownership, symlink substitutions and unowned/manual binaries fail without privilege escalation. Repair missing/mismatched ownership with the original installer rather than removing the safeguards. Failed preparation leaves the old installation untouched; failed publication rolls back executable and metadata. If rollback itself fails, the error identifies retained recovery files: inspect these and the installation before retrying. Publication is not a crash-atomic multi-file transaction; forced termination/power loss during installation may require recovery from `.ghostty-wall-update-*` files. Restart Ghostty Wall after replacement, especially an open TUI: an already-running process retains its original version. See `ghostty-wall update --help`.

## Inline CLI choices

Standalone prompts separate headings, options and the input question. Type the displayed key or number, then Enter. Defaults are explicit: after Save, Enter means **Not now**; deletion defaults to **Cancel**. Creation choices without a default ask again on invalid input. Existing aliases and EOF/cancellation behavior are unchanged.

```text
Wallpaper:

  [g] Generate wallpaper
  [i] Image from Downloads/Pictures
  cancel  Discard draft

Choice (g/i/cancel; no default):
```

Headings, choices, warnings, retry errors and success use restrained semantic colors only when both input and output are terminals with a non-dumb `TERM`. Set `NO_COLOR=1` (even an empty `NO_COLOR` disables styling) for plain prompts. Redirected/piped interaction stays plain; `plan --json`, normal command output and stderr routing are unchanged. Long prompt labels and image-list paths wrap with indented continuations at terminal width without truncating their contents. Full-screen TUI controls are separate from these line prompts.

## Delete a Profile

`ghostty-wall delete boy` confirms `boy`; `ghostty-wall delete` shows a numbered selector with `[active]` marked. Enter, EOF, or `n` cancels; `y` confirms removal of the named Profile and only proven-exclusive owned image data. The TUI's `x` action uses the same flow. Welcome is protected. Active deletion commits existing Welcome before removal; fallback/reconciliation failure retains the Profile. Reload is best-effort, not proof of visible change. If removal fails after fallback, Welcome may remain active. Shared/reused/ambiguous images, originals, Sources, History, Environments, and Durable Assets are preserved. Identical-file reuse does not grant cleanup ownership, and deletion cannot follow a substituted `profiles/` symlink. Removal isolates and verifies the captured file before unlinking; unconfirmed replacements are retained. A Profile found again during removal or the renewed ownership check keeps its image and requires fresh deletion confirmation; committed Welcome fallback is not undone. Interrupted removal may leave files in `<Managed Root>/.tmp-delete-<token>/` for inspection, never automatic cleanup. After History replay, apply a Profile before deleting. Older installations without Welcome are not silently changed. See `delete --help` and the [deletion guide](https://giovannicaiazzo01.github.io/Ghostty-wall/profiles/#delete).

## Create a profile

New image Profiles start with **wallpaper opacity `0.05`** (previously `0.1`), including guided CLI/TUI creation, direct imports, generated images, Source-based creation, and a newly installed Welcome Profile. This subdues the image behind text; it does **not** change terminal transparency (`terminal.background_opacity`) or text colors. A light theme may become lighter rather than darker as the image fades toward its background. Existing Profiles (including omitted opacity), edits, duplicates, and replayed Environments retain their settings; no migration occurs. Explicit draft opacity choices take precedence. To customize a saved Profile, use `ghostty-wall edit PROFILE wallpaper.opacity 0.2` or the visual editor.

For a reproducible **static compositing study**, run `bash scripts/compare-wallpaper-opacity.sh /tmp/gw-opacity-comparison` with ImageMagick installed. It compares `0.1`, `0.075`, and `0.05` with sample text, light/dark versions of the bundled artwork, and light/dark backgrounds. This is not a Ghostty screenshot or proof of photographic fidelity. Separate isolated Ghostty comparisons with mountain/forest photographs support `0.05` as less distracting on dark backgrounds; light backgrounds fade the photo rather than darkening it, and low-contrast text colors still need a suitable theme.

`new PROFILE IMAGE` handles single images; `new PROFILE --generate SEED_HEX` stores a one-time generated PNG and recipe. Version 2 generated colors support per-slot `edit PROFILE colors.background/foreground/cursor/selection_background/selection_foreground/palette.0..15 VALUE` (lowercase RGB hex or `auto` to reset); version 1 behavior is unchanged. Version 2 requires a compatible reader for rollback; back up Intent and History before downgrading. Use `source add`, `new PROFILE --source`, and `edit PROFILE` for directory/GitHub Sources and settings without opening files (examples in [Wallpaper sources](https://giovannicaiazzo01.github.io/Ghostty-wall/wallpapers/)). `create [PROFILE]` uses an in-memory draft and inline prompts: generate once (explicit **Another variant** before Save only), or browse system Downloads/Pictures and copy a decoded PNG/JPEG. See the [guided creation controls](https://giovannicaiazzo01.github.io/Ghostty-wall/profiles/#create). `edit [PROFILE]` opens the [keyboard visual editor](https://giovannicaiazzo01.github.io/Ghostty-wall/profiles/#edit): Wallpaper, Colors, and Terminal controls, color samples/exact hex, numeric steps/direct entry, and image replacement using the creation picker. Nothing is saved until confirmed **Save and use**; declining returns to the intact draft. Esc/q cancels. The internal sample is approximate, not live reload. The full TUI offers the same guided Create and visual Edit drafts; advanced `edit PROFILE FIELD VALUE` and TUI field actions still save immediately. Browser/CLI previews are read-only and do not live-reload Ghostty. `apply`, including **Use now** after creation, commits an Activation and then attempts best-effort reload. The experimental Linux [library-only draft session](https://giovannicaiazzo01.github.io/Ghostty-wall/troubleshooting/#reload) captures the starting Environment, stages validated draft images temporarily, and requests reload on update/cancel/finish without saving Profiles or appending History. Cancel/recovery restores committed Projection and cleans owned temporary images; save and reload outcomes remain separate. It is not connected to the editor or TUI. Adapter success is not evidence of visible change or restoration, nor of which windows reloaded; isolated real-window verification is still needed. TOML remains available for advanced workflows.

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
opacity = 0.05 # wallpaper strength; not terminal transparency

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

See the [Configuration reference](https://giovannicaiazzo01.github.io/Ghostty-wall/configuration/) for managed settings.

## Profile management center

```bash
ghostty-wall tui
```

The full-screen TUI opens on Profiles, marks the latest Profile Activation `[active]`, and automatically previews the initial and every arrow-selected Profile. Preparation runs off the input loop: navigation and quit remain available while loading. Moving selection never activates; only **Use** does. History replay has no active-Profile marker.

```text
n       Create: generate or choose image → review → Save → Use now / Not now
e       Edit visual draft: Wallpaper / Colors / Terminal → Save and use
x       Delete with default-cancel confirmation
a       Use selected Profile; remain in the management center
↑ / ↓   select and automatically preview (j/k also work)
Enter/p enlarge/restore the internal sample on small terminals
Tab     switch Profiles / advanced Sources
?       Actions: Sources, History, Previous, settings, maintenance, advanced edits
v       scrollable result/error details
q       quit
```

The management browser shows both list and automatic sample from **60×18** upward, leaving photo space below the representative text. Smaller windows show resize guidance and allow cancellation. Compact lists elide long IDs, show the selected ID in the header, and mark the active Profile with `*`. Wide Create/Edit forms show the sample alongside controls; compact Create/Edit forms still support **40×12** and retain their sample toggle. Samples include wallpaper/background blending, normal text, ANSI colors, cursor and selection; they are approximate, **not live Ghostty reload**. Creation and visual editing use a full-screen localized Downloads/Pictures picker: arrows select, Enter opens, `d`/`p` then Enter switches roots, `/text` then Enter searches, and `path:/absolute/path` opens an exact path. Esc or Ctrl-C returns to the draft without importing. Edit cancellation discards only the draft. Save, Activation and best-effort reload remain separate outcomes.

In Ghostty the management preview sends a bounded, full-color image behind representative text using the Kitty graphics protocol. Managed wallpaper fit, position, repeat and opacity are composed with the matching resolved colors (including random Profiles). Wallpaper is blended in **linear-light sRGB**, preserving saved opacity/RGB and image alpha, then encoded back to sRGB. This models Ghostty's `linear` / `linear-corrected` blending (the Linux default), not an inferred effective setting: `native` blending, Display P3 and user overrides can differ. An opacity of zero really hides the photo; arbitrarily transparent or low-contrast recipes cannot guarantee recognition. Unsupported terminals show a labelled color-cell fallback, **not a recognizable photograph**. Unmanaged colors and omitted settings cannot reveal your effective Ghostty configuration: neutral colors and cover/center/no-repeat/opacity 1.0 illustrate inherited values, never claim to determine them. `v` explains the limitations; font, blur and desktop transparency are not simulated. The sample uses synthetic 8×16-pixel cells, so unscaled image geometry is approximate.

Only one preparation runs at a time, with one replaceable pending selection; obsolete results cannot replace the current pane. Repainting does not resolve again, and resize reuses the resolved image while its local inputs are unchanged. Successful samples watch local Profile/config/selected-image/source-root metadata for changes; reselect to retry errors or refresh remote content. No persisted preview cache is created. Random preview and Use share a session seed, but Use resolves again: changed Sources can change the candidate. CLI seed rules are unchanged.

The Actions menu retains Source creation, duplicate/rename, Plan JSON, Previous, Doctor, updates, initialization/repair, migration and Uninstall. Profile actions remain full-screen, including direct import (`N`), Source-based creation (`m` or Enter on a Source), rename/duplicate (`r`/`d`), and immediate field edits (`f/c/t/w`, without applying). Input forms retain values on errors: Enter advances/submits the last field, Tab/Shift-Tab revisits fields, Ctrl-U clears, F1 shows full errors, and Esc/Ctrl-C/Ctrl-D cancels. Delete shows the exact removal summary with arrows to scroll; only `y` confirms, while Enter/n/Esc/Ctrl-C cancels. Maintenance, first-start initialization, image views and reports also stay full-screen. Reports (including Plan JSON, Settings and Source configuration) scroll with arrows/PageUp/PageDown; Enter/Esc returns. Source creation includes nested GitHub ref/subdirectory fields; Source administration supports add/inspect, not rename/remove. Initialization, repair, Welcome creation, Previous, migration, update installation and uninstall require explicit `y`; Enter/n/Esc/Ctrl-C/Ctrl-D declines. Uninstall preserves Intent/History and returns to the browser, where Initialize/Repair remains available. Long reports and maintenance run off the input thread. Esc can close a read-only job (work may finish in the background); another maintenance read-only job must wait for it to finish. Reports are capped at 4 MiB with truncation disclosed. After mutation starts, cancellation is unavailable until completion or recovery, and the screen says so. Update progress/errors use the same updater as the CLI; after replacement, restart Ghostty Wall—the running process still uses its original version. Run `ghostty-wall tui --help` for controls. `i` opens the original wallpaper image inside the interface via Ghostty's Kitty graphics protocol, with a text fallback elsewhere; it is not the Profile-opacity sample or a live reload. Non-terminal input retains the legacy line-based browser (`n` direct image import and `e` field edit, key then Enter).

For contributor testing in a local checkout, use `cargo run --release --` (not an older installed `ghostty-wall`); release mode keeps image previews responsive. Full-screen TUI verification: `cargo build --locked && python3 -m unittest discover -s tests -p management_pty.py -v` (also `responsive_preview_pty.py` for blocked preparation/automatic graphics and `tui_pty.py` for advanced actions); tests isolate HOME and XDG_CONFIG_HOME and drive a real pseudo-terminal. PTY protocol assertions are not Ghostty visual proof.

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

The curl installer uses the prebuilt `x86_64-unknown-linux-gnu` release. Other Linux architectures have no supported installer at this time.

### macOS

Experimental code remains in the project, but there is no supported macOS installer at this time. The curl installer is Linux x86_64 only.

See [installation and platform support](https://giovannicaiazzo01.github.io/Ghostty-wall/installation/#requirements) and contribute [macOS compatibility feedback](https://github.com/GiovanniCaiazzo01/Ghostty-wall/issues/5).

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

- [User documentation](https://giovannicaiazzo01.github.io/Ghostty-wall/)
- [Command reference](https://giovannicaiazzo01.github.io/Ghostty-wall/commands/)
- [Maintenance and recovery](https://giovannicaiazzo01.github.io/Ghostty-wall/troubleshooting/)
- [Changelog](CHANGELOG.md)
- [Domain glossary](CONTEXT.md)

## License

MIT — see [LICENSE](LICENSE).
