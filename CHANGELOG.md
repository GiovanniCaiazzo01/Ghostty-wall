# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, adapted to the current size of this repository.

## [Unreleased]

## [1.4.0] - Unreleased

### Added

- Varied, seed-repeatable `composition-v1` wallpapers, saved until explicit regeneration.
- Reversible `image-effects-v1` wallpaper blur and adjustable linear-light darkening.
- Role-aware `oklch-v2` palettes with explicit contrast-infeasibility diagnostics.

### Changed

- Responsive browsing/editing with bounded asynchronous previews and authoritative Use validation.
- Preview-led wide/compact TUI, contextual controls, visible focus and explicit graphics fallback.
- Updated CLI help, user guides, website and release notes.

### Fixed

- Synchronized clear/redraw where supported; RGB chrome independent of terminal ANSI colors.
- Editor details explain missing samples and known contrast infeasibility before Preview opens.

### Compatibility and limitations

- No migration: existing Profiles, overrides, Sources, captured Environments and History stay intact. `gradient-v1`, `oklch-v1` output and Save / Save and use / Cancel semantics are preserved. New guided/imported Profiles and confirmed Reset ALL colors use v2.
- Infeasible v2 is a **non-conforming fallback**, not valid 4.5:1. No universal accessibility or timing guarantee; [known timing outliers](https://github.com/GiovanniCaiazzo01/Ghostty-wall/issues/19) remain disclosed.
- Back up Managed Root before downgrading: older binaries reject new effect/palette identifiers. [Effects and compatibility](docs/advanced-usage.md#reversible-image-blur-and-darkening).

## [1.3.0] - 2026-10-23

### Added

- Consistent full-screen Save without applying, Save and use with default-Back confirmation, and Cancel across Profile creation/editing, including compact layouts.
- Source inspection, complete-definition editing, listing-only availability checks and confirmed removal of unused Sources; preserve original images and replayable History.

### Fixed

- Browser Use commits the reviewed image bytes, resolved colors and managed settings without resolving again. Input changes are checked under the mutation lock and require a refreshed preview and another explicit Use; loading and failed previews cannot apply. Standalone CLI apply retains normal resolution.
- Automatic `kmeans-v4` text colors account for linear-sRGB wallpaper brightness, alpha and selected opacity; customized slots and historical replay remain unchanged. Samples warn when contrast is insufficient, including recipes where image opacity is too high for automatic colors alone.
- Profile browser no longer repaints while idle. Palette clustering stops at stable centers without changing v1–v3 palette output; editor opacity changes reuse image analysis.

### Compatibility and limitations

- New generated-color Activations use `kmeans-v4` provenance. Existing v1–v3 History remains readable; older binaries may reject v4 Activations. Back up the Managed Root before downgrading.
- Readability uses a conservative managed linear-sRGB model, not effective Ghostty settings or a universal accessibility guarantee. Ghostty 1.3.1 linear-corrected rendering was compared with the model, and D-Bus reload visibly updated one and two owned windows. Native/P3 blending, desktop transparency, dim/application-specific text and experimental macOS remain outside that proof. See [release validation](docs/release-1.3.0-validation.md).

## [1.2.0] - 2026-09-28

### Added

- Automatic, asynchronous Profile previews with full-color Ghostty graphics, linear-light wallpaper compositing, matching colors and bounded background preparation; navigation and quit remain available while loading.
- Full-screen Profile forms, image pickers, confirmations and scrollable reports, including discoverable full-path/error details for long picker entries.
- Contained TUI maintenance and update progress, explicit mutation confirmation, and clear cancellation/restart boundaries.
- Safe updates for Cargo installations alongside release-installer installations, including custom prefixes, staged official-tag builds, ownership verification and binary/metadata rollback.
- Readable standalone CLI choices with explicit defaults, terminal-aware semantic colors, NO_COLOR support and wrapped labels; redirected output and JSON stay plain.

### Changed

- New image Profiles, including newly initialized Welcome, default to wallpaper opacity `0.05` instead of `0.1`. Existing Profiles, omitted settings, edits, duplicates, replay and terminal transparency are unchanged; explicit draft choices win.
- The management browser requires 60×18 for simultaneous list/sample display; Create/Edit forms retain their separate 40×12 minimum.
- README and documentation site describe the new preview, management, update and creation workflows.

### Fixed

- Obsolete preview results cannot replace the current selection; redraw/resize reuse prepared content while local inputs remain unchanged.
- Archive expansion limits now cover tar headers and extension records before parsing, not just extracted file payloads.

### Compatibility and limitations

- No persisted-format migration or seed-algorithm change. Existing version 1/2 Profiles and History remain readable; back up the complete Managed Root before downgrading.
- Linux is supported; macOS remains experimental. Native macOS and non-x86_64 updater execution have not been verified; prebuilt artifacts remain Linux x86_64 only.
- Samples are approximate and read-only, not live Ghostty reload. Native blending, Display P3, effective user configuration, fonts, blur and desktop transparency can differ. Unsupported terminals show a labelled color-cell fallback.
- Updates concern Ghostty Wall, not Ghostty. Keep other installers idle and restart after replacement. Binary/metadata publication supports rollback but is not crash-atomic; forced termination or power loss may require recovery from retained files.

## [1.1.0]

### Added

- Guided `create [PROFILE]`: generate a stable wallpaper or pick a decoded PNG/JPEG from localized Downloads/Pictures, save a complete Profile, then choose Use now or Not now.
- Visual `edit [PROFILE]` with Wallpaper, Colors and Terminal controls, exact hex/numeric input, internal samples, draft cancellation and confirmed Save and use.
- Full-screen Profile management with Create, Edit, Delete, Use, active markers, error details, compact layouts and an Actions menu; no command opens the TUI.
- Confirmed `delete [PROFILE]` with protected Welcome fallback for active Profiles and conservative, proven-exclusive image cleanup.
- Profile/Source management commands, read-only `preview`, `list` and `history`, duplication, safe rename and retained advanced field edits.
- Version 2 Profile Intent for owned images, stable generation recipes and per-color automatic/customized overrides. Existing version 1 Profiles and historical palettes remain readable.
- Library-only provisional preview sessions with state locking, crash recovery, temporary image cleanup and distinct restoration/reload outcomes.

### Changed

- Wallpaper-coordinated `kmeans-v3` colors retain readable automatic text/ANSI contrast; wallpaper replacement preserves customized slots.
- Website guides cover current commands, visual workflows, safe deletion, updates and compatibility. Documentation version and minimum Rust version follow Cargo metadata.
- Rust 1.88 or newer is required by current dependencies.

### Fixed

- Explicit state-lock release prevents inherited child descriptors from causing intermittent false `Busy` errors after an operation ends.
- Visual editing preserves legacy inline/dotted colors, concurrent changes and uncertain-publication evidence.
- Save rollback preserves original/shared images; deletion verifies confirmed file and directory identities before removal.
- PTY signal tests normalize SIGINT only in disposable children, including background runners.

### Compatibility and limitations

- Back up the complete Managed Root before downgrading. Older readers may reject version 2 Intent or new generated-color History; restore a matching data backup with an older executable.
- Editor/TUI samples are internal, not live Ghostty reload. Real-window draft/cancel restoration and window scope remain unverified; the live session API is not exposed by CLI/TUI.
- Linux is the stable target; macOS remains experimental. Reload acceptance is not proof of visible change.

## [1.0.4]

### Added

- Explicit `ghostty-wall update` and read-only `update --check` for official Linux x86_64 releases.
- SHA-256-verified, atomic executable replacement for release-installer-owned binaries; Cargo and manual installations keep their original update method.

## [1.0.3]

### Added

- One-command Linux x86_64 release installer with SHA-256 verification and explicit next steps; initialization remains manual.
- Stable release archive and checksum aliases alongside versioned artifacts for `/releases/latest/download/`.

### Changed

- Improved welcome profile contrast, desktop reload diagnostics, and user documentation.

## [1.0.2]

### Added

- Explicit `init --welcome` seeds the bundled example into an untouched empty v1.0.0 installation; refuses customized Intent and durable state.
- One `scripts/install.sh` installs Rust v1 from source or release archive; legacy Bash installer remains at tag `v0.2.2`.
- Local `.scratch/` tracker is no longer published in the current repository tree.

## [1.0.1]

### Added

- Fresh `init` provides a working `welcome` Profile with bundled wallpaper and generated colors; existing installations are preserved.
- Project mascot in README and Linux release archive.

## [1.0.0]

### Added

- Rust CLI for initialization, planning, apply, History replay, Doctor, migration, safe uninstall, and terminal Profile browsing.
- Reproducible Profile resolution for local-directory and commit-pinned GitHub Sources.
- Immutable Environments, content-addressed Assets, append-only Activations, and recoverable Ghostty Projection.
- Explicit, named-theme, and deterministic wallpaper-generated colors.
- Stable Linux systemd reload adapter and experimental macOS AppleScript adapter.
- Tagged Linux x86_64 binary archive with checksum and installer.

### Changed

- Linux is stable v1 platform; macOS remains experimental pending real-system verification.
- Bash v0 is superseded by Rust v1 and remains available at tag `v0.2.2`.

## [0.2.2]

### Changed
- Added Linux automatic reload support through Ghostty's documented systemd user service, `app-com.mitchellh.ghostty.service`.
- Improved Linux reload fallback messages to explain that the wallpaper is already configured and can be applied immediately with a manual Ghostty reload or restart.

### Documentation
- Clarified the Linux systemd-user-service requirement for automatic Ghostty reload support.

## [0.2.1]

### Changed
- Updated the Integration GitHub Actions workflow to `actions/checkout@v5`.

### Documentation
- Aligned the changelog and release history after publishing `v0.2.0`.

## [0.2.0]

### Changed
- Improved shell script linting and CI shellcheck coverage.
- Improved shell portability across Linux and macOS for regex and `mktemp` usage.
- Updated GitHub Actions checkout usage to `actions/checkout@v5`.
- Moved regular CI coverage to Ubuntu while keeping macOS release validation manual.

### Documentation
- Added a source-only GitHub Releases flow to the README.
- Expanded the release checklist with CI, tagging, and publish steps.

## [0.1.0]

### Added
- Initial public release with the `ghostty-wall` CLI.
- Installer and uninstall scripts for macOS and Linux.
- GitHub repository-based wallpaper selection for Ghostty.
