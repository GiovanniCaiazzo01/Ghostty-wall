# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, adapted to the current size of this repository.

## [Unreleased]

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
