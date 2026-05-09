# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, adapted to the current size of this repository.

## [Unreleased]

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
