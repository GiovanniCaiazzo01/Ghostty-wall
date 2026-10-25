# Ghostty Wall v1.4.0

## What's new

- Responsive browsing/editing: asynchronous previews, bounded reuse, unchanged authoritative Use validation.
- Varied `composition-v1` wallpapers, saved until explicit regeneration.
- Reversible wallpaper blur and adjustable darkening; original images stay untouched.
- Role-aware `oklch-v2` colors with explicit contrast-infeasibility warnings.
- Preview-led wide/compact TUI, keyboard focus and synchronized clear/redraw.

## Compatibility

Existing Profiles, customized colors, captured Environments and History are preserved. `gradient-v1` and `oklch-v1` retain their outputs. Save / Save and use / Cancel remain distinct.

**Back up Managed Root before downgrading:** older binaries reject new effect/palette identifiers. Infeasible `oklch-v2` is a **non-conforming fallback**, not valid 4.5:1. Previews are approximate, not live reload; no universal accessibility or performance guarantee.

## Availability

Linux x86_64: use `ghostty-wall update` or the [installer](https://giovannicaiazzo01.github.io/Ghostty-wall/installation/). Release archives include matching SHA-256 checksums. macOS remains experimental.

## Validation

[PR #20](https://github.com/GiovanniCaiazzo01/Ghostty-wall/pull/20): CI passed, including 121 PTY tests. [Issue #19](https://github.com/GiovanniCaiazzo01/Ghostty-wall/issues/19): previous local tests, Ghostty review, timings and known outliers. These do not identify a version-bumped release artifact; release CI validates that separately.

[Guide](https://giovannicaiazzo01.github.io/Ghostty-wall/) · [Changelog](https://github.com/GiovanniCaiazzo01/Ghostty-wall/blob/main/CHANGELOG.md)
