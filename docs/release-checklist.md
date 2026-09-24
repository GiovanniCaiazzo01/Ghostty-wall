# Release Checklist

## Automated

1. Run `bash scripts/test.sh`
2. Run `bash scripts/integration-test.sh`
3. If available locally, run `shellcheck -x bin/ghostty-wall scripts/install.sh scripts/mac/install-mac.sh scripts/linux/install-linux.sh scripts/uninstall.sh scripts/test.sh scripts/integration-test.sh`
4. Verify the `CI` GitHub Actions workflow passed for the exact commit you plan to tag
5. Trigger the `Integration` GitHub Actions workflow and verify it passes

## Linux Smoke Test

1. Start from a clean temp HOME or a disposable machine
2. Run `./scripts/install.sh`
3. Confirm `ghostty-wall --list` works
4. Run `ghostty-wall`
5. Confirm `${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/wallpaper.conf` exists
6. Confirm `${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/config` contains the include line
7. If Ghostty is running via `app-com.mitchellh.ghostty.service`, confirm automatic reload works through the systemd user service
8. Otherwise, confirm the fallback log clearly instructs the user to press `Ctrl+Shift+,` or restart Ghostty
9. Run `./scripts/uninstall.sh`
10. Confirm the binary was removed and config files were left intact

## macOS Smoke Test

1. Run `./scripts/install.sh`
2. Launch `Ghostty` and repeat with `Ghostty Nightly` if you support both
3. Run `ghostty-wall`
4. Confirm the wallpaper file and include were written
5. Confirm Ghostty reloads automatically when Automation/Accessibility permissions are granted
6. Confirm the fallback log is clear when Ghostty is not running
7. Confirm the script still works with the default macOS Bash/userland environment
8. Run `./scripts/uninstall.sh`

## Rust v1 Runtime Adapter Smoke

### Linux (stable)

1. Record Ghostty and systemd versions and confirm `systemctl` resolves through `PATH`.
2. Launch Ghostty through `app-com.mitchellh.ghostty.service` and verify `systemctl --user is-active --quiet app-com.mitchellh.ghostty.service` succeeds.
3. Run `ghostty-wall apply PROFILE`; confirm the Activation and `current.ghostty` commit before runtime reload.
4. Confirm Ghostty updates and `systemctl --user reload app-com.mitchellh.ghostty.service` succeeds.
5. Stop the service and apply again; confirm reload reports unavailable while the new Activation remains committed.
6. Force the reload command to fail on a disposable setup; confirm reload reports failed while the new Activation remains committed.

### macOS (experimental)

1. Record macOS and Ghostty versions, enable Ghostty's `macos-applescript` option, and launch Ghostty with one terminal.
2. Verify the API directly with `osascript -e 'tell application "Ghostty" to perform action "reload_config" on terminal 1'`.
3. Run `ghostty-wall apply PROFILE`; confirm the Activation and `current.ghostty` commit before the experimental reload, then confirm Ghostty updates.
4. Quit Ghostty and apply again; confirm reload reports unavailable while the new Activation remains committed.
5. Disable AppleScript or deny automation on a disposable setup and apply again; confirm reload reports failed while the new Activation remains committed.
6. Record results as experimental evidence; do not promote macOS support until issue 14's real-system verification passes.

## Failure Cases

1. Run with an invalid repo line and confirm validation errors are clear
2. Run with a bad GitHub path and confirm the tool falls through to the next repo
3. Run without network access and confirm install warns instead of pretending the first run succeeded
4. Run with a missing `curl` binary and confirm the tool exits with a clear error

## Release Decision

1. README matches current platform support and commands
2. Installer, runtime, and uninstall behavior were all exercised at least once on Linux and macOS
3. CI and Integration workflows passed recently enough to trust the tagged source

## Publish Source Release

1. Choose the version tag (for example `v0.2.0`)
2. Ensure the tag points to the exact commit validated above
3. Push the tag to GitHub
4. Draft a GitHub Release from that tag
5. Use GitHub's auto-generated source archives only (`.zip` and `.tar.gz`)
6. Draft release notes from `docs/release-notes-template.md` and reconcile them with `CHANGELOG.md`
7. Publish the release and verify the source archives are downloadable
