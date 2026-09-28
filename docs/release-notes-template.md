# Ghostty Wall v1.2.0

Six UX improvements make everyday Profile management more responsive and keep operations inside the terminal interface.

## Highlights

- **Automatic Profile previews:** initial selection and arrow navigation prepare the wallpaper and matching colors in the background. Ghostty gets a full-color, linear-light composited sample; obsolete results cannot replace the current selection. Navigation and quit remain available while loading.
- **Full-screen Profile workflows:** create, edit, image picking, rename, duplicate and delete stay inside the TUI. Long paths and errors have scrollable details.
- **Cargo and release-installer updates:** update the invoked installation, including custom prefixes, with verified staging, ownership checks and executable/metadata rollback. Cargo builds the official stable tag rather than overwriting another installation on PATH.
- **Contained maintenance:** initialization, reports, History, Doctor and updates show full-screen progress/results, explicit consent and clear cancellation boundaries. Restart Ghostty Wall after an update.
- **Quieter new wallpapers:** new image Profiles and newly initialized Welcome default to wallpaper opacity **0.05**, previously 0.1. Explicit draft choices win. Existing Profiles, omitted settings, edits, duplicates, replay and terminal transparency are unchanged.
- **Clearer CLI choices:** separated headings/options, explicit defaults, wrapped paths and restrained terminal colors. `NO_COLOR`, redirected interaction and JSON retain plain output.

The management browser supports **60×18** and larger; Create/Edit forms retain their separate **40×12** minimum. Archive decompression limits now include tar headers and extension records before parsing.

## Install and update

Prebuilt target: **Linux x86_64**. Download and verify either archive with its matching checksum:

- `ghostty-wall-v1.2.0-x86_64-unknown-linux-gnu.tar.gz`
- `ghostty-wall-v1.2.0-x86_64-unknown-linux-gnu.tar.gz.sha256`
- `ghostty-wall-x86_64-unknown-linux-gnu.tar.gz` (stable alias)
- `ghostty-wall-x86_64-unknown-linux-gnu.tar.gz.sha256`

Run `sha256sum --check FILE.tar.gz.sha256` beside the downloaded archive, extract it, then run the included `install.sh`. Initialization remains explicit: `ghostty-wall init`.

Existing release-installer users can run `ghostty-wall update --check` then `ghostty-wall update`. To upgrade an older Cargo installation that does not yet support self-update:

```sh
cargo install --git https://github.com/GiovanniCaiazzo01/Ghostty-wall --tag v1.2.0 --locked
```

Preserve your original `--root` if using a custom Cargo prefix. Cargo builds require Rust **1.88+**, a native linker, network access and temporary disk space. From v1.2.0, supported Cargo installations can use `ghostty-wall update` for subsequent releases.

[Updated documentation](https://giovannicaiazzo01.github.io/Ghostty-wall/) · [Changelog](https://github.com/GiovanniCaiazzo01/Ghostty-wall/blob/v1.2.0/CHANGELOG.md) · [Source at v1.2.0](https://github.com/GiovanniCaiazzo01/Ghostty-wall/tree/v1.2.0)

## Compatibility and limitations

- No persisted-format migration or seed-algorithm change. Existing version 1/2 Profiles and History remain readable. Back up the complete Managed Root before downgrading; older readers may not understand version 2 Intent or newer generated-color provenance.
- Linux is supported; **macOS remains experimental**. Native macOS and non-x86_64 updater runtime have not been verified. Source-build availability does not imply a prebuilt artifact or verified platform behavior.
- Samples are approximate, read-only and **not live Ghostty reload**. Unsupported terminals show a labelled color-cell fallback, not a photograph. Native blending, Display P3, user configuration, fonts, blur and desktop transparency can differ. Lower image opacity fades toward the background; a light background can become lighter rather than darker.
- Updates change Ghostty Wall, not Ghostty or managed user data. Keep other installers idle. Failed publication rolls back binary and metadata, but this is **not a crash-atomic multi-file transaction**. Forced termination/power loss or failed rollback may require inspecting retained `.ghostty-wall-update-*` recovery files. Restart running processes after replacement.

## Verification

The integrated runtime passed Rust formatting, all-target/all-feature Clippy/tests, 97 release-binary PTYs, targeted deletion/publication/updater fault tests, Bash/installer checks and generated-site checks. Ten blocked-preview trials measured current-selection response below 21 ms and exit below 1 ms on the test machine; these are observations, not universal performance guarantees.

Independent Linux Ghostty 1.3.1 checks inspected 28 captures at compact/wide sizes, including fixed/random photographs, managed colors, nested reports, original-image view and below-minimum guidance. Those captures identify the **pre-version-bump integrated binary**, not the downloadable release binary; runtime source is unchanged by the version bump. No private screenshots or logs are shipped.

The release workflow repeats formatting, Clippy, Rust tests, Bash/installer and site checks, builds the locked versioned binary, runs release PTYs, verifies both package checksums and checks disposable installation, `--version`, `init` and `doctor` before uploading assets. Public workflow results are available under [GitHub Actions](https://github.com/GiovanniCaiazzo01/Ghostty-wall/actions).
