# Ghostty Wall v1 Release Notes

## Summary

- Rust v1 Profile-to-Environment workflow: init, plan, apply, previous, doctor, and terminal browser.
- Durable immutable Environments, content-addressed Assets, append-only Activation History, and recoverable Ghostty Projection.
- Local-directory and commit-pinned GitHub Sources; explicit, theme, and generated colors.

## Install

Linux x86_64 release archive includes binary and `install-v1.sh`. Verify matching SHA-256 checksum before installing.

macOS support remains experimental and requires source build.

## Verification

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`
- `bash scripts/test.sh` (Bash v0 regression)
- shellcheck for all maintained shell scripts
- live GitHub integration workflow
- Linux artifact install and Profile-to-Environment smoke test
- macOS automated adapter smoke; real-system verification tracked separately

## Upgrade from Bash v0

```bash
ghostty-wall init --migrate-legacy --dry-run
ghostty-wall init --migrate-legacy
```

Legacy files remain untouched. Users create Profiles after reviewing imported Sources. Bash v0 remains available at tag `v0.2.2`.

## Uninstall

Run `ghostty-wall uninstall` before removing binary. This removes Integration Hooks and disposable Projection/cache while preserving Intent and durable History.

## Known limitations

- Linux release binary targets x86_64 GNU/Linux; other architectures build from source.
- macOS remains experimental pending issue 14 real-system evidence.
- Windows, direct `random`/`set` commands, schedulers, and destructive purge are outside v1.
