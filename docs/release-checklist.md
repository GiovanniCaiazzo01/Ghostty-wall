# Release Checklist

## Automated

1. Run `bash scripts/test.sh`
2. Run `bash scripts/integration-test.sh`
3. If available locally, run `shellcheck bin/ghostty-wall scripts/install.sh scripts/mac/install-mac.sh scripts/linux/install-linux.sh scripts/uninstall.sh scripts/test.sh scripts/integration-test.sh`
4. Trigger the `Integration` GitHub Actions workflow and verify it passes

## Linux Smoke Test

1. Start from a clean temp HOME or a disposable machine
2. Run `./scripts/install.sh`
3. Confirm `ghostty-wall --list` works
4. Run `ghostty-wall`
5. Confirm `${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/wallpaper.conf` exists
6. Confirm `${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/config` contains the include line
7. Run `./scripts/uninstall.sh`
8. Confirm the binary was removed and config files were left intact

## macOS Smoke Test

1. Run `./scripts/install.sh`
2. Launch `Ghostty` and repeat with `Ghostty Nightly` if you support both
3. Run `ghostty-wall`
4. Confirm the wallpaper file and include were written
5. Confirm Ghostty reloads automatically when Automation/Accessibility permissions are granted
6. Confirm the fallback log is clear when Ghostty is not running
7. Run `./scripts/uninstall.sh`

## Failure Cases

1. Run with an invalid repo line and confirm validation errors are clear
2. Run with a bad GitHub path and confirm the tool falls through to the next repo
3. Run without network access and confirm install warns instead of pretending the first run succeeded
4. Run with a missing `curl` binary and confirm the tool exits with a clear error

## Release Decision

1. README matches current platform support and commands
2. Installer, runtime, and uninstall behavior were all exercised at least once on Linux and macOS
3. Integration workflow passed recently enough to trust the live GitHub path
