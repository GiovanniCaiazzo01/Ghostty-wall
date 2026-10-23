# Ghostty Wall v1.3.0

Predictable, readable Profiles: review the result, save without applying, or explicitly use exactly what you reviewed.

## Highlights

- **Consistent Save / Save and use / Cancel:** full-screen Create/Edit share these actions, including compact layouts. Save does not activate; Save and use asks for confirmation, defaulting to Back to editor. Cancel discards unpublished changes.
- **Readable automatic colors:** `kmeans-v4` accounts for wallpaper brightness, alpha and explicit opacity when correcting foreground, ANSI text colors and cursor toward a 4.5:1 target. Customized colors remain untouched. Samples warn when image strength or customized colors prevent sufficient contrast.
- **Reviewed-result identity:** full-screen browser Use commits the captured image bytes, resolved colors and managed settings, without another Source/theme resolution. Changed watched inputs require a refreshed sample and another explicit Use. Loading, failed and obsolete previews cannot apply. Standalone CLI apply still resolves normally.
- **Source maintenance:** inspect definitions and dependent Profiles, edit locations, check candidate availability, and confirm removal of unused Sources. Source removal preserves original images, retained assets and History.
- **Less unnecessary work:** browser no longer redraws while idle; palette clustering stops at stable centers; editor opacity changes reuse image analysis. A controlled debug clustering case improved from 4.46 s to 1.02 s, not a universal theme-switch speed claim.

## Install and update

Prebuilt target: **Linux x86_64**. Verify the archive using its matching checksum:

- `ghostty-wall-v1.3.0-x86_64-unknown-linux-gnu.tar.gz`
- `ghostty-wall-v1.3.0-x86_64-unknown-linux-gnu.tar.gz.sha256`
- `ghostty-wall-x86_64-unknown-linux-gnu.tar.gz` (stable alias)
- `ghostty-wall-x86_64-unknown-linux-gnu.tar.gz.sha256`

Run `sha256sum --check FILE.tar.gz.sha256` beside the archive, extract it, then run the included `install.sh`. Initialization remains explicit: `ghostty-wall init`.

Existing installations: `ghostty-wall update --check`, then `ghostty-wall update`. Restart running processes afterward. Older Cargo installations can use:

```sh
cargo install --git https://github.com/GiovanniCaiazzo01/Ghostty-wall --tag v1.3.0 --locked
```

Preserve your original `--root` for a custom Cargo prefix. Source builds require Rust **1.88+**, a native linker, network access and temporary disk space. macOS remains experimental, without a supported installer or prebuilt artifact.

## Compatibility and limitations

- Existing version 1/2 Profiles remain readable and are not migrated. New resolutions of automatic colors use **kmeans-v4**. Saved History keeps replaying its original resolved colors, including v1–v3 provenance.
- **Back up the complete Managed Root before downgrading. Older binaries may reject History containing v4 Activations.**
- Contrast uses a conservative managed linear-sRGB model. It includes all source pixels, even highlights outside the current crop. Selection uses its solid background. Customized backgrounds may invalidate automatic contrast bounds. High opacity may make 4.5:1 impossible even with white text; lower opacity explicitly rather than expecting silent changes.
- Omitted opacity remains unknown: generation keeps flat-background correction; the sample illustrates 1.0. Native blending, Display P3, desktop transparency, overrides, inherited settings, faint/dim text and application-specific colors are not certified. This is not a universal accessibility guarantee.
- Internal samples remain approximate and are **not live Ghostty draft sessions**. Managed-content identity does not promise pixel-perfect effective rendering. Full-screen management supports 60×18; Create/Edit support 40×12.
- Activation and reload are separate outcomes. A failed/unavailable reload does not undo committed History. D-Bus reload was visibly checked on one and two owned Ghostty 1.3.1 windows; direct custom-config launches may require manual reload. A real systemd user-service launch was unavailable in the isolated test environment; automated adapter tests cover that path and failure reports.
- Updates replace Ghostty Wall, not Ghostty or user data. Keep other installers idle. Binary/metadata publication supports rollback but is not crash-atomic; interruption may require inspecting retained recovery files.

## Verification

[Validation record](https://github.com/GiovanniCaiazzo01/Ghostty-wall/blob/v1.3.0/docs/release-1.3.0-validation.md) distinguishes exact-candidate tests, local artifacts, observed Ghostty pixels, adapter acceptance and unverified routes. [Release plan #7](https://github.com/GiovanniCaiazzo01/Ghostty-wall/issues/7) links implementation PRs #13, #15, #16 and #17; integrated validation is tracked in #11.

The release workflow repeats formatting, all-target/all-feature Clippy and Rust tests, release-binary PTYs, Bash/installer checks, site checks, package checksums and disposable installed `--version`/`init`/`doctor` before uploading Linux assets. Published workflow artifacts have a different build identity from the local candidate and are verified separately.

[Documentation](https://giovannicaiazzo01.github.io/Ghostty-wall/) · [Changelog](https://github.com/GiovanniCaiazzo01/Ghostty-wall/blob/v1.3.0/CHANGELOG.md) · [Source](https://github.com/GiovanniCaiazzo01/Ghostty-wall/tree/v1.3.0)
